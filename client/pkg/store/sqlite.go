package store

import (
	"context"
	"database/sql"
	_ "embed"
	"fmt"
	"os"
	"path/filepath"
	"time"

	_ "modernc.org/sqlite"
)

//go:embed migrations/001_initial.sql
var migration001SQL string

//go:embed migrations/002_cluster_state.sql
var migration002SQL string

// SQLiteStore implements Store using SQLite
type SQLiteStore struct {
	db     *sql.DB
	dbPath string
}

// NewSQLiteStore creates a new SQLite store
// dbPath should be the path to the SQLite database file (e.g., ~/.gumball/registry.db)
func NewSQLiteStore(dbPath string) (*SQLiteStore, error) {
	// Ensure directory exists
	dir := filepath.Dir(dbPath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return nil, fmt.Errorf("failed to create directory: %w", err)
	}

	db, err := sql.Open("sqlite", dbPath)
	if err != nil {
		return nil, fmt.Errorf("failed to open database: %w", err)
	}

	// Enable foreign keys
	if _, err := db.Exec("PRAGMA foreign_keys = ON"); err != nil {
		db.Close()
		return nil, fmt.Errorf("failed to enable foreign keys: %w", err)
	}

	store := &SQLiteStore{
		db:     db,
		dbPath: dbPath,
	}

	// Auto-initialize
	if err := store.Initialize(); err != nil {
		db.Close()
		return nil, err
	}

	return store, nil
}

// Initialize runs database migrations
func (s *SQLiteStore) Initialize() error {
	// Run migrations in order
	migrations := []string{migration001SQL, migration002SQL}
	for i, migration := range migrations {
		_, err := s.db.Exec(migration)
		if err != nil {
			return fmt.Errorf("failed to run migration %d: %w", i+1, err)
		}
	}
	return nil
}

// Close closes the database connection
func (s *SQLiteStore) Close() error {
	return s.db.Close()
}

// Install registers a new Capsule
func (s *SQLiteStore) Install(ctx context.Context, capsule *Capsule) error {
	_, err := s.db.ExecContext(ctx, `
		INSERT INTO capsules (id, version, type, manifest_path, status, installed_at)
		VALUES (?, ?, ?, ?, ?, ?)
		ON CONFLICT(id) DO UPDATE SET
			version = excluded.version,
			type = excluded.type,
			manifest_path = excluded.manifest_path
	`, capsule.Name, capsule.Version, capsule.Type, capsule.ManifestPath, string(capsule.Status), capsule.InstalledAt.Unix())

	if err != nil {
		return fmt.Errorf("failed to install capsule: %w", err)
	}
	return nil
}

// Get retrieves a Capsule by name
func (s *SQLiteStore) Get(ctx context.Context, name string) (*Capsule, error) {
	row := s.db.QueryRowContext(ctx, `
		SELECT id, version, type, manifest_path, status, installed_at, last_used_at
		FROM capsules WHERE id = ?
	`, name)

	var c Capsule
	var installedAtUnix int64
	var lastUsedAtUnix sql.NullInt64
	var status string

	err := row.Scan(&c.Name, &c.Version, &c.Type, &c.ManifestPath, &status, &installedAtUnix, &lastUsedAtUnix)
	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("capsule not found: %s", name)
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get capsule: %w", err)
	}

	c.Status = CapsuleStatus(status)
	c.InstalledAt = time.Unix(installedAtUnix, 0)
	if lastUsedAtUnix.Valid {
		c.LastUsed = time.Unix(lastUsedAtUnix.Int64, 0)
	}

	return &c, nil
}

// List returns all installed Capsules
func (s *SQLiteStore) List(ctx context.Context) ([]*Capsule, error) {
	rows, err := s.db.QueryContext(ctx, `
		SELECT id, version, type, manifest_path, status, installed_at, last_used_at
		FROM capsules ORDER BY installed_at DESC
	`)
	if err != nil {
		return nil, fmt.Errorf("failed to list capsules: %w", err)
	}
	defer rows.Close()

	return s.scanCapsules(rows)
}

// ListByStatus returns Capsules with a specific status
func (s *SQLiteStore) ListByStatus(ctx context.Context, status CapsuleStatus) ([]*Capsule, error) {
	rows, err := s.db.QueryContext(ctx, `
		SELECT id, version, type, manifest_path, status, installed_at, last_used_at
		FROM capsules WHERE status = ? ORDER BY installed_at DESC
	`, string(status))
	if err != nil {
		return nil, fmt.Errorf("failed to list capsules: %w", err)
	}
	defer rows.Close()

	return s.scanCapsules(rows)
}

