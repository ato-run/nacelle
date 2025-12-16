// Package router provides Local ↔ Cloud routing decisions for Capsules.
// Implements the Resource Router for Gumball v0.3.0.
//
// Decision Logic (v0.3.0 - Simple 2-value):
//   - Light capsule + enough resources → Local
//   - Light capsule + insufficient resources + fallback enabled → Cloud
//   - Heavy capsule → Always Cloud
//   - VRAM > 95% block threshold → Force Cloud (prevent Mac freeze)
package router

import (
	"context"
	"fmt"
	"regexp"
	"strconv"
	"strings"

	"github.com/onescluster/coordinator/pkg/capsule"
	"github.com/onescluster/coordinator/pkg/hardware"
)

// RouteDecision represents where a Capsule should run
type RouteDecision string

const (
	RouteLocal RouteDecision = "local"
	RouteCloud RouteDecision = "cloud"
)

// Decision contains the routing decision with metadata
type Decision struct {
	Route       RouteDecision `json:"route"`
	CapsuleName string        `json:"capsule_name"`
	Reason      string        `json:"reason,omitempty"`
	Warning     bool          `json:"warning,omitempty"`
	WarningMsg  string        `json:"warning_msg,omitempty"`
}

// Config holds router configuration
type Config struct {
	// VRAMWarningPercent triggers a warning but still allows local execution
	VRAMWarningPercent float64 `yaml:"vram_warning_percent" json:"vram_warning_percent"`
	// VRAMBlockPercent blocks local execution and forces cloud
	VRAMBlockPercent float64 `yaml:"vram_block_percent" json:"vram_block_percent"`
	// RAMWarningPercent triggers a warning for RAM usage
	RAMWarningPercent float64 `yaml:"ram_warning_percent" json:"ram_warning_percent"`
}

// DefaultConfig returns sensible default configuration
func DefaultConfig() Config {
	return Config{
		VRAMWarningPercent: 80.0,
		VRAMBlockPercent:   95.0,
		RAMWarningPercent:  85.0,
	}
}

// ManifestProvider provides access to Capsule manifests
type ManifestProvider interface {
	GetManifest(name string) (*capsule.CapsuleManifest, error)
}

// Router decides whether to run Capsules locally or in the cloud
type Router struct {
	monitor hardware.HardwareMonitor
	store   ManifestProvider
	config  Config
}

// NewRouter creates a new Router instance
func NewRouter(monitor hardware.HardwareMonitor, store ManifestProvider, config Config) *Router {
	return &Router{
		monitor: monitor,
		store:   store,
		config:  config,
	}
}

