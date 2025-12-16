// Package store provides a SQLite-based store for Capsule state management.
// Replaces rqlite in Gumball v0.3.0 with local-only state management.
package store

import (
	"context"
	"time"
)

// CapsuleStatus represents the current state of a Capsule
type CapsuleStatus string

const (
	StatusStopped  CapsuleStatus = "stopped"
	StatusStarting CapsuleStatus = "starting"
	StatusRunning  CapsuleStatus = "running"
	StatusError    CapsuleStatus = "error"
)

// Capsule represents an installed capsule in the store
type Capsule struct {
	Name         string        `json:"name"`
	Version      string        `json:"version"`
	Type         string        `json:"type"`
	ManifestPath string        `json:"manifest_path"`
	Status       CapsuleStatus `json:"status"`
	InstalledAt  time.Time     `json:"installed_at"`
	LastUsed     time.Time     `json:"last_used,omitempty"`
}

// ProcessInfo represents a running capsule process
type ProcessInfo struct {
	CapsuleName string    `json:"capsule_name"`
	PID         int       `json:"pid"`
	Port        int       `json:"port,omitempty"`
	StartedAt   time.Time `json:"started_at"`
}

// Store defines the interface for Capsule state management
type Store interface {
	// Initialize the database (run migrations)
	Initialize() error

	// Close the database connection
	Close() error

	// Capsule CRUD operations
	Install(ctx context.Context, capsule *Capsule) error
	Get(ctx context.Context, name string) (*Capsule, error)
	List(ctx context.Context) ([]*Capsule, error)
	ListByStatus(ctx context.Context, status CapsuleStatus) ([]*Capsule, error)
	UpdateStatus(ctx context.Context, name string, status CapsuleStatus) error
	UpdateLastUsed(ctx context.Context, name string) error
	Delete(ctx context.Context, name string) error

	// Process tracking
	RecordStart(ctx context.Context, name string, pid int) error
	RecordStop(ctx context.Context, name string, pid int) error
	GetProcess(ctx context.Context, name string) (*ProcessInfo, error)
	GetRunningProcesses(ctx context.Context) ([]*ProcessInfo, error)

	// Hardware monitoring
	RecordHardwareSnapshot(ctx context.Context, snapshot *HardwareSnapshot) error
	GetLatestHardware(ctx context.Context) (*HardwareSnapshot, error)
	GetHardwareHistory(ctx context.Context, since time.Time, limit int) ([]*HardwareSnapshot, error)

	// Route decision logging (v0.3.0)
	RecordRouteDecision(ctx context.Context, capsuleName, decision, reason string, vramUsage float64) error
	GetRecentRouteDecisions(ctx context.Context, limit int) ([]*RouteDecisionLog, error)

	// Local node configuration (v0.3.0)
	GetLocalNode(ctx context.Context) (*LocalNodeConfig, error)
	SetLocalNode(ctx context.Context, cfg *LocalNodeConfig) error
}

// HardwareSnapshot represents a point-in-time snapshot of system resources
type HardwareSnapshot struct {
	ID              int64     `json:"id"`
	Timestamp       time.Time `json:"timestamp"`
	TotalVRAMGB     float64   `json:"total_vram_gb"`
	AvailableVRAMGB float64   `json:"available_vram_gb"`
	TotalRAMGB      float64   `json:"total_ram_gb"`
	AvailableRAMGB  float64   `json:"available_ram_gb"`
	CPUUsagePercent float64   `json:"cpu_usage_percent"`
}

// VRAMUsagePercent calculates the percentage of VRAM in use
func (h *HardwareSnapshot) VRAMUsagePercent() float64 {
	if h.TotalVRAMGB == 0 {
		return 0
	}
	return (h.TotalVRAMGB - h.AvailableVRAMGB) / h.TotalVRAMGB * 100
}

// RAMUsagePercent calculates the percentage of RAM in use
func (h *HardwareSnapshot) RAMUsagePercent() float64 {
	if h.TotalRAMGB == 0 {
		return 0
	}
	return (h.TotalRAMGB - h.AvailableRAMGB) / h.TotalRAMGB * 100
}
