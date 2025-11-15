# Capsuled コードレビュー (ARCHITECTURE.md ベース)

**レビュー日**: 2025-11-15  
**レビュー対象**: capsuled リポジトリ全体  
**基準文書**: ARCHITECTURE.md

---

## 📋 レビューサマリー

### ステータス: ⚠️ 改善が必要

- **適合度**: 75%
- **重大な問題**: 3件
- **中程度の問題**: 5件
- **軽微な問題**: 8件
- **推奨事項**: 6件

---

## 🔴 重大な問題 (Critical Issues)

### 1. ドキュメント間の命名不一致

**問題**: Engine コンポーネントのドキュメントで "Rig-Manager" と "Capsuled" が混在

**該当箇所**:
- `engine/README.md` - "Rig-Manager" として記載
- `engine/PROJECT_OVERVIEW.md` - "Rig-Manager Project Overview" として記載
- メインの `README.md` - "Capsuled" として記載

**影響**:
- 開発者の混乱を招く
- ブランディングの一貫性欠如
- ドキュメント検索の困難

**推奨アクション**:
```bash
# engine/README.md を更新
- "Rig-Manager" → "Capsuled Engine"
# engine/PROJECT_OVERVIEW.md を更新または削除
- プロジェクト名を統一
- または ARCHITECTURE.md に統合
```

**優先度**: 🔴 High

---

### 2. アーキテクチャ図と実装の不一致

**問題**: README.md の簡略図が実際のアーキテクチャを正確に表現していない

**README.md の図**:
```
┌─────────────────┐
│   rig-client    │  (TypeScript/Go CLI)  ← TypeScript実装は存在しない
└────────┬────────┘
```

**実際の実装**:
- `rig-client` という名前のコンポーネントは存在しない
- Client は Go のみで実装されている
- "rig-client" は外部 CLI ツールを指している可能性があるが、未実装

**推奨アクション**:
```markdown
# README.md の図を以下に修正:
┌─────────────────┐
│  外部クライアント  │  (CLI/API Client - 将来実装予定)
└────────┬────────┘
         │ HTTPS API
    ┌────┴────┐
    ↓         ↓
┌────────┐  ┌────────┐
│Client 1│  │Client 2│ (Go - Coordinator)
└───┬────┘  └───┬────┘
    │ gRPC     │ gRPC
┌───┴────┐  ┌──┴─────┐
│Engine 1│  │Engine 2│ (Rust - Agent)
└────────┘  └────────┘
```

**優先度**: 🔴 High

---

### 3. Proto 定義の二重構造

**問題**: `engine.proto` と `coordinator.proto` で類似機能が重複定義されている

**該当箇所**:
- `proto/engine.proto` - `DeployCapsule` サービス定義
- `proto/coordinator.proto` - `DeployWorkload` サービス定義

**具体例**:

**engine.proto**:
```protobuf
service Engine {
  rpc DeployCapsule(DeployRequest) returns (DeployResponse);
  rpc StopCapsule(StopRequest) returns (StopResponse);
  rpc ValidateManifest(ValidateRequest) returns (ValidationResult);
}
```

**coordinator.proto**:
```protobuf
service Coordinator {
  rpc DeployWorkload(DeployWorkloadRequest) returns (DeployWorkloadResponse);
}
```

**影響**:
- どちらを使うべきか不明確
- メンテナンスコストの増加
- API の一貫性欠如

**推奨アクション**:
1. `coordinator.proto` を主要プロトコルとして採用（より包括的な設計）
2. `engine.proto` を非推奨化するか、内部 API として明示
3. ARCHITECTURE.md に使い分けを明記

**優先度**: 🔴 High

---

## 🟡 中程度の問題 (Medium Issues)

### 4. Client パッケージ構造の複雑性

**問題**: Client の役割が "Coordinator" と "Client" で混在

**該当箇所**:
- `client/cmd/client/main.go` - "Capsuled Coordinator starting..." とログ出力
- Go モジュール名 - `github.com/onescluster/coordinator`
- README.md - "Capsuled Client" と記載

**推奨アクション**:
- 用語を統一: "Client" を "Coordinator" に変更するか、逆にする
- ARCHITECTURE.md に役割を明確に定義

**優先度**: 🟡 Medium

---

### 5. Wasm ファイルパスのハードコード

**問題**: Engine のデフォルト Wasm パスが相対パスでハードコードされている

**該当箇所**:
```rust
// engine/src/main.rs
const DEFAULT_WASM_PATH: &str =
    "../adep-logic/target/wasm32-unknown-unknown/release/adep_logic.wasm";
```

**影響**:
- デプロイ環境で動作しない
- 開発環境依存

**推奨アクション**:
```rust
// 埋め込みまたは設定ファイルから読み込み
const DEFAULT_WASM_PATH: &str = "/usr/share/capsuled/adep_logic.wasm";

// または埋め込み:
const WASM_BYTES: &[u8] = include_bytes!("../wasm/adep_logic.wasm");
```

