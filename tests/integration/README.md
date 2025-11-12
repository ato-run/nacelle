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
# Start rqlite cluster for testing
docker run -d --name rqlite-test -p 4001:4001 rqlite/rqlite

# Run integration tests
go test -v ./tests/integration/...

# Cleanup
docker stop rqlite-test && docker rm rqlite-test
```

### Environment Variables

- `RQLITE_ADDR`: rqlite address (default: http://localhost:4001)
- `HEADSCALE_MOCK`: Use mock headscale server (default: true)
- `AGENT_ADDR`: Agent gRPC address for testing
- `SKIP_SLOW_TESTS`: Skip slow integration tests

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
