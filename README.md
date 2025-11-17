# Capsuled

Personal Cloud OS - モノレポ

## アーキテクチャ

```
┌─────────────────┐
│  外部クライアント  │  (CLI/API Client - 将来実装予定)
└────────┬────────┘
         │ HTTPS API
    ┌────┴──────┬──────┐
    ↓           ↓      ↓
┌────────────┐  ┌────────────┐  ┌────────────┐
│Coordinator │  │Coordinator │  │Coordinator │ (Go)
│ (Client 1) │  │ (Client 2) │  │ (Client 3) │
└─────┬──────┘  └─────┬──────┘  └─────┬──────┘
      │ gRPC          │ gRPC          │ gRPC
┌─────┴──────┐  ┌─────┴──────┐  ┌─────┴──────┐
│  Engine 1  │  │  Engine 2  │  │  Engine 3  │ (Rust)
│  (Agent)   │  │  (Agent)   │  │  (Agent)   │
└────────────┘  └────────────┘  └────────────┘
```

## コンポーネント

### `client/` - Client (Go)

- Master 選出
- スケジューリング
- HTTP API サーバー
- Wasmer による Wasm 実行

### `engine/` - Engine (Rust)
> NOTE: 用語の統一 — `capsuled` では以下の名称を採用します。
>
>- `Capsuled Client` (旧: `rig-client`)
>- `Capsuled Engine` (旧: `rig-manager`)
>
> この用語は `documents/capsuled/GLOSSARY.md` に記載されています。

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

- `coordinator.proto` (推奨・Canonical)
- `engine.proto` (レガシー: 非推奨 — `coordinator.proto` に統合予定)

> NOTE: ドキュメントのシングルソースは `documents/capsuled/` を参照してください。用語統一と設計決定は `documents/capsuled/GLOSSARY.md` と `DOCUMENTATION_GUIDELINES.md` に従ってください。
> `coordinator.proto` を単一の真実のソース (single source of truth) として採用しています。
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
3. **client (Go)** - 以下の2種類のビルド:
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
make test        # 全ユニットテスト (Go + Rust)
make test-all    # 全テスト (ユニット + 統合 + E2E)
```

## テスト

Capsuled は包括的なテストインフラを提供しています:

- **ユニットテスト**: 各関数とモジュールの単体テスト
- **統合テスト**: コンポーネント間の連携テスト
- **E2Eテスト**: システム全体のエンドツーエンドテスト

### テスト実行

```bash
# 全ユニットテスト
make test-unit

# Go ユニットテスト
make test-go-unit

# Rust ユニットテスト
make test-rust-unit

# 統合テスト (rqlite が必要)
make test-integration

# E2E テスト
make test-e2e

# 全テスト
make test-all

# カバレッジレポート生成
make test-coverage
```

詳細は [TESTING.md](./TESTING.md) を参照してください。

### テストカバレッジ

現在のカバレッジ状況:

**Go コンポーネント**:
- API middleware: 100%
- Master election: 89.2%
- gRPC server: 88.5%
- Config: 87.5%
- Headscale client: 85.0%
- Reconciler: 37.0%+
- 他のパッケージ: 40-80%

**Rust コンポーネント**:
- Storage (LVM/LUKS): ユニットテスト + 統合テスト
- Storage error: 100%
- Adep parser: ユニットテスト
- Metrics: ユニットテスト
- 合計: 82 テスト通過
```
