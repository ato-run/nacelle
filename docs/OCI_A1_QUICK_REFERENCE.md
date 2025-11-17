# OCI A1 デプロイメント クイックリファレンス

## 📊 タスク一覧（全31タスク）

### Phase 0: 環境準備・前提条件確認（3タスク、2-3人日）

| ID | タイトル | 優先度 | 工数 |
|----|---------|--------|------|
| P0-1 | OCI A1インスタンスの起動と基本設定 | Critical | 0.5日 |
| P0-2 | ビルド用依存パッケージのインストール確認 | Critical | 1日 |
| P0-3 | ビルド環境の検証 | High | 0.5日 |

**成果物**: 起動済みOCI A1インスタンス、全依存パッケージ、ビルド環境検証

### Phase 1: capsuled本体のビルド（5タスク、3-4人日）

| ID | タイトル | 優先度 | 工数 | 依存 |
|----|---------|--------|------|------|
| P1-1 | リポジトリのクローンと初期セットアップ | Critical | 0.5日 | P0-3 |
| P1-2 | adep-logic (Wasm)のビルド | Critical | 0.5日 | P1-1 |
| P1-3 | capsuled-engine (Rust)のビルド | Critical | 1日 | P1-2 |
| P1-4 | capsuled-client (Go)のビルド | Critical | 1日 | P1-2 |
| P1-5 | ビルド成果物の検証とパッケージング | High | 0.5日 | P1-3, P1-4 |

**成果物**: adep_logic.wasm, capsuled-engine (ARM64), capsuled-client (ARM64)

### Phase 2: デプロイと設定（6タスク、4-5人日）

| ID | タイトル | 優先度 | 工数 | 依存 |
|----|---------|--------|------|------|
| P2-1 | バイナリの転送と配置 | Critical | 0.5日 | P1-5 |
| P2-2 | 設定ファイルの作成と配置 | Critical | 1日 | P2-1 |
| P2-3 | 必要なディレクトリの作成 | High | 0.5日 | P2-1 |
| P2-4 | 環境変数の設定 | High | 0.5日 | P2-2 |
| P2-5 | データベースの初期化 | High | 0.5日 | P2-3 |
| P2-6 | systemdサービスファイルの作成と設定 | Critical | 1日 | P2-4, P2-5 |

**成果物**: インストール済みバイナリ、設定ファイル、systemdサービス

### Phase 3: ネットワークとセキュリティ（5タスク、3-4人日）

| ID | タイトル | 優先度 | 工数 | 依存 |
|----|---------|--------|------|------|
| P3-1 | ファイアウォール設定（基本） | Critical | 1日 | P2-6 |
| P3-2 | desktop（MacBook）からのアクセス設定 | Critical | 1日 | P3-1 |
| P3-3 | SSL/TLS証明書の設定 | High | 0.5日 | P2-6 |
| P3-4 | SSH鍵管理とセキュリティ強化 | High | 0.5日 | P3-2 |
| P3-5 | 不要なサービスの停止 | Medium | 0.5日 | P2-6 |

**成果物**: ファイアウォール設定、Tailscale接続、SSL証明書、強化されたSSH

### Phase 4: 動作確認とテスト（6タスク、3-4人日）

| ID | タイトル | 優先度 | 工数 | 依存 |
|----|---------|--------|------|------|
| P4-1 | サービス起動確認 | Critical | 0.5日 | P2-6, P3-1 |
| P4-2 | ログ出力確認 | High | 0.5日 | P4-1 |
| P4-3 | API疎通確認 | Critical | 1日 | P4-1, P3-2 |
| P4-4 | gRPC通信確認 | High | 0.5日 | P4-1 |
| P4-5 | 簡単なコンテナデプロイテスト | High | 1日 | P4-3, P4-4 |
| P4-6 | 自動起動確認 | Medium | 0.5日 | P4-1 |

**成果物**: 動作確認済みシステム、テストレポート

### Phase 5: 監視と運用準備（3タスク、2-3人日）

| ID | タイトル | 優先度 | 工数 | 依存 |
|----|---------|--------|------|------|
| P5-1 | 監視設定（基本） | Medium | 1日 | P4-1 |
| P5-2 | バックアップ設定 | Medium | 0.5日 | P2-5 |
| P5-3 | 運用ドキュメントの作成 | High | 1日 | P4-6 |

