.PHONY: all proto wasm client engine test clean

all: proto wasm engine client

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

# Client ビルド
client: proto wasm
	@echo "Building Client..."
	@mkdir -p bin
	cd client && go build -o ../bin/capsuled-client ./cmd/client

# Engine ビルド
engine: proto wasm
	@echo "Building Engine..."
	cd engine && cargo build --release
	@mkdir -p bin
	@cp engine/target/release/capsuled-engine bin/

# 統合テスト
test: all
	@echo "Running tests..."
	cd engine && cargo test

# クリーン
clean:
	@echo "Cleaning..."
	rm -rf bin/ wasm/
	cd adep-logic && cargo clean
	cd engine && cargo clean
	cd client && go clean

# 開発モード実行
run-engine:
	cd engine && cargo run

run-client:
	cd client && go run ./cmd/client/main.go
