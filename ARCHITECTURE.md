# Capsuled Architecture

**最終更新:** 2025-11-15  
**バージョン:** 0.1.0

---

## 📋 目次

1. [概要](#概要)
2. [システムアーキテクチャ](#システムアーキテクチャ)
3. [コンポーネント詳細](#コンポーネント詳細)
4. [通信プロトコル](#通信プロトコル)
5. [データフロー](#データフロー)
6. [技術スタック](#技術スタック)
7. [ディレクトリ構造](#ディレクトリ構造)

---

## 概要

Capsuled は Personal Cloud OS のコア実装で、Capsule（原則: 1 Capsule = 1 Container）のデプロイメントと実行を行うランタイムです。

### 主な特徴

- **ローカル優先（互換/任意で分散）**: ローカル運用（SQLite）を優先しつつ、互換要件により分散構成（rqlite 等）も併用可能
- **マルチ言語実装**: Go (Client) + Rust (Engine) + Wasm (共通ロジック)
- **gRPC 通信**: クライアント・エンジン間の高効率な通信
- **Wasm ベースの共通ロジック**: adep.json のパース・バリデーションを Client/Engine で共有
- **GPU リソース管理**: NVIDIA GPU の検出、監視、スケジューリング

---

## システムアーキテクチャ

### 全体構成図

```
┌───────────────────────┐
│ Web UI / CLI           │
│ (capsuled/web, CLI)    │
└───────────┬───────────┘
      │ HTTP/gRPC
   ┌──────┴───────┐
   │ Coordinator   │ (Go)
   │ :8081 / :50050│
   └──────┬────────┘
      │ gRPC
   ┌──────┴───────┐
   │ Engine        │ (Rust)
   │ :4500 / :50051│
   └───────────────┘
```

### レイヤー構造

```
┌──────────────────────────────────────────┐
│          User Interface Layer            │
│     (CLI/API - capsule-cli / Web UI)     │
└────────────────┬─────────────────────────┘
                 │ HTTPS/REST
┌────────────────┴─────────────────────────┐
│       Control Plane (Client)             │
│  • Master Election                       │
│  • Scheduling & Coordination             │
│  • HTTP API Server                       │
│  • Wasmer (Wasm Execution)               │
└────────────────┬─────────────────────────┘
                 │ gRPC (Coordinator/Engine Protocol)
┌────────────────┴─────────────────────────┐
│         Execution Plane (Engine)         │
│  • gRPC Server                           │
│  • Container Runtime (youki/OCI)         │
│  • LVM/LUKS Storage Management           │
│  • Caddy Network Management              │
│  • Wasmtime (Wasm Execution)             │
│  • GPU Detection & Monitoring            │
└──────────────────────────────────────────┘
```

---

## コンポーネント詳細

### 1. Client (Go)

**場所**: `client/`  
**言語**: Go  
**役割**: Control Plane - クラスターの調整とスケジューリング

#### 責務

- **Master 選出**: 分散環境での Master ノード選出
- **スケジューリング**: Capsule を適切な Engine に配置
- **HTTP API サーバー**: 外部クライアントからのリクエスト受付
- **Wasm 実行**: Wasmer による adep-logic.wasm の実行
- **状態管理**: ローカル SQLite（互換/任意で rqlite 併用）

#### 主要パッケージ

```
client/pkg/
├── api/           # HTTP API ハンドラー
├── grpc/          # gRPC サーバー実装
├── proto/         # 生成された gRPC コード
├── master/        # Master 選出ロジック
├── scheduler/     # スケジューリングエンジン
├── store/         # ローカル SQLite ストア
├── db/            # 互換/任意の DB 抽象（rqlite 等）
├── reconcile/     # 調整ループ
├── headscale/     # Headscale VPN 統合
├── gossip/        # ノード間通信
└── config/        # 設定管理
```

#### HTTP API エンドポイント（想定）

- `POST /api/v1/capsules` - Capsule デプロイ
- `GET /api/v1/capsules/:id` - Capsule 状態取得
- `DELETE /api/v1/capsules/:id` - Capsule 削除
- `GET /api/v1/nodes` - Engine ノード一覧
- `GET /health` - ヘルスチェック

---

### 2. Engine (Rust)

**場所**: `engine/`  
**言語**: Rust  
**役割**: Execution Plane - コンテナの実行と管理

#### 責務

- **gRPC サーバー**: Client からの指示を受信
- **コンテナ実行**: youki による OCI コンテナ実行
- **ストレージ管理**: LVM/LUKS による暗号化ストレージ
- **ネットワーク管理**: Caddy によるリバースプロキシ設定
- **Wasm 実行**: Wasmtime による adep-logic.wasm の実行
- **ハードウェア監視**: GPU 検出・VRAM 監視

#### 主要モジュール

```
engine/src/
├── grpc_server.rs          # gRPC サービス実装
├── coordinator_service.rs  # Coordinator プロトコル実装
├── capsule_manager.rs      # Capsule ライフサイクル管理
├── wasm_host.rs           # Wasmtime ホスト環境
├── status_reporter.rs     # 状態レポート送信
├── adep/                  # adep.json 処理
├── runtime/               # コンテナランタイム統合
├── oci/                   # OCI 仕様実装
├── hardware/              # GPU 検出・監視
│   ├── gpu_detector.rs
│   ├── gpu_process_monitor.rs
│   └── hardware_report.rs
└── proto/                 # 生成された gRPC コード
```

#### gRPC サービス

**Engine Service** (`engine.proto`)
```protobuf
service Engine {
  rpc DeployCapsule(DeployRequest) returns (DeployResponse);
  rpc StopCapsule(StopRequest) returns (StopResponse);
  rpc GetResources(GetResourcesRequest) returns (ResourceInfo);
  rpc ValidateManifest(ValidateRequest) returns (ValidationResult);
}
```

**Coordinator Service** (`coordinator.proto`)
```protobuf
service Coordinator {
  rpc ReportStatus(StatusReportRequest) returns (StatusReportResponse);
  rpc DeployWorkload(DeployWorkloadRequest) returns (DeployWorkloadResponse);
}
```

---

### 3. adep-logic (Rust → Wasm)

**場所**: `adep-logic/`  
**言語**: Rust (コンパイル先: Wasm32)  
**役割**: 共通ロジック - Client と Engine で共有

#### 責務

- **adep.json パース**: マニフェストファイルの解析
- **バリデーション**: スキーマ検証、制約チェック
- **プラットフォーム非依存**: Client (Wasmer) と Engine (Wasmtime) の両方で動作

#### ビルド成果物

- `adep_logic.wasm` (~72KB)
- Client/Engine の両方に埋め込まれて使用

#### 利用パターン

```rust
// Engine (Wasmtime) での使用例
let wasm_bytes = include_bytes!("../wasm/adep_logic.wasm");
let result = wasmtime_host.validate_manifest(adep_json)?;

// Client (Wasmer) での使用例
let wasm_bytes = include_bytes!("../wasm/adep_logic.wasm");
let result = wasmer_instance.validate(adep_json)?;
```

---

### 4. Proto Definitions (gRPC)

**場所**: `proto/`  
**ツール**: buf  
**言語**: Protocol Buffers v3

#### 定義ファイル

- `engine.proto` - Engine サービス定義（レガシー、互換性のために保持）
- `coordinator.proto` - Coordinator サービス定義（**推奨**: より包括的な設計）

**プロトコル使用ガイドライン**:

現在、プロジェクトには2つの gRPC プロトコル定義が存在します：

1. **engine.proto** (旧設計)
   - シンプルな Capsule デプロイメント用
   - `DeployCapsule`, `StopCapsule` などの基本操作
   - 初期実装との後方互換性のために保持

2. **coordinator.proto** (新設計) ⭐ **推奨**
   - より包括的な Workload 管理
   - GPU/ハードウェア状態レポート機能
   - スケジューリング情報（Taint/Toleration）
   - 厳密に型付けされた AdePManifest スキーマ

**推奨事項**:
- 新規開発では `coordinator.proto` を使用
- `engine.proto` は段階的に非推奨化を検討
- 両プロトコルの統合を長期的な目標とする

#### コード生成

```bash
cd proto
buf generate
```

生成先:
- Go: `client/pkg/proto/`
- Rust: `engine/src/proto/`

---

## 通信プロトコル

### Client ↔ Engine 通信

**プロトコル**: gRPC over HTTP/2  
**認証**: API Key (想定)  
**通信フロー**:

```
1. Client → Engine: DeployWorkloadRequest
   {
     workload_id: "capsule-123",
     manifest: AdePManifest { ... }
   }

2. Engine → Client: DeployWorkloadResponse
   {
     success: true,
     message: "Deployed successfully"
   }

3. Engine → Client: ReportStatus (定期)
   {
     status: RigStatus {
       rig_id: "engine-001",
       hardware: HardwareState { gpus: [...] },
       running_workloads: [...]
     }
   }
```

### Wasm ホスト通信

**Client**:
- ランタイム: Wasmer
- 言語: Go → Wasm (CGO 経由)

**Engine**:
- ランタイム: Wasmtime
- 言語: Rust → Wasm (ネイティブ統合)

---

## データフロー

### Capsule デプロイメントフロー

```
┌──────────┐
│ユーザー    │ POST /api/v1/capsules
└─────┬────┘       ↓
      │      ┌──────────────┐
      │      │ Client (API) │
      │      └──────┬───────┘
      │             │ 1. adep.json バリデーション (Wasmer)
      │             │ 2. スケジューラによる Engine 選択
      │             ↓
      │      ┌──────────────┐
      │      │ Scheduler    │ GPU/VRAM/Taint を考慮
      │      └──────┬───────┘
      │             │ gRPC: DeployWorkload
      │             ↓
      │      ┌──────────────┐
      └─────→│ Engine       │
             └──────┬───────┘
                    │ 1. adep.json 再検証 (Wasmtime)
                    │ 2. OCI bundle 準備
                    │ 3. youki create & start
                    │ 4. GPU プロセス監視開始
                    │ 5. Caddy ルート設定
                    ↓
             ┌──────────────┐
             │ Container    │ https://capsule-123.rig.example.com
             └──────────────┘
```

### 状態レポートフロー

```
┌──────────────┐
│ Engine       │ (タイマーで定期実行)
└──────┬───────┘
       │ 1. GPU 状態取得 (nvidia-smi)
       │ 2. 実行中 Capsule 一覧
       │ 3. VRAM 使用量測定
       ↓
┌──────────────┐
│ Coordinator  │ gRPC: ReportStatus
│ Service      │
└──────┬───────┘
       │ RigStatus {
       │   rig_id, hardware, workloads
       │ }
       ↓
┌──────────────┐
│ Client       │
│ (Reconciler) │ スケジューリング判断に使用
└──────────────┘
```

---

## 技術スタック

### Client (Go)

| カテゴリ | ライブラリ/ツール | 用途 |
|---------|-----------------|------|
| gRPC | google.golang.org/grpc | Engine との通信 |
| Wasm | wasmer-go | adep-logic.wasm 実行 |
| データベース | rqlite | 分散状態管理 |
| HTTP | net/http, gorilla/mux (想定) | REST API サーバー |
| VPN | headscale client | Tailscale 互換 VPN |
| ロギング | log/slog (想定) | 構造化ログ |

### Engine (Rust)

| カテゴリ | ライブラリ | 用途 |
|---------|----------|------|
| gRPC | tonic | gRPC サーバー/クライアント |
| Wasm | wasmtime | adep-logic.wasm 実行 |
| 非同期 | tokio | 非同期ランタイム |
| シリアライズ | serde, serde_json | JSON/Protobuf 処理 |
| OCI | oci-spec | OCI 仕様サポート |
| コンテナ | youki (外部) | OCI ランタイム |
| プロキシ | Caddy Admin API | リバースプロキシ管理 |
| GPU | nvidia-smi (外部) | GPU 監視 |
| ストレージ | LVM/LUKS (外部) | 暗号化ストレージ |

### adep-logic (Rust → Wasm)

| カテゴリ | ライブラリ | 用途 |
|---------|----------|------|
| シリアライズ | serde, serde_json | JSON パース |
| バリデーション | validator (想定) | スキーマ検証 |
| ターゲット | wasm32-unknown-unknown | Wasm コンパイル |

### Proto (gRPC)

| ツール | 用途 |
|--------|------|
| buf | Protocol Buffers 管理 |
| protoc | コード生成 |
| protoc-gen-go | Go コード生成 |
| prost | Rust コード生成 |

---

## ディレクトリ構造

```
capsuled/
├── client/                    # Client (Go)
│   ├── cmd/
│   │   └── client/           # メインエントリポイント
│   ├── pkg/
│   │   ├── api/              # HTTP API ハンドラー
│   │   ├── grpc/             # gRPC サーバー
│   │   ├── proto/            # 生成コード (Go)
│   │   ├── master/           # Master 選出
│   │   ├── scheduler/        # スケジューラ
│   │   ├── db/               # rqlite 統合
│   │   ├── reconcile/        # 調整ループ
│   │   ├── headscale/        # VPN 統合
│   │   ├── gossip/           # ノード通信
│   │   └── config/           # 設定管理
│   ├── e2e/                  # E2E テスト
│   ├── go.mod
│   ├── go.sum
│   ├── README.md
│   ├── QUICKSTART.md
│   ├── MIGRATION_SUMMARY.md
│   └── SECURITY_FIXES.md
│
├── engine/                    # Engine (Rust)
│   ├── src/
│   │   ├── main.rs           # メインエントリポイント
│   │   ├── grpc_server.rs    # gRPC サーバー実装
│   │   ├── coordinator_service.rs
│   │   ├── capsule_manager.rs
│   │   ├── wasm_host.rs      # Wasmtime ホスト
│   │   ├── status_reporter.rs
│   │   ├── adep/             # adep.json 処理
│   │   ├── runtime/          # コンテナランタイム
│   │   ├── oci/              # OCI 仕様
│   │   ├── hardware/         # GPU 検出・監視
│   │   └── proto/            # 生成コード (Rust)
│   ├── migrations/           # データベースマイグレーション
│   ├── provisioning/         # プロビジョニングスクリプト
│   ├── test-data/            # テストデータ
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── build.rs
│   ├── README.md
│   └── PROJECT_OVERVIEW.md
│
├── adep-logic/               # 共通ロジック (Rust → Wasm)
│   ├── src/
│   │   └── lib.rs            # Wasm エントリポイント
│   ├── Cargo.toml
│   └── Cargo.lock
│
├── proto/                    # gRPC 定義
│   ├── engine.proto          # Engine サービス
│   ├── coordinator.proto     # Coordinator サービス
│   ├── buf.yaml              # buf 設定
│   ├── buf.gen.yaml          # コード生成設定
│   └── client/               # 生成先 (Go)
│
├── tests/                    # 統合テスト
│   └── integration/
│
├── docs/                     # ドキュメント
│   ├── CI_CD.md
│   ├── CI_CD_ARCHITECTURE.md
│   └── gpu-mock-configuration.md
│
├── Makefile                  # ビルドスクリプト
├── README.md                 # プロジェクト概要
├── ARCHITECTURE.md           # このファイル
└── .gitignore
```

---

## 設計原則

### 1. 関心の分離

- **Client**: スケジューリングとコーディネーション
- **Engine**: 実行と低レベル管理
- **adep-logic**: プラットフォーム非依存のロジック

### 2. 言語選択の理由

- **Go (Client)**: 高速な開発、優れた並行処理、HTTP/gRPC サポート
- **Rust (Engine)**: メモリ安全性、システムレベル操作、パフォーマンス
- **Wasm**: ポータビリティ、サンドボックス化、言語間共有

### 3. gRPC の利点

- 型安全な通信
- HTTP/2 による多重化
- ストリーミングサポート
- 言語間互換性

### 4. セキュリティ考慮事項

- API Key 認証
- Headscale VPN によるノード間通信の暗号化
- LUKS によるストレージ暗号化
- Wasm サンドボックスによる安全な共有ロジック実行

---

## 今後の拡張

### Phase 1: 基盤強化 (進行中)

- [ ] Master 選出の安定化
- [ ] スケジューラアルゴリズムの改善
- [ ] GPU リソース管理の完成

### Phase 2: 運用機能

- [ ] ログストリーミング (WebSocket)
- [ ] メトリクス収集 (Prometheus)
- [ ] ヘルスチェックと自動復旧

### Phase 3: スケーラビリティ

- [ ] 水平スケーリング
- [ ] マルチリージョン対応
- [ ] 高可用性構成

---

**最終更新**: 2025-11-15  
**次回レビュー**: Phase 1 完了時
