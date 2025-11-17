# Integration Test Utilities

This package provides shared utilities and configuration for integration tests.

## Configuration

### Shared Test Configuration

The `testutil` package provides centralized configuration for integration tests to eliminate code duplication and improve maintainability.

#### Default Values

```go
MaxRetries: 3              // Maximum retry attempts for database connections
RetryDelay: 1 * time.Second  // Delay between retry attempts
Timeout:    10 * time.Second // Request timeout
```

#### Usage

**Basic usage with defaults:**
```go
import "github.com/onescluster/coordinator/tests/integration/testutil"

cfg := testutil.NewDBConfig([]string{"http://localhost:4001"})
client, err := db.NewClient(cfg)
```

**Using with custom overrides (e.g., for connection checks):**
```go
cfg := testutil.NewDBConfigWithOverrides(
    []string{getRQLiteAddr()},
    1,                      // maxRetries
    100*time.Millisecond,  // retryDelay
    2*time.Second,         // timeout
)
```

#### Environment Variable Overrides

Configuration values can be overridden via environment variables:

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `TEST_MAX_RETRIES` | Maximum retry attempts | 3 |
| `TEST_RETRY_DELAY` | Retry delay in seconds | 1 |
| `TEST_TIMEOUT` | Request timeout in seconds | 10 |

**Example:**
```bash
# Run integration tests with custom timeouts
TEST_MAX_RETRIES=5 TEST_TIMEOUT=20 go test -tags=integration ./tests/integration/...
```

#### When to Use Which Function

- **`NewDBConfig(addrs)`**: Use this for standard integration test scenarios. It respects environment variable overrides and provides sensible defaults.

- **`NewDBConfigWithOverrides(addrs, maxRetries, retryDelay, timeout)`**: Use this when you need specific values for a particular test scenario, such as:
  - Connection availability checks (shorter timeouts)
  - Stress tests (higher retry counts)
  - Specific timing requirements

### Design Principles

The configuration design follows these principles:

1. **DRY (Don't Repeat Yourself)**: Configuration values are defined once and reused across all tests.

2. **KISS (Keep It Simple, Stupid)**: Simple factory functions instead of complex builders.

3. **Flexibility**: Environment variable overrides for CI/CD and local development variations.

4. **Backward Compatibility**: Maintains the same `db.Config` structure used throughout the codebase.

### Migration Example

**Before (repeated configuration):**
```go
cfg := &db.Config{
    Addresses:  []string{getRQLiteAddr()},
    MaxRetries: 3,
    RetryDelay: 1 * time.Second,
    Timeout:    10 * time.Second,
}
```

**After (using shared config):**
```go
cfg := testutil.NewDBConfig([]string{getRQLiteAddr()})
```

### Testing

The configuration utilities include comprehensive tests:

```bash
cd tests/integration/testutil
go test -v -tags=integration
```

Test coverage includes:
- Default value validation
- Environment variable overrides
- Invalid environment variable handling
- Custom override functionality
- Environment variable precedence rules
