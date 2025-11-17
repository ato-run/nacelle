# OCI A1インスタンス デプロイメントガイド

このディレクトリには、capsuledをOracle Cloud Infrastructure (OCI)のA1インスタンスにデプロイするための詳細なタスクリストと手順が含まれています。

## 📋 ドキュメント一覧

### [OCI_A1_DEPLOYMENT_TASKS.yaml](./OCI_A1_DEPLOYMENT_TASKS.yaml)
OCI A1インスタンスへのデプロイに必要な全タスクを細分化したYAMLドキュメントです。各タスクは個別のissueとして起票できるように構造化されています。

## 🎯 デプロイメントの目的

OCIのA1インスタンス（ARM64/aarch64）にcapsuledをデプロイし、MacBookのdesktopから操作できる状態にすることを目指します。

## 📊 タスク概要

全体で**6つのフェーズ**、**25の主要タスク**に分割されています：

### Phase 0: 環境準備・前提条件確認（3タスク）
- OCI A1インスタンスの起動と基本設定
- ビルド用依存パッケージのインストール
- ビルド環境の検証

**工数見積**: 2-3 人日

### Phase 1: capsuled本体のビルド（5タスク）
- リポジトリのクローンと初期セットアップ
- adep-logic (Wasm)のビルド
- capsuled-engine (Rust)のビルド
- capsuled-client (Go)のビルド
- ビルド成果物の検証とパッケージング

**工数見積**: 3-4 人日

### Phase 2: デプロイと設定（6タスク）
- バイナリの転送と配置
- 設定ファイルの作成と配置
- 必要なディレクトリの作成
- 環境変数の設定
- データベースの初期化
- systemdサービスファイルの作成と設定

**工数見積**: 4-5 人日

### Phase 3: ネットワークとセキュリティ（5タスク）
- ファイアウォール設定
- desktop（MacBook）からのアクセス設定
- SSL/TLS証明書の設定
- SSH鍵管理とセキュリティ強化
- 不要なサービスの停止

**工数見積**: 3-4 人日

### Phase 4: 動作確認とテスト（6タスク）
- サービス起動確認
- ログ出力確認
- API疎通確認
- gRPC通信確認
- 簡単なコンテナデプロイテスト
- 自動起動確認

**工数見積**: 3-4 人日

### Phase 5: 監視と運用準備（3タスク）
- 監視設定（基本）
- バックアップ設定
- 運用ドキュメントの作成

**工数見積**: 2-3 人日

### Phase 6: 最終確認とドキュメント化（3タスク）
- エンドツーエンドテスト
- セキュリティチェック
- デプロイメントチェックリストの作成

**工数見積**: 1-2 人日

**合計見積工数**: 15-20 人日

## 🚀 使い方

### 1. YAMLドキュメントの確認

```bash
# YAMLファイルを開いて確認
cat docs/OCI_A1_DEPLOYMENT_TASKS.yaml

# Pythonで解析
python3 -c "import yaml, json; print(json.dumps(yaml.safe_load(open('docs/OCI_A1_DEPLOYMENT_TASKS.yaml')), indent=2))"
```

### 2. 個別issueの作成

各タスクには以下の情報が含まれています：

- `task_id`: タスクの一意なID（例: P0-1, P1-1）
- `title`: タスクのタイトル
- `description`: タスクの詳細説明
- `priority`: 優先度（critical, high, medium, low）
- `estimated_effort`: 工数見積（人日）
- `subtasks`: 具体的なサブタスクリスト
- `dependencies`: 依存するタスクのID
- `deliverables`: 成果物
- `related_files`: 関連するファイル（該当する場合）

### 3. issueテンプレート例

```markdown
## タスク概要

**タスクID**: P1-3
**タイトル**: capsuled-engine (Rust)のビルド
**フェーズ**: Phase 1 - capsuled本体のビルド
**優先度**: Critical
**工数見積**: 1 人日

## 説明

Engine（Rust）をARM64向けにビルド

## サブタスク

- [ ] protoファイルからgRPCコードの生成（buf generate）
- [ ] cd engine && cargo build --release
- [ ] ビルドエラーの解決（ARM64特有の問題対応）
- [ ] バイナリサイズの確認
- [ ] 依存ライブラリのリンク確認
- [ ] bin/capsuled-engineへのコピー

## 依存関係

- P1-2: adep-logic (Wasm)のビルド

## 成果物

- bin/capsuled-engine（ARM64バイナリ）

## 関連ファイル

- engine/Cargo.toml
- proto/coordinator.proto
- Makefile
```

## 📝 タスク進捗の追跡

YAMLファイルの各フェーズとタスクに対して：

1. GitHubのissueを作成
2. issueに適切なラベルを付与（priority, phase）
3. マイルストーンを設定（各フェーズ毎）
4. プロジェクトボードで進捗を可視化

## 🔧 前提条件

### 必要なハードウェア
- OCI A1インスタンス（推奨: VM.Standard.A1.Flex）
- vCPU: 2-4
- メモリ: 8-16 GB
- ストレージ: 100 GB以上

### 必要なソフトウェア
- OS: Ubuntu 22.04 LTS (ARM64)
- Rust 1.70+
- Go 1.23+
- Python 3.10+
- youki v0.3.3
- Caddy
- その他（詳細はYAMLファイル参照）

## ⚠️ リスクと対策

YAMLファイルには以下のリスクと対策が記載されています：

1. **ARM64特有のビルドエラー**
   - 対策: 事前に小規模なテストビルドを実施

2. **youkiのARM64互換性問題**
   - 対策: runcをフォールバックオプションとして準備

3. **ネットワーク遅延**
   - 対策: Tailscaleを使用して効率的なルーティングを確保

4. **OCI A1インスタンスのリソース不足**
   - 対策: 最小要件の確認、必要に応じてインスタンスサイズを拡大

## 📚 関連ドキュメント

- [README.md](../README.md) - プロジェクト概要とビルド手順
- [TODO.md](../TODO.md) - 全体の実装ロードマップ
- [engine/provisioning/scripts/deploy.py](../engine/provisioning/scripts/deploy.py) - デプロイスクリプトの実装
- [engine/provisioning/MIGRATION.md](../engine/provisioning/MIGRATION.md) - プロビジョニング統合ガイド
- [docs/CI_CD.md](./CI_CD.md) - CI/CDパイプライン

## 🛠️ 有用なコマンド

YAMLファイルには以下のような有用なコマンドが記載されています：

```bash
# アーキテクチャ確認
uname -m

# サービス状態確認
systemctl status capsuled-engine capsuled-client

# ログ確認
journalctl -u capsuled-engine -f

# API疎通確認
curl -H 'X-API-Key: YOUR_KEY' http://localhost:8080/health

# ファイアウォール状態確認
sudo ufw status verbose
```

## 💡 ヒント

1. **フェーズ順に進める**: 依存関係があるため、Phase 0から順番に実施してください
2. **検証を忘れずに**: 各タスク完了後、必ず動作確認を行ってください
3. **ドキュメント化**: 発生した問題と解決策は記録しておいてください
4. **バックアップ**: 設定変更前には必ずバックアップを取ってください

## 📞 サポート

問題が発生した場合は、以下を確認してください：

1. [TROUBLESHOOTING.md](./TROUBLESHOOTING.md)（Phase 5-3で作成予定）
2. GitHubのissueを検索
3. 新しいissueを作成して質問

---

**最終更新**: 2025-11-17
**バージョン**: 1.0.0