// Decide determines where the Capsule should run
func (r *Router) Decide(capsuleName string) (*Decision, error) {
	// 1. Get capsule manifest
	manifest, err := r.store.GetManifest(capsuleName)
	if err != nil {
		return nil, fmt.Errorf("failed to get capsule manifest: %w", err)
	}

	// 2. Heavy capsules always go to cloud
	if manifest.Routing.Weight == capsule.WeightHeavy {
		cloudCapsule := manifest.Routing.CloudCapsule
		if cloudCapsule == "" {
			cloudCapsule = capsuleName
		}
		return &Decision{
			Route:       RouteCloud,
			CapsuleName: cloudCapsule,
			Reason:      "heavy capsule: always routes to cloud",
		}, nil
	}

	// 3. Get current system resources
	resources, err := r.monitor.GetCurrentResources()
	if err != nil {
		return nil, fmt.Errorf("failed to get system resources: %w", err)
	}

	// 4. Check VRAM block threshold (95% default)
	vramUsagePercent := resources.VRAMUsagePercent()
	if vramUsagePercent >= r.config.VRAMBlockPercent {
		if !manifest.Routing.FallbackToCloud {
			return nil, fmt.Errorf("VRAM usage %.1f%% exceeds block threshold %.1f%% and no cloud fallback configured",
				vramUsagePercent, r.config.VRAMBlockPercent)
		}
		cloudCapsule := manifest.Routing.CloudCapsule
		if cloudCapsule == "" {
			cloudCapsule = capsuleName
		}
		return &Decision{
			Route:       RouteCloud,
			CapsuleName: cloudCapsule,
			Reason:      fmt.Sprintf("VRAM usage %.1f%% exceeds block threshold %.1f%%", vramUsagePercent, r.config.VRAMBlockPercent),
		}, nil
	}

	// 5. Parse VRAM requirement
	vramRequired := int64(0)
	if manifest.Requirements.VRAMMin != "" {
		vramRequired, err = ParseSize(manifest.Requirements.VRAMMin)
		if err != nil {
			return nil, fmt.Errorf("failed to parse VRAM requirement: %w", err)
		}
	}

	// 6. Check if enough VRAM is available
	if vramRequired > resources.AvailableVRAM {
		if !manifest.Routing.FallbackToCloud {
			return nil, fmt.Errorf("insufficient VRAM: need %s, have %s available, and no cloud fallback configured",
				manifest.Requirements.VRAMMin, formatSize(resources.AvailableVRAM))
		}
		cloudCapsule := manifest.Routing.CloudCapsule
		if cloudCapsule == "" {
			cloudCapsule = capsuleName
		}
		return &Decision{
			Route:       RouteCloud,
			CapsuleName: cloudCapsule,
			Reason:      fmt.Sprintf("insufficient VRAM: need %s, have %s available", manifest.Requirements.VRAMMin, formatSize(resources.AvailableVRAM)),
		}, nil
	}

	// 7. Check for warning threshold (80% default)
	warning := false
	warningMsg := ""
	if vramUsagePercent >= r.config.VRAMWarningPercent {
		warning = true
		warningMsg = fmt.Sprintf("VRAM usage %.1f%% is above warning threshold %.1f%%", vramUsagePercent, r.config.VRAMWarningPercent)
	}

	// 8. Local execution
	return &Decision{
		Route:       RouteLocal,
		CapsuleName: capsuleName,
		Warning:     warning,
		WarningMsg:  warningMsg,
	}, nil
}

// DecideWithContext is like Decide but respects context cancellation
func (r *Router) DecideWithContext(ctx context.Context, capsuleName string) (*Decision, error) {
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	default:
		return r.Decide(capsuleName)
	}
}

// ParseSize parses a size string like "6GB" or "512MB" into bytes
func ParseSize(s string) (int64, error) {
	s = strings.TrimSpace(s)
	if s == "" {
		return 0, fmt.Errorf("empty size string")
	}

	// Regex to match number (with optional decimal) and unit
	re := regexp.MustCompile(`(?i)^(\d+(?:\.\d+)?)\s*(GB|MB|KB|B)?$`)
	matches := re.FindStringSubmatch(s)
	if matches == nil {
		return 0, fmt.Errorf("invalid size format: %s", s)
	}

	value, err := strconv.ParseFloat(matches[1], 64)
	if err != nil {
		return 0, fmt.Errorf("invalid number in size: %s", s)
	}

	if value < 0 {
		return 0, fmt.Errorf("negative size not allowed: %s", s)
	}

	unit := strings.ToUpper(matches[2])
	if unit == "" {
		unit = "B"
	}

	var multiplier float64
	switch unit {
	case "B":
		multiplier = 1
	case "KB":
		multiplier = 1024
	case "MB":
		multiplier = 1024 * 1024
	case "GB":
		multiplier = 1024 * 1024 * 1024
	default:
		return 0, fmt.Errorf("unknown size unit: %s", unit)
	}

	return int64(value * multiplier), nil
}

// formatSize formats bytes as a human-readable string
func formatSize(bytes int64) string {
	const (
		GB = 1024 * 1024 * 1024
		MB = 1024 * 1024
		KB = 1024
	)

	switch {
	case bytes >= GB:
		return fmt.Sprintf("%.1fGB", float64(bytes)/float64(GB))
	case bytes >= MB:
		return fmt.Sprintf("%.1fMB", float64(bytes)/float64(MB))
	case bytes >= KB:
		return fmt.Sprintf("%.1fKB", float64(bytes)/float64(KB))
	default:
		return fmt.Sprintf("%dB", bytes)
	}
}