// UpdateStatus updates a Capsule's status
func (s *SQLiteStore) UpdateStatus(ctx context.Context, name string, status CapsuleStatus) error {
result, err := s.db.ExecContext(ctx, `UPDATE capsules SET status = ? WHERE id = ?`, string(status), name)
if err != nil {
return fmt.Errorf("failed to update status: %w", err)
}

rows, err := result.RowsAffected()
if err != nil {
return err
}
if rows == 0 {
return fmt.Errorf("capsule not found: %s", name)
}
return nil
}

// UpdateLastUsed updates the last_used_at timestamp
func (s *SQLiteStore) UpdateLastUsed(ctx context.Context, name string) error {
_, err := s.db.ExecContext(ctx, `UPDATE capsules SET last_used_at = ? WHERE id = ?`, time.Now().Unix(), name)
if err != nil {
return fmt.Errorf("failed to update last_used_at: %w", err)
}
return nil
}

// Delete removes a Capsule
func (s *SQLiteStore) Delete(ctx context.Context, name string) error {
result, err := s.db.ExecContext(ctx, `DELETE FROM capsules WHERE id = ?`, name)
if err != nil {
return fmt.Errorf("failed to remove capsule: %w", err)
}

rows, err := result.RowsAffected()
if err != nil {
return err
}
if rows == 0 {
return fmt.Errorf("capsule not found: %s", name)
}
return nil
}

// RecordStart records that a Capsule process has started
func (s *SQLiteStore) RecordStart(ctx context.Context, name string, pid int) error {
_, err := s.db.ExecContext(ctx, `
INSERT INTO processes (capsule_id, pid, started_at)
VALUES (?, ?, ?)
ON CONFLICT(capsule_id) DO UPDATE SET
pid = excluded.pid,
started_at = excluded.started_at
`, name, pid, time.Now().Unix())

if err != nil {
return fmt.Errorf("failed to record start: %w", err)
}

return s.UpdateStatus(ctx, name, StatusStarting)
}

// RecordStop records that a Capsule process has stopped
func (s *SQLiteStore) RecordStop(ctx context.Context, name string, pid int) error {
_, err := s.db.ExecContext(ctx, `DELETE FROM processes WHERE capsule_id = ? AND pid = ?`, name, pid)
if err != nil {
return fmt.Errorf("failed to record stop: %w", err)
}

return s.UpdateStatus(ctx, name, StatusStopped)
}

// GetProcess retrieves process info for a Capsule
func (s *SQLiteStore) GetProcess(ctx context.Context, name string) (*ProcessInfo, error) {
row := s.db.QueryRowContext(ctx, `
SELECT capsule_id, pid, port, started_at
FROM processes WHERE capsule_id = ?
`, name)

var info ProcessInfo
var startedAtUnix int64
var port sql.NullInt64
err := row.Scan(&info.CapsuleName, &info.PID, &port, &startedAtUnix)
if err == sql.ErrNoRows {
return nil, nil
}
if err != nil {
return nil, fmt.Errorf("failed to get process: %w", err)
}

if port.Valid {
info.Port = int(port.Int64)
}
info.StartedAt = time.Unix(startedAtUnix, 0)
return &info, nil
}

// GetRunningProcesses returns all running Capsule processes
func (s *SQLiteStore) GetRunningProcesses(ctx context.Context) ([]*ProcessInfo, error) {
rows, err := s.db.QueryContext(ctx, `
SELECT capsule_id, pid, port, started_at
FROM processes
`)
if err != nil {
return nil, fmt.Errorf("failed to get running processes: %w", err)
}
defer rows.Close()

var processes []*ProcessInfo
for rows.Next() {
var info ProcessInfo
var startedAtUnix int64
var port sql.NullInt64
if err := rows.Scan(&info.CapsuleName, &info.PID, &port, &startedAtUnix); err != nil {
return nil, err
}
if port.Valid {
info.Port = int(port.Int64)
}
info.StartedAt = time.Unix(startedAtUnix, 0)
processes = append(processes, &info)
}

return processes, nil
}

