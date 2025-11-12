package reconcile

import (
	"context"
	"database/sql"
	"fmt"
	"strings"
	"time"

	"github.com/onescluster/coordinator/pkg/db"
)

type RQLiteStore struct {
	client *db.Client
}

func NewRQLiteStore(client *db.Client) *RQLiteStore {
	return &RQLiteStore{client: client}
}

func (s *RQLiteStore) ListDesiredWorkloads(ctx context.Context) (map[string]string, error) {
	query := fmt.Sprintf(`
        SELECT id, node_id
        FROM capsules
        WHERE status IN ('%s', '%s', '%s')
    `,
		db.CapsuleStatusPending,
		db.CapsuleStatusRunning,
		db.CapsuleStatusFailed,
	)

	result, err := s.client.QueryContext(ctx, query)
	if err != nil {
		return nil, fmt.Errorf("query desired workloads: %w", err)
	}

	desired := make(map[string]string)
	for result.Next() {
		var id, nodeID string
		if err := result.Scan(&id, &nodeID); err != nil {
			return nil, fmt.Errorf("scan desired workload: %w", err)
		}
		desired[id] = nodeID
	}

	return desired, nil
}

func (s *RQLiteStore) ListNodeWorkloads(ctx context.Context) ([]*db.NodeWorkload, error) {
	query := `
		SELECT
			node_id,
			workload_id,
			name,
			reserved_vram_bytes,
			observed_vram_bytes,
			pid,
			phase,
			updated_at
		FROM node_workloads
	`

	result, err := s.client.QueryContext(ctx, query)
	if err != nil {
		return nil, fmt.Errorf("query node workloads: %w", err)
	}

	workloads := make([]*db.NodeWorkload, 0)
	for result.Next() {
		var wl db.NodeWorkload
		var reserved int64
		var observed int64
		var pid sql.NullInt64
		var updatedAt int64
		if err := result.Scan(
			&wl.NodeID,
			&wl.WorkloadID,
			&wl.Name,
			&reserved,
			&observed,
			&pid,
			&wl.Phase,
			&updatedAt,
		); err != nil {
			return nil, fmt.Errorf("scan node workload: %w", err)
		}
		wl.ReservedVRAMBytes = uint64(reserved)
		wl.ObservedVRAMBytes = uint64(observed)
		wl.PID = pid
		wl.UpdatedAt = time.Unix(updatedAt, 0)
		workloads = append(workloads, &wl)
	}

	return workloads, nil
}

func (s *RQLiteStore) MarkCapsuleFailed(ctx context.Context, capsuleID string) error {
	return s.setCapsuleStatus(ctx, capsuleID, db.CapsuleStatusFailed)
}

func (s *RQLiteStore) MarkCapsulePending(ctx context.Context, capsuleID string) error {
	return s.setCapsuleStatus(ctx, capsuleID, db.CapsuleStatusPending)
}

func (s *RQLiteStore) setCapsuleStatus(ctx context.Context, capsuleID string, status db.CapsuleStatus) error {
	query := fmt.Sprintf(`
        UPDATE capsules
        SET status = '%s', updated_at = %d
        WHERE id = '%s'
    `,
		escapeSQLString(string(status)),
		time.Now().Unix(),
		escapeSQLString(capsuleID),
	)

	if err := s.execute(ctx, query); err != nil {
		return fmt.Errorf("update capsule status: %w", err)
	}
	return nil
}

func escapeSQLString(input string) string {
	return strings.ReplaceAll(input, "'", "''")
}

func (s *RQLiteStore) execute(ctx context.Context, query string) error {
	select {
	case <-ctx.Done():
		return ctx.Err()
	default:
	}

	return s.client.Execute(query)
}
