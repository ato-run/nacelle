# CI/CD パイプライン ドキュメント

このドキュメントは、Capsuled プロジェクトの CI/CD パイプラインの詳細について説明します。

## 概要

GitHub Actions を使用した自動化されたビルド・テスト・デプロイパイプラインを構築しました。

## トリガー条件

以下のイベントで CI ワークフローが自動実行されます：

- `main` または `develop` ブランチへの push
- `main` または `develop` ブランチへの Pull Request
- `v*` パターンのタグ作成（例: `v1.0.0`, `v2.1.3`）

## ワークフロージョブ

### 1. build-adep-logic (Wasm ビルド)

**目的**: adep-logic を Wasm にコンパイル

**実行内容**:
- Rust toolchain のセットアップ（wasm32-unknown-unknown target）
- 依存関係のキャッシュ
- リリースビルド (`cargo build --release --target wasm32-unknown-unknown`)
- ユニットテスト実行
- 成果物のアップロード（`adep_logic.wasm`）

**成果物**: `adep-logic-wasm/adep_logic.wasm` (約 72KB)

### 2. build-engine (Engine ビルド)

**目的**: Rust 製の Engine をビルドしてテスト

**実行内容**:
- Rust toolchain のセットアップ
- protobuf compiler のインストール
- 依存関係のキャッシュ
- デバッグビルドとテスト実行
- リリースビルド
- 成果物のアップロード（`capsuled-engine`）

**成果物**: `capsuled-engine-linux-x86_64/capsuled-engine` (約 19MB)

### 3. build-client (Go Client ビルド)

**目的**: Go 製の Client を複数の構成でビルド

**ビルドバリアント**:
1. **標準ビルド** (`linux-x86_64`)
   - CGO_ENABLED=1
   - 標準的な Linux 環境向け

2. **静的ビルド** (`linux-x86_64-static`)
   - CGO_ENABLED=0
   - 完全な静的リンク
   - 依存ライブラリ不要
   - Alpine/musl libc 環境にも対応

**成果物**: 各バリアント約 21MB

### 4. test-client (Go テスト)

**目的**: Client のユニットテストと統合テストを実行

**実行内容**:
- すべてのパッケージテスト (`go test -v ./pkg/...`)
- GPU シミュレーション E2E テスト

### 5. integration-test (統合テスト)

**目的**: すべてのコンポーネントが正常にビルドできることを確認

**実行内容**:
- Rust と Go の開発環境セットアップ
- protoc と buf のインストール
- Wasm、Engine、Client を順次ビルド
- 成果物の存在確認

### 6. release (リリース作成)

**トリガー**: タグ `v*` が push された場合のみ実行

**実行内容**:
- すべてのビルドジョブの成果物をダウンロード
- GitHub Release を作成
- 以下のファイルをリリースアセットとして添付:
  - `adep_logic.wasm`
  - `capsuled-engine` (Linux x86_64)
  - `capsuled-client-linux-x86_64`
  - `capsuled-client-linux-x86_64-static`

## セキュリティ

### GITHUB_TOKEN 権限

すべてのジョブに明示的な権限ブロックを設定：

- 通常のビルド・テストジョブ: `contents: read`
- リリースジョブ: `contents: write`

これにより、最小権限の原則に従い、不要な権限の付与を防止します。

### 検証済み

- ✅ CodeQL セキュリティスキャン: 0 件のアラート
- ✅ すべてのジョブで明示的な権限設定

## キャッシュ戦略

ビルド時間を短縮するため、以下をキャッシュ：

- Rust 依存関係 (`~/.cargo/registry`, `~/.cargo/git`, `target/`)
- Go 依存関係 (Go setup action が自動管理)

キャッシュキーは各コンポーネントの `Cargo.lock` / `go.sum` のハッシュ値を使用。

## ローカルでの再現

CI 環境をローカルで再現する手順：

### 必要なツール

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown

# Go 1.23+
# https://go.dev/dl/

# protobuf compiler
sudo apt-get update
sudo apt-get install -y protobuf-compiler

# buf (オプション)
BUF_VERSION=1.28.1
curl -sSL "https://github.com/bufbuild/buf/releases/download/v${BUF_VERSION}/buf-$(uname -s)-$(uname -m)" -o /usr/local/bin/buf
chmod +x /usr/local/bin/buf
```

### ビルド手順

```bash
# Wasm ビルド
cd adep-logic
cargo build --release --target wasm32-unknown-unknown

# Engine ビルド
cd ../engine
cargo build --release

# Client ビルド（標準）
cd ../client
go build -o capsuled-client ./cmd/client

# Client ビルド（静的）
CGO_ENABLED=0 go build -o capsuled-client-static ./cmd/client
```

### テスト実行

```bash
# Rust テスト
cd adep-logic && cargo test
cd ../engine && cargo test

# Go テスト
cd ../client && go test -v ./pkg/...
```

## 今後の拡張計画

- [ ] コードカバレッジ計測と Codecov 連携
- [ ] Clippy / golangci-lint などの静的解析追加
- [ ] マルチプラットフォームビルド（macOS, Windows）
- [ ] Docker イメージビルドとプッシュ
- [ ] ベンチマークテストの自動実行
- [ ] Dependabot による依存関係自動更新

## トラブルシューティング

### ビルドが失敗する場合

1. **protoc が見つからない**
   - `build-engine` ジョブで protoc をインストールしているか確認
   - ローカルでは `sudo apt-get install protobuf-compiler`

2. **Wasm ターゲットが見つからない**
   - `rustup target add wasm32-unknown-unknown` を実行

3. **Go モジュールのダウンロード失敗**
   - ネットワーク接続を確認
   - `go mod download` を手動実行

### ワークフローが実行されない場合

- トリガー条件を確認（`main`/`develop` ブランチへの push/PR）
- GitHub Actions が有効になっているか確認
- ワークフローファイルの YAML 構文エラーがないか確認

## 参考リンク

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Rust CI/CD Best Practices](https://doc.rust-lang.org/cargo/guide/continuous-integration.html)
- [Go CI/CD with GitHub Actions](https://docs.github.com/en/actions/automating-builds-and-tests/building-and-testing-go)
