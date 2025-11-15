# Capsuled 実装 TODO リスト

**バージョン:** 1.0.0  
**最終更新:** 2025-11-15  
**ベース:** CAPSULED_ROADMAP.md

このファイルは CAPSULED_ROADMAP.md の 14週間計画を実装可能な TODO に分解したものです。

---

## 📋 凡例

- ✅ **完了**: 実装とテスト完了
- 🚧 **進行中**: 実装中
- ⏳ **予定**: 未着手
- 🔴 **Critical**: 最優先で対応必須
- 🟡 **High**: 優先度高
- 🟢 **Medium**: 優先度中
- ⚪ **Low**: 優先度低

---

## Phase 1: 基盤強化 (Week 1-3)

### Week 1: コンテナランタイム統合

#### 🔴 Task 1.1: youki 統合完成 (Engine)
**工数**: 3日  
**優先度**: Critical  
**担当**: Rust Engineer

- ⏳ `engine/src/runtime/youki.rs` 実装
  - ⏳ `create_container()` - youki create コマンド実行
  - ⏳ `start_container()` - youki start コマンド実行
  - ⏳ `delete_container()` - youki delete コマンド実行
  - ⏳ `container_state()` - youki state コマンド実行
  - ⏳ エラーハンドリング (コマンド失敗、タイムアウト)
  - ⏳ ログ出力の構造化

- ⏳ youki バイナリパス設定
  - ⏳ config.toml に youki_path 追加
  - ⏳ 環境変数 YOUKI_PATH でオーバーライド可能に

- ⏳ 単体テスト追加
  - ⏳ Mock 実装 (trait-based)
  - ⏳ コマンド実行テスト
  - ⏳ エラーケーステスト

**依存関係**: なし  
**成果物**: youki でコンテナの作成・起動・削除が可能

---

#### 🔴 Task 1.2: OCI Bundle 生成ロジック (Engine)
**工数**: 2日  
**優先度**: Critical  
**担当**: Rust Engineer

- ⏳ `engine/src/oci/bundle.rs` 追加
  - ⏳ `create_bundle()` - Bundle ディレクトリ作成
  - ⏳ `generate_config_json()` - config.json 生成 (spec_builder.rs 活用)
  - ⏳ `prepare_rootfs()` - rootfs 準備
  - ⏳ `cleanup_bundle()` - Bundle クリーンアップ

- ⏳ config.json テンプレート拡張
  - ⏳ GPU デバイスマウント (`/dev/nvidia*`)
  - ⏳ 環境変数設定
  - ⏳ ボリュームマウント
  - ⏳ ネットワーク設定

- ⏳ テスト追加
  - ⏳ Bundle 生成テスト
  - ⏳ config.json バリデーション
  - ⏳ rootfs 準備テスト

**依存関係**: Task 1.1  
**成果物**: OCI 準拠の Bundle が生成可能

---

#### 🔴 Task 1.3: Capsule Manager 完成 (Engine)
**工数**: 2日  
**優先度**: Critical  
**担当**: Rust Engineer

- ⏳ `engine/src/capsule_manager.rs` 拡張
  - ⏳ デプロイメントフロー実装
    - ⏳ Pending → Creating → Running 状態遷移
    - ⏳ PID 追跡 (youki state から取得)
    - ⏳ ログファイルパス管理
  - ⏳ エラーリカバリ
    - ⏳ 起動失敗時の状態更新 (Failed)
    - ⏳ リソースクリーンアップ (Bundle 削除)
  - ⏳ 停止・削除フロー
    - ⏳ Running → Stopping → Stopped
    - ⏳ youki delete 実行

- ⏳ 統合テスト追加
  - ⏳ 単純コンテナ起動テスト
  - ⏳ 環境変数、ボリュームマウントテスト
  - ⏳ GPU デバイスアクセステスト (Mock)

**依存関係**: Task 1.1, Task 1.2  
**成果物**: Capsule のライフサイクル管理完成

---

#### 🔴 Task 1.4: Client Wasm 統合 (Client) ✅ **完了**
**工数**: 2日  
**優先度**: Critical  
**担当**: Go Engineer

- ✅ `client/pkg/wasm/` パッケージ追加
  - ✅ `wasmer.go` - Wasmer バインディング実装
  - ✅ `NewWasmerHost()` - Wasm モジュールロード
  - ✅ `ValidateManifest()` - adep-logic 呼び出し
  - ✅ エラーハンドリング

