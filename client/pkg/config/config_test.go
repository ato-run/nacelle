package config

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestLoadConfig(t *testing.T) {
	// Create a temporary config file
	tempDir := t.TempDir()
	configPath := filepath.Join(tempDir, "config.yaml")

	configContent := `
coordinator:
  node_id: "test-node-1"
  address: "192.168.1.100:8080"
  headscale_name: "test-coordinator"

rqlite:
  addresses:
    - "http://localhost:4001"
    - "http://localhost:4002"
  max_retries: 5
  retry_delay: 3
  timeout: 15

cluster:
  gossip_bind_addr: "0.0.0.0:7946"
  peers:
    - "192.168.1.101:7946"
  heartbeat_interval: 10
  node_timeout: 60

headscale:
  api_url: "http://headscale:8080"
  api_key: "test-key"
  timeout: 20

api:
  listen_addr: "0.0.0.0:9090"
  tls_enabled: true
  tls_cert: "/path/to/cert.pem"
  tls_key: "/path/to/key.pem"

logging:
  level: "debug"
  format: "json"
`

	if err := os.WriteFile(configPath, []byte(configContent), 0644); err != nil {
		t.Fatalf("Failed to create test config file: %v", err)
	}

	// Load the config
	cfg, err := LoadConfig(configPath)
	if err != nil {
		t.Fatalf("Failed to load config: %v", err)
	}

	// Verify coordinator settings
	if cfg.Coordinator.NodeID != "test-node-1" {
		t.Errorf("Expected NodeID to be 'test-node-1', got '%s'", cfg.Coordinator.NodeID)
	}

	if cfg.Coordinator.Address != "192.168.1.100:8080" {
		t.Errorf("Expected Address to be '192.168.1.100:8080', got '%s'", cfg.Coordinator.Address)
	}

	if cfg.Coordinator.HeadscaleName != "test-coordinator" {
		t.Errorf("Expected HeadscaleName to be 'test-coordinator', got '%s'", cfg.Coordinator.HeadscaleName)
	}

	// Verify rqlite settings
	if len(cfg.RQLite.Addresses) != 2 {
		t.Errorf("Expected 2 rqlite addresses, got %d", len(cfg.RQLite.Addresses))
	}

	if cfg.RQLite.MaxRetries != 5 {
		t.Errorf("Expected MaxRetries to be 5, got %d", cfg.RQLite.MaxRetries)
	}

	if cfg.RQLite.RetryDelay != 3 {
		t.Errorf("Expected RetryDelay to be 3, got %d", cfg.RQLite.RetryDelay)
	}

	if cfg.RQLite.GetRetryDelay() != 3*time.Second {
		t.Errorf("Expected GetRetryDelay() to be 3s, got %v", cfg.RQLite.GetRetryDelay())
	}

	// Verify cluster settings
	if cfg.Cluster.HeartbeatInterval != 10 {
		t.Errorf("Expected HeartbeatInterval to be 10, got %d", cfg.Cluster.HeartbeatInterval)
	}

	if cfg.Cluster.GetHeartbeatInterval() != 10*time.Second {
		t.Errorf("Expected GetHeartbeatInterval() to be 10s, got %v", cfg.Cluster.GetHeartbeatInterval())
	}

	// Verify logging settings
	if cfg.Logging.Level != "debug" {
		t.Errorf("Expected Logging.Level to be 'debug', got '%s'", cfg.Logging.Level)
	}

	if cfg.Logging.Format != "json" {
		t.Errorf("Expected Logging.Format to be 'json', got '%s'", cfg.Logging.Format)
	}
}

func TestConfigValidation(t *testing.T) {
	tests := []struct {
		name        string
		config      Config
		expectError bool
	}{
		{
			name: "valid config",
			config: Config{
				Coordinator: CoordinatorConfig{
					HeadscaleName: "test-node",
				},
				RQLite: RQLiteConfig{
					Addresses: []string{"http://localhost:4001"},
				},
				Logging: LoggingConfig{
					Level: "info",
				},
			},
			expectError: false,
		},
		{
			name: "missing rqlite addresses",
			config: Config{
				Coordinator: CoordinatorConfig{
					HeadscaleName: "test-node",
				},
				RQLite: RQLiteConfig{
					Addresses: []string{},
				},
				Logging: LoggingConfig{
					Level: "info",
				},
			},
			expectError: true,
		},
		{
			name: "missing headscale name",
			config: Config{
				Coordinator: CoordinatorConfig{
					HeadscaleName: "",
				},
				RQLite: RQLiteConfig{
					Addresses: []string{"http://localhost:4001"},
				},
				Logging: LoggingConfig{
					Level: "info",
				},
			},
			expectError: true,
		},
		{
			name: "invalid logging level",
			config: Config{
				Coordinator: CoordinatorConfig{
					HeadscaleName: "test-node",
				},
				RQLite: RQLiteConfig{
					Addresses: []string{"http://localhost:4001"},
				},
				Logging: LoggingConfig{
					Level: "invalid-level",
				},
			},
			expectError: true,
		},
		{
			name: "TLS enabled but missing cert",
			config: Config{
				Coordinator: CoordinatorConfig{
					HeadscaleName: "test-node",
				},
				RQLite: RQLiteConfig{
					Addresses: []string{"http://localhost:4001"},
				},
				API: APIConfig{
					TLSEnabled: true,
					TLSCert:    "",
					TLSKey:     "",
				},
				Logging: LoggingConfig{
					Level: "info",
				},
			},
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tt.config.applyDefaults()
			err := tt.config.Validate()

			if tt.expectError && err == nil {
				t.Errorf("Expected validation error, but got none")
			}

			if !tt.expectError && err != nil {
				t.Errorf("Unexpected validation error: %v", err)
			}
		})
	}
}

func TestConfigDefaults(t *testing.T) {
	cfg := &Config{
		Coordinator: CoordinatorConfig{
			HeadscaleName: "test-node",
		},
		RQLite: RQLiteConfig{
			Addresses: []string{"http://localhost:4001"},
		},
	}

	cfg.applyDefaults()

	// Check RQLite defaults
	if cfg.RQLite.MaxRetries != 3 {
		t.Errorf("Expected default MaxRetries to be 3, got %d", cfg.RQLite.MaxRetries)
	}

	if cfg.RQLite.RetryDelay != 2 {
		t.Errorf("Expected default RetryDelay to be 2, got %d", cfg.RQLite.RetryDelay)
	}

	if cfg.RQLite.Timeout != 10 {
		t.Errorf("Expected default Timeout to be 10, got %d", cfg.RQLite.Timeout)
	}

	// Check Cluster defaults
	if cfg.Cluster.HeartbeatInterval != 5 {
		t.Errorf("Expected default HeartbeatInterval to be 5, got %d", cfg.Cluster.HeartbeatInterval)
	}

	if cfg.Cluster.NodeTimeout != 30 {
		t.Errorf("Expected default NodeTimeout to be 30, got %d", cfg.Cluster.NodeTimeout)
	}

	// Check Logging defaults
	if cfg.Logging.Level != "info" {
		t.Errorf("Expected default Logging.Level to be 'info', got '%s'", cfg.Logging.Level)
	}

	if cfg.Logging.Format != "text" {
		t.Errorf("Expected default Logging.Format to be 'text', got '%s'", cfg.Logging.Format)
	}
}