**優先度**: 🟡 Medium

---

### 6. データベース実装の抽象化不足

**問題**: Client が rqlite に直接依存しており、データベース抽象化が不十分

**該当箇所**:
- `client/pkg/db/rqlite.go` - rqlite 直接実装
- `client/pkg/db/state_manager.go` - rqlite 特化の実装

**影響**:
- 他のデータベースへの移行が困難
- テスト時のモック作成が複雑

**推奨アクション**:
```go
// データベースインターフェースの定義
type StateStore interface {
    GetNode(id string) (*Node, bool)
    PutNode(node *Node) error
    // ...
}

// rqlite 実装
type RQLiteStore struct { ... }

// メモリ実装（テスト用）
type InMemoryStore struct { ... }
```

**優先度**: 🟡 Medium

---

### 7. GPU 監視機能の文書化不足

**問題**: GPU 検出・監視機能が ARCHITECTURE.md に記載されているが、実装詳細が不明

**該当箇所**:
- `engine/src/hardware/gpu_detector.rs`
- `engine/src/hardware/gpu_process_monitor.rs`

**推奨アクション**:
- GPU 監視機能の設計ドキュメント追加
- `docs/gpu-mock-configuration.md` の内容を ARCHITECTURE.md に統合
- API エンドポイントでの GPU 情報取得方法を明記

**優先度**: 🟡 Medium

---

### 8. エラーハンドリングの一貫性欠如

**問題**: Engine と Client でエラーハンドリングのパターンが異なる

**Engine (Rust)**:
```rust
use anyhow::Result;
use thiserror::Error;
```

**Client (Go)**:
```go
// 標準 error インターフェース
return fmt.Errorf("failed: %w", err)
```

**推奨アクション**:
- 各言語のベストプラクティスに従う（現状維持）
- gRPC エラーコードの統一的な使用
- ARCHITECTURE.md にエラーハンドリング方針を追加

**優先度**: 🟡 Medium

---

## 🟢 軽微な問題 (Minor Issues)

### 9. コメントの言語混在

**問題**: 日本語と英語のコメントが混在

**例**:
```rust
// Capsuled Engine - Agent for running capsules
/// Coordinator gRPC endpoint (host:port or URL)
```

```go
// Parse command line flags
// 日本語のコメント
```

**推奨アクション**:
- プロジェクト全体で言語を統一（英語推奨）
- または、パブリック API は英語、内部実装は日本語と明確に分ける

**優先度**: 🟢 Low

---

### 10. 設定ファイル形式の不統一

**問題**: Client は YAML、Engine は TOML を使用

**該当箇所**:
- `client/config.yaml.example`
- `engine/config.toml.example`

**推奨アクション**:
- 両方を YAML に統一（Kubernetes との親和性）
- または、両方を TOML に統一（Rust エコシステム）
- 設定スキーマを文書化

**優先度**: 🟢 Low

---

### 11. テストカバレッジの可視化不足

**問題**: テストはあるが、カバレッジが不明

**推奨アクション**:
```bash
# Cargo.toml に追加
[profile.coverage]
inherits = "dev"

# CI で実行
cargo tarpaulin --out Lcov
go test -coverprofile=coverage.out ./...
```

**優先度**: 🟢 Low

---

### 12. Makefile のターゲット説明不足

**問題**: Makefile にヘルプ機能がない

**推奨アクション**:
```makefile
.PHONY: help
help: ## Show this help message
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

all: proto wasm engine client ## Build all components

proto: ## Generate gRPC code
	...
```

**優先度**: 🟢 Low

---

### 13. ログレベル設定の欠如

**問題**: Engine はトレーシングを使用しているが、Client のログレベル設定が不明確

**推奨アクション**:
- 両コンポーネントで環境変数によるログレベル設定を統一
- `CAPSULED_LOG_LEVEL=debug` など

**優先度**: 🟢 Low

---

### 14. バージョン管理の不統一

**問題**: コンポーネントごとのバージョン番号が不明

**推奨アクション**:
- セマンティックバージョニングの採用
- `Cargo.toml` と `go.mod` でバージョンを統一
- Git タグとの連携

**優先度**: 🟢 Low

---

### 15. ドキュメントの日付管理

**問題**: 一部のドキュメントに更新日付がない

**推奨アクション**:
- 全ドキュメントの先頭に "最終更新" を追加
- CI で古いドキュメントを検出

**優先度**: 🟢 Low

---

### 16. 依存関係のセキュリティスキャン

**問題**: Dependabot や Renovate の設定がない可能性

**推奨アクション**:
```yaml
# .github/dependabot.yml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/engine"
    schedule:
      interval: "weekly"
  - package-ecosystem: "gomod"
    directory: "/client"
    schedule:
      interval: "weekly"
```

**優先度**: 🟢 Low

---

## ✅ 良好な点 (Strengths)

### 1. モジュール設計の明確さ

- Client、Engine、adep-logic の責務分離が明確
- gRPC による疎結合な通信