- ✅ wasmer-go 依存関係追加
  - ✅ `go.mod` に追加: `github.com/wasmerio/wasmer-go v1.0.4`
  - ✅ adep_logic.wasm バイナリの埋め込み

- ✅ API Handler への統合
  - ✅ `pkg/api/deploy_handler.go` 更新
  - ✅ マニフェストバリデーション追加

- ✅ テスト追加
  - ✅ Wasm ロードテスト
  - ✅ バリデーション成功・失敗テスト (5 tests, all passing)

**依存関係**: なし  
**成果物**: Client 側で adep.json バリデーション可能 ✅

---

#### 🟡 Task 1.5: 統合テスト (tests/)
**工数**: 1日  
**優先度**: High  
**担当**: QA + Both Engineers

- ⏳ `tests/e2e/deployment_test.go` 追加
  - ⏳ Client → Engine → Container のエンドツーエンドテスト
  - ⏳ テストフィクスチャ (adep.json, Docker イメージ)
  - ⏳ 成功ケース、失敗ケース

- ⏳ CI 統合
  - ⏳ `.github/workflows/ci.yml` 更新
  - ⏳ E2E テストジョブ追加

**依存関係**: Task 1.1, 1.2, 1.3, 1.4  
**成果物**: E2E テスト成功

---

### Week 2: HTTP API 完成

#### 🔴 Task 2.1: CRUD エンドポイント実装 (Client) 🚧 **70% 完了**
**工数**: 3日  
**優先度**: Critical  
**担当**: Go Engineer

- ✅ `client/pkg/api/capsule_handler.go` 作成
  - ✅ `HandleGetCapsule()` - GET /api/v1/capsules/:id (placeholder)
  - ✅ `HandleListCapsules()` - GET /api/v1/capsules (placeholder)
  - ✅ `HandleDeleteCapsule()` - DELETE /api/v1/capsules/:id (placeholder)

- ✅ `client/pkg/api/node_handler.go` 作成
  - ✅ `HandleListNodes()` - GET /api/v1/nodes (fully functional)
  - ✅ `HandleGetNode()` - GET /api/v1/nodes/:id (placeholder)

- ✅ `client/pkg/api/health_handler.go` 作成
  - ✅ `HandleHealth()` - GET /health
  - ✅ `HandleReadiness()` - GET /ready
  - ✅ `HandleLiveness()` - GET /live

- ✅ エラーハンドリング
  - ✅ 標準エラーレスポンス (JSON)
  - ✅ HTTP ステータスコード統一

- ✅ テスト追加
  - ✅ Health handler テスト (4 tests, all passing)
  - ⏳ Capsule handler テスト (requires CapsuleStore)
  - ⏳ Node handler テスト

**依存関係**: Task 1.3  
**成果物**: REST API で Capsule/Node 管理可能 (部分実装)

---

#### 🟡 Task 2.2: 認証・認可 (Client)
**工数**: 1日  
**優先度**: High  
**担当**: Go Engineer

- ⏳ `client/pkg/api/middleware/auth.go` 追加
  - ⏳ API Key 認証ミドルウェア
  - ⏳ `X-API-Key` ヘッダー検証
  - ⏳ 無効な Key の拒否

- ⏳ API Key 管理
  - ⏳ config.yaml に api_keys リスト
  - ⏳ 環境変数 API_KEYS でオーバーライド

- ⏳ テスト追加
  - ⏳ 認証成功・失敗テスト

**依存関係**: なし  
**成果物**: API Key 認証機能

---

#### 🟡 Task 2.3: OpenAPI 仕様書 (Docs)
**工数**: 1日  
**優先度**: High  
**担当**: Go Engineer

- ⏳ `docs/openapi.yaml` 作成
  - ⏳ 全エンドポイントの定義
  - ⏳ リクエスト/レスポンススキーマ
  - ⏳ 認証スキーマ (API Key)

- ⏳ (オプション) Swagger UI 統合
  - ⏳ `/api/docs` エンドポイント
  - ⏳ swagger-ui 埋め込み

**依存関係**: Task 2.1  
**成果物**: API 仕様書完成

---

#### 🟢 Task 2.4: E2E テスト (tests/)
**工数**: 1日  
**優先度**: Medium  
**担当**: QA

- ⏳ `tests/e2e/api_test.go` 追加
  - ⏳ API エンドポイントテスト
  - ⏳ 認証テスト

