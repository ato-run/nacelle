# libadep Workspace

`libadep/` には ADEP の共通ライブラリ（`libadep-core` / `libadep-deps` / `libadep-cas` / `libadep-observability`）とメタクレート `libadep` が含まれます。CLI や Desktop からクロスリポで利用することを前提に、以下の開発手順を押さえてください。

## 前提ツール

| 用途 | macOS / Linux | 補足 |
|------|---------------|------|
| Rust | `rustup install stable` | テストは Rust 1.79+ を前提 |
| Protobuf | `brew install protobuf` / `sudo apt-get install protobuf-compiler` | `libadep-deps`（gRPC 生成）をビルドする場合は必須 |
| protoc 指定 | `PROTOC=/usr/local/bin/protoc` など | PATH に無い場合は環境変数でパスを明示 |

> **スタブ運用**  
> `libadep-deps` をビルドせずに `libadep-core` のみ確認したい場合は `cargo check -p libadep-core` を利用してください（`protoc` 不要）。  
> gRPC を含むテストが不要な CI では、`cargo check -p libadep-core` や `cargo test -p libadep-core` を個別ジョブとして実行することで `protoc` 依存を避けられます。

## コマンド例

```bash
# 依存込みでビルド（protoc 必須）
cargo check

# libadep-core だけを確認（protoc 不要）
cargo check -p libadep-core

# テスト
cargo test -p libadep-core
```

## よくあるエラー

| 症状 | 原因 / 対処 |
|------|-------------|
| `Could not find protoc` | 上記の Protobuf セットアップを実施し、`PROTOC` がパスを指すようにする |
| `dep_capsules must start with oci:// or adep://` | `manifest migrate` 実行時に `adep://autofill/...` へ自動置換されるため、ログに警告が出たか確認 |

## ディレクトリ構成

```
libadep/
├─ libadep/         # メタクレート（pub use）
├─ libadep-core/    # manifest / package / signing などコア API
├─ deps/            # gRPC クライアント・同期 defaults stub
├─ cas/             # CAS ユーティリティ
└─ observability/   # 監査ログユーティリティ
```
