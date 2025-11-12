# Capsuled

Personal Cloud OS - モノレポ

## アーキテクチャ

```
┌─────────────────┐
│   rig-client    │  (TypeScript/Go CLI)
└────────┬────────┘
         │ HTTPS API
    ┌────┴──────┬──────┐
    ↓           ↓      ↓
┌────────┐  ┌────────┐  ┌────────┐
│Client 1│  │Client 2│  │Client 3│ (Go)
└───┬────┘  └───┬────┘  └───┬────┘
    │ gRPC     │ gRPC     │ gRPC
┌───┴────┐  ┌──┴─────┐  ┌──┴─────┐
│Engine 1│  │Engine 2│  │Engine 3│ (Rust)
└────────┘  └────────┘  └────────┘
```

## コンポーネント

### `client/` - Client (Go)

- Master 選出
- スケジューリング
- HTTP API サーバー
- Wasmer による Wasm 実行

### `engine/` - Engine (Rust)

- gRPC サーバー
- コンテナ実行
- LVM/LUKS ストレージ管理
- Caddy ネットワーク管理
- Wasmtime による Wasm 実行

### `adep-logic/` - 共通ロジック (Rust → Wasm)

- adep.json パーサー
- バリデーター
- Client と Engine の両方で使用

### `proto/` - gRPC 定義

- engine.proto
- buf.yaml

## ビルド

```bash
# 全体
make

# 個別
make wasm
make client
make engine
```

## 開発

```bash
# Wasm ビルド
cd adep-logic
cargo build --target wasm32-unknown-unknown --release

# Engine ビルド
cd engine
cargo build

# Client ビルド
cd client
go build -o bin/capsuled-client ./cmd/client
```
