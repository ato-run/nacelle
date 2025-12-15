# Testing Guide

This document describes the testing strategy and infrastructure for the Capsuled project.

## Overview

Capsuled uses a comprehensive testing approach with three levels:

1. **Unit Tests**: Test individual functions and modules in isolation
2. **Integration Tests**: Test interactions between components
3. **E2E Tests**: Test complete workflows across the entire system

Both Go and Rust components have their own test suites, plus cross-language integration tests.

## Test Structure

```
capsuled/
├── client/                    # Go Coordinator/Client
│   ├── pkg/                  # Unit tests (*_test.go)
│   │   ├── api/
│   │   ├── db/
│   │   ├── grpc/
│   │   ├── master/
│   │   ├── reconcile/
│   │   ├── scheduler/
│   │   └── ...
│   └── e2e/                  # Go E2E tests
├── engine/                    # Rust Agent/Engine
│   ├── src/                  # Unit tests (mod tests)
│   │   ├── storage/
│   │   ├── adep/
│   │   ├── logs/
│   │   └── ...
│   └── tests/                # Rust integration tests
│       └── storage_integration.rs
├── adep-logic/               # Rust Wasm logic
│   └── src/                  # Unit tests
└── tests/                    # Cross-language tests
    ├── integration/          # Go/Rust integration tests
    └── e2e/                  # End-to-end tests
```

## Running Tests

### Quick Start

```bash
# Run all unit tests (Go + Rust)
make test

# Run all tests including integration and E2E
make test-all

# Generate coverage reports
make test-coverage
```

### Go Tests

```bash
# Unit tests only
make test-go-unit

# Integration tests (requires rqlite)
make test-go-integration

# E2E tests
make test-go-e2e

# All Go tests with coverage
make test-go-coverage
```

**Manual Go test commands:**

```bash
cd client

# Run all tests
go test ./...

# Run tests with coverage
go test -cover ./pkg/...

# Run tests with race detector
go test -race ./pkg/...

# Run specific package
go test -v ./pkg/master

# Run specific test
go test -v ./pkg/master -run TestElectMaster
```

### Rust Tests

```bash
# Unit tests only
make test-rust-unit

# Integration tests (requires root for storage tests)
make test-rust-integration

# All Rust tests with coverage
make test-rust-coverage
```

**Manual Rust test commands:**

````bash
cd engine

# Run all tests
cargo test

# Run unit tests only (no integration tests)
cargo test --lib

# Run specific test
cargo test test_is_valid_volume_name

## Proto regeneration guard

Generated gRPC/Proto files must stay in sync:

```bash
cd capsuled
make proto
git diff --exit-code  # should be clean; if not, commit regenerated files
````

Add the above to CI when possible so drift is caught automatically.

# Run with output visible

cargo test -- --nocapture

# Run integration tests (requires root/LVM setup)

cargo test --test storage_integration -- --ignored

````

## Test Network Topology

To improve test maintainability and consistency, test files use named constants for network addresses instead of hardcoded IPs.

### Standard Test Network Configuration

**Integration and E2E tests** use the following standard addresses:

```go
const (
    // Local addresses for coordinator/server components
    testLocalAddr    = "127.0.0.1:8080"     // Primary local coordinator
    testLocalAddrAlt = "192.168.1.100:8080" // Alternative for multi-node tests

    // Distributed node addresses for GPU scheduling tests
    testNodeAddr1    = "192.168.1.10:50051" // Node 1 (various VRAM configs)
    testNodeAddr2    = "192.168.1.20:50051" // Node 2 (various VRAM configs)
    testNodeAddr3    = "192.168.1.30:50051" // Node 3 (various VRAM configs)
)
````

### Test Network Segments

- **127.0.0.1**: Local loopback for coordinator and server components
- **192.168.1.0/24**: Simulated distributed cluster network for:
  - GPU-enabled Rigs (10-19 range)
  - Standard compute nodes (20-99 range)
  - Test coordinators (100+ range)

### E2E Test Configurations

**GPU Simulation Tests** (`client/e2e/gpu_simulation_test.go`):

- Uses dynamic port allocation (`:0`) for local testing
- Simulates 3 GPU rigs with different VRAM configurations:
  - Rig A (192.168.1.10): 2x RTX 4090 GPUs
  - Rig B (192.168.1.11): 1x RTX 4090 GPU
  - Rig C (192.168.1.12): 4x A100 GPUs

**Integration Tests** (`tests/integration/`):

- Uses standard node addresses for distributed scheduling tests
- Tests coordinator clustering and master election scenarios
- Simulates varying VRAM capacities and utilization patterns

### Modifying Test Networks

When adding new tests:

1. Use existing constants when possible for consistency
2. Add new constants at the package level if needed
3. Document the purpose of each address in comments
4. Follow the network segmentation scheme above

Example:

```go
const (
    testNodeAddr4 = "192.168.1.40:50051" // Node 4: High-memory node
)
```

## Test Categories

### Unit Tests

**Go Unit Tests:**

- Located in `client/pkg/` alongside source files
- Named `*_test.go`
- Use `testing` package and `testify` for assertions
- Mock external dependencies (database, gRPC, etc.)

Example:

```go
func TestElectMaster(t *testing.T) {
    tests := []struct {
        name           string
        aliveNodes     []string
        expectError    bool
        expectedMaster string
    }{
        {
            name: "simple election with 3 nodes",
            aliveNodes: []string{"node1", "node2", "node3"},
            expectError: false,
            expectedMaster: "node1",
        },
    }

    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            // Test implementation
        })
    }
}
```

**Rust Unit Tests:**

- Located in the same file as the code being tested
- Inside `#[cfg(test)] mod tests { ... }`
- Use Rust's built-in test framework