// RecordHardwareSnapshot stores a hardware snapshot
func (s *SQLiteStore) RecordHardwareSnapshot(ctx context.Context, snapshot *HardwareSnapshot) error {
_, err := s.db.ExecContext(ctx, `
INSERT INTO hardware_snapshots (timestamp, total_vram_gb, available_vram_gb, total_ram_gb, available_ram_gb, cpu_usage_percent)
VALUES (?, ?, ?, ?, ?, ?)
`, snapshot.Timestamp.Unix(), snapshot.TotalVRAMGB, snapshot.AvailableVRAMGB, snapshot.TotalRAMGB, snapshot.AvailableRAMGB, snapshot.CPUUsagePercent)

if err != nil {
return fmt.Errorf("failed to record hardware snapshot: %w", err)
}
return nil
}

// GetLatestHardware returns the most recent hardware snapshot
func (s *SQLiteStore) GetLatestHardware(ctx context.Context) (*HardwareSnapshot, error) {
row := s.db.QueryRowContext(ctx, `
SELECT id, timestamp, total_vram_gb, available_vram_gb, total_ram_gb, available_ram_gb, cpu_usage_percent
FROM hardware_snapshots ORDER BY timestamp DESC LIMIT 1
`)

var snapshot HardwareSnapshot
var timestampUnix int64
err := row.Scan(&snapshot.ID, &timestampUnix, &snapshot.TotalVRAMGB, &snapshot.AvailableVRAMGB, &snapshot.TotalRAMGB, &snapshot.AvailableRAMGB, &snapshot.CPUUsagePercent)
if err == sql.ErrNoRows {
return nil, nil
}
if err != nil {
return nil, fmt.Errorf("failed to get latest hardware: %w", err)
}

snapshot.Timestamp = time.Unix(timestampUnix, 0)
return &snapshot, nil
}

// GetHardwareHistory returns hardware snapshots since a given time
func (s *SQLiteStore) GetHardwareHistory(ctx context.Context, since time.Time, limit int) ([]*HardwareSnapshot, error) {
rows, err := s.db.QueryContext(ctx, `
SELECT id, timestamp, total_vram_gb, available_vram_gb, total_ram_gb, available_ram_gb, cpu_usage_percent
FROM hardware_snapshots
WHERE timestamp >= ?
ORDER BY timestamp DESC
LIMIT ?
`, since.Unix(), limit)
if err != nil {
return nil, fmt.Errorf("failed to get hardware history: %w", err)
}
defer rows.Close()

var snapshots []*HardwareSnapshot
for rows.Next() {
var snapshot HardwareSnapshot
var timestampUnix int64
if err := rows.Scan(&snapshot.ID, &timestampUnix, &snapshot.TotalVRAMGB, &snapshot.AvailableVRAMGB, &snapshot.TotalRAMGB, &snapshot.AvailableRAMGB, &snapshot.CPUUsagePercent); err != nil {
return nil, err
}
snapshot.Timestamp = time.Unix(timestampUnix, 0)
snapshots = append(snapshots, &snapshot)
}

return snapshots, nil
}

// Helper functions

func (s *SQLiteStore) scanCapsules(rows *sql.Rows) ([]*Capsule, error) {
	var capsules []*Capsule

	for rows.Next() {
		var c Capsule
		var installedAtUnix int64
		var lastUsedAtUnix sql.NullInt64
		var status string

		if err := rows.Scan(&c.Name, &c.Version, &c.Type, &c.ManifestPath, &status, &installedAtUnix, &lastUsedAtUnix); err != nil {
			return nil, err
		}

		c.Status = CapsuleStatus(status)
		c.InstalledAt = time.Unix(installedAtUnix, 0)
		if lastUsedAtUnix.Valid {
			c.LastUsed = time.Unix(lastUsedAtUnix.Int64, 0)
		}

		capsules = append(capsules, &c)
	}

	return capsules, nil
}

// =============================================================================
// Route Decision Logging (v0.3.0)
// =============================================================================

// RouteDecisionLog represents a logged routing decision
type RouteDecisionLog struct {
	ID               int64     `json:"id"`
	CapsuleName      string    `json:"capsule_name"`
	Decision         string    `json:"decision"` // "local" or "cloud"
	Reason           string    `json:"reason,omitempty"`
	VRAMUsagePercent float64   `json:"vram_usage_percent"`
	Timestamp        time.Time `json:"timestamp"`
}