**依存関係**: Task 2.1, 2.2  
**成果物**: API E2E テスト成功

---

#### 🟢 Task 2.5: ドキュメント (Docs)
**工数**: 1日  
**優先度**: Medium  
**担当**: Both Engineers

- ⏳ `docs/API_REFERENCE.md` 作成
  - ⏳ エンドポイント一覧
  - ⏳ リクエスト例
  - ⏳ レスポンス例

**依存関係**: Task 2.1  
**成果物**: API ドキュメント

---

### Week 3: 統合テスト・ドキュメント

#### 🟡 Task 3.1: E2E テスト拡充 (tests/)
**工数**: 2日  
**優先度**: High  
**担当**: QA + Both Engineers

- ⏳ `tests/e2e/gpu_scheduling_test.go` 拡張
  - ⏳ VRAM 不足シナリオ
  - ⏳ 複数 Capsule スケジューリング
  - ⏳ GPU タイプフィルタリング

- ⏳ `tests/e2e/master_failover_test.go` 追加
  - ⏳ Master ダウンシミュレーション
  - ⏳ 自動選出テスト

**依存関係**: Week 1-2 完了  
**成果物**: E2E テストカバレッジ 50%+

---

#### 🟢 Task 3.2: パフォーマンステスト (tests/)
**工数**: 1日  
**優先度**: Medium  
**担当**: QA

- ⏳ `tests/performance/load_test.go` 追加
  - ⏳ 負荷テストスクリプト
  - ⏳ ベンチマーク (k6 または Go bench)

- ⏳ ベースライン測定
  - ⏳ API レスポンスタイム
  - ⏳ スケジューリング遅延
  - ⏳ gRPC 通信遅延

**依存関係**: Week 2 完了  
**成果物**: パフォーマンスベースライン確立

---

#### 🟡 Task 3.3: ドキュメント整備 (Docs)
**工数**: 3日  
**優先度**: High  
**担当**: Both Engineers

- ⏳ `QUICKSTART.md` 更新
  - ⏳ 最新の API を反映
  - ⏳ コンテナデプロイ例

- ⏳ `docs/DEPLOYMENT.md` 作成
  - ⏳ systemd サービス設定
  - ⏳ 環境変数一覧
  - ⏳ 依存ツールインストール

- ⏳ `docs/TROUBLESHOOTING.md` 作成
  - ⏳ よくある問題と解決策
  - ⏳ ログの見方
  - ⏳ デバッグ方法

**依存関係**: Week 1-2 完了  
**成果物**: 運用ドキュメント一式

---

## Phase 2: GPU機能完成 (Week 4-6)

### Week 4: GPU プロセス監視強化

#### 🟡 Task 4.1: VRAM 計測実装 (Engine)
**工数**: 3日  
**優先度**: High  
**担当**: Rust Engineer

- ⏳ `engine/src/hardware/gpu_process_monitor.rs` 拡張
  - ⏳ nvidia-smi 詳細パース
    - ⏳ プロセス単位の VRAM 使用量
    - ⏳ GPU 使用率
    - ⏳ 温度・電力
  - ⏳ `get_vram_usage()` - Capsule ID → VRAM 計測
  - ⏳ `track_process()` - Capsule ↔ PID 紐付け

- ⏳ Mock モード拡張
  - ⏳ 動的 VRAM 使用シミュレーション
  - ⏳ プロセス紐付けエミュレーション

- ⏳ テスト追加
  - ⏳ VRAM 計測テスト
  - ⏳ Mock モードテスト

**依存関係**: Task 1.3  
**成果物**: VRAM 使用量をリアルタイム監視

---

#### 🟢 Task 4.2: 自動リソース回収 (Engine)
**工数**: 2日  
**優先度**: Medium  
**担当**: Rust Engineer

- ⏳ `engine/src/hardware/gpu_process_monitor.rs` 拡張
  - ⏳ `cleanup_terminated()` - Zombie プロセスクリーンアップ
  - ⏳ プロセス終了検知
  - ⏳ VRAM 解放確認

- ⏳ テスト追加
  - ⏳ クリーンアップテスト

**依存関係**: Task 4.1  
**成果物**: 自動リソース回収機能

---

#### 🟢 Task 4.3: Status Reporter 強化 (Engine)
**工数**: 1日  
**優先度**: Medium  
**担当**: Rust Engineer

