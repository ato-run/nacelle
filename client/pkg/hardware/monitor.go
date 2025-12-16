// Package hardware provides system resource monitoring for Gumball.
// Critical for preventing Mac freezes by tracking VRAM/RAM usage.
package hardware

import (
	"context"
	"time"
)

// SystemResources represents current system resource state
type SystemResources struct {
	// VRAM (GPU memory)
	TotalVRAM     int64 `json:"total_vram"`     // bytes
	AvailableVRAM int64 `json:"available_vram"` // bytes

	// RAM (System memory)
	TotalRAM     int64 `json:"total_ram"`     // bytes
	AvailableRAM int64 `json:"available_ram"` // bytes

	// CPU
	CPUUsage float64 `json:"cpu_usage"` // 0.0 - 1.0

	// Timestamp
	Timestamp time.Time `json:"timestamp"`
}

// VRAMUsagePercent returns the VRAM usage as a percentage (0-100)
func (r *SystemResources) VRAMUsagePercent() float64 {
	if r.TotalVRAM == 0 {
		return 0
	}
	return float64(r.TotalVRAM-r.AvailableVRAM) / float64(r.TotalVRAM) * 100
}

// RAMUsagePercent returns the RAM usage as a percentage (0-100)
func (r *SystemResources) RAMUsagePercent() float64 {
	if r.TotalRAM == 0 {
		return 0
	}
	return float64(r.TotalRAM-r.AvailableRAM) / float64(r.TotalRAM) * 100
}

// TotalVRAMGB returns total VRAM in GB
func (r *SystemResources) TotalVRAMGB() float64 {
	return float64(r.TotalVRAM) / (1024 * 1024 * 1024)
}

// AvailableVRAMGB returns available VRAM in GB
func (r *SystemResources) AvailableVRAMGB() float64 {
	return float64(r.AvailableVRAM) / (1024 * 1024 * 1024)
}

// ResourceThresholds defines thresholds for resource alerts
type ResourceThresholds struct {
	VRAMWarningPercent float64       `yaml:"vram_warning_threshold" json:"vram_warning_percent"` // e.g., 80
	VRAMBlockPercent   float64       `yaml:"vram_block_threshold" json:"vram_block_percent"`     // e.g., 95
	RAMWarningPercent  float64       `yaml:"ram_warning_threshold" json:"ram_warning_percent"`   // e.g., 85
	MonitorInterval    time.Duration `yaml:"monitor_interval" json:"monitor_interval"`
}

// DefaultThresholds returns sensible default thresholds
func DefaultThresholds() ResourceThresholds {
	return ResourceThresholds{
		VRAMWarningPercent: 80.0,
		VRAMBlockPercent:   95.0,
		RAMWarningPercent:  85.0,
		MonitorInterval:    5 * time.Second,
	}
}

// ResourceCheckResult represents the result of a resource check
type ResourceCheckResult struct {
	CanRun      bool   `json:"can_run"`
	Reason      string `json:"reason,omitempty"`
	VRAMWarning bool   `json:"vram_warning"`
	RAMWarning  bool   `json:"ram_warning"`
}

// HardwareMonitor defines the interface for hardware monitoring
type HardwareMonitor interface {
	// GetCurrentResources returns the current system resource state
	GetCurrentResources() (*SystemResources, error)

	// CanRunCapsule checks if there are enough resources to run a Capsule
	// vramRequired is the minimum VRAM in bytes needed by the Capsule
	CanRunCapsule(vramRequired int64) (*ResourceCheckResult, error)

	// Watch starts continuous monitoring, calling the callback on each interval
	// Cancel the context to stop watching
	Watch(ctx context.Context, interval time.Duration, callback func(*SystemResources))
}

// ResourceCallback is called when resources are updated
type ResourceCallback func(*SystemResources)
