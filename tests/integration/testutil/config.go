// +build integration

package testutil

import (
	"os"
	"strconv"
	"time"

	"github.com/onescluster/coordinator/pkg/db"
)

// IntegrationTestConfig holds common configuration values for integration tests.
// These values can be overridden via environment variables for CI/CD flexibility.
type IntegrationTestConfig struct {
	// MaxRetries is the maximum number of connection retry attempts
	MaxRetries int
	// RetryDelay is the delay between retry attempts
	RetryDelay time.Duration
	// Timeout is the timeout for individual requests
	Timeout time.Duration
}

// DefaultConfig provides default configuration values for integration tests.
// These values are tuned for local development and CI environments.
var DefaultConfig = IntegrationTestConfig{
	MaxRetries: 3,
	RetryDelay: 1 * time.Second,
	Timeout:    10 * time.Second,
}

// LoadConfig loads configuration from environment variables with fallback to defaults.
// Supported environment variables:
//   - TEST_MAX_RETRIES: Maximum retry attempts (default: 3)
//   - TEST_RETRY_DELAY: Retry delay in seconds (default: 1)
//   - TEST_TIMEOUT: Request timeout in seconds (default: 10)
func LoadConfig() IntegrationTestConfig {
	cfg := DefaultConfig

	if val := os.Getenv("TEST_MAX_RETRIES"); val != "" {
		if maxRetries, err := strconv.Atoi(val); err == nil && maxRetries > 0 {
			cfg.MaxRetries = maxRetries
		}
	}

	if val := os.Getenv("TEST_RETRY_DELAY"); val != "" {
		if retryDelay, err := strconv.Atoi(val); err == nil && retryDelay > 0 {
			cfg.RetryDelay = time.Duration(retryDelay) * time.Second
		}
	}

	if val := os.Getenv("TEST_TIMEOUT"); val != "" {
		if timeout, err := strconv.Atoi(val); err == nil && timeout > 0 {
			cfg.Timeout = time.Duration(timeout) * time.Second
		}
	}

	return cfg
}

// NewDBConfig creates a new db.Config using the shared test configuration.
// The addresses parameter should contain rqlite node addresses.
//
// Example:
//   cfg := testutil.NewDBConfig([]string{"http://localhost:4001"})
//   client, err := db.NewClient(cfg)
func NewDBConfig(addrs []string) *db.Config {
	cfg := LoadConfig()
	return &db.Config{
		Addresses:  addrs,
		MaxRetries: cfg.MaxRetries,
		RetryDelay: cfg.RetryDelay,
		Timeout:    cfg.Timeout,
	}
}

// NewDBConfigWithOverrides creates a db.Config with custom overrides.
// Use this when you need different values for specific test scenarios
// (e.g., shorter timeouts for connection checks).
//
// Example:
//   cfg := testutil.NewDBConfigWithOverrides([]string{"http://localhost:4001"}, 1, 100*time.Millisecond, 2*time.Second)
func NewDBConfigWithOverrides(addrs []string, maxRetries int, retryDelay, timeout time.Duration) *db.Config {
	return &db.Config{
		Addresses:  addrs,
		MaxRetries: maxRetries,
		RetryDelay: retryDelay,
		Timeout:    timeout,
	}
}