- ⏳ `engine/src/status_reporter.rs` 拡張
  - ⏳ 詳細 VRAM 情報レポート
  - ⏳ エラー状態レポート
  - ⏳ レポート間隔設定 (config.toml)

**依存関係**: Task 4.1  
**成果物**: 詳細な状態レポート

---

### Week 5: GPU スケジューリング最適化

#### 🟡 Task 5.1: 追加フィルタ実装 (Client)
**工数**: 2日  
**優先度**: High  
**担当**: Go Engineer

- ⏳ `client/pkg/scheduler/gpu/filters.go` 拡張
  - ⏳ `FilterByGpuModel()` - GPU モデルフィルタ
  - ⏳ `FilterByTaint()` - Taint/Toleration フィルタ

- ⏳ テスト追加
  - ⏳ フィルタテスト

**依存関係**: なし  
**成果物**: 追加フィルタ実装

---

#### 🟡 Task 5.2: 追加スコアラー実装 (Client)
**工数**: 2日  
**優先度**: High  
**担当**: Go Engineer

- ⏳ `client/pkg/scheduler/gpu/scorers.go` 拡張
  - ⏳ `ScoreByLoadBalancing()` - ノード負荷分散スコア
  - ⏳ `ScoreByGpuTemperature()` - GPU 温度スコア
  - ⏳ 重み付け設定可能化 (config.yaml)

- ⏳ テスト追加
  - ⏳ スコアラーテスト

**依存関係**: なし  
**成果物**: 追加スコアラー実装

---

#### 🟡 Task 5.3: Dynamic Scheduling (Client)
**工数**: 3日  
**優先度**: High  
**担当**: Go Engineer

- ⏳ `client/pkg/scheduler/gpu/scheduler.go` 拡張
  - ⏳ リスケジューリング機能
  - ⏳ GPU 解放検知と再スケジュール
  - ⏳ 優先度ベーススケジューリング

- ⏳ テスト追加
  - ⏳ リスケジューリングテスト

**依存関係**: Task 4.1, 4.3  
**成果物**: 動的スケジューリング機能

---

#### 🟢 Task 5.4: スケジューリングポリシー (Client)
**工数**: 1日  
**優先度**: Medium  
**担当**: Go Engineer

- ⏳ ポリシー設定
  - ⏳ BestFit (デフォルト)
  - ⏳ LeastAllocated (負荷分散)
  - ⏳ Custom ポリシープラグイン設計

**依存関係**: Task 5.1, 5.2  
**成果物**: ポリシー設定可能

---

### Week 6: GPU 機能テスト

#### 🟡 Task 6.1: 負荷テスト (tests/)
**工数**: 2日  
**優先度**: High  
**担当**: QA

- ⏳ `tests/e2e/gpu_load_test.go` 追加
  - ⏳ 大量 Capsule デプロイ
  - ⏳ GPU 競合シナリオ
  - ⏳ VRAM 不足時の挙動

**依存関係**: Week 4-5 完了  
**成果物**: 負荷テスト成功

---

#### 🟡 Task 6.2: カオステスト (tests/)
**工数**: 2日  
**優先度**: High  
**担当**: QA

- ⏳ `tests/e2e/gpu_chaos_test.go` 追加
  - ⏳ Engine クラッシュシミュレーション
  - ⏳ ネットワーク分断テスト
  - ⏳ GPU 故障シミュレーション

**依存関係**: Week 4-5 完了  
**成果物**: カオステスト成功

---

#### 🟢 Task 6.3: GPU ドキュメント (Docs)
**工数**: 2日  
**優先度**: Medium  
**担当**: Go Engineer

- ⏳ `docs/GPU_SCHEDULING_GUIDE.md` 作成
  - ⏳ GPU スケジューリング戦略
  - ⏳ フィルタ・スコアラーの使い方
  - ⏳ ポリシー設定例

- ⏳ `docs/GPU_TROUBLESHOOTING.md` 作成
  - ⏳ GPU 関連トラブルシューティング

**依存関係**: Week 4-5 完了  
**成果物**: GPU ドキュメント完成

---

## Phase 3: 運用機能実装 (Week 7-9)

### Week 7: ログストリーミング

#### 🟡 Task 7.1: ログ収集 (Engine)
**工数**: 3日  
**優先度**: High  
**担当**: Rust Engineer

- ⏳ `engine/src/logs/collector.rs` 追加
  - ⏳ コンテナログファイル監視 (inotify)
  - ⏳ stdout/stderr キャプチャ
  - ⏳ ログローテーション

