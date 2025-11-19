package config

import (
	"fmt"
	"os"
	"time"

	"gopkg.in/yaml.v3"
)

// Config represents the complete configuration for the Coordinator
type Config struct {
	Coordinator CoordinatorConfig `yaml:"coordinator"`
	RQLite      RQLiteConfig      `yaml:"rqlite"`
	Cluster     ClusterConfig     `yaml:"cluster"`
	Headscale   HeadscaleConfig   `yaml:"headscale"`
	API         APIConfig         `yaml:"api"`
	Logging     LoggingConfig     `yaml:"logging"`
	TLS         TLSConfig         `yaml:"tls"`
}

// TLSConfig contains TLS settings for gRPC communication
type TLSConfig struct {
	Enabled    bool   `yaml:"enabled"`
	CACert     string `yaml:"ca_cert"`
	ClientCert string `yaml:"client_cert"`
	ClientKey  string `yaml:"client_key"`
}

// CoordinatorConfig contains coordinator-specific settings
type CoordinatorConfig struct {
	NodeID        string `yaml:"node_id"`
	Address       string `yaml:"address"`
	HeadscaleName string `yaml:"headscale_name"`
}

// RQLiteConfig contains rqlite connection settings
type RQLiteConfig struct {
	Addresses  []string `yaml:"addresses"`
	MaxRetries int      `yaml:"max_retries"`
	RetryDelay int      `yaml:"retry_delay"` // seconds
	Timeout    int      `yaml:"timeout"`     // seconds
}

// ClusterConfig contains clustering settings
type ClusterConfig struct {
	GossipBindAddr    string   `yaml:"gossip_bind_addr"`
	Peers             []string `yaml:"peers"`
	HeartbeatInterval int      `yaml:"heartbeat_interval"` // seconds
	NodeTimeout       int      `yaml:"node_timeout"`       // seconds
}

// HeadscaleConfig contains Headscale API settings
type HeadscaleConfig struct {
	APIURL  string `yaml:"api_url"`
	APIKey  string `yaml:"api_key"`
	Timeout int    `yaml:"timeout"` // seconds
}

// APIConfig contains HTTP API server settings
type APIConfig struct {
	ListenAddr string `yaml:"listen_addr"`
	TLSEnabled bool   `yaml:"tls_enabled"`
	TLSCert    string `yaml:"tls_cert"`
	TLSKey     string `yaml:"tls_key"`
}

// LoggingConfig contains logging settings
type LoggingConfig struct {
	Level  string `yaml:"level"`
	Format string `yaml:"format"`
}

// LoadConfig loads configuration from a YAML file
func LoadConfig(path string) (*Config, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("failed to read config file: %w", err)
	}

	var config Config
	if err := yaml.Unmarshal(data, &config); err != nil {
		return nil, fmt.Errorf("failed to parse config file: %w", err)
	}

	// Apply defaults
	config.applyDefaults()

	// Validate configuration
	if err := config.Validate(); err != nil {
		return nil, fmt.Errorf("invalid configuration: %w", err)
	}

	return &config, nil
}

// applyDefaults applies default values for missing configuration
func (c *Config) applyDefaults() {
	// RQLite defaults
	if c.RQLite.MaxRetries == 0 {
		c.RQLite.MaxRetries = 3
	}
	if c.RQLite.RetryDelay == 0 {
		c.RQLite.RetryDelay = 2
	}
	if c.RQLite.Timeout == 0 {
		c.RQLite.Timeout = 10
	}

	// Cluster defaults
	if c.Cluster.GossipBindAddr == "" {
		c.Cluster.GossipBindAddr = "0.0.0.0:7946"
	}
	if c.Cluster.HeartbeatInterval == 0 {
		c.Cluster.HeartbeatInterval = 5
	}
	if c.Cluster.NodeTimeout == 0 {
		c.Cluster.NodeTimeout = 30
	}

	// Headscale defaults
	if c.Headscale.Timeout == 0 {
		c.Headscale.Timeout = 10
	}

	// API defaults
	if c.API.ListenAddr == "" {
		c.API.ListenAddr = "0.0.0.0:8080"
	}

	// Logging defaults
	if c.Logging.Level == "" {
		c.Logging.Level = "info"
	}
	if c.Logging.Format == "" {
		c.Logging.Format = "text"
	}

	// Coordinator defaults
	if c.Coordinator.Address == "" {
		c.Coordinator.Address = "0.0.0.0:8080"
	}
}

// Validate validates the configuration
func (c *Config) Validate() error {
	// Validate rqlite addresses
	if len(c.RQLite.Addresses) == 0 {
		return fmt.Errorf("at least one rqlite address is required")
	}

	// Validate coordinator settings
	if c.Coordinator.HeadscaleName == "" {
		return fmt.Errorf("coordinator.headscale_name is required")
	}

	// Validate logging level
	validLevels := map[string]bool{
		"debug": true,
		"info":  true,
		"warn":  true,
		"error": true,
	}
	if !validLevels[c.Logging.Level] {
		return fmt.Errorf("invalid logging level: %s", c.Logging.Level)
	}

	// Validate TLS configuration
	if c.API.TLSEnabled {
		if c.API.TLSCert == "" || c.API.TLSKey == "" {
			return fmt.Errorf("TLS cert and key are required when TLS is enabled")
		}
	}

	return nil
}

// GetRetryDelay returns the retry delay as a time.Duration
func (r *RQLiteConfig) GetRetryDelay() time.Duration {
	return time.Duration(r.RetryDelay) * time.Second
}

// GetTimeout returns the timeout as a time.Duration
func (r *RQLiteConfig) GetTimeout() time.Duration {
	return time.Duration(r.Timeout) * time.Second
}

// GetHeartbeatInterval returns the heartbeat interval as a time.Duration
func (c *ClusterConfig) GetHeartbeatInterval() time.Duration {
	return time.Duration(c.HeartbeatInterval) * time.Second
}

// GetNodeTimeout returns the node timeout as a time.Duration
func (c *ClusterConfig) GetNodeTimeout() time.Duration {
	return time.Duration(c.NodeTimeout) * time.Second
}

// GetTimeout returns the timeout as a time.Duration
func (h *HeadscaleConfig) GetTimeout() time.Duration {
	return time.Duration(h.Timeout) * time.Second
}
