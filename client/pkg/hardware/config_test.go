package hardware

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDefaultHardwareConfig(t *testing.T) {
	cfg := DefaultHardwareConfig()

	assert.Equal(t, 80.0, cfg.Thresholds.VRAMWarningPercent)
	assert.Equal(t, 95.0, cfg.Thresholds.VRAMBlockPercent)
	assert.Equal(t, 85.0, cfg.Thresholds.RAMWarningPercent)
	assert.Equal(t, 98.0, cfg.Thresholds.RAMBlockPercent)
	assert.True(t, cfg.Monitoring.Enabled)
	assert.Equal(t, 5*time.Second, cfg.Monitoring.Interval)
}

func TestLoadHardwareConfig_FileNotFound(t *testing.T) {
	cfg, err := LoadHardwareConfig("/nonexistent/path/config.yaml")
	require.NoError(t, err) // Should return defaults

	assert.Equal(t, 80.0, cfg.Thresholds.VRAMWarningPercent)
}

func TestLoadHardwareConfig_ValidFile(t *testing.T) {
	tmpDir := t.TempDir()
	configPath := filepath.Join(tmpDir, "config.yaml")

	content := `
hardware:
  thresholds:
    vram_warning_percent: 70.0
    vram_block_percent: 90.0
    ram_warning_percent: 75.0
    ram_block_percent: 95.0
  monitoring:
    enabled: true
    interval: 10s
`
	err := os.WriteFile(configPath, []byte(content), 0644)
	require.NoError(t, err)

	cfg, err := LoadHardwareConfig(configPath)
	require.NoError(t, err)

	assert.Equal(t, 70.0, cfg.Thresholds.VRAMWarningPercent)
	assert.Equal(t, 90.0, cfg.Thresholds.VRAMBlockPercent)
	assert.Equal(t, 75.0, cfg.Thresholds.RAMWarningPercent)
	assert.Equal(t, 95.0, cfg.Thresholds.RAMBlockPercent)
	assert.True(t, cfg.Monitoring.Enabled)
	assert.Equal(t, 10*time.Second, cfg.Monitoring.Interval)
}

func TestLoadHardwareConfig_PartialConfig(t *testing.T) {
	tmpDir := t.TempDir()
	configPath := filepath.Join(tmpDir, "config.yaml")

	// Only override VRAM warning
	content := `
hardware:
  thresholds:
    vram_warning_percent: 75.0
`
	err := os.WriteFile(configPath, []byte(content), 0644)
	require.NoError(t, err)

	cfg, err := LoadHardwareConfig(configPath)
	require.NoError(t, err)

	// Override should work
	assert.Equal(t, 75.0, cfg.Thresholds.VRAMWarningPercent)

	// Defaults should be preserved
	assert.Equal(t, 95.0, cfg.Thresholds.VRAMBlockPercent)
	assert.Equal(t, 85.0, cfg.Thresholds.RAMWarningPercent)
}

func TestLoadHardwareConfig_DisabledMonitoring(t *testing.T) {
	tmpDir := t.TempDir()
	configPath := filepath.Join(tmpDir, "config.yaml")

	content := `
hardware:
  monitoring:
    enabled: false
    interval: 30s
`
	err := os.WriteFile(configPath, []byte(content), 0644)
	require.NoError(t, err)

	cfg, err := LoadHardwareConfig(configPath)
	require.NoError(t, err)

	assert.False(t, cfg.Monitoring.Enabled)
	assert.Equal(t, 30*time.Second, cfg.Monitoring.Interval)
}

func TestHardwareConfig_ToResourceThresholds(t *testing.T) {
	cfg := HardwareConfig{
		Thresholds: ThresholdConfig{
			VRAMWarningPercent: 70.0,
			VRAMBlockPercent:   90.0,
			RAMWarningPercent:  75.0,
		},
		Monitoring: MonitorConfig{
			Interval: 10 * time.Second,
		},
	}

	thresholds := cfg.ToResourceThresholds()

	assert.Equal(t, 70.0, thresholds.VRAMWarningPercent)
	assert.Equal(t, 90.0, thresholds.VRAMBlockPercent)
	assert.Equal(t, 75.0, thresholds.RAMWarningPercent)
	assert.Equal(t, 10*time.Second, thresholds.MonitorInterval)
}
