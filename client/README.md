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

## 実行

```bash
./bin/capsuled-client
```
