# Integration Tests

This directory contains integration tests for the One'sCluster (Capsuled) system.

## Test Scenarios

### Coordinator Tests (`coordinator_test.go`)

- Master election with multiple nodes
- Failover when master dies
- State consistency across nodes
- rqlite reconnection handling
- Headscale API integration

### Agent-Coordinator Tests (`agent_coordinator_test.go`)

- Capsule deployment end-to-end
- Wasm manifest validation flow
- Resource allocation and tracking
- gRPC communication

### Mock Services (`mocks/`)

- Mock headscale API server
- Mock Caddy Admin API server

## Running Tests

### Prerequisites

- Docker or Podman for running rqlite
- Go 1.21+
- Rust toolchain (for Agent tests)

### Setup

```bash
# From capsuled/ bring up only rqlite via Docker Compose
docker compose up -d rqlite

# Run integration tests (defaults to http://localhost:4001)
go test -v ./tests/integration/...

# Cleanup
docker compose down
```

### Environment Variables

#### General Test Configuration

- `RQLITE_ADDR`: rqlite address (default: http://localhost:4001)
- `HEADSCALE_MOCK`: Use mock headscale server (default: true)
- `AGENT_ADDR`: Agent gRPC address for testing
- `SKIP_SLOW_TESTS`: Skip slow integration tests

#### Test Configuration (via testutil)

The `testutil` package provides shared configuration with environment variable overrides:

- `TEST_MAX_RETRIES`: Maximum retry attempts for database connections (default: 3)
- `TEST_RETRY_DELAY`: Retry delay in seconds (default: 1)
- `TEST_TIMEOUT`: Request timeout in seconds (default: 10)

Example:

```bash
# Run with custom timeout settings
TEST_TIMEOUT=20 TEST_MAX_RETRIES=5 go test -v -tags=integration ./tests/integration/...
```

See [testutil/README.md](testutil/README.md) for more details on test configuration.

## Test Coverage

Integration tests focus on:

- Cross-component communication
- State management consistency
- Failure scenarios and recovery
- External API integration
- End-to-end workflows

Unit tests (in `pkg/*/` packages) focus on:

- Individual function behavior
- Error handling
- Edge cases
- Mock-based isolation