- ⏳ 依存関係追加
  - ⏳ `notify = "6"` (Cargo.toml)

**依存関係**: Task 1.3  
**成果物**: ログ収集機能

---

#### 🟡 Task 7.2: WebSocket API (Client)
**工数**: 3日  
**優先度**: High  
**担当**: Go Engineer

- ⏳ `client/pkg/api/logs_handler.go` 追加
  - ⏳ `StreamLogsHandler()` - WS /api/v1/capsules/:id/logs
  - ⏳ リアルタイムストリーミング
  - ⏳ 履歴ログ取得

- ⏳ 依存関係追加
  - ⏳ `github.com/gorilla/websocket` (go.mod)

**依存関係**: Task 7.1  
**成果物**: WebSocket 経由でログストリーミング可能

---

#### 🟢 Task 7.3: ログ集約 (オプション)
**工数**: 1日  
**優先度**: Low  
**担当**: DevOps

- ⏳ Loki 統合検討
- ⏳ ログフォーマット統一 (JSON)

**依存関係**: Task 7.1, 7.2  
**成果物**: ログ集約システム

---

### Week 8: メトリクス・監視

#### 🟡 Task 8.1: Prometheus メトリクス (Engine)
**工数**: 2日  
**優先度**: High  
**担当**: Rust Engineer

- ⏳ `engine/src/metrics/prometheus.rs` 追加
  - ⏳ カスタムメトリクス定義
    - ⏳ capsule_count
    - ⏳ gpu_vram_used_bytes
    - ⏳ container_cpu_usage
  - ⏳ `/metrics` エンドポイント

- ⏳ 依存関係追加
  - ⏳ `prometheus = "0.13"` (Cargo.toml)

**依存関係**: なし  
**成果物**: Prometheus メトリクス公開

---

#### 🟡 Task 8.2: ヘルスチェック (Client + Engine)
**工数**: 1日  
**優先度**: High  
**担当**: Both Engineers

- ⏳ Client: `GET /health` 実装
- ⏳ Engine: `GET /health` 実装
- ⏳ Readiness/Liveness プローブ
- ⏳ 依存サービスチェック

**依存関係**: なし  
**成果物**: ヘルスチェック機能

---

#### 🟢 Task 8.3: Grafana ダッシュボード (Docs)
**工数**: 2日  
**優先度**: Medium  
**担当**: DevOps

- ⏳ `docs/grafana/` 追加
  - ⏳ サンプルダッシュボード JSON
  - ⏳ アラートルール定義

**依存関係**: Task 8.1  
**成果物**: Grafana ダッシュボード

---

### Week 9: ストレージ管理

#### 🟢 Task 9.1: LVM 統合 (Engine)
**工数**: 4日  
**優先度**: Medium  
**担当**: Rust Engineer

- ⏳ `engine/src/storage/lvm.rs` 実装
  - ⏳ ボリューム作成・削除
  - ⏳ スナップショット機能

**依存関係**: なし  
**成果物**: LVM 管理機能

---

#### 🟢 Task 9.2: LUKS 暗号化 (Engine)
**工数**: 3日  
**優先度**: Medium  
**担当**: Rust Engineer

- ⏳ `engine/src/storage/luks.rs` 実装
  - ⏳ 暗号化ボリューム作成
  - ⏳ 鍵管理

**依存関係**: Task 9.1  
**成果物**: 暗号化ストレージ機能

---

## Phase 4: 高可用性・スケーラビリティ (Week 10-12)

### Week 10: Master フェイルオーバー

#### 🟡 Task 10.1: 選出ロジック強化 (Client)
**工数**: 3日  
**優先度**: High  
**担当**: Go Engineer

- ⏳ `client/pkg/master/election.go` 拡張
  - ⏳ Split-brain 対策
  - ⏳ Quorum 設定
  - ⏳ 手動 Master 指定

**依存関係**: なし  
**成果物**: 強化された Master 選出

---

#### 🟡 Task 10.2: 状態同期 (Client)
**工数**: 2日  
**優先度**: High  
**担当**: Go Engineer

- ⏳ rqlite レプリケーション設定
- ⏳ 一貫性保証

**依存関係**: Task 10.1  
**成果物**: 状態同期機能

---

### Week 11: 水平スケーリング