**成果物**: 監視設定、バックアップシステム、運用ドキュメント

### Phase 6: 最終確認とドキュメント化（3タスク、1-2人日）

| ID | タイトル | 優先度 | 工数 | 依存 |
|----|---------|--------|------|------|
| P6-1 | エンドツーエンドテスト | Critical | 0.5日 | P4-5, P3-2 |
| P6-2 | セキュリティチェック | High | 0.5日 | P3-5 |
| P6-3 | デプロイメントチェックリストの作成 | Medium | 0.5日 | P6-1, P6-2 |

**成果物**: E2Eテストレポート、セキュリティ監査、デプロイチェックリスト

## 🎯 クリティカルパス

1. P0-1 → P0-2 → P0-3 (環境準備)
2. P1-1 → P1-2 → P1-3/P1-4 → P1-5 (ビルド)
3. P2-1 → P2-2 → P2-4 → P2-6 (デプロイ)
4. P3-1 → P3-2 (ネットワーク)
5. P4-1 → P4-3 → P4-5 (テスト)
6. P6-1 (最終確認)

## 📋 優先度別タスク数

| 優先度 | タスク数 | パーセンテージ |
|--------|----------|----------------|
| Critical | 11 | 35.5% |
| High | 13 | 41.9% |
| Medium | 7 | 22.6% |

## 🛠️ 必須ツール・依存関係

### システムレベル
- Ubuntu 22.04 LTS (ARM64)
- 4GB+ RAM
- 2+ vCPU

### ビルドツール
- Rust 1.70+ (rustup)
- Go 1.23+
- Python 3.10+
- protobuf-compiler
- buf (optional)

### ランタイム依存
- youki v0.3.3
- Caddy
- SQLite3
- LVM2
- cryptsetup
- Tailscale

## 🔥 リスクトップ5

1. **ARM64特有のビルドエラー** (High)
   - 対策: 事前テストビルド、クロスコンパイル準備

2. **youki ARM64互換性** (Medium)
   - 対策: runcをフォールバック準備

3. **OCI A1リソース不足** (Medium)
   - 対策: 最小要件確認、スケールアップ準備

4. **ネットワーク遅延** (Low)
   - 対策: Tailscale使用

5. **SSL証明書取得失敗** (Low)
   - 対策: 自己署名証明書フォールバック

## 📞 よく使うコマンド

```bash
# サービス確認
systemctl status capsuled-engine capsuled-client

# ログ確認
journalctl -u capsuled-engine -f
journalctl -u capsuled-client -f

# API確認
curl -H 'X-API-Key: YOUR_KEY' http://localhost:8080/health

# ファイアウォール確認
sudo ufw status verbose

# リソース確認
df -h
free -h
top

# プロセス確認
ps aux | grep capsuled

# ネットワーク確認
ss -tuln
netstat -tuln

# Tailscale確認
tailscale status --peers
```

## 📝 チェックリスト（簡易版）

- [ ] Phase 0: OCI A1インスタンス起動完了
- [ ] Phase 0: 全依存パッケージインストール完了
- [ ] Phase 1: Wasmビルド成功
- [ ] Phase 1: Engineビルド成功
- [ ] Phase 1: Clientビルド成功
- [ ] Phase 2: バイナリデプロイ完了
- [ ] Phase 2: systemdサービス設定完了
- [ ] Phase 3: ファイアウォール設定完了
- [ ] Phase 3: Tailscale接続確認
- [ ] Phase 4: サービス起動確認
- [ ] Phase 4: API疎通確認
- [ ] Phase 4: コンテナデプロイテスト成功
- [ ] Phase 5: 監視設定完了
- [ ] Phase 5: 運用ドキュメント完成
- [ ] Phase 6: E2Eテスト完了
- [ ] Phase 6: セキュリティチェック完了

## 🔗 関連ドキュメント

- [OCI_A1_DEPLOYMENT_TASKS.yaml](./OCI_A1_DEPLOYMENT_TASKS.yaml) - 詳細タスク定義
- [OCI_A1_DEPLOYMENT_README.md](./OCI_A1_DEPLOYMENT_README.md) - デプロイガイド
- [README.md](../README.md) - プロジェクト概要
- [TODO.md](../TODO.md) - 全体ロードマップ

---

**最終更新**: 2025-11-17
**バージョン**: 1.0.0
**合計タスク**: 31
**合計工数**: 15-20 人日