### 2. CI/CD の充実

- `docs/CI_CD_ARCHITECTURE.md` による詳細な文書化
- GitHub Actions による自動ビルド・テスト

### 3. Wasm の活用

- プラットフォーム非依存のロジック共有
- セキュアなサンドボックス実行

### 4. 型安全性

- Protocol Buffers による厳格な型定義
- Rust の型システムによる安全性

### 5. 設定ファイルの例示

- `.example` ファイルによる設定例の提供

### 6. プロビジョニングスクリプト

- `engine/provisioning/` による自動セットアップ

---

## 📝 推奨アクション (Action Items)

### 即座に実施すべき項目

1. ✅ **ARCHITECTURE.md の作成** (完了)
2. 🔴 **命名の統一**: "Rig-Manager" → "Capsuled Engine"
3. 🔴 **README.md のアーキテクチャ図修正**
4. 🔴 **Proto 定義の整理**: 主要プロトコルの明確化

### 短期（1-2週間）で実施すべき項目

5. 🟡 **用語の統一**: "Client" vs "Coordinator"
6. 🟡 **Wasm パスの修正**: 埋め込みまたは適切なデフォルトパス
7. 🟡 **GPU 監視機能の文書化**

### 中期（1ヶ月）で実施すべき項目

8. 🟡 **データベース抽象化の改善**
9. 🟢 **テストカバレッジの可視化**
10. 🟢 **設定ファイル形式の統一**

### 長期（3ヶ月）で検討すべき項目

11. 🟢 **コメント言語の統一**
12. 🟢 **エラーハンドリングガイドラインの策定**
13. 🟢 **Dependabot の設定**

---

## 🎯 アーキテクチャ適合性評価

### Client (Coordinator) - Go

| 項目 | 評価 | コメント |
|-----|------|---------|
| Master 選出 | ✅ 実装済 | `client/pkg/master/` に実装 |
| スケジューリング | ✅ 実装済 | `client/pkg/scheduler/` に実装 |
| HTTP API サーバー | ⚠️ 部分実装 | `client/pkg/api/` はあるが詳細不明 |
| Wasmer 統合 | ❓ 確認必要 | コードからは確認できず |
| gRPC クライアント | ✅ 実装済 | `client/pkg/grpc/` に実装 |

### Engine (Agent) - Rust

| 項目 | 評価 | コメント |
|-----|------|---------|
| gRPC サーバー | ✅ 実装済 | `engine/src/grpc_server.rs` |
| コンテナ実行 | ✅ 実装済 | `engine/src/runtime/` |
| LVM/LUKS 管理 | ❓ 確認必要 | コードからは確認できず |
| Caddy 統合 | ❓ 確認必要 | PROJECT_OVERVIEW には記載 |
| Wasmtime 統合 | ✅ 実装済 | `engine/src/wasm_host.rs` |
| GPU 検出・監視 | ✅ 実装済 | `engine/src/hardware/` |

### adep-logic - Rust → Wasm

| 項目 | 評価 | コメント |
|-----|------|---------|
| adep.json パース | ✅ 想定される | `adep-logic/src/` に実装 |
| バリデーション | ✅ 想定される | Wasm としてコンパイル可能 |
| 両環境での動作 | ✅ 設計上可能 | Wasmer/Wasmtime 両対応 |

---

## 📊 総合評価

### スコアカード

| カテゴリ | スコア | 満点 |
|---------|-------|------|
| アーキテクチャ適合性 | 12 | 15 |
| ドキュメント品質 | 7 | 10 |
| コード品質 | 8 | 10 |
| テスト充実度 | 6 | 10 |
| セキュリティ | 7 | 10 |
| 保守性 | 8 | 10 |
| **合計** | **48** | **65** |

### 総合評価: 74% (B)

**所見**:
- 基本的なアーキテクチャは良好に実装されている
- ドキュメント間の不整合が主な問題
- 命名の統一とプロトコル定義の整理が急務
- 実装品質は高いが、文書化が追いついていない

---

## 🔍 詳細調査が必要な項目

以下の項目は、コードレビューでは確認できなかったため、実装者への確認が必要:

1. **Wasmer 統合の実装状況** (Client)
2. **LVM/LUKS ストレージ管理の実装状況** (Engine)
3. **Caddy 統合の実装状況** (Engine)
4. **HTTP API エンドポイントの完全な仕様** (Client)
5. **Master 選出アルゴリズムの詳細**
6. **スケジューリングアルゴリズムの詳細**

---

## 🚀 次のステップ

1. **緊急**: 命名とドキュメントの統一（3日以内）
2. **重要**: Proto 定義の整理と主要プロトコルの明確化（1週間以内）
3. **中期**: 不明な実装項目の確認と文書化（2週間以内）
4. **継続**: テストカバレッジの向上と CI/CD の強化（継続的）

---

**レビュー実施者**: AI Code Reviewer  
**レビュー日**: 2025-11-15  
**次回レビュー予定**: Phase 1 完了時
