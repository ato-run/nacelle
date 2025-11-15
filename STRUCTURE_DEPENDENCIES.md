# Capsuled 構造・依存関係分析

**バージョン:** 1.0.0  
**最終更新:** 2025-11-15  
**目的:** コードベース横断探索による構造と依存関係の可視化

---

## 📋 目次

1. [リポジトリ構造](#リポジトリ構造)
2. [コンポーネント間依存関係](#コンポーネント間依存関係)
3. [データフロー分析](#データフロー分析)
4. [モジュール依存グラフ](#モジュール依存グラフ)
5. [外部依存関係](#外部依存関係)
6. [統合ポイント](#統合ポイント)

---

## リポジトリ構造

### ディレクトリツリー

```
capsuled/
├── client/                          # Client (Go) - 8,461 LOC
│   ├── cmd/
│   │   └── client/                 # エントリポイント
│   │       └── main.go             # CLI 起動
│   ├── pkg/                        # パッケージ群
│   │   ├── api/                    # HTTP API ハンドラー
│   │   │   └── deploy_handler.go  # デプロイ API (470 LOC)
│   │   ├── config/                 # 設定管理
│   │   │   ├── config.go           # YAML パース (190 LOC)
│   │   │   └── config_test.go
│   │   ├── db/                     # データベース (rqlite)
│   │   │   ├── init.go             # 初期化
│   │   │   ├── state_manager.go    # 状態管理
│   │   │   ├── rqlite.go           # rqlite クライアント
│   │   │   ├── models.go           # データモデル
│   │   │   ├── node_store.go       # ノードストア
│   │   │   └── state_persistence.go
│   │   ├── gossip/                 # Memberlist (ノード検出)
│   │   │   ├── memberlist.go       # クラスタ管理 (250 LOC)
│   │   │   └── memberlist_test.go
│   │   ├── grpc/                   # gRPC サーバー
│   │   │   ├── server.go           # Coordinator サービス (400 LOC)
│   │   │   └── server_test.go
│   │   ├── headscale/              # Headscale VPN
│   │   │   ├── client.go           # HTTP クライアント (280 LOC)
│   │   │   └── client_test.go
│   │   ├── master/                 # Master 選出
│   │   │   ├── election.go         # 選出ロジック (450 LOC)
│   │   │   └── election_test.go
│   │   ├── proto/                  # 生成された gRPC コード
│   │   │   ├── coordinator.pb.go   # Coordinator プロトコル
│   │   │   ├── coordinator_grpc.pb.go
│   │   │   ├── engine.pb.go        # Engine プロトコル (レガシー)
│   │   │   └── engine_grpc.pb.go
│   │   ├── reconcile/              # 調整ループ
│   │   │   ├── reconciler.go       # Reconciler (500 LOC)
│   │   │   ├── store_rqlite.go     # rqlite ストア
│   │   │   └── reconciler_test.go
│   │   └── scheduler/              # スケジューラ
│   │       └── gpu/                # GPU-aware スケジューラ
│   │           ├── scheduler.go    # メインロジック (350 LOC)
│   │           ├── filters.go      # フィルタ関数 (280 LOC)
│   │           ├── scorers.go      # スコアリング関数 (200 LOC)
│   │           ├── types.go        # データ型
│   │           └── scheduler_test.go
│   ├── e2e/                        # E2E テスト
│   │   ├── gpu_simulation_test.go
│   │   └── agent_coordinator_integration_test.go
│   ├── go.mod                      # Go モジュール定義
│   ├── go.sum
│   ├── README.md
│   ├── QUICKSTART.md
│   ├── MIGRATION_SUMMARY.md
│   └── SECURITY_FIXES.md
│
├── engine/                          # Engine (Rust) - 6,072 LOC
│   ├── src/
│   │   ├── main.rs                 # エントリポイント (500 LOC)
│   │   ├── lib.rs                  # ライブラリルート
│   │   ├── grpc_server.rs          # gRPC サーバー (400 LOC)
│   │   ├── coordinator_service.rs  # Coordinator プロトコル (600 LOC)
│   │   ├── status_reporter.rs      # 状態レポート (800 LOC)
│   │   ├── capsule_manager.rs      # Capsule 管理 (600 LOC)
│   │   ├── wasm_host.rs            # Wasmtime ホスト (300 LOC)
│   │   ├── config.rs               # TOML 設定 (400 LOC)
│   │   ├── adep/                   # adep.json 処理
│   │   │   └── mod.rs              # Manifest パース (300 LOC)
│   │   ├── hardware/               # ハードウェア検出
│   │   │   ├── mod.rs              # モジュール定義
│   │   │   ├── gpu_detector.rs     # GPU 検出 (600 LOC)
│   │   │   ├── gpu_process_monitor.rs # プロセス監視 (350 LOC)
│   │   │   └── hardware_report.rs  # ハードウェアレポート (250 LOC)
│   │   ├── oci/                    # OCI 仕様
│   │   │   ├── mod.rs
│   │   │   └── spec_builder.rs     # config.json 生成 (400 LOC)
│   │   ├── runtime/                # ランタイム統合
│   │   │   └── mod.rs              # trait 定義 (200 LOC)
│   │   ├── proto/                  # 生成された gRPC コード
│   │   │   ├── mod.rs
│   │   │   ├── onescluster.coordinator.v1.rs
│   │   │   ├── onescluster.engine.v1.rs
│   │   │   └── onescluster.agent.v1.rs
│   │   └── bin/
│   │       └── status_reporter_driver.rs # テスト用ドライバ
│   ├── migrations/                 # SQL マイグレーション
│   ├── provisioning/               # プロビジョニング
│   │   ├── scripts/                # Python 自動化
│   │   │   ├── deploy.py
│   │   │   ├── cleanup.py
│   │   │   ├── manage_services.py
│   │   │   └── manage_api_keys.py
│   │   ├── systemd/                # systemd サービス
│   │   ├── INTEGRATION_SUMMARY.md
│   │   └── MIGRATION.md
│   ├── test-data/                  # テストデータ
│   ├── Cargo.toml                  # Rust 依存関係
│   ├── Cargo.lock
│   ├── build.rs                    # ビルドスクリプト (Proto 生成)
│   ├── config.toml.example
│   ├── .env.example
│   ├── README.md
│   └── PROJECT_OVERVIEW.md
│
├── adep-logic/                     # 共通ロジック (Wasm) - 200 LOC
│   ├── src/
│   │   └── lib.rs                  # Manifest バリデーション
│   ├── Cargo.toml
│   └── Cargo.lock
│
├── proto/                          # gRPC 定義
│   ├── coordinator.proto           # Coordinator プロトコル (130 LOC) — 推奨 (Canonical)
│   ├── engine.proto                # Engine プロトコル (52 LOC, レガシー, 非推奨)
│   ├── buf.yaml                    # buf 設定
│   └── buf.gen.yaml                # コード生成設定
│
├── tests/                          # 統合テスト
│   ├── e2e/
│   │   └── agent_coordinator_integration_test.go
│   └── integration/
│       ├── coordinator_test.go
│       └── README.md
│
├── docs/                           # ドキュメント
│   ├── CI_CD.md                    # CI/CD 設計
│   ├── CI_CD_ARCHITECTURE.md
│   └── gpu-mock-configuration.md   # GPU Mock 設定ガイド
│
├── .github/
│   └── workflows/
│       └── ci.yml                  # GitHub Actions ワークフロー
│
├── Makefile                        # ビルドスクリプト
├── README.md                       # プロジェクト概要
├── ARCHITECTURE.md                 # アーキテクチャドキュメント
├── CODE_REVIEW.md                  # コードレビュー記録
├── CAPSULED_ROADMAP.md             # 技術ロードマップ (このドキュメント作成)
├── CAPSULED_REQUIREMENTS_SUMMARY.md # 要件サマリー (このドキュメント作成)
└── .gitignore
```

### ファイル統計

| カテゴリ | ファイル数 | 総行数 | 言語 |
|---------|----------|--------|------|
| Client (Go) | 30+ | 8,461 | Go |
| Engine (Rust) | 25+ | 6,072 | Rust |
| adep-logic (Wasm) | 1 | 200 | Rust → Wasm |
| Proto 定義 | 2 | 182 | Protocol Buffers |
| テスト | 15+ | ~2,000 | Go, Rust |
| ドキュメント | 15+ | ~5,000 | Markdown |
| **合計** | **90+** | **~22,000** | - |

---

## コンポーネント間依存関係

### 高レベル依存グラフ

```
┌─────────────────────────────────────────────────────────┐
│                    外部クライアント                        │
│                  (CLI/Desktop/Web)                       │
└──────────────────────┬──────────────────────────────────┘
                       │ HTTPS REST API
                       ↓
┌──────────────────────────────────────────────────────────┐
│                   Client (Coordinator)                    │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐        │
│  │   Master   │←→│  Gossip    │←→│  Database  │        │
│  │  Election  │  │(Memberlist)│  │  (rqlite)  │        │
│  └────────────┘  └────────────┘  └────────────┘        │
│         ↓              ↓                                  │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐        │
│  │   HTTP     │  │ Reconciler │  │ Scheduler  │        │
│  │    API     │  │            │  │  (GPU)     │        │
│  └────────────┘  └────────────┘  └────────────┘        │
│         ↓              ↓              ↓                  │
│  ┌──────────────────────────────────────────┐           │
│  │         gRPC Client (Coordinator)        │           │
│  └──────────────────────────────────────────┘           │
└──────────────────────┬──────────────────────────────────┘
                       │ gRPC over HTTP/2
                       ↓
┌──────────────────────────────────────────────────────────┐
│                     Engine (Agent)                        │
│  ┌──────────────────────────────────────────┐           │
│  │         gRPC Server (Coordinator)        │           │
│  └──────────────────────────────────────────┘           │
│         ↓              ↓              ↓                  │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐        │
│  │  Capsule   │  │   Status   │  │    GPU     │        │
│  │  Manager   │  │  Reporter  │  │  Detector  │        │
│  └────────────┘  └────────────┘  └────────────┘        │
│         ↓              ↑              ↑                  │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐        │
│  │    OCI     │  │   Wasm     │  │  Hardware  │        │
│  │  Runtime   │  │   Host     │  │  Monitor   │        │
│  └────────────┘  └────────────┘  └────────────┘        │
│         ↓                                                │
│  ┌──────────────────────────────────────────┐           │
│  │    youki / runc (OCI Runtime)            │           │
│  └──────────────────────────────────────────┘           │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ↓
              ┌────────────────┐
              │   Container    │
              │   (Capsule)    │
              └────────────────┘
```

---

## データフロー分析

### 1. Capsule デプロイメントフロー

```
┌─────────────┐
│ 1. Client   │  POST /api/v1/capsules
│    Request  │  body: { manifest: {...}, image: "..." }
└──────┬──────┘
       │
       ↓
┌──────────────────────────────────────────┐
│ 2. Client (API Handler)                  │
│    - Manifest バリデーション (Wasm)       │  ❌ 未実装
│    - Workload ID 生成 (ULID)             │  ✅ 実装済み
└──────┬──────────────────────────────────┘
       │
       ↓
┌──────────────────────────────────────────┐
│ 3. Client (Scheduler)                    │
│    - 利用可能 Rig 取得 (from Database)   │  ✅ 実装済み
│    - Filter 実行 (VRAM, CUDA, GPU)       │  ✅ 実装済み
│    - Score 計算 (BestFit)                │  ✅ 実装済み
│    - 最適 Rig 選択                        │  ✅ 実装済み
└──────┬──────────────────────────────────┘
       │
       ↓
┌──────────────────────────────────────────┐
│ 4. Client (gRPC Client)                  │
│    - DeployWorkloadRequest 作成          │  ✅ 実装済み
│    - Engine へ送信                        │  ✅ 実装済み
└──────┬──────────────────────────────────┘
       │ gRPC
       ↓
┌──────────────────────────────────────────┐
│ 5. Engine (Coordinator Service)          │
│    - Request 受信                         │  ✅ 実装済み
│    - Manifest 変換 (Proto → Rust)        │  ✅ 実装済み
│    - Manifest 再検証 (Wasm)              │  ✅ 実装済み
└──────┬──────────────────────────────────┘
       │
       ↓
┌──────────────────────────────────────────┐
│ 6. Engine (Capsule Manager)              │
│    - Capsule 登録 (状態: Pending)        │  ✅ 実装済み
│    - デプロイタスク作成                   │  🟡 部分実装
└──────┬──────────────────────────────────┘
       │
       ↓
┌──────────────────────────────────────────┐
│ 7. Engine (OCI + Runtime)                │
│    - OCI Bundle 生成                     │  ❌ 未実装
│    - youki create                        │  ❌ 未実装
│    - youki start                         │  ❌ 未実装
│    - 状態更新 (Running)                  │  🟡 部分実装
└──────┬──────────────────────────────────┘
       │
       ↓
┌──────────────────────────────────────────┐
│ 8. Engine (Status Reporter)              │
│    - Capsule 状態収集                     │  ✅ 実装済み
│    - GPU 使用状況収集                     │  🟡 部分実装
│    - ReportStatus 送信                   │  ✅ 実装済み
└──────┬──────────────────────────────────┘
       │ gRPC
       ↓
┌──────────────────────────────────────────┐
│ 9. Client (Reconciler)                   │
│    - 状態受信・更新                       │  ✅ 実装済み
│    - 差分検出                             │  🟡 部分実装
│    - 修復アクション (future)              │  ❌ 未実装
└──────────────────────────────────────────┘
```

**実装状況**:
- ✅ 完成: 7/9 ステップ (78%)
- 🟡 部分実装: 3/9 ステップ
- ❌ 未実装: 2/9 ステップ (OCI Bundle 生成、youki 統合)

---

### 2. GPU スケジューリングフロー

```
┌─────────────────────────────────────────┐
│ 1. Engine (GPU Detector)                │
│    - nvidia-smi 実行 (Real)              │  ✅ 実装済み
│    - 環境変数読込 (Mock)                 │  ✅ 実装済み
│    - GpuInfo 構造体生成                  │  ✅ 実装済み
└──────┬──────────────────────────────────┘
       │
       ↓
┌─────────────────────────────────────────┐
│ 2. Engine (Hardware Report)             │
│    - RigHardwareReport 作成             │  ✅ 実装済み
│    - total_vram, used_vram 計算         │  🟡 部分実装
│    - CUDA version, driver version       │  ✅ 実装済み
└──────┬──────────────────────────────────┘
       │
       ↓
┌─────────────────────────────────────────┐
│ 3. Engine (Status Reporter)             │
│    - RigStatus 作成                      │  ✅ 実装済み
│    - ReportStatus RPC 送信              │  ✅ 実装済み
└──────┬──────────────────────────────────┘
       │ gRPC
       ↓
┌─────────────────────────────────────────┐
│ 4. Client (gRPC Server)                 │
│    - ReportStatus 受信                  │  ✅ 実装済み
│    - Database へ保存                     │  ✅ 実装済み
└──────┬──────────────────────────────────┘
       │
       ↓
┌─────────────────────────────────────────┐
│ 5. Client (Scheduler - Filter Stage)    │
│    - FilterHasGPU: GPU 存在確認         │  ✅ 実装済み
│    - FilterByVRAM: 空き VRAM 確認       │  ✅ 実装済み
│    - FilterByCudaVersion: CUDA 互換性   │  ✅ 実装済み
└──────┬──────────────────────────────────┘
       │
       ↓
┌─────────────────────────────────────────┐
│ 6. Client (Scheduler - Score Stage)     │
│    - ScoreByVRAMBinPacking: BestFit     │  ✅ 実装済み
│    - 追加スコアラー (future)             │  ❌ 未実装
│    - 重み付け合計                        │  ✅ 実装済み
└──────┬──────────────────────────────────┘
       │
       ↓
┌─────────────────────────────────────────┐
│ 7. Client (Scheduler - Select Stage)    │
│    - 最高スコア Rig 選択                 │  ✅ 実装済み
│    - タイブレーク処理                    │  ✅ 実装済み
└─────────────────────────────────────────┘
```

**実装状況**:
- ✅ 完成: 7/7 コアステップ (100%)
- 🟡 拡張機能: VRAM 詳細計測、追加スコアラー

---

### 3. 状態レポートフロー

```
       ┌─────────────┐
       │   Engine    │  (定期実行: 30秒間隔)
       └──────┬──────┘
              │
   ┌──────────┴──────────┐
   ↓                     ↓
┌────────────┐    ┌──────────────┐
│  Hardware  │    │   Capsule    │
│  Detector  │    │   Manager    │
└──────┬─────┘    └──────┬───────┘
       │                 │
       └────────┬────────┘
                ↓
       ┌─────────────────┐
       │ Status Reporter │
       │  - RigStatus    │
       │  - HardwareState│
       │  - Workloads    │
       └────────┬────────┘
                │ gRPC: ReportStatus
                ↓
       ┌─────────────────┐
       │ Client (gRPC)   │
       │  - Receive      │
       └────────┬────────┘
                │
                ↓
       ┌─────────────────┐
       │   Database      │
       │  - Update Node  │
       │  - Update State │
       └────────┬────────┘
                │
                ↓
       ┌─────────────────┐
       │   Reconciler    │
       │  - Diff Check   │
       │  - Repair(TODO) │
       └─────────────────┘
```

---

## モジュール依存グラフ

### Client (Go) 内部依存

```
main.go
  ↓
config → api ← db
  ↓       ↓     ↑
  ↓    grpc → proto
  ↓       ↓
  ↓    scheduler/gpu
  ↓       ↓
  ↓    reconcile
  ↓       ↑
  ↓       ↓
master ← gossip
  ↑       ↓
  └─── headscale (optional)
```

**依存関係詳細**:

| モジュール | 依存先 | 説明 |
|-----------|--------|------|
| `config` | - | 設定読込、他から参照される |
| `db` | `config` | 設定からDB接続情報取得 |
| `proto` | - | 自動生成、他から参照される |
| `grpc` | `proto`, `db`, `scheduler` | gRPC サーバー実装 |
| `api` | `proto`, `db`, `scheduler`, `grpc` | HTTP API 実装 |
| `scheduler` | `proto`, `db` | スケジューリングロジック |
| `reconcile` | `db`, `grpc` | 調整ループ |
| `gossip` | `config` | ノード検出 |
| `master` | `gossip`, `db` | Master 選出 |
| `headscale` | `config` | VPN 統合 (オプション) |

**循環依存**: なし ✅

---

### Engine (Rust) 内部依存

```
main.rs
  ↓
config → grpc_server → proto
  ↓         ↓
  ↓    coordinator_service
  ↓         ↓
  ↓    capsule_manager → adep
  ↓         ↓             ↓
  ↓    runtime ← oci  wasm_host
  ↓         ↓
  ↓    status_reporter
  ↓         ↑
  └────→ hardware
           ├── gpu_detector
           ├── gpu_process_monitor
           └── hardware_report
```

**依存関係詳細**:

| モジュール | 依存先 | 説明 |
|-----------|--------|------|
| `config` | - | 設定読込 |
| `proto` | - | 自動生成 |
| `adep` | - | Manifest 型定義 |
| `wasm_host` | - | Wasmtime ホスト |
| `hardware/*` | `config` | GPU 検出・監視 |
| `oci` | `adep` | OCI Spec 生成 |
| `runtime` | `oci` | ランタイム抽象化 |
| `capsule_manager` | `adep`, `runtime` | Capsule 管理 |
| `coordinator_service` | `proto`, `capsule_manager`, `runtime` | gRPC サービス |
| `status_reporter` | `proto`, `capsule_manager`, `hardware` | 状態レポート |
| `grpc_server` | `coordinator_service`, `status_reporter` | gRPC サーバー |

**循環依存**: なし ✅

---

## 外部依存関係

### Client (Go) 外部依存

#### 必須依存

```go
// gRPC 通信
google.golang.org/grpc v1.65.0
google.golang.org/protobuf v1.36.10

// 分散状態管理
github.com/rqlite/gorqlite v0.0.0

// クラスタ管理
github.com/hashicorp/memberlist v0.5.3

// ユーティリティ
github.com/oklog/ulid/v2 v2.1.0          // ID 生成
github.com/Masterminds/semver/v3 v3.4.0  // バージョン比較
gopkg.in/yaml.v3 v3.0.1                  // YAML パース
```

#### 今後追加が必要

```go
// Wasm 統合
github.com/wasmerio/wasmer-go v1.0.4

// WebSocket (ログストリーミング)
github.com/gorilla/websocket v1.5.0

// メトリクス
github.com/prometheus/client_golang v1.17.0
```

---

### Engine (Rust) 外部依存

#### 必須依存

```toml
# 非同期ランタイム
tokio = { version = "1", features = ["full"] }

# gRPC
tonic = "0.10"
prost = "0.12"
prost-types = "0.12"

# Wasm ランタイム
wasmtime = "16.0"

# シリアライズ
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# OCI 仕様
oci-spec = "0.6"

# 設定管理
toml = "0.8"

# エラーハンドリング
anyhow = "1.0"
thiserror = "1.0"
```

#### オプション依存

```toml
# GPU 検出 (real-gpu feature)
nvml-wrapper = { version = "0.9", optional = true }
```

#### 今後追加が必要

```toml
# メトリクス
prometheus = "0.13"

# ログファイル監視
notify = "6"

# ストレージ管理 (将来)
# lvm-sys2 = "0.2"
# libcryptsetup-rs = "0.9"
```

---

### システム依存

| ツール | 用途 | 必須 | 代替手段 |
|-------|------|------|---------|
| **youki** | OCI ランタイム | ✅ | runc |
| **runc** | OCI ランタイム (フォールバック) | ⚪ | youki |
| **nvidia-smi** | GPU 監視 | 🟡 | Mock モード |
| **Caddy** | リバースプロキシ | 🟡 | Nginx, Traefik |
| **rqlite** | 分散データベース | ✅ | etcd, Consul |
| **Headscale** | VPN | 🟡 | Tailscale, WireGuard |
| **LVM** | ストレージ管理 | 🟡 | Direct FS |
| **cryptsetup** | 暗号化 | 🟡 | なし |

---

## 統合ポイント

### 1. gRPC 通信 (Client ↔ Engine)

**プロトコル**: coordinator.proto (推奨)

**双方向通信**:
```
Client → Engine: DeployWorkloadRequest
Engine → Client: ReportStatus (定期)
```

**実装状況**:
- ✅ DeployWorkload RPC: 完成
- ✅ ReportStatus RPC: 完成
- ❌ StopWorkload RPC: 未実装
- ❌ LogStream RPC: 未実装

**課題**:
- engine.proto との二重管理
- エラーハンドリングの統一

---

### 2. Wasm 統合

**共有ロジック**: adep-logic.wasm

**統合状況**:

| コンポーネント | ランタイム | 統合状況 | 備考 |
|--------------|----------|----------|------|
| Client | Wasmer | ❌ 未実装 | バインディング未作成 |
| Engine | Wasmtime | ✅ 完成 | wasm_host.rs で使用中 |

**統合方法**:

```go
// Client (Go)
import "github.com/wasmerio/wasmer-go/wasmer"

func ValidateManifest(json []byte) (bool, error) {
    wasmBytes := loadWasmModule()
    instance := wasmer.NewInstance(wasmBytes)
    result := instance.Call("validate_manifest", json)
    return result, nil
}
```

```rust
// Engine (Rust)
use wasmtime::*;

pub fn validate_manifest(json: &[u8]) -> Result<bool> {
    let engine = Engine::default();
    let module = Module::from_file(&engine, "adep_logic.wasm")?;
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[])?;
    // Call validate_manifest
}
```

---

### 3. データベース統合

**Client**: rqlite (分散 SQLite)

**用途**:
- ノード情報 (RigGpuInfo)
- Capsule 状態
- Master 選出状態

**スキーマ** (想定):
```sql
-- nodes テーブル
CREATE TABLE nodes (
    rig_id TEXT PRIMARY KEY,
    address TEXT NOT NULL,
    hardware_state TEXT,  -- JSON
    last_seen_at INTEGER,
    is_master BOOLEAN
);

-- capsules テーブル
CREATE TABLE capsules (
    capsule_id TEXT PRIMARY KEY,
    rig_id TEXT,
    manifest TEXT,         -- JSON
    status TEXT,
    created_at INTEGER,
    FOREIGN KEY (rig_id) REFERENCES nodes(rig_id)
);
```

**実装状況**:
- ✅ rqlite クライアント
- ✅ Node ストア
- ✅ State 永続化
- 🟡 マイグレーション (一部)

---

### 4. Headscale 統合 (VPN)

**目的**: ノード間のセキュアな通信

**統合状況**:
- ✅ Client 実装 (`pkg/headscale/`)
- ❌ 自動デバイス登録
- ❌ E2E テスト

**統合シナリオ**:
```
1. Engine 起動時に Headscale に登録
2. Tailscale IP 取得
3. Client はこの IP で Engine と通信
```

---

## サマリー

### 依存関係の健全性

| カテゴリ | 評価 | 詳細 |
|---------|------|------|
| 循環依存 | ✅ Good | なし |
| 外部依存数 | ✅ Good | 適度 (Go: 15, Rust: 12) |
| システム依存 | 🟡 Moderate | youki, nvidia-smi に依存 |
| プロトコル統一 | ⚠️ Warning | engine.proto との二重管理 |
| Wasm 統合 | 🟡 Moderate | Client 側未完 |

### 推奨改善項目

1. **Proto 統合** (Priority: High)
   - coordinator.proto に統一
   - engine.proto を非推奨化

2. **Client Wasm 統合** (Priority: High)
   - Wasmer バインディング実装
   - adep-logic 呼び出し

3. **システム依存の抽象化** (Priority: Medium)
   - youki/runc の切り替え可能化
   - Caddy の抽象化

4. **テスト統合** (Priority: Medium)
   - E2E テスト拡充
   - Docker Compose による統合テスト環境

---

**最終更新**: 2025-11-15  
**次回レビュー**: Phase 1 完了時
