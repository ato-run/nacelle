//go:build darwin
// +build darwin

package hardware

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDarwinMonitor_GetCurrentResources(t *testing.T) {
	monitor := NewDarwinMonitor(DefaultThresholds())

	resources, err := monitor.GetCurrentResources()
	require.NoError(t, err)
	require.NotNil(t, resources)

	// Sanity checks
	assert.True(t, resources.TotalRAM > 0, "TotalRAM should be positive")
	assert.True(t, resources.AvailableRAM > 0, "AvailableRAM should be positive")
	assert.True(t, resources.AvailableRAM <= resources.TotalRAM, "Available should be <= Total")

	// VRAM checks (may be same as RAM on Apple Silicon)
	assert.True(t, resources.TotalVRAM > 0, "TotalVRAM should be positive")
}

func TestDarwinMonitor_GetSystemSummary(t *testing.T) {
	monitor := NewDarwinMonitor(DefaultThresholds())

	summary, err := monitor.GetSystemSummary()
	require.NoError(t, err)

	assert.Contains(t, summary, "RAM:")
	assert.Contains(t, summary, "VRAM:")
	assert.Contains(t, summary, "CPU:")

	t.Logf("System Summary:\n%s", summary)
}

func TestDarwinMonitor_CanRunCapsule(t *testing.T) {
	monitor := NewDarwinMonitor(DefaultThresholds())

	// Test with small VRAM requirement (should pass)
	result, err := monitor.CanRunCapsule(1 * 1024 * 1024 * 1024) // 1GB
	require.NoError(t, err)
	assert.True(t, result.CanRun, "Should be able to run with 1GB VRAM requirement")

	// Test with unrealistic VRAM requirement (should fail)
	result, err = monitor.CanRunCapsule(1000 * 1024 * 1024 * 1024) // 1000GB
	require.NoError(t, err)
	assert.False(t, result.CanRun, "Should NOT be able to run with 1000GB VRAM requirement")
	assert.Contains(t, result.Reason, "Insufficient VRAM")
}

func TestDarwinMonitor_Watch(t *testing.T) {
	monitor := NewDarwinMonitor(DefaultThresholds())

	ctx, cancel := context.WithTimeout(context.Background(), 500*time.Millisecond)
	defer cancel()

	callCount := 0
	callback := func(r *SystemResources) {
		callCount++
		assert.True(t, r.TotalRAM > 0)
	}

	monitor.Watch(ctx, 100*time.Millisecond, callback)

	// Should have been called multiple times
	assert.True(t, callCount >= 1, "Callback should have been called at least once")
}

func TestSystemResources_UsagePercent(t *testing.T) {
	resources := &SystemResources{
		TotalVRAM:     24 * 1024 * 1024 * 1024, // 24GB
		AvailableVRAM: 6 * 1024 * 1024 * 1024,  // 6GB available
		TotalRAM:      32 * 1024 * 1024 * 1024, // 32GB
		AvailableRAM:  8 * 1024 * 1024 * 1024,  // 8GB available
	}

	assert.InDelta(t, 75.0, resources.VRAMUsagePercent(), 0.01)
	assert.InDelta(t, 75.0, resources.RAMUsagePercent(), 0.01)
}

func TestResourceThresholds_Default(t *testing.T) {
	thresholds := DefaultThresholds()

	assert.Equal(t, 80.0, thresholds.VRAMWarningPercent)
	assert.Equal(t, 95.0, thresholds.VRAMBlockPercent)
	assert.Equal(t, 85.0, thresholds.RAMWarningPercent)
	assert.Equal(t, 5*time.Second, thresholds.MonitorInterval)
}

func TestResourceCheckResult_Warnings(t *testing.T) {
	// Test with high VRAM usage threshold
	thresholds := ResourceThresholds{
		VRAMWarningPercent: 10.0, // Very low warning threshold
		VRAMBlockPercent:   95.0,
		RAMWarningPercent:  10.0, // Very low warning threshold
	}
	monitor := NewDarwinMonitor(thresholds)

	result, err := monitor.CanRunCapsule(0)
	require.NoError(t, err)

	// With such low thresholds, we should likely see warnings
	// (unless the system is nearly idle)
	t.Logf("CanRun: %v, VRAMWarning: %v, RAMWarning: %v, Reason: %s",
		result.CanRun, result.VRAMWarning, result.RAMWarning, result.Reason)
}
