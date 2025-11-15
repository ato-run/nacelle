# ロードマップドキュメント利用ガイド

**作成日**: 2025-11-15  
**バージョン**: 1.0.0

---

## 📚 ドキュメント構成

このプロジェクトには、4つのロードマップ関連ドキュメントが存在します。読者の役割や目的に応じて、適切なドキュメントを参照してください。

---

## 🎯 読者別推奨ドキュメント

### エグゼクティブ・プロダクトオーナー向け

**読むべきドキュメント**: `ROADMAP_EXECUTIVE_SUMMARY.md` (9.7KB)

**内容**:
- プロジェクトの現状サマリー (完成度52%)
- 14週間計画の概要
- リソース見積もり (121人日)
- 成功指標 (KPI)
- リスクと対策

**推定読了時間**: 10分

**こんな方におすすめ**:
- 全体の進捗を把握したい
- 予算・リソース計画を立てたい
- リスクを評価したい
- ステークホルダーへの報告資料が必要

---

### プロジェクトマネージャー・Tech Lead 向け

**読むべきドキュメント**: `CAPSULED_ROADMAP.md` (20KB)

**内容**:
- Phase 1-5 の週次詳細計画
- 各週のタスク分解 (工数付き)
- Phase 毎の成果物とリスク
- 技術的負債リスト
- マイルストーン定義

**推定読了時間**: 30分

**こんな方におすすめ**:
- Sprint 計画を立てたい
- タスクをチームに割り当てたい
- 進捗管理・トラッキングが必要
- リスク管理を行いたい

---

### エンジニア・開発者向け

**読むべきドキュメント**: `CAPSULED_REQUIREMENTS_SUMMARY.md` (20KB)

**内容**:
- コンポーネント別実装状況 (Client, Engine, Wasm)
- 機能要件マトリクス (30機能)
- 未実装機能の詳細分析
- 優先度付けバックログ
- Sprint 別工数見積もり

**推定読了時間**: 40分

**こんな方におすすめ**:
- 何を実装すべきか知りたい
- 自分が担当する領域を把握したい
- 他のコンポーネントとの関係を理解したい
- 実装の優先度を知りたい

---

### アーキテクト・テックリード向け

**読むべきドキュメント**: `STRUCTURE_DEPENDENCIES.md` (32KB)

**内容**:
- リポジトリ構造の完全マップ (90+ファイル)
- コンポーネント間依存グラフ
- データフロー分析 (デプロイメント、GPU、状態レポート)
- 外部依存関係整理
- 統合ポイント分析

**推定読了時間**: 50分

**こんな方におすすめ**:
- システムアーキテクチャを理解したい
- 依存関係を把握したい
- リファクタリング計画を立てたい
- 新メンバーのオンボーディング

---

## 📖 読む順序の推奨

### 初めての方

1. **ROADMAP_EXECUTIVE_SUMMARY.md** (10分)
   - まず全体像を把握

2. **CAPSULED_REQUIREMENTS_SUMMARY.md** (15分)
   - 自分の担当領域を特定

3. 必要に応じて詳細ドキュメントへ

### プロジェクト開始前

1. **ROADMAP_EXECUTIVE_SUMMARY.md** - 全体計画
2. **CAPSULED_ROADMAP.md** - Phase 1 の詳細
3. **CAPSULED_REQUIREMENTS_SUMMARY.md** - Sprint 1 のタスク

### 実装中

1. **CAPSULED_REQUIREMENTS_SUMMARY.md** - 機能仕様確認
2. **STRUCTURE_DEPENDENCIES.md** - 依存関係確認
3. **CAPSULED_ROADMAP.md** - 次のタスク確認

---

## 🔍 ドキュメント詳細

### 1. ROADMAP_EXECUTIVE_SUMMARY.md

```
サイズ: 9.7KB
行数: 336行
最終更新: 2025-11-15
```

**セクション構成**:
- 現状サマリー (プロジェクト規模、完成度)
- プロジェクト目標 (ビジョン、マイルストーン)
- 実装済み機能 (52%)
- 未実装機能 (48%)
- 14週間計画 (Phase 1-5概要)
- リソース見積もり (121人日、2-3名体制)
- 成功指標 (KPI)
- リスクと対策
- 次のアクション

**特徴**:
- ビジュアル重視 (進捗バー、表)
- 意思決定に必要な情報を凝縮
- 承認欄あり

---

### 2. CAPSULED_ROADMAP.md

```
サイズ: 20KB
行数: 749行
最終更新: 2025-11-15
```

**セクション構成**:
- エグゼクティブサマリー
- 現状分析 (実装済み/未実装機能)
- Phase 1-5 の週次詳細計画
  - Phase 1 (Week 1-3): 基盤強化
  - Phase 2 (Week 4-6): GPU機能完成
  - Phase 3 (Week 7-9): 運用機能実装
  - Phase 4 (Week 10-12): HA・スケーラビリティ
  - Phase 5 (Week 13-14): プロダクション準備
- 技術的負債と課題
- 成功指標 (KPI)

**特徴**:
- 週次タスク分解 (チェックリスト形式)
- 工数見積もり (日単位)
- リスクと対策を各 Phase に記載
- 成果物を明記

**使い方**:
- Sprint 計画の参考資料
- 進捗トラッキング (チェックリスト更新)
- リスク管理

---

### 3. CAPSULED_REQUIREMENTS_SUMMARY.md

```
サイズ: 20KB
行数: 722行
最終更新: 2025-11-15
```

