// +build integration

package testutil

import (
	"os"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDefaultConfig(t *testing.T) {
	// Verify default configuration values
	assert.Equal(t, 3, DefaultConfig.MaxRetries, "Default MaxRetries should be 3")
	assert.Equal(t, 1*time.Second, DefaultConfig.RetryDelay, "Default RetryDelay should be 1 second")
	assert.Equal(t, 10*time.Second, DefaultConfig.Timeout, "Default Timeout should be 10 seconds")
}

func TestLoadConfig_Defaults(t *testing.T) {
	// Clear any environment variables that might interfere
	os.Unsetenv("TEST_MAX_RETRIES")
	os.Unsetenv("TEST_RETRY_DELAY")
	os.Unsetenv("TEST_TIMEOUT")

	cfg := LoadConfig()
	
	assert.Equal(t, 3, cfg.MaxRetries, "LoadConfig should return default MaxRetries")
	assert.Equal(t, 1*time.Second, cfg.RetryDelay, "LoadConfig should return default RetryDelay")
	assert.Equal(t, 10*time.Second, cfg.Timeout, "LoadConfig should return default Timeout")
}

func TestLoadConfig_EnvironmentOverrides(t *testing.T) {
	// Set environment variables
	os.Setenv("TEST_MAX_RETRIES", "5")
	os.Setenv("TEST_RETRY_DELAY", "2")
	os.Setenv("TEST_TIMEOUT", "20")
	defer func() {
		os.Unsetenv("TEST_MAX_RETRIES")
		os.Unsetenv("TEST_RETRY_DELAY")
		os.Unsetenv("TEST_TIMEOUT")
	}()

	cfg := LoadConfig()
	
	assert.Equal(t, 5, cfg.MaxRetries, "MaxRetries should be overridden by environment variable")
	assert.Equal(t, 2*time.Second, cfg.RetryDelay, "RetryDelay should be overridden by environment variable")
	assert.Equal(t, 20*time.Second, cfg.Timeout, "Timeout should be overridden by environment variable")
}

func TestLoadConfig_InvalidEnvironmentVariables(t *testing.T) {
	// Set invalid environment variables
	os.Setenv("TEST_MAX_RETRIES", "invalid")
	os.Setenv("TEST_RETRY_DELAY", "-1")
	os.Setenv("TEST_TIMEOUT", "not_a_number")
	defer func() {
		os.Unsetenv("TEST_MAX_RETRIES")
		os.Unsetenv("TEST_RETRY_DELAY")
		os.Unsetenv("TEST_TIMEOUT")
	}()

	cfg := LoadConfig()
	
	// Should fall back to defaults when invalid values are provided
	assert.Equal(t, 3, cfg.MaxRetries, "Should use default MaxRetries when env var is invalid")
	assert.Equal(t, 1*time.Second, cfg.RetryDelay, "Should use default RetryDelay when env var is invalid")
	assert.Equal(t, 10*time.Second, cfg.Timeout, "Should use default Timeout when env var is invalid")
}

func TestNewDBConfig(t *testing.T) {
	// Clear environment variables for clean test
	os.Unsetenv("TEST_MAX_RETRIES")
	os.Unsetenv("TEST_RETRY_DELAY")
	os.Unsetenv("TEST_TIMEOUT")

	addrs := []string{"http://localhost:4001", "http://localhost:4002"}
	cfg := NewDBConfig(addrs)

	require.NotNil(t, cfg, "NewDBConfig should return non-nil config")
	assert.Equal(t, addrs, cfg.Addresses, "Addresses should match input")
	assert.Equal(t, 3, cfg.MaxRetries, "MaxRetries should be default value")
	assert.Equal(t, 1*time.Second, cfg.RetryDelay, "RetryDelay should be default value")
	assert.Equal(t, 10*time.Second, cfg.Timeout, "Timeout should be default value")
}

func TestNewDBConfig_WithEnvironmentOverrides(t *testing.T) {
	// Set environment variables
	os.Setenv("TEST_MAX_RETRIES", "7")
	os.Setenv("TEST_RETRY_DELAY", "3")
	os.Setenv("TEST_TIMEOUT", "15")
	defer func() {
		os.Unsetenv("TEST_MAX_RETRIES")
		os.Unsetenv("TEST_RETRY_DELAY")
		os.Unsetenv("TEST_TIMEOUT")
	}()

	addrs := []string{"http://localhost:4001"}
	cfg := NewDBConfig(addrs)

	require.NotNil(t, cfg, "NewDBConfig should return non-nil config")
	assert.Equal(t, addrs, cfg.Addresses, "Addresses should match input")
	assert.Equal(t, 7, cfg.MaxRetries, "MaxRetries should be overridden from env")
	assert.Equal(t, 3*time.Second, cfg.RetryDelay, "RetryDelay should be overridden from env")
	assert.Equal(t, 15*time.Second, cfg.Timeout, "Timeout should be overridden from env")
}

func TestNewDBConfigWithOverrides(t *testing.T) {
	addrs := []string{"http://localhost:4001"}
	cfg := NewDBConfigWithOverrides(addrs, 1, 100*time.Millisecond, 2*time.Second)

	require.NotNil(t, cfg, "NewDBConfigWithOverrides should return non-nil config")
	assert.Equal(t, addrs, cfg.Addresses, "Addresses should match input")
	assert.Equal(t, 1, cfg.MaxRetries, "MaxRetries should match custom value")
	assert.Equal(t, 100*time.Millisecond, cfg.RetryDelay, "RetryDelay should match custom value")
	assert.Equal(t, 2*time.Second, cfg.Timeout, "Timeout should match custom value")
}

func TestNewDBConfigWithOverrides_IgnoresEnvironment(t *testing.T) {
	// Set environment variables - these should be ignored when using overrides
	os.Setenv("TEST_MAX_RETRIES", "10")
	os.Setenv("TEST_RETRY_DELAY", "5")
	os.Setenv("TEST_TIMEOUT", "30")
	defer func() {
		os.Unsetenv("TEST_MAX_RETRIES")
		os.Unsetenv("TEST_RETRY_DELAY")
		os.Unsetenv("TEST_TIMEOUT")
	}()

	addrs := []string{"http://localhost:4001"}
	cfg := NewDBConfigWithOverrides(addrs, 1, 100*time.Millisecond, 2*time.Second)

	// Override values should take precedence over environment variables
	assert.Equal(t, 1, cfg.MaxRetries, "MaxRetries should use override, not env var")
	assert.Equal(t, 100*time.Millisecond, cfg.RetryDelay, "RetryDelay should use override, not env var")
	assert.Equal(t, 2*time.Second, cfg.Timeout, "Timeout should use override, not env var")
}
