# Capsuled Client

Capsuled のクライアント実装です。複数の Engine インスタンスを管理し、Capsule のデプロイメントを調整します。

## アーキテクチャ

- Go で実装
- gRPC クライアントとして Engine と通信
- Wasmer を使用して adep-logic.wasm を実行
- Master 選出とスケジューリングを実装

## ビルド

```bash
cd capsuled/client
go build -o bin/capsuled-client ./cmd/client
```

## マニフェストをデプロイ（RunPlan 優先）

`capsule-cli` はマニフェストを読み込み、可能なら `RunPlan`（proto）に正規化して Engine の `DeployCapsule` に送ります。
Engine 側が `run_plan` を未対応の場合は、互換のため `toml_content` にフォールバックします。

```bash
cd capsuled/client
go build ./cmd/capsule-cli

# 例: Engine が localhost:50051 で待ち受けている場合
./capsule-cli deploy --engine localhost:50051 ./verify.toml

# 互換検証用（古い Engine が [capsule]/[runtime] 形式を期待する場合）
./capsule-cli deploy --engine localhost:50051 ./verify_legacy.toml
```

## 実行

```bash
./bin/capsuled-client
```
