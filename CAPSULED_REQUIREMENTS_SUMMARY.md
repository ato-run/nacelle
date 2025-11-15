# Capsuled 要件・実装状況サマリー

**バージョン:** 1.0.0  
**最終更新:** 2025-11-15  
**目的:** コードベース横断探索による機能一覧と実装ギャップの可視化

---

## 📋 目次

1. [コンポーネント別実装状況](#コンポーネント別実装状況)
2. [機能要件マトリクス](#機能要件マトリクス)
3. [未実装機能詳細](#未実装機能詳細)
4. [技術的依存関係](#技術的依存関係)
5. [優先度付けバックログ](#優先度付けバックログ)

---

## コンポーネント別実装状況

### Client (Coordinator) - Go

**ディレクトリ**: `client/`  
**総行数**: 8,461 LOC  
**完成度**: 60%

| パッケージ | ファイル数 | 行数 | 実装状況 | テスト | 備考 |
|-----------|----------|------|----------|--------|------|
| `api/` | 1 | ~500 | 🟡 40% | ❌ なし | DeployHandler のみ実装 |
| `config/` | 2 | ~300 | ✅ 100% | ✅ あり | YAML パース完成 |
| `db/` | 7 | ~1,500 | ✅ 90% | ✅ あり | rqlite 統合完成 |
| `gossip/` | 2 | ~400 | ✅ 100% | ✅ あり | Memberlist 完成 |
| `grpc/` | 2 | ~600 | ✅ 90% | ✅ あり | Server/Client 実装 |
| `headscale/` | 2 | ~400 | 🟡 60% | ✅ あり | Client 実装、統合未完 |
| `master/` | 2 | ~700 | ✅ 100% | ✅ あり | 選出ロジック完成 |
| `proto/` | 4 | ~2,000 | ✅ 100% | - | 自動生成 |
| `reconcile/` | 3 | ~800 | 🟡 50% | ✅ あり | 基本ロジックのみ |
| `scheduler/gpu/` | 4 | ~1,500 | ✅ 95% | ✅ あり | Filter-Score 完成 |
| **未実装** | - | - | - | - | - |
| `wasm/` | 0 | 0 | ❌ 0% | ❌ なし | Wasmer 統合未着手 |
| `proxy/` | 0 | 0 | ❌ 0% | ❌ なし | Caddy クライアント未着手 |
| `metrics/` | 0 | 0 | ❌ 0% | ❌ なし | Prometheus 未着手 |

#### Client 実装詳細

##### ✅ 完成済み

1. **Master Election** (`pkg/master/`)
   - Memberlist ベースの分散選出
   - リーダー検出・フォロワー動作
   - テストカバレッジ: 85%

2. **GPU Scheduler** (`pkg/scheduler/gpu/`)
   - Filter-Score パイプライン
   - 3 種類のフィルタ: HasGPU, VRAM, CUDA
   - BestFit スコアラー実装
   - テストカバレッジ: 90%

3. **Database** (`pkg/db/`)
   - rqlite 統合
   - Migration サポート
   - Node/State ストア
   - セキュリティテスト実装

4. **Gossip** (`pkg/gossip/`)
   - Memberlist によるノード検出
   - クラスタメンバーシップ管理
   - イベント通知

5. **Config** (`pkg/config/`)
   - YAML ベース設定
   - バリデーション
   - デフォルト値

##### 🟡 部分実装

6. **API Handler** (`pkg/api/`)
   - **実装済み**: `DeployHandler` (POST /api/v1/capsules)
   - **未実装**:
     - GET /api/v1/capsules/:id
     - GET /api/v1/capsules (一覧)
     - DELETE /api/v1/capsules/:id
     - GET /api/v1/nodes
     - WS /api/v1/capsules/:id/logs

7. **Reconciler** (`pkg/reconcile/`)
   - **実装済み**: 基本的な調整ループ
   - **未実装**:
     - エラーハンドリング
     - リトライロジック
     - メトリクス

8. **Headscale** (`pkg/headscale/`)
   - **実装済み**: HTTP クライアント
   - **未実装**:
     - 完全な統合テスト
     - 自動デバイス登録

##### ❌ 未実装

9. **Wasm 統合** (`pkg/wasm/`)
   - Wasmer バインディング
   - adep-logic 呼び出し

10. **Proxy Management** (`pkg/proxy/`)
    - Caddy Admin API クライアント
    - 動的ルート設定

11. **Metrics** (`pkg/metrics/`)
    - Prometheus メトリクス
    - カスタムメトリクス定義

---

### Engine (Agent) - Rust

**ディレクトリ**: `engine/`  
**総行数**: 6,072 LOC  
**完成度**: 55%

| モジュール | ファイル数 | 行数 | 実装状況 | テスト | 備考 |
|-----------|----------|------|----------|--------|------|
| `grpc_server.rs` | 1 | ~400 | ✅ 100% | ✅ あり | gRPC サーバー完成 |
| `coordinator_service.rs` | 1 | ~600 | ✅ 90% | ✅ あり | DeployWorkload 実装 |
| `status_reporter.rs` | 1 | ~800 | ✅ 100% | ✅ あり | 定期レポート完成 |
| `capsule_manager.rs` | 1 | ~600 | 🟡 40% | ❌ なし | 状態管理のみ |
| `wasm_host.rs` | 1 | ~300 | ✅ 100% | ✅ あり | Wasmtime 完成 |
| `hardware/` | 4 | ~1,200 | ✅ 85% | ✅ あり | GPU 検出完成 |
| `oci/` | 2 | ~400 | 🟡 50% | ❌ なし | Spec Builder のみ |
| `runtime/` | 1 | ~200 | 🟡 30% | ❌ なし | trait 定義のみ |
| `adep/` | 1 | ~300 | ✅ 80% | ✅ あり | Manifest パース |
| `config.rs` | 1 | ~400 | ✅ 100% | ✅ あり | TOML 設定完成 |
| **未実装** | - | - | - | - | - |
| `storage/` | 0 | 0 | ❌ 0% | ❌ なし | LVM/LUKS 未着手 |
| `proxy/` | 0 | 0 | ❌ 0% | ❌ なし | Caddy 統合未着手 |
| `metrics/` | 0 | 0 | ❌ 0% | ❌ なし | Prometheus 未着手 |
| `logs/` | 0 | 0 | ❌ 0% | ❌ なし | ログ収集未着手 |

#### Engine 実装詳細

##### ✅ 完成済み

1. **gRPC Server** (`grpc_server.rs`, `coordinator_service.rs`)
   - Tonic ベース実装
   - Coordinator プロトコル実装
   - Engine プロトコル (レガシー) 実装
   - エラーハンドリング

2. **GPU Detector** (`hardware/gpu_detector.rs`)
   - Mock モード実装
   - Real モード (NVML) 実装
   - 環境変数ベース設定
   - フェイルセーフ機構

3. **Status Reporter** (`status_reporter.rs`)
   - 定期的な状態レポート送信
   - ハードウェア情報収集
   - Workload 状態収集
   - リトライロジック

4. **Wasm Host** (`wasm_host.rs`)
   - Wasmtime 統合
   - adep-logic.wasm ロード
   - Manifest バリデーション
   - エラーハンドリング

5. **Config** (`config.rs`)
   - TOML パース
   - 環境変数オーバーライド
   - デフォルト値

##### 🟡 部分実装

6. **Capsule Manager** (`capsule_manager.rs`)
   - **実装済み**:
     - Capsule 状態管理 (HashMap)
     - 基本 CRUD
   - **未実装**:
     - 実際のコンテナ起動
     - ログファイル管理
     - リソース追跡

7. **OCI** (`oci/`)
   - **実装済み**: Spec Builder (config.json 生成)
   - **未実装**:
     - Bundle 生成
     - rootfs 準備
     - ファイルシステム管理

8. **Runtime** (`runtime/`)
   - **実装済み**: trait 定義
   - **未実装**:
     - youki 統合実装
     - runc フォールバック
     - エラーリカバリ

9. **GPU Process Monitor** (`hardware/gpu_process_monitor.rs`)
   - **実装済み**: nvidia-smi パース
   - **未実装**:
     - プロセス紐付け
     - VRAM 追跡
     - 自動リソース回収

##### ❌ 未実装

10. **Storage** (`storage/`)
    - LVM ボリューム管理
    - LUKS 暗号化
    - クリーンアップ

11. **Proxy** (`proxy/`)
    - Caddy Admin API クライアント
    - 動的ルート設定

12. **Metrics** (`metrics/`)
    - Prometheus メトリクス
    - カスタムメトリクス

13. **Log Collection** (`logs/`)
    - コンテナログ収集
    - ログローテーション

---

### adep-logic - Wasm

**ディレクトリ**: `adep-logic/`  
**総行数**: ~200 LOC  
**完成度**: 20%

| 機能 | 実装状況 | 備考 |
|------|----------|------|
| JSON パース | ✅ 完成 | serde_json 使用 |
| 基本バリデーション | 🟡 部分実装 | name, version のみ |
| Client 統合 (Wasmer) | ❌ 未実装 | バインディング未作成 |
| Engine 統合 (Wasmtime) | ✅ 完成 | wasm_host.rs で使用中 |
| スキーマ検証 | ❌ 未実装 | 詳細ルール未実装 |
| エラーメッセージ | 🟡 部分実装 | 簡易メッセージのみ |

#### adep-logic 拡張計画

**Phase 1 (Week 1)**:
- [ ] スキーマ検証拡張 (GPU constraints, volumes)
- [ ] エラーメッセージ改善
- [ ] Client 統合 (Wasmer バインディング)

**Phase 2**:
- [ ] バリデーションルール追加
- [ ] カスタムバリデータープラグイン

---

### Proto Definitions

**ディレクトリ**: `proto/`  
**完成度**: 80%

| ファイル | 行数 | 状態 | 用途 |
|---------|------|------|------|
| `coordinator.proto` | ~130 | ✅ 完成 | **推奨** - 包括的な Workload 管理 |
| `engine.proto` | ~52 | ⚠️ レガシー | 後方互換性のため保持 |

#### Proto 課題

1. **二重管理**
   - `engine.proto` と `coordinator.proto` が併存
   - API の一貫性に課題

2. **推奨事項**
   - 新規開発は `coordinator.proto` を使用
   - `engine.proto` は Phase 2 で非推奨化検討

---

## 機能要件マトリクス

### コア機能

| 機能 | 要件 | 実装状況 | 優先度 | Phase |
|------|------|----------|--------|-------|
| **コンテナ実行** | youki/runc で OCI コンテナ実行 | 🟡 30% | 🔴 Critical | 1 |
| **GPU スケジューリング** | VRAM/CUDA 考慮したノード選択 | ✅ 95% | 🔴 Critical | 2 |
| **Master 選出** | 分散環境でのリーダー選出 | ✅ 100% | 🔴 Critical | - |
| **状態管理** | rqlite による分散状態管理 | ✅ 90% | 🔴 Critical | - |
| **HTTP API** | REST API によるコンテナ管理 | 🟡 40% | 🔴 Critical | 1 |
| **gRPC 通信** | Client ↔ Engine 通信 | ✅ 90% | 🔴 Critical | - |

### GPU 機能

| 機能 | 要件 | 実装状況 | 優先度 | Phase |
|------|------|----------|--------|-------|
| **GPU 検出** | NVIDIA GPU 検出 (Mock/Real) | ✅ 100% | 🔴 Critical | - |
| **VRAM 監視** | プロセス単位の VRAM 使用量 | 🟡 40% | 🟡 High | 2 |
| **GPU プロセス紐付け** | Capsule と GPU プロセスの関連付け | ❌ 0% | 🟡 High | 2 |
| **リソース回収** | プロセス終了時の VRAM 解放確認 | ❌ 0% | 🟡 High | 2 |
| **動的スケジューリング** | リアルタイム VRAM 状態反映 | 🟡 50% | 🟢 Medium | 2 |

### 運用機能

| 機能 | 要件 | 実装状況 | 優先度 | Phase |
|------|------|----------|--------|-------|
| **ログストリーミング** | WebSocket 経由のリアルタイムログ | ❌ 0% | 🟡 High | 3 |
| **メトリクス** | Prometheus メトリクス公開 | ❌ 0% | 🟡 High | 3 |
| **ヘルスチェック** | Readiness/Liveness プローブ | 🟡 30% | 🟡 High | 3 |
| **ストレージ管理** | LVM/LUKS による暗号化ストレージ | ❌ 0% | 🟢 Medium | 3 |
| **プロキシ管理** | Caddy による動的ルーティング | ❌ 0% | 🟢 Medium | 3 |

### 高可用性

| 機能 | 要件 | 実装状況 | 優先度 | Phase |
|------|------|----------|--------|-------|
| **Master フェイルオーバー** | リーダー障害時の自動切り替え | 🟡 50% | 🟡 High | 4 |
| **データレプリケーション** | rqlite レプリケーション | ✅ 80% | 🟡 High | 4 |
| **自動復旧** | Capsule 再起動ポリシー | ❌ 0% | 🟡 High | 4 |
| **Circuit Breaker** | gRPC 通信の障害保護 | ❌ 0% | 🟢 Medium | 4 |

### スケーラビリティ

| 機能 | 要件 | 実装状況 | 優先度 | Phase |
|------|------|----------|--------|-------|
| **動的ノード追加** | クラスタへの動的参加 | 🟡 60% | 🟢 Medium | 4 |
| **ロードバランシング** | ノード間負荷分散 | 🟡 40% | 🟢 Medium | 4 |
| **Auto Scaling** | リソース使用率ベーススケーリング | ❌ 0% | ⚪ Low | 4 |

---

## 未実装機能詳細

### Priority 1 (Critical) - Phase 1 対応必須

#### 1. コンテナライフサイクル管理 (Engine)

**現状**:
- Capsule Manager に状態管理のみ実装
- 実際の youki 実行コードなし

**必要な実装**:
```rust
// engine/src/runtime/youki.rs
pub struct YoukiRuntime {
    youki_path: PathBuf,
    state_dir: PathBuf,
}

impl YoukiRuntime {
    pub async fn create_container(&self, id: &str, bundle: &Path) -> Result<()>;
    pub async fn start_container(&self, id: &str) -> Result<()>;
    pub async fn delete_container(&self, id: &str) -> Result<()>;
    pub async fn container_state(&self, id: &str) -> Result<ContainerState>;
}
```

**依存関係**:
- youki バイナリのインストール
- OCI bundle 生成ロジック
- ログファイル管理

**工数**: 3 日

---

#### 2. HTTP API 完成 (Client)

**現状**:
- `DeployHandler` のみ実装
- 他のエンドポイントなし

**必要な実装**:
```go
// client/pkg/api/capsule_handler.go
func GetCapsuleHandler(w http.ResponseWriter, r *http.Request)
func ListCapsulesHandler(w http.ResponseWriter, r *http.Request)
func DeleteCapsuleHandler(w http.ResponseWriter, r *http.Request)

// client/pkg/api/node_handler.go
func ListNodesHandler(w http.ResponseWriter, r *http.Request)

// client/pkg/api/logs_handler.go
func StreamLogsHandler(w http.ResponseWriter, r *http.Request) // WebSocket
```

**依存関係**:
- OpenAPI 仕様書作成
- 認証ミドルウェア

**工数**: 4 日

---

#### 3. Wasm 統合 (Client)

**現状**:
- Engine 側は完成 (Wasmtime)
- Client 側は未着手

**必要な実装**:
```go
// client/pkg/wasm/wasmer.go
package wasm

import "github.com/wasmerio/wasmer-go/wasmer"

type WasmerHost struct {
    instance *wasmer.Instance
}

func NewWasmerHost(wasmBytes []byte) (*WasmerHost, error)
func (h *WasmerHost) ValidateManifest(json []byte) (bool, error)
```

**依存関係**:
- wasmer-go ライブラリ
- adep_logic.wasm バイナリ

**工数**: 2 日

---

### Priority 2 (High) - Phase 2-3 対応

#### 4. GPU プロセス監視強化 (Engine)

**現状**:
- nvidia-smi パースのみ
- プロセス紐付けなし

**必要な実装**:
```rust
// engine/src/hardware/gpu_process_monitor.rs
pub struct GpuProcessMonitor {
    detector: Arc<dyn GpuDetector>,
    capsule_manager: Arc<CapsuleManager>,
}

impl GpuProcessMonitor {
    pub fn get_vram_usage(&self, capsule_id: &str) -> Result<u64>;
    pub fn track_process(&self, capsule_id: &str, pid: u32) -> Result<()>;
    pub fn cleanup_terminated(&self) -> Result<Vec<String>>;
}
```

**依存関係**:
- Capsule Manager の PID 追跡
- nvidia-smi 詳細パース

**工数**: 5 日

---

#### 5. ログストリーミング (Client + Engine)

**必要な実装**:
```go
// client/pkg/api/logs_handler.go
func StreamLogsHandler(w http.ResponseWriter, r *http.Request) {
    // WebSocket アップグレード
    // Engine からログ取得
    // クライアントへストリーミング
}
```

```rust
// engine/src/logs/collector.rs
pub struct LogCollector {
    log_dir: PathBuf,
}

impl LogCollector {
    pub fn tail_logs(&self, capsule_id: &str) -> Result<impl Stream<Item = String>>;
}
```

**依存関係**:
- WebSocket ライブラリ (gorilla/websocket)
- ログファイル監視 (inotify)

**工数**: 6 日

---

#### 6. メトリクス (Engine + Client)

**必要な実装**:
```rust
// engine/src/metrics/prometheus.rs
use prometheus::{Registry, Counter, Gauge};

pub struct Metrics {
    capsule_count: Gauge,
    gpu_vram_used: Gauge,
    container_cpu_usage: Gauge,
}

pub fn register_metrics(registry: &Registry) -> Metrics;
pub fn serve_metrics(addr: SocketAddr, registry: Registry);
```

**依存関係**:
- prometheus クレート
- /metrics エンドポイント

**工数**: 3 日

---

### Priority 3 (Medium) - Phase 3-4 対応

#### 7. ストレージ管理 (Engine)

**必要な実装**:
```rust
// engine/src/storage/lvm.rs
pub struct LvmManager {
    vg_name: String,
}

impl LvmManager {
    pub fn create_volume(&self, name: &str, size_gb: u64) -> Result<PathBuf>;
    pub fn delete_volume(&self, name: &str) -> Result<()>;
}

// engine/src/storage/luks.rs
pub struct LuksManager;

impl LuksManager {
    pub fn encrypt_volume(&self, device: &Path, key: &[u8]) -> Result<()>;
    pub fn open_volume(&self, device: &Path, key: &[u8]) -> Result<PathBuf>;
}
```

**依存関係**:
- lvm2 コマンド
- cryptsetup コマンド
- 鍵管理システム

**工数**: 7 日

---

#### 8. プロキシ管理 (Engine)

**必要な実装**:
```rust
// engine/src/proxy/caddy.rs
pub struct CaddyClient {
    admin_url: String,
    client: reqwest::Client,
}

impl CaddyClient {
    pub async fn add_route(&self, domain: &str, upstream: &str) -> Result<()>;
    pub async fn remove_route(&self, domain: &str) -> Result<()>;
    pub async fn reload_config(&self) -> Result<()>;
}
```

**依存関係**:
- Caddy Admin API
- DNS 設定

**工数**: 4 日

---

## 技術的依存関係

### 外部ツール

| ツール | 用途 | 必須 | 現状 |
|-------|------|------|------|
| youki | OCI ランタイム | ✅ | パス指定のみ |
| runc | OCI ランタイム (フォールバック) | ⚪ | 未対応 |
| nvidia-smi | GPU 監視 | 🟡 | パース実装済み |
| Caddy | リバースプロキシ | 🟡 | 未統合 |
| rqlite | 分散データベース | ✅ | 統合済み |
| Headscale | VPN | 🟡 | Client 実装のみ |
| LVM | ストレージ管理 | 🟡 | 未統合 |
| cryptsetup | 暗号化 | 🟡 | 未統合 |

### ライブラリ依存関係

#### Go (Client)

```go
// 現在使用中
google.golang.org/grpc v1.65.0
github.com/hashicorp/memberlist v0.5.3
github.com/rqlite/gorqlite v0.0.0
modernc.org/sqlite v1.40.0

// 追加が必要
github.com/wasmerio/wasmer-go // Wasm 統合
github.com/gorilla/websocket  // ログストリーミング
github.com/prometheus/client_golang // メトリクス
```

#### Rust (Engine)

```toml
# 現在使用中
[dependencies]
tokio = "1"
tonic = "0.10"
wasmtime = "16.0"
nvml-wrapper = { version = "0.9", optional = true }

# 追加が必要
prometheus = "0.13"          # メトリクス
notify = "6"                 # ログファイル監視
```

---

## 優先度付けバックログ

### Sprint 1 (Week 1) - コンテナ実行基盤

| Task | 工数 | 担当 | 依存 |
|------|------|------|------|
| youki 統合実装 | 3d | Engine | - |
| OCI Bundle 生成 | 2d | Engine | youki |
| Capsule Manager 完成 | 2d | Engine | youki |
| Client Wasm 統合 | 2d | Client | - |
| 統合テスト | 1d | Test | All |

**Total**: 10 人日

---

### Sprint 2 (Week 2) - HTTP API

| Task | 工数 | 担当 | 依存 |
|------|------|------|------|
| CRUD エンドポイント | 3d | Client | - |
| 認証ミドルウェア | 1d | Client | - |
| OpenAPI 仕様書 | 1d | Docs | - |
| E2E テスト | 1d | Test | API |
| ドキュメント | 1d | Docs | - |

**Total**: 7 人日

---

### Sprint 3 (Week 3) - テスト・ドキュメント

| Task | 工数 | 担当 | 依存 |
|------|------|------|------|
| E2E テスト拡充 | 2d | Test | - |
| パフォーマンステスト | 1d | Test | - |
| QUICKSTART 更新 | 1d | Docs | - |
| DEPLOYMENT ガイド | 1d | Docs | - |
| TROUBLESHOOTING | 1d | Docs | - |

**Total**: 6 人日

---

### Sprint 4 (Week 4) - GPU 監視強化

| Task | 工数 | 担当 | 依存 |
|------|------|------|------|
| VRAM 計測実装 | 3d | Engine | - |
| プロセス紐付け | 2d | Engine | VRAM |
| 自動リソース回収 | 2d | Engine | 紐付け |
| Mock モード拡張 | 1d | Engine | - |
| テスト | 2d | Test | All |

**Total**: 10 人日

---

### Sprint 5 (Week 5) - スケジューラ最適化

| Task | 工数 | 担当 | 依存 |
|------|------|------|------|
| 追加フィルタ | 2d | Client | - |
| 追加スコアラー | 2d | Client | - |
| Dynamic Scheduling | 3d | Client | - |
| ポリシー設定 | 1d | Client | - |
| テスト | 2d | Test | All |

**Total**: 10 人日

---

### Sprint 6 (Week 6) - GPU テスト

| Task | 工数 | 担当 | 依存 |
|------|------|------|------|
| 負荷テスト | 2d | Test | - |
| カオステスト | 2d | Test | - |
| GPU ドキュメント | 2d | Docs | - |

**Total**: 6 人日

---

## サマリー

### 実装状況統計

| カテゴリ | 完成 | 部分実装 | 未実装 | 合計 |
|---------|------|----------|--------|------|
| Client パッケージ | 6 | 4 | 3 | 13 |
| Engine モジュール | 5 | 4 | 4 | 13 |
| 機能要件 | 8 | 12 | 10 | 30 |

### 工数見積もり

| Phase | 期間 | 工数 (人日) | 備考 |
|-------|------|------------|------|
| Phase 1 | Week 1-3 | 23 | コア機能完成 |
| Phase 2 | Week 4-6 | 26 | GPU 機能完成 |
| Phase 3 | Week 7-9 | 30 | 運用機能 |
| Phase 4 | Week 10-12 | 28 | HA・スケーラビリティ |
| Phase 5 | Week 13-14 | 14 | プロダクション準備 |
| **合計** | 14 週間 | **121 人日** | 約 6 ヶ月 (2 名体制) |

---

**最終更新**: 2025-11-15  
**次回レビュー**: Week 3 完了時
