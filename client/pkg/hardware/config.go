package hardware

import (
"os"
"time"

"gopkg.in/yaml.v3"
)

// HardwareConfig represents the hardware configuration section of config.yaml
type HardwareConfig struct {
	Thresholds ThresholdConfig `yaml:"thresholds"`
	Monitoring MonitorConfig   `yaml:"monitoring"`
}

// ThresholdConfig represents threshold settings
type ThresholdConfig struct {
	VRAMWarningPercent float64 `yaml:"vram_warning_percent"`
	VRAMBlockPercent   float64 `yaml:"vram_block_percent"`
	RAMWarningPercent  float64 `yaml:"ram_warning_percent"`
	RAMBlockPercent    float64 `yaml:"ram_block_percent"`
}

// MonitorConfig represents monitoring settings
type MonitorConfig struct {
	Enabled  bool          `yaml:"enabled"`
	Interval time.Duration `yaml:"interval"`
}

// DefaultHardwareConfig returns sensible defaults
func DefaultHardwareConfig() HardwareConfig {
	return HardwareConfig{
		Thresholds: ThresholdConfig{
			VRAMWarningPercent: 80.0,
			VRAMBlockPercent:   95.0,
			RAMWarningPercent:  85.0,
			RAMBlockPercent:    98.0,
		},
		Monitoring: MonitorConfig{
			Enabled:  true,
			Interval: 5 * time.Second,
		},
	}
}

// LoadHardwareConfig loads hardware configuration from a YAML file
func LoadHardwareConfig(path string) (*HardwareConfig, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			cfg := DefaultHardwareConfig()
			return &cfg, nil
		}
		return nil, err
	}

	// Parse outer config structure
	var outer struct {
		Hardware HardwareConfig `yaml:"hardware"`
	}
	
	if err := yaml.Unmarshal(data, &outer); err != nil {
		return nil, err
	}

	// Merge with defaults
	cfg := DefaultHardwareConfig()
	
	if outer.Hardware.Thresholds.VRAMWarningPercent > 0 {
		cfg.Thresholds.VRAMWarningPercent = outer.Hardware.Thresholds.VRAMWarningPercent
	}
	if outer.Hardware.Thresholds.VRAMBlockPercent > 0 {
		cfg.Thresholds.VRAMBlockPercent = outer.Hardware.Thresholds.VRAMBlockPercent
	}
	if outer.Hardware.Thresholds.RAMWarningPercent > 0 {
		cfg.Thresholds.RAMWarningPercent = outer.Hardware.Thresholds.RAMWarningPercent
	}
	if outer.Hardware.Thresholds.RAMBlockPercent > 0 {
		cfg.Thresholds.RAMBlockPercent = outer.Hardware.Thresholds.RAMBlockPercent
	}
	if outer.Hardware.Monitoring.Interval > 0 {
		cfg.Monitoring.Interval = outer.Hardware.Monitoring.Interval
	}
	// Explicit false check for enabled
	cfg.Monitoring.Enabled = outer.Hardware.Monitoring.Enabled

	return &cfg, nil
}

// ToResourceThresholds converts config to ResourceThresholds
func (c *HardwareConfig) ToResourceThresholds() ResourceThresholds {
	return ResourceThresholds{
		VRAMWarningPercent: c.Thresholds.VRAMWarningPercent,
		VRAMBlockPercent:   c.Thresholds.VRAMBlockPercent,
		RAMWarningPercent:  c.Thresholds.RAMWarningPercent,
		MonitorInterval:    c.Monitoring.Interval,
	}
}
