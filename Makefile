.PHONY: all proto wasm client cli engine test clean \
	test-go test-go-unit test-go-integration test-go-e2e test-go-coverage \
	test-rust test-rust-unit test-rust-integration test-rust-coverage \
	test-all test-unit test-integration test-e2e test-coverage \
	lint lint-go lint-rust \
	run-engine run-client

all: proto wasm engine client cli

# gRPC コード生成
proto:
	@echo "Generating gRPC code..."
	cd proto && buf generate

# Wasm ビルド
wasm:
	@echo "Building Wasm..."
	cd adep-logic && \
	cargo build --release --target wasm32-unknown-unknown
	@mkdir -p wasm
	@cp adep-logic/target/wasm32-unknown-unknown/release/adep_logic.wasm wasm/
	@mkdir -p client/pkg/wasm
	@cp adep-logic/target/wasm32-unknown-unknown/release/adep_logic.wasm client/pkg/wasm/
	@cp adep-logic/target/wasm32-unknown-unknown/release/adep_logic.wasm wasm/

# Client ビルド (Coordinator server)
client: proto wasm
	@echo "Building Client (Coordinator)..."
	@mkdir -p bin
	cd client && CGO_ENABLED=0 go build -o ../bin/capsuled-client ./cmd/client

# CLI ビルド (rig-client CLI tool)
cli: proto
	@echo "Building CLI tool (rig-client)..."
	@mkdir -p bin
	cd client && CGO_ENABLED=0 go build -o ../bin/rig-client ./cmd/rig-client

# Engine ビルド
engine: proto wasm
	@echo "Building Engine..."
	cd engine && cargo build --release
	@mkdir -p bin
	@cp engine/target/release/capsuled-engine bin/

# =============================================================================
# Testing Targets
# =============================================================================

# Go Tests
test-go: wasm test-go-unit

test-go-unit:
	@echo "Running Go unit tests..."
	cd client && go test -v -cover ./pkg/...

test-go-integration:
	@echo "Running Go integration tests..."
	@echo "Note: Requires rqlite running at http://localhost:4001"
	cd tests/integration && go test -v -tags=integration ./...

test-go-e2e:
	@echo "Running Go E2E tests..."
	cd client/e2e && go test -v ./...
	cd tests/e2e && go test -v ./...

test-go-coverage:
	@echo "Generating Go test coverage report..."
	cd client && go test -coverprofile=../coverage-go.out -covermode=atomic ./pkg/...
	@echo "Go coverage report: coverage-go.out"
	@go tool cover -func=coverage-go.out | grep total:

# Rust Tests
test-rust: test-rust-unit

test-rust-unit:
	@echo "Running Rust unit tests..."
	cd engine && cargo test --lib
	cd adep-logic && cargo test

test-rust-integration:
	@echo "Running Rust integration tests..."
	@echo "Note: Storage integration tests require root privileges and LVM setup"
	cd engine && cargo test --test storage_integration -- --ignored || echo "⚠️  Storage integration tests skipped (requires root/LVM)"

test-rust-coverage:
	@echo "Generating Rust test coverage report..."
	@which cargo-tarpaulin > /dev/null 2>&1 || (echo "Installing cargo-tarpaulin..." && cargo install cargo-tarpaulin)
	cd engine && cargo tarpaulin --out Html --output-dir ../coverage-rust --skip-clean
	@echo "Rust coverage report: coverage-rust/index.html"

# Combined Tests
test-all: test-unit test-integration test-e2e

test-unit: test-go-unit test-rust-unit

test-integration: test-go-integration test-rust-integration

test-e2e: test-go-e2e

test-coverage: test-go-coverage test-rust-coverage
	@echo ""
	@echo "✅ Coverage reports generated:"
	@echo "   - Go:   coverage-go.out (use: go tool cover -html=coverage-go.out)"
	@echo "   - Rust: coverage-rust/index.html"

# Default test target (unit tests only)
test: test-unit

# =============================================================================
# Linting Targets
# =============================================================================

lint: lint-go lint-rust

lint-go:
	@echo "Linting Go code..."
	cd client && go fmt ./...
	cd client && go vet ./...
	@which golangci-lint > /dev/null 2>&1 && (cd client && golangci-lint run) || echo "⚠️  golangci-lint not installed, skipping advanced linting"

lint-rust:
	@echo "Linting Rust code..."
	cd engine && cargo fmt -- --check || (echo "Run 'cargo fmt' to fix formatting" && exit 1)
	cd engine && cargo clippy -- -D warnings
	cd adep-logic && cargo fmt -- --check || (echo "Run 'cargo fmt' to fix formatting" && exit 1)
	cd adep-logic && cargo clippy -- -D warnings

# =============================================================================
# Cleanup
# =============================================================================

clean:
	@echo "Cleaning..."
	rm -rf bin/ wasm/ coverage-go.out coverage-rust/
	cd adep-logic && cargo clean
	cd engine && cargo clean
	cd client && go clean

# =============================================================================
# Development Targets
# =============================================================================

run-engine:
	cd engine && cargo run

run-client:
	cd client && go run ./cmd/client/main.go
