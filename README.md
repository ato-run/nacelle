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

## CI/CD

このプロジェクトは GitHub Actions を使用した自動化された CI/CD パイプラインを備えています。

### 自動実行

以下のイベントで CI が自動的に実行されます:
- `main` または `develop` ブランチへの push
- `main` または `develop` ブランチへの Pull Request
- `v*` パターンのタグ作成時

### ビルドジョブ

1. **adep-logic (Wasm)** - Rust → Wasm32 のリリースビルド
2. **engine (Rust)** - デバッグ・リリースビルドとテスト実行
3. **client (Go)** - 以下の3種類のビルド:
   - 標準ビルド (CGO 有効)
   - 静的ビルド (CGO_ENABLED=0、Alpine/musl 対応)
4. **統合テスト** - 全コンポーネントのビルドと依存関係の検証

### リリース

`v*` タグを作成すると、自動的に GitHub Release が作成され、以下の成果物が添付されます:
- `adep_logic.wasm` - Wasm バイナリ
- `capsuled-engine` - Engine バイナリ (Linux x86_64)
- `capsuled-client-linux-x86_64` - Client バイナリ (標準)
- `capsuled-client-linux-x86_64-static` - Client バイナリ (静的、Alpine/musl 対応)

### アーティファクト

Pull Request や開発ブランチでは、ビルド成果物は GitHub Actions のアーティファクトとして 7 日間保存されます。

### ローカルでの CI 環境再現

```bash
# 必要なツールのインストール
# - Rust toolchain (rustup)
# - Go 1.23+
# - protobuf-compiler
# - buf (オプション)

# Rust wasm32 ターゲットの追加
rustup target add wasm32-unknown-unknown

# 全コンポーネントのビルド
make all

# テスト実行
cd engine && cargo test
cd client && go test ./pkg/...
```