**セクション構成**:
- コンポーネント別実装状況
  - Client (Go): 8,461 LOC, 60%
  - Engine (Rust): 6,072 LOC, 55%
  - adep-logic (Wasm): 200 LOC, 20%
  - Proto: 182 LOC, 80%
- 機能要件マトリクス (30機能)
  - コア機能 (6機能)
  - GPU機能 (5機能)
  - 運用機能 (5機能)
  - 高可用性 (4機能)
  - スケーラビリティ (3機能)
- 未実装機能詳細 (Priority 1-3)
- 技術的依存関係
- 優先度付けバックログ (Sprint 1-6)

**特徴**:
- 実装状況を %で可視化
- 優先度付け (🔴 Critical, 🟡 High, 🟢 Medium, ⚪ Low)
- 工数見積もり (日単位)
- 依存関係明記

**使い方**:
- 実装優先度の確認
- 機能仕様の参照
- バックログ管理

---

### 4. STRUCTURE_DEPENDENCIES.md

```
サイズ: 32KB
行数: 770行
最終更新: 2025-11-15
```

**セクション構成**:
- リポジトリ構造 (ディレクトリツリー)
- コンポーネント間依存関係 (依存グラフ)
- データフロー分析
  - Capsule デプロイメントフロー (9ステップ)
  - GPU スケジューリングフロー (7ステップ)
  - 状態レポートフロー
- モジュール依存グラフ (Client/Engine 内部)
- 外部依存関係 (Go/Rust ライブラリ)
- 統合ポイント (gRPC, Wasm, Database, VPN)

**特徴**:
- ファイル単位の詳細情報 (LOC、完成度)
- 循環依存チェック済み
- データフローを図解
- 外部ツール依存を整理

**使い方**:
- アーキテクチャ理解
- リファクタリング計画
- 新メンバーオンボーディング
- 依存関係の影響分析

---

## 🔄 ドキュメント更新フロー

### 更新頻度

| ドキュメント | 更新頻度 | 更新タイミング |
|------------|---------|---------------|
| ROADMAP_EXECUTIVE_SUMMARY.md | Phase毎 | Phase完了時 |
| CAPSULED_ROADMAP.md | 週次 | Sprint終了時 |
| CAPSULED_REQUIREMENTS_SUMMARY.md | 隔週 | 機能実装完了時 |
| STRUCTURE_DEPENDENCIES.md | 月次 | アーキテクチャ変更時 |

### 更新担当

| 役割 | 担当ドキュメント |
|------|----------------|
| プロジェクトマネージャー | ROADMAP_EXECUTIVE_SUMMARY.md |
| Tech Lead | CAPSULED_ROADMAP.md |
| エンジニア | CAPSULED_REQUIREMENTS_SUMMARY.md |
| アーキテクト | STRUCTURE_DEPENDENCIES.md |

### 更新手順

1. **進捗を反映**
   - チェックリストを更新 ([x] 完了, [ ] 未完)
   - 完成度 % を更新

2. **新規課題を追加**
   - 技術的負債セクションに追加
   - 優先度を設定

3. **リスクを更新**
   - 顕在化したリスクを記録
   - 対策を追記

4. **レビュー**
   - チーム内レビュー
   - ステークホルダー報告

---

## 💡 活用例

### Sprint 計画

1. **CAPSULED_ROADMAP.md** で該当週のタスク確認
2. **CAPSULED_REQUIREMENTS_SUMMARY.md** で詳細仕様確認
3. 工数見積もりを参考に Sprint バックログ作成
4. チームに割り当て

### 進捗報告

1. **ROADMAP_EXECUTIVE_SUMMARY.md** の KPI 確認
2. 達成状況を更新
3. 次の Phase への準備状況を報告
4. リスクがあれば対策を提示

### 新メンバーオンボーディング

1. **ROADMAP_EXECUTIVE_SUMMARY.md** でプロジェクト概要把握
2. **STRUCTURE_DEPENDENCIES.md** でアーキテクチャ学習
3. **CAPSULED_REQUIREMENTS_SUMMARY.md** で担当領域特定
4. **CAPSULED_ROADMAP.md** で次のタスク確認

---

## 📞 サポート

### ドキュメントに関する質問

- **一般的な質問**: GitHub Discussions
- **具体的な実装**: 該当 Issue にコメント
- **アーキテクチャ**: Tech Lead に連絡

### フィードバック

ドキュメントの改善提案は以下の方法で受け付けています:

1. **GitHub Issue** 作成
   - ラベル: `documentation`
   - テンプレート: "Documentation Improvement"

2. **Pull Request** 送信
   - 小さな修正 (誤字、リンク切れ等)
   - レビュー必須

---

## 🔗 関連リソース

### プロジェクトドキュメント

- [README.md](../README.md) - プロジェクト概要
- [ARCHITECTURE.md](../ARCHITECTURE.md) - システムアーキテクチャ
- [CODE_REVIEW.md](../CODE_REVIEW.md) - コードレビュー記録

### 技術ドキュメント

- [CI_CD.md](./CI_CD.md) - CI/CD パイプライン
- [gpu-mock-configuration.md](./gpu-mock-configuration.md) - GPU Mock設定

### Client ドキュメント

- [client/README.md](../client/README.md) - Client 概要
- [client/QUICKSTART.md](../client/QUICKSTART.md) - クイックスタート

### Engine ドキュメント

- [engine/README.md](../engine/README.md) - Engine 概要
- [engine/PROJECT_OVERVIEW.md](../engine/PROJECT_OVERVIEW.md) - プロジェクト詳細

---

**最終更新**: 2025-11-15  
**次回レビュー**: Phase 1 完了時 (Week 3)