Example:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_volume_name() {
        assert!(LvmManager::is_valid_volume_name("my_volume"));
        assert!(!LvmManager::is_valid_volume_name(""));
    }
}
```

### Integration Tests

**Go Integration Tests:**

- Located in `tests/integration/`
- Require external services (rqlite, etc.)
- Use build tag: `// +build integration`

Setup rqlite for integration tests:

```bash
# Using Docker
docker run -d -p 4001:4001 rqlite/rqlite

# Run tests
make test-go-integration
```

**Rust Integration Tests:**

- Located in `engine/tests/`
- Storage tests require root privileges and LVM setup
- Marked with `#[ignore]` to prevent accidental execution

Setup for storage integration tests:

```bash
# Create test volume group
sudo truncate -s 1G /tmp/test_vg.img
sudo losetup -f /tmp/test_vg.img
sudo pvcreate /dev/loop0
sudo vgcreate test_vg /dev/loop0

# Run tests
sudo -E cargo test --test storage_integration -- --ignored

# Cleanup
sudo vgremove -f test_vg
sudo pvremove /dev/loop0
sudo losetup -d /dev/loop0
sudo rm /tmp/test_vg.img
```

### E2E Tests

End-to-end tests verify complete workflows:

- **Go E2E**: Located in `client/e2e/` and `tests/e2e/`
- Test coordinator-agent communication
- Test GPU scheduling and VRAM management
- Require building Rust components

Example E2E test flow:

1. Start gRPC coordinator server
2. Build and run Rust status reporter
3. Verify data flows from Rust → gRPC → Go database
4. Assert correct resource tracking

```bash
# Run E2E tests
make test-go-e2e
```

## Test Coverage

### Go Coverage

```bash
# Generate coverage report
make test-go-coverage

# View in terminal
go tool cover -func=coverage-go.out

# View in browser
go tool cover -html=coverage-go.out
```

**Coverage targets:**

- Core logic: ≥80%
- Public APIs: 100%
- Error paths: Must be tested

### Rust Coverage

```bash
# Generate coverage report (requires cargo-tarpaulin)
make test-rust-coverage

# View in browser
open coverage-rust/index.html
```

Install cargo-tarpaulin:

```bash
cargo install cargo-tarpaulin
```

## Continuous Integration

Tests run automatically in GitHub Actions on:

- Every push to `main` or `develop`
- Every pull request
- Tag pushes (releases)

See `.github/workflows/ci.yml` for CI configuration.

### CI Test Matrix

1. **Build adep-logic**: Wasm build + tests
2. **Build engine**: Rust build + unit tests
3. **Build client**: Go build (CGO_ENABLED=0 and CGO_ENABLED=1)
4. **Test client**: Go unit tests
5. **Integration test**: Full build + verification

## Writing Tests

### Test-Driven Development (TDD)

Follow the Red-Green-Refactor cycle:

1. **Red**: Write a failing test

```go
func TestNewFeature(t *testing.T) {
    result := NewFeature()
    assert.Equal(t, "expected", result)
}
```

2. **Green**: Write minimal code to pass

```go
func NewFeature() string {
    return "expected"
}
```

3. **Refactor**: Improve without breaking tests

### Best Practices

**Go Tests:**

- Use table-driven tests for multiple scenarios
- Use `testify/require` for critical assertions (stop on failure)
- Use `testify/assert` for non-critical assertions (continue on failure)
- Mock external dependencies (database, APIs, etc.)
- Use `t.Parallel()` for tests that can run in parallel
- Use `t.TempDir()` for temporary files/directories

**Rust Tests:**

- One test per logical scenario
- Use descriptive test names: `test_valid_volume_name_accepts_alphanumeric`
- Use `#[should_panic]` for tests expecting panics
- Use `Result<()>` for tests that can fail with errors
- Mark integration tests with `#[ignore]` if they require special setup

### Common Patterns

**Mocking in Go:**

```go
type mockStateManager struct {
    masterID string
    setMasterErr error
}

func (m *mockStateManager) SetMaster(masterID string) error {
    if m.setMasterErr != nil {
        return m.setMasterErr
    }
    m.masterID = masterID
    return nil
}
```

**Rust Test Helpers:**

```rust
fn is_root() -> bool {
    std::env::var("USER").unwrap_or_default() == "root"
        || std::env::var("SUDO_USER").is_ok()
}

#[test]
#[ignore]
fn test_requires_root() {
    if !is_root() {
        eprintln!("Skipping test: requires root privileges");
        return;
    }
    // Test implementation
}
```

## Troubleshooting

### Common Issues

**Go tests fail with "no such file or directory":**

- Make sure you're running from the correct directory
- Use `cd client` before running Go tests

**Rust tests fail with "no LVM tools":**

- Storage integration tests require LVM tools installed
- Run `sudo apt-get install lvm2` on Ubuntu/Debian
- These tests are marked `#[ignore]` by default

**Integration tests timeout:**

- Check that required services (rqlite, etc.) are running
- Increase timeout values in test code if needed

**E2E tests fail to build Rust components:**

- Ensure Rust toolchain is installed
- Run `make engine` first to verify Rust builds correctly

### Debug Mode

Run tests with verbose output:

```bash
# Go
go test -v ./pkg/...

# Rust
cargo test -- --nocapture
```

Enable race detector (Go):

```bash
go test -race ./pkg/...
```

## Contributing

When adding new features:

1. Write tests first (TDD)
2. Ensure tests pass locally
3. Run `make test-all` before submitting PR
4. Aim for ≥80% coverage on new code
5. Update this document if adding new test categories

## Resources

- [Go Testing Package](https://pkg.go.dev/testing)
- [Testify Documentation](https://github.com/stretchr/testify)
- [Rust Testing](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Cargo Test Documentation](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
