# Capsuled 技術ロードマップ (14週間計画)

**バージョン:** 1.0.0  
**最終更新:** 2025-11-15  
**策定目的:** コードベース横断探索による今後の技術ロードマップの策定

---

## 📋 目次

1. [エグゼクティブサマリー](#エグゼクティブサマリー)
2. [現状分析](#現状分析)
3. [Phase 1: 基盤強化 (Week 1-3)](#phase-1-基盤強化-week-1-3)
4. [Phase 2: GPU機能完成 (Week 4-6)](#phase-2-gpu機能完成-week-4-6)
5. [Phase 3: 運用機能実装 (Week 7-9)](#phase-3-運用機能実装-week-7-9)
6. [Phase 4: 高可用性・スケーラビリティ (Week 10-12)](#phase-4-高可用性スケーラビリティ-week-10-12)
7. [Phase 5: プロダクション準備 (Week 13-14)](#phase-5-プロダクション準備-week-13-14)
8. [技術的負債と課題](#技術的負債と課題)
9. [成功指標 (KPI)](#成功指標-kpi)

---

## エグゼクティブサマリー

### プロジェクト目標

Capsuled は Personal Cloud OS のコア実装で、分散コンピューティング環境において複数の Engine ノードを管理し、GPU-aware なコンテナスケジューリングを実現するシステムです。

### 主要マイルストーン

| Phase | 期間 | 目標 | 完了率 |
|-------|------|------|--------|
| Phase 1 | Week 1-3 | 基盤強化・API完成 | 40% |
| Phase 2 | Week 4-6 | GPU機能完成 | 30% |
| Phase 3 | Week 7-9 | 運用機能実装 | 10% |
| Phase 4 | Week 10-12 | HA・スケーラビリティ | 5% |
| Phase 5 | Week 13-14 | プロダクション準備 | 0% |

### 現在のステータス

- **実装完了**: gRPC基盤、GPU検出(Mock/Real)、基本スケジューラ、Master選出
- **部分実装**: コンテナ実行、ストレージ管理、API Handler
- **未実装**: ログストリーミング、メトリクス、HA構成、自動復旧

---

## 現状分析

### 実装済み機能

#### Client (Go - 8,461 LOC)

✅ **完成度: 60%**

| コンポーネント | ステータス | 詳細 |
|--------------|----------|------|
| Master Election | ✅ 完成 | Memberlist ベース、テスト済み |
| GPU Scheduler | ✅ 完成 | Filter-Score パイプライン実装 |
| gRPC Client | ✅ 完成 | Coordinator/Engine 両プロトコル対応 |
| HTTP API | 🟡 部分実装 | DeployHandler 基本実装のみ |
| Database (rqlite) | ✅ 完成 | 状態管理、マイグレーション実装 |
| Reconciler | 🟡 部分実装 | 基本ロジックのみ、エラーハンドリング未完 |
| Headscale 統合 | 🟡 部分実装 | Client 実装済み、統合テスト未完 |
| Gossip (Memberlist) | ✅ 完成 | ノード検出、クラスタ管理実装 |
| Config 管理 | ✅ 完成 | YAML ベース、テスト済み |
| Wasm (Wasmer) | ❌ 未実装 | adep-logic 統合未完 |

#### Engine (Rust - 6,072 LOC)

✅ **完成度: 55%**

| コンポーネント | ステータス | 詳細 |
|--------------|----------|------|
| gRPC Server | ✅ 完成 | Coordinator/Engine サービス実装 |
| GPU Detector | ✅ 完成 | Mock/Real 両モード実装、テスト済み |
| GPU Process Monitor | 🟡 部分実装 | nvidia-smi 統合のみ |
| Status Reporter | ✅ 完成 | 定期レポート送信実装 |
| Capsule Manager | 🟡 部分実装 | 状態管理のみ、実行ロジック未完 |
| OCI Runtime 統合 | 🟡 部分実装 | Spec Builder 実装、youki 統合未完 |
| Wasm (Wasmtime) | ✅ 完成 | adep-logic ホスト実装 |
| Storage (LVM/LUKS) | ❌ 未実装 | 設計のみ、実装なし |
| Proxy (Caddy) | ❌ 未実装 | 設計のみ、実装なし |

#### adep-logic (Wasm)

✅ **完成度: 20%**

| 機能 | ステータス | 詳細 |
|------|----------|------|
| JSON パース | ✅ 完成 | serde_json 使用 |
| バリデーション | 🟡 部分実装 | 基本検証のみ |
| Client 統合 | ❌ 未実装 | Wasmer バインディング未完 |
| Engine 統合 | ✅ 完成 | Wasmtime で使用中 |

#### Proto Definitions

✅ **完成度: 80%**

- `coordinator.proto` ⭐ **推奨**: 包括的な Workload 管理
- `engine.proto` (レガシー): 後方互換性のため保持

### 未実装機能

#### Critical (Phase 1-2 で対応必須)

1. **コンテナライフサイクル管理**
   - youki/runc 完全統合
   - Bundle 生成・展開
   - ログファイル管理

2. **HTTP API 完成**
   - Capsule CRUD エンドポイント
   - ログストリーミング (WebSocket)
   - ヘルスチェック

3. **Wasm 統合 (Client側)**
   - Wasmer バインディング
   - adep-logic 呼び出し

#### High Priority (Phase 2-3 で対応)

4. **GPU プロセス監視強化**
   - VRAM 使用量計測
   - プロセス紐付け
   - 自動リソース回収

5. **ストレージ管理**
   - LVM ボリューム作成
   - LUKS 暗号化
   - 自動クリーンアップ

6. **ネットワーク管理**
   - Caddy 統合
   - 動的ルート設定
   - SSL 証明書管理

#### Medium Priority (Phase 3-4 で対応)

7. **監視・メトリクス**
   - Prometheus メトリクス
   - ログ集約
   - 分散トレーシング

8. **高可用性**
   - Master フェイルオーバー
   - データレプリケーション
   - 自動復旧

9. **スケーラビリティ**
   - 水平スケーリング
   - ロードバランシング
   - クラスタ拡張

### 技術的負債

#### Code Quality

1. **エラーハンドリングの統一**
   - Go: 標準 error vs anyhow style
   - Rust: anyhow vs thiserror の混在

2. **テストカバレッジ**
   - Client: 約 40% (目標: 80%)
   - Engine: 約 30% (目標: 80%)
   - 統合テスト不足

3. **ドキュメント不足**
   - API 仕様書 (OpenAPI) 未整備
   - 運用マニュアル未整備
   - トラブルシューティングガイド未整備

#### Architecture

4. **Proto 統合**
   - engine.proto と coordinator.proto の統合
   - 非推奨 API の削除計画

5. **設定管理**
   - 環境変数 vs YAML の統一
   - Secret 管理の標準化

6. **依存関係**
   - 外部ツール依存 (youki, Caddy) のバージョン固定
   - 依存ライブラリの定期アップデート計画

---

## Phase 1: 基盤強化 (Week 1-3)

**目標**: コア機能の完成とエンドツーエンドのデプロイメントフロー確立

### Week 1: コンテナランタイム統合

#### 実装タスク

1. **youki 統合完成** (Engine)
   - [ ] `runtime/youki.rs` 実装
     - `create`, `start`, `delete`, `state` コマンド実行
     - エラーハンドリング強化
   - [ ] OCI Bundle 生成ロジック
     - `oci/bundle.rs` 追加
     - config.json 生成
     - rootfs 準備
   - [ ] 統合テスト追加
     - 単純コンテナ起動テスト
     - 環境変数、ボリュームマウントテスト

2. **Capsule Manager 完成** (Engine)
   - [ ] デプロイメントフロー実装
     - Pending → Running 状態遷移
     - PID 追跡
     - ログファイルパス管理
   - [ ] エラーリカバリ
     - 起動失敗時の状態更新
     - リソースクリーンアップ

3. **Client Wasm 統合** (Client)
   - [ ] `pkg/wasm/` パッケージ追加
   - [ ] Wasmer バインディング実装
   - [ ] adep-logic 呼び出しラッパー
   - [ ] バリデーション統合

**成果物**:
- youki でコンテナが起動できる
- Client から Engine 経由でコンテナデプロイ可能

**リスク**:
- youki のバージョン互換性問題
- OCI 仕様の理解不足

---

### Week 2: HTTP API 完成

#### 実装タスク

1. **CRUD エンドポイント** (Client)
   - [ ] `POST /api/v1/capsules` - デプロイ
   - [ ] `GET /api/v1/capsules/:id` - 状態取得
   - [ ] `GET /api/v1/capsules` - 一覧取得
   - [ ] `DELETE /api/v1/capsules/:id` - 削除
   - [ ] `GET /api/v1/nodes` - ノード一覧

2. **認証・認可** (Client)
   - [ ] API Key 認証ミドルウェア
   - [ ] JWT トークン生成 (将来拡張用)
   - [ ] RBAC 基礎設計

3. **OpenAPI 仕様書** (Docs)
   - [ ] `docs/openapi.yaml` 作成
   - [ ] Swagger UI 統合 (オプション)

**成果物**:
- REST API 経由でコンテナ管理可能
- API 仕様書完成

**リスク**:
- API 設計の後方互換性
- 認証方式の決定

---

### Week 3: 統合テスト・ドキュメント

#### 実装タスク

1. **E2E テスト** (tests/e2e/)
   - [ ] Client → Engine デプロイテスト
   - [ ] GPU スケジューリングテスト
   - [ ] Master フェイルオーバーテスト

2. **パフォーマンステスト**
   - [ ] ベンチマークスクリプト
   - [ ] 負荷テスト設定

3. **ドキュメント整備**
   - [ ] QUICKSTART.md 更新
   - [ ] DEPLOYMENT.md 作成
   - [ ] TROUBLESHOOTING.md 作成

**成果物**:
- CI で E2E テストがパスする
- 運用ドキュメント一式

**リスク**:
- テスト環境構築の複雑さ
- ドキュメント作成の時間不足

---

## Phase 2: GPU機能完成 (Week 4-6)

**目標**: GPU-aware スケジューリングの完全実装と VRAM 管理強化

### Week 4: GPU プロセス監視強化

#### 実装タスク

1. **VRAM 計測** (Engine)
   - [ ] `nvidia-smi` 詳細パース
     - プロセス単位の VRAM 使用量
     - GPU 使用率
     - 温度・電力
   - [ ] Mock モード拡張
     - 動的 VRAM 使用シミュレーション
     - プロセス紐付けエミュレーション

2. **自動リソース回収** (Engine)
   - [ ] プロセス終了検知
   - [ ] VRAM 解放確認
   - [ ] Zombie プロセスクリーンアップ

3. **Status Reporter 強化** (Engine)
   - [ ] 詳細 VRAM 情報レポート
   - [ ] エラー状態レポート
   - [ ] レポート間隔設定

**成果物**:
- VRAM 使用量をリアルタイム監視
- 異常終了時の自動リソース回収

**リスク**:
- nvidia-smi の出力形式変更
- Mock モードの精度

---

### Week 5: GPU スケジューリング最適化

#### 実装タスク

1. **スケジューラ改善** (Client)
   - [ ] 追加フィルタ実装
     - GPU モデルフィルタ
     - Taint/Toleration フィルタ
   - [ ] 追加スコアラー実装
     - ノード負荷分散スコア
     - GPU 温度スコア
   - [ ] 重み付け設定可能化

2. **Dynamic Scheduling** (Client)
   - [ ] リスケジューリング機能
   - [ ] GPU 解放検知と再スケジュール
   - [ ] 優先度ベーススケジューリング

3. **スケジューリングポリシー** (Client)
   - [ ] BestFit (デフォルト)
   - [ ] LeastAllocated (負荷分散)
   - [ ] Custom ポリシープラグイン設計

**成果物**:
- 高度な GPU スケジューリング戦略
- ポリシー設定可能

**リスク**:
- 複雑性の増加
- パフォーマンス劣化

---

### Week 6: GPU 機能テスト

#### 実装タスク

1. **負荷テスト** (tests/)
   - [ ] 大量 Capsule デプロイ
   - [ ] GPU 競合シナリオ
   - [ ] VRAM 不足時の挙動

2. **カオステスト** (tests/)
   - [ ] Engine クラッシュシミュレーション
   - [ ] ネットワーク分断テスト
   - [ ] GPU 故障シミュレーション

3. **ドキュメント**
   - [ ] GPU_SCHEDULING_GUIDE.md 作成
   - [ ] GPU_TROUBLESHOOTING.md 作成

**成果物**:
- GPU 機能の安定性確認
- GPU 機能ドキュメント完成

**リスク**:
- テストシナリオの網羅性
- GPU ハードウェアアクセス (CI/CD)

---

## Phase 3: 運用機能実装 (Week 7-9)

**目標**: プロダクション運用に必要な監視・ログ・ストレージ機能の実装

### Week 7: ログストリーミング

#### 実装タスク

1. **ログ収集** (Engine)
   - [ ] コンテナログファイル監視
   - [ ] stdout/stderr キャプチャ
   - [ ] ログローテーション

2. **WebSocket API** (Client)
   - [ ] `WS /api/v1/capsules/:id/logs` 実装
   - [ ] リアルタイムストリーミング
   - [ ] 履歴ログ取得

3. **ログ集約** (オプション)
   - [ ] Loki 統合検討
   - [ ] ログフォーマット統一

**成果物**:
- WebSocket 経由でログストリーミング可能
- ログ管理機能完成

**リスク**:
- WebSocket スケーラビリティ
- ログ量の増大

---

### Week 8: メトリクス・監視

#### 実装タスク

1. **Prometheus メトリクス** (Engine)
   - [ ] `metrics.rs` 実装
   - [ ] カスタムメトリクス定義
     - capsule_count
     - gpu_vram_used_bytes
     - container_cpu_usage
   - [ ] `/metrics` エンドポイント

2. **ヘルスチェック** (Client + Engine)
   - [ ] `GET /health` 実装
   - [ ] Readiness/Liveness プローブ
   - [ ] 依存サービスチェック

3. **Grafana ダッシュボード** (Docs)
   - [ ] サンプルダッシュボード作成
   - [ ] アラートルール定義

**成果物**:
- Prometheus でメトリクス収集可能
- Grafana で可視化可能

**リスク**:
- メトリクスのオーバーヘッド
- ダッシュボード設計

---

### Week 9: ストレージ管理

#### 実装タスク

1. **LVM 統合** (Engine)
   - [ ] `storage/lvm.rs` 実装
   - [ ] ボリューム作成・削除
   - [ ] スナップショット機能

2. **LUKS 暗号化** (Engine)
   - [ ] `storage/luks.rs` 実装
   - [ ] 暗号化ボリューム作成
   - [ ] 鍵管理

3. **自動クリーンアップ** (Engine)
   - [ ] 未使用ボリューム検出
   - [ ] スケジュールドクリーンアップ
   - [ ] 容量監視

**成果物**:
- 暗号化ストレージ管理機能
- 自動クリーンアップ機能

**リスク**:
- LVM/LUKS コマンド依存
- データ損失リスク

---

## Phase 4: 高可用性・スケーラビリティ (Week 10-12)

**目標**: 本番環境でのクラスタ運用を可能にする HA 構成とスケーラビリティ

### Week 10: Master フェイルオーバー

#### 実装タスク

1. **選出ロジック強化** (Client)
   - [ ] Split-brain 対策
   - [ ] Quorum 設定
   - [ ] 手動 Master 指定

2. **状態同期** (Client)
   - [ ] rqlite レプリケーション設定
   - [ ] 一貫性保証
   - [ ] データ移行

3. **フェイルオーバーテスト** (tests/)
   - [ ] Master ダウンシナリオ
   - [ ] ネットワーク分断テスト
   - [ ] 自動復旧テスト

**成果物**:
- Master の自動フェイルオーバー
- データ一貫性保証

**リスク**:
- 分散合意の複雑さ
- レプリケーション遅延

---

### Week 11: 水平スケーリング

#### 実装タスク

1. **動的ノード追加** (Client + Engine)
   - [ ] 新規ノード自動検出
   - [ ] クラスタ参加プロトコル
   - [ ] ノード削除処理

2. **ロードバランシング** (Client)
   - [ ] ノード間負荷分散
   - [ ] リバランシング
   - [ ] ドレイン機能

3. **Auto Scaling** (オプション)
   - [ ] ノード数自動調整
   - [ ] リソース使用率ベーススケーリング
   - [ ] Cloud API 統合 (AWS/GCP/Azure)

**成果物**:
- クラスタの動的拡張・縮小
- Auto Scaling 機能

**リスク**:
- スケーリングのオーバーヘッド
- Cloud API 依存

---

### Week 12: 自動復旧・障害対策

#### 実装タスク

1. **自動復旧** (Client)
   - [ ] Capsule 再起動ポリシー
   - [ ] ノード障害時の再スケジュール
   - [ ] ローリングアップデート

2. **Circuit Breaker** (Client + Engine)
   - [ ] gRPC 通信の Circuit Breaker
   - [ ] Retry 戦略
   - [ ] タイムアウト設定

3. **Backup & Restore** (Docs)
   - [ ] バックアップスクリプト
   - [ ] リストア手順書
   - [ ] ディザスタリカバリ計画

**成果物**:
- 自動復旧機能
- DR 計画

**リスク**:
- 復旧ロジックのバグ
- データ損失リスク

---

## Phase 5: プロダクション準備 (Week 13-14)

**目標**: プロダクション環境での運用開始準備

### Week 13: セキュリティ強化

#### 実装タスク

1. **セキュリティ監査** (全体)
   - [ ] コードレビュー
   - [ ] 脆弱性スキャン (Dependabot)
   - [ ] ペネトレーションテスト

2. **認証・認可強化** (Client)
   - [ ] mTLS 対応
   - [ ] RBAC 実装
   - [ ] OAuth2/OIDC 統合 (オプション)

3. **Secret 管理** (全体)
   - [ ] HashiCorp Vault 統合
   - [ ] Secret 暗号化
   - [ ] Secret ローテーション

**成果物**:
- セキュリティ監査レポート
- Secret 管理システム

**リスク**:
- セキュリティ脆弱性の発見
- Vault 運用の複雑さ

---

### Week 14: 運用マニュアル・リリース

#### 実装タスク

1. **運用マニュアル** (Docs)
   - [ ] OPERATIONS_GUIDE.md
   - [ ] MONITORING_GUIDE.md
   - [ ] TROUBLESHOOTING_GUIDE.md
   - [ ] UPGRADE_GUIDE.md

2. **リリース準備**
   - [ ] バージョン番号決定
   - [ ] CHANGELOG 作成
   - [ ] リリースノート作成
   - [ ] タグ作成

3. **プロダクション展開**
   - [ ] ステージング環境テスト
   - [ ] 本番環境デプロイ
   - [ ] モニタリング設定
   - [ ] オンコール体制

**成果物**:
- v1.0.0 リリース
- 本番環境稼働

**リスク**:
- 予期しない本番障害
- ドキュメント不足

---

## 技術的負債と課題

### 優先度 High

1. **Proto 統合**
   - 現状: `engine.proto` と `coordinator.proto` が併存
   - 対応: `coordinator.proto` への統一 (Phase 2)
   - 影響: API 後方互換性の維持が必要

2. **テストカバレッジ向上**
   - 現状: Client 40%, Engine 30%
   - 目標: 80% 以上
   - 対応: 各 Phase でテスト追加

3. **Wasm 統合完成**
   - 現状: Engine のみ使用、Client 未統合
   - 対応: Week 1 で対応

### 優先度 Medium

4. **設定管理統一**
   - 現状: YAML, TOML, 環境変数が混在
   - 対応: 統一フォーマットへ移行 (Phase 3)

5. **依存関係管理**
   - 現状: youki, Caddy のバージョン未固定
   - 対応: Dockerfile/systemd で明示

6. **ログフォーマット統一**
   - 現状: 構造化ログ vs プレーンテキスト
   - 対応: JSON 形式へ統一 (Phase 3)

### 優先度 Low

7. **パフォーマンス最適化**
   - 対応: Phase 4 以降

8. **Multi-region 対応**
   - 対応: v2.0 で検討

---

## 成功指標 (KPI)

### Phase 1 完了時 (Week 3)

- [ ] youki でコンテナが起動できる
- [ ] Client → Engine → Container のエンドツーエンドテスト成功
- [ ] HTTP API でデプロイ可能
- [ ] CI/CD パイプラインが正常動作

### Phase 2 完了時 (Week 6)

- [ ] GPU スケジューリングが正常動作
- [ ] VRAM 使用量を監視できる
- [ ] 10 Capsule 同時起動テスト成功
- [ ] GPU 機能ドキュメント完成

### Phase 3 完了時 (Week 9)

- [ ] ログストリーミングが動作
- [ ] Prometheus/Grafana で監視可能
- [ ] 暗号化ストレージが動作
- [ ] 運用ドキュメント一式完成

### Phase 4 完了時 (Week 12)

- [ ] Master フェイルオーバーが動作
- [ ] クラスタの動的拡張が可能
- [ ] 自動復旧が動作
- [ ] DR 計画完成

### Phase 5 完了時 (Week 14)

- [ ] v1.0.0 リリース
- [ ] 本番環境稼働
- [ ] セキュリティ監査完了
- [ ] 運用マニュアル完成

---

## 次のステップ

### 即座に開始すべき項目 (Week 1 開始前)

1. **開発環境統一**
   - Docker Compose による開発環境構築
   - VS Code devcontainer 設定

2. **Issue/Project 管理**
   - GitHub Projects でロードマップ管理
   - Issue テンプレート作成

3. **CI/CD 改善**
   - テストカバレッジレポート
   - Dependabot 有効化

### Phase 間のレビューポイント

- 各 Phase 完了時に進捗レビュー
- KPI 達成度確認
- ロードマップ調整

### リスク管理

- 週次でリスク評価
- ブロッカーの早期エスカレーション
- バッファ週を Phase 間に設定

---

**最終更新**: 2025-11-15  
**次回レビュー**: Week 3 完了時 (Phase 1 終了時)