// RecordRouteDecision logs a routing decision for analytics
func (s *SQLiteStore) RecordRouteDecision(ctx context.Context, capsuleName, decision, reason string, vramUsage float64) error {
	_, err := s.db.ExecContext(ctx, `
		INSERT INTO route_decisions (capsule_name, decision, reason, vram_usage_percent, timestamp)
		VALUES (?, ?, ?, ?, ?)
	`, capsuleName, decision, reason, vramUsage, time.Now().Unix())

	if err != nil {
		return fmt.Errorf("failed to record route decision: %w", err)
	}
	return nil
}

// GetRecentRouteDecisions returns recent routing decisions
func (s *SQLiteStore) GetRecentRouteDecisions(ctx context.Context, limit int) ([]*RouteDecisionLog, error) {
	rows, err := s.db.QueryContext(ctx, `
		SELECT id, capsule_name, decision, reason, vram_usage_percent, timestamp
		FROM route_decisions
		ORDER BY timestamp DESC
		LIMIT ?
	`, limit)
	if err != nil {
		return nil, fmt.Errorf("failed to get route decisions: %w", err)
	}
	defer rows.Close()

	var decisions []*RouteDecisionLog
	for rows.Next() {
		var d RouteDecisionLog
		var timestampUnix int64
		var reason sql.NullString

		if err := rows.Scan(&d.ID, &d.CapsuleName, &d.Decision, &reason, &d.VRAMUsagePercent, &timestampUnix); err != nil {
			return nil, err
		}

		if reason.Valid {
			d.Reason = reason.String
		}
		d.Timestamp = time.Unix(timestampUnix, 0)
		decisions = append(decisions, &d)
	}

	return decisions, nil
}

// =============================================================================
// Local Node Configuration (v0.3.0)
// =============================================================================

// LocalNodeConfig represents this node's configuration
type LocalNodeConfig struct {
	NodeID    string    `json:"node_id"`
	Hostname  string    `json:"hostname"`
	TailnetIP string    `json:"tailnet_ip,omitempty"`
	IsOnline  bool      `json:"is_online"`
	CreatedAt time.Time `json:"created_at"`
	UpdatedAt time.Time `json:"updated_at"`
}

// GetLocalNode retrieves the local node configuration
func (s *SQLiteStore) GetLocalNode(ctx context.Context) (*LocalNodeConfig, error) {
	row := s.db.QueryRowContext(ctx, `
		SELECT node_id, hostname, tailnet_ip, is_online, created_at, updated_at
		FROM local_node WHERE id = 'self'
	`)

	var cfg LocalNodeConfig
	var tailnetIP sql.NullString
	var isOnline int
	var createdAtUnix, updatedAtUnix int64

	err := row.Scan(&cfg.NodeID, &cfg.Hostname, &tailnetIP, &isOnline, &createdAtUnix, &updatedAtUnix)
	if err == sql.ErrNoRows {
		return nil, nil // Not configured yet
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get local node: %w", err)
	}

	if tailnetIP.Valid {
		cfg.TailnetIP = tailnetIP.String
	}
	cfg.IsOnline = isOnline == 1
	cfg.CreatedAt = time.Unix(createdAtUnix, 0)
	cfg.UpdatedAt = time.Unix(updatedAtUnix, 0)

	return &cfg, nil
}

// SetLocalNode configures the local node
func (s *SQLiteStore) SetLocalNode(ctx context.Context, cfg *LocalNodeConfig) error {
	now := time.Now().Unix()
	isOnline := 0
	if cfg.IsOnline {
		isOnline = 1
	}

	_, err := s.db.ExecContext(ctx, `
		INSERT INTO local_node (id, node_id, hostname, tailnet_ip, is_online, created_at, updated_at)
		VALUES ('self', ?, ?, ?, ?, ?, ?)
		ON CONFLICT(id) DO UPDATE SET
			node_id = excluded.node_id,
			hostname = excluded.hostname,
			tailnet_ip = excluded.tailnet_ip,
			is_online = excluded.is_online,
			updated_at = excluded.updated_at
	`, cfg.NodeID, cfg.Hostname, cfg.TailnetIP, isOnline, now, now)

	if err != nil {
		return fmt.Errorf("failed to set local node: %w", err)
	}
	return nil
}