#### 🟢 Task 11.1: 動的ノード追加 (Client + Engine)
**工数**: 3日  
**優先度**: Medium  
**担当**: Both Engineers

- ⏳ 新規ノード自動検出
- ⏳ クラスタ参加プロトコル

**依存関係**: なし  
**成果物**: 動的ノード管理

---

#### 🟢 Task 11.2: ロードバランシング (Client)
**工数**: 2日  
**優先度**: Medium  
**担当**: Go Engineer

- ⏳ ノード間負荷分散
- ⏳ リバランシング

**依存関係**: Task 11.1  
**成果物**: ロードバランシング機能

---

### Week 12: 自動復旧・障害対策

#### 🟡 Task 12.1: 自動復旧 (Client)
**工数**: 3日  
**優先度**: High  
**担当**: Go Engineer

- ⏳ Capsule 再起動ポリシー
- ⏳ ノード障害時の再スケジュール

**依存関係**: なし  
**成果物**: 自動復旧機能

---

#### 🟢 Task 12.2: Circuit Breaker (Client + Engine)
**工数**: 2日  
**優先度**: Medium  
**担当**: Both Engineers

- ⏳ gRPC 通信の Circuit Breaker
- ⏳ Retry 戦略

**依存関係**: なし  
**成果物**: Circuit Breaker 実装

---

## Phase 5: プロダクション準備 (Week 13-14)

### Week 13: セキュリティ強化

#### 🟡 Task 13.1: セキュリティ監査
**工数**: 3日  
**優先度**: High  
**担当**: Security Team

- ⏳ コードレビュー
- ⏳ 脆弱性スキャン (Dependabot)
- ⏳ ペネトレーションテスト

**依存関係**: All  
**成果物**: セキュリティ監査レポート

---

#### 🟡 Task 13.2: 認証・認可強化 (Client)
**工数**: 2日  
**優先度**: High  
**担当**: Go Engineer

- ⏳ mTLS 対応
- ⏳ RBAC 実装

**依存関係**: なし  
**成果物**: 強化された認証

---

### Week 14: 運用マニュアル・リリース

#### 🟡 Task 14.1: 運用マニュアル (Docs)
**工数**: 2日  
**優先度**: High  
**担当**: Both Engineers

- ⏳ `docs/OPERATIONS_GUIDE.md`
- ⏳ `docs/MONITORING_GUIDE.md`
- ⏳ `docs/UPGRADE_GUIDE.md`

**依存関係**: All  
**成果物**: 運用マニュアル一式

---

#### 🔴 Task 14.2: リリース準備
**工数**: 1日  
**優先度**: Critical  
**担当**: Tech Lead

- ⏳ バージョン番号決定
- ⏳ CHANGELOG 作成
- ⏳ リリースノート作成
- ⏳ タグ作成

**依存関係**: All  
**成果物**: v1.0.0 リリース

---

## 📊 進捗トラッキング

### Phase 別進捗

| Phase | 完了 | 進行中 | 予定 | 完了率 |
|-------|------|--------|------|--------|
| Phase 1 (Week 1-3) | 1 | 1 | 12 | 14% |
| Phase 2 (Week 4-6) | 0 | 0 | 11 | 0% |
| Phase 3 (Week 7-9) | 0 | 0 | 8 | 0% |
| Phase 4 (Week 10-12) | 0 | 0 | 6 | 0% |
| Phase 5 (Week 13-14) | 0 | 0 | 4 | 0% |
| **Total** | **1** | **1** | **41** | **5%** |

### 優先度別タスク

| 優先度 | タスク数 | 完了 | 残り |
|--------|----------|------|------|
| 🔴 Critical | 7 | 1 | 6 |
| 🟡 High | 18 | 0 | 18 |
| 🟢 Medium | 15 | 0 | 15 |
| ⚪ Low | 3 | 0 | 3 |
| **Total** | **43** | **1** | **42** |

---

## 🎯 次のアクション

### 今週 (Week 1) の優先タスク

1. 🔴 Task 1.1: youki 統合完成 (3日)
2. 🔴 Task 1.2: OCI Bundle 生成 (2日)
3. 🔴 Task 1.3: Capsule Manager 完成 (2日)
4. 🔴 Task 1.4: Client Wasm 統合 (2日)
5. 🟡 Task 1.5: 統合テスト (1日)

**Total**: 10 人日 (2名体制で 1週間)

---

**最終更新**: 2025-11-15  
**次回レビュー**: Week 1 完了時
