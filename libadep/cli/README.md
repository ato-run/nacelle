# ADEP CLI

Rust製のADEPパッケージ操作ツールです。`init`/`keygen`/`build`/`sign`/`verify`/`pack`/`run`の各サブコマンドを提供し、v1.2仕様に沿ったアプリパッケージを10分以内に作成・実行できることを目標としています。

## セットアップ

```bash
cargo build --release
# または開発用
cargo build
```

## 主なワークフロー

### パッケージ作成

1. `adep init` – パッケージ雛形と`manifest.json`を生成
2. `adep keygen` – Ed25519鍵を生成し、必要ならマニフェストへ公開鍵を反映
3. `adep build` – `dist/`の成果物を走査し`manifest.files`を更新
4. `adep sign` – パッケージ全体のSHA-256を算出して`_sig/developer.sig`を作成
5. `adep verify` – マニフェストハッシュ・ファイル・署名を検証
6. `adep pack` – `.adep`アーカイブを生成

### 実行（NEW!）

7. `adep run` – 署名検証済みパッケージをローカルHTTPサーバーで実行

```bash
# 静的アプリ（manifest.jsonにruntimeなし）
# → http://localhost:3000（デフォルト）

# Containerアプリ（manifest.jsonにruntime/platform指定）
# → http://localhost:8000（自動ポート検出 8000-8010）
adep run

# カスタムポート
adep run --port 8080

# 検証スキップ（開発モードのみ）
adep run --skip-verify
```

## クイックスタート

```bash
# 1. 新規プロジェクト作成
mkdir my-app && cd my-app
adep init --app-name "My App"

# 2. ビルド成果物を配置（既存のnpm/webpack等）
# npm run build && cp -r dist/* .

# または手動で dist/ にファイルを配置
mkdir -p dist
echo '<html><body>Hello ADEP!</body></html>' > dist/index.html

# 3. ADEP化
adep build
adep keygen
adep sign
adep verify

# 4. 依存カプセルを取得・検証（pack.profile=dist+cas が既定）
adep deps pull --reference "$(jq -r '.dep_capsules[0]' manifest.json)"
adep deps verify --json
adep deps resolve
# pnpm/offline 復元（install は --offline / --frozen-lockfile 相当を内部で実行）
adep deps install

# 5. ローカル実行（Fail-Closed: 依存不足なら RUN-CACHE-MISS）
adep run
# 静的アプリ: http://localhost:3000
# Containerアプリ: http://localhost:8000（ポート自動検出）

# 6. 配布用アーカイブ作成
adep pack

# カスタム CAS を使う場合（例: manifest.x-cas の指定に従う）
adep deps verify --index cas/index.json --blobs-dir cas/blobs
```

`adep deps verify --json` の出力例:

```json
{
  "count": 3,
  "total_bytes": 24576,
  "entries": [
    { "path": "dist/runtime.wasm", "bytes": 12288 },
    { "path": "dist/app.js", "bytes": 8192 },
    { "path": "dist/asset.css", "bytes": 4096 }
  ]
}
```

### 依存カプセル (OCI) の Push / Pull とオフライン復元

1. 依存インデックスとキャプセルを生成

   ```bash
   adep deps capsule --root . --key ~/.adep/keys/deps.json
   ```

2. 署名鍵ベースの認証で OCI/ORAS レジストリへ push

   ```bash
   # 例: ローカルで起動したテストレジストリ
   export ADEP_REGISTRY_ALLOW_INSECURE=1          # http:// を許可（本番では使用しない）
   adep deps push \
     --root . \
     --capsule cas/capsule-manifest.json \
     --registry http://127.0.0.1:5000 \
     --reference sample/deps:v1 \
     --cas-dir cas
   ```

   認証ヘッダは以下の優先順で自動投入されます（明示的な値は `Authorization` に設定）:

   - `ADEP_REGISTRY_AUTH_HEADER` …… 完全なヘッダ文字列を指定
   - `ADEP_REGISTRY_TOKEN` …… `Bearer <token>`
   - `ADEP_REGISTRY_USERNAME` / `ADEP_REGISTRY_PASSWORD` …… Basic 認証
   - 上記が未設定の場合は `~/.adep/keys/deps.json` の公開鍵から `AdepKey <fingerprint>` を送信

   TLS 検証を無効化する場合は `ADEP_REGISTRY_ALLOW_INVALID_CERTS=1` を利用できます（開発用途のみ）。

3. pull → resolve → install で完全オフライン復元

   ```bash
   adep deps pull \
     --registry oci://ghcr.io \
     --reference sample/deps:v1 \
     --cas-dir ~/.adep/cas

   adep deps resolve \
     --root . \
     --capsule ~/.adep/cas/capsule-manifest.json \
     --cas-dir ~/.adep/cas \
     --output deps-cache

    adep deps install \
      --root . \
      --capsule ~/.adep/cas/capsule-manifest.json \
      --cas-dir ~/.adep/cas \
      --output deps-cache \
      --dry-run            # 実際に pip/pnpm を実行する場合はオプションを外す
   ```

   `resolve` は CAS に格納された zstd 圧縮アーティファクトを検証しつつ、`deps-cache/python/wheels/` や `deps-cache/node/store/` に復元します。`install --dry-run` は実行コマンドを確認するだけなので、CI では `--dry-run`、本番ではオプションを外して使用します。

   `adep deps install` は `ADEP_DEPSD_ENDPOINT` が未設定の場合、同梱された `depsd` バイナリを自動起動し gRPC 経由で pip / pnpm を実行します。`ADEP_DEPSD_BIN` でバイナリパスを上書きでき、`ADEP_DEPSD_AUTOSTART=0` を設定すると自動起動を無効化して既存デーモンへ接続します。処理結果は JSON Lines 形式の監査ログ（`ADEP_AUDIT_LOG`、デフォルトは `~/.adep/logs/deps.audit.jsonl`）および Prometheus テキストフォーマットのメトリクス（`ADEP_METRICS_LOG`）として出力され、失敗時は `E_ADEP_DEPS_*` 形式のエラーコードが CLI に表示されます。

### depsd サービスの使い方

`adep deps install` を安全に実行するためのサイドカーとして gRPC サービス `depsd` が同梱されています。自動起動で十分なケースがほとんどですが、CI やデバッグで直接扱う場合は次を参考にしてください。

1. **バイナリのビルド**
   ```bash
   cargo build -p adep-depsd --bin depsd
   ```
   実行すると `target/debug/depsd`（または `target/release/depsd`）が生成されます。自動起動の環境でも 1 回はこのビルドを済ませておくと安心です。

2. **CLI からの自動起動**
   追加設定なしでも `adep deps install` が depsd を起動します。カスタマイズする場合は次の環境変数を利用してください。

   | 変数名 | 役割 |
   | --- | --- |
   | `ADEP_DEPSD_BIN` | depsd バイナリのパスを明示すると自動起動時にそれを使用します。CI では `ADEP_DEPSD_BIN=target/debug/depsd` が便利です。 |
   | `ADEP_DEPSD_AUTOSTART` | `0`/`false`/`off` を指定すると自動起動を無効化し、既存のエンドポイントにのみ接続します。 |
   | `ADEP_DEPSD_ENDPOINT` | 自動起動を抑止して手動起動した depsd（例: `127.0.0.1:50052`）へ接続します。 |

3. **監査ログとメトリクス**
   | 変数名 | 既定値 | 内容 |
   | --- | --- | --- |
   | `ADEP_AUDIT_LOG` | `~/.adep/logs/deps.audit.jsonl` | JSON Lines 形式の監査イベントが追記されます。CI では一時ディレクトリを指定すると後片付けが容易です。 |
   | `ADEP_METRICS_LOG` | 未設定（出力なし） | Prometheus テキスト形式でメトリクスを出力します。メトリクス連携が必要な場合にのみ設定してください。 |

4. **手動起動例**
   ```bash
   ./target/debug/depsd &                     # TCP 127.0.0.1:50051 で待ち受け
   export ADEP_DEPSD_ENDPOINT=127.0.0.1:50051
   adep deps install --root . --capsule cas/capsule-manifest.json --cas-dir cas
   ```
   Unix ソケットで待ち受けたい場合は `ADEP_DEPSD_LISTEN=unix:///tmp/adep-depsd.sock ./target/debug/depsd` のように起動します（Unix 系 OS のみ）。

4. 取得後の整合性検証

   ```bash
   adep deps verify \
     --index ~/.adep/cas/index.json \
     --blobs-dir ~/.adep/cas/blobs
   ```

エラー例:

- `remote registry reported manifest digest ...` …… push した manifest とレジストリが返す digest が一致しない場合（レジストリ側の改竄検知）。
- `capsule entry '...' missing metadata.filename` …… `deps resolve` で wheel/tarball のファイル名が見つからない場合はインデックスを再生成してください。
- `registry blob 'sha256:...' missing` …… pull 時にレジストリへ未 push のカプセル層がある場合。push の成否を再確認します。

## v1.2 Manifest ハイライト

```jsonc
{
  "schemaVersion": "1.2",
  "pack": {
    "profile": "dist+cas",
    "engines": { "node": ">=18 <23" },
    "pm": "pnpm"
  },
  "network": {
    "egress_allow": ["https://cas.example.net"],
    "egress_id_allow": [
      {
        "type": "spiffe",
        "value": "spiffe://example.net/deps/cas",
        "scheme": ["https"],
        "ports": [443],
        "trust": {
          "mode": "spiffe",
          "spiffe_id": "spiffe://example.net/deps/cas"
        }
      }
    ],
    "http_proxy_dev": true,
    "dev_resolver": "spire-lite"
  },
  "x-cas": {
    "index": "cas/index.json",
    "blobs": "cas/blobs/",
    "policy": {
      "trustDomain": "official",
      "verify": ["compressed", "raw"]
    }
  },
  "dep_capsules": [
    "oci://ghcr.io/example/adep-deps@sha256:..."
  ],
  "deps": {
    "python": {
      "requirements": "requirements.lock",
      "source": "cas://pypi/wheels",
      "install": { "mode": "offline", "target": ".venv", "no_deps": true }
    },
    "node": {
      "lockfile": "pnpm-lock.yaml",
      "store": "cas://pnpm",
      "install": { "mode": "offline", "frozen_lockfile": true }
    }
  },
  "files": [
    {
      "path": "dist/runtime.wasm",
      "sha256": "<raw_sha256>",
      "size": 1024,
      "role": "runtime",
      "compressed": {
        "alg": "zstd",
        "size": 512,
        "sha256": "<compressed_sha256>"
      }
    }
  ]
}
```

- `x-cas`: 共有 CAS インデックスとポリシーを宣言し、`adep deps verify` が zstd 圧縮→展開の順でハッシュを厳格チェックします（その他のアルゴリズムはエラーを返却）。
- `dep_capsules`: OCI/ORAS または `.adep` カプセルを参照し、オフラインで依存を復元します。
- `deps.python` / `deps.node`: lockfile と install モードを明示し、`offline` 実行時の厳格復元を強制します。
- `files[].compressed`: 圧縮アルゴリズム・サイズ・圧縮ハッシュを記録し、ランタイムおよび CLI が `compressed → raw` の二重検証を実施、ハッシュ不一致時はエラー詳細を表示します。
- `pack.profile`: パッケージング戦略を定義します。既定は `dist+cas` で、`dist/`（ビルド成果物）＋ `dep_capsules` 参照を保持し、依存の実体は CAS 経由で配布します。  
  - `pack.engines`: PATH 解決するランタイム要件を宣言（例: Node 18〜22）。バージョン外は `RUN-ENGINE-NOTFOUND`。
  - `pack.pm`: 使用するパッケージマネージャ（`pnpm` / `npm` / `pip` など）を宣言。

### パッケージング・プロフィール（`pack.profile`）

| profile        | 何が入る                                      | 配布サイズ | 起動時ネット | 主な用途             |
| -------------- | --------------------------------------------- | ---------- | ------------ | -------------------- |
| `dist`         | `dist/` のみ                                  | 最小       | 不要         | 静的アプリ、SSR不要     |
| `dist+cas`※    | `dist/` + **dep_capsules の参照**              | 小〜中      | 不要         | 標準（SSR/依存あり）     |
| `frozen`       | `dist/` + 最小限の `node_modules/` 等を同梱     | 中〜大      | 不要         | デモ／エッジ／一時配布    |

※既定。依存の実体は `.webcapsule` に埋め込まず、CAS の Dep Capsule を pull→verify→resolve→install で復元します。

### `.webcapsule` の最小構成

```
app.webcapsule
├─ manifest.json         # pack.profile=dist+cas が既定
├─ dist/                 # ★必須: ビルド済み成果物一式
├─ sbom.json             # ソース同梱時は必須（SPDX/CycloneDX）
└─ _sig/attestation.json # DSSE (package_digest / manifest_digest)
```

依存カプセルは **参照のみ**（`dep_capsules[]`）。実体はローカル CAS (`~/.adep/cas` など) に保存し、`adep deps` で完全オフライン復元します。

### `start.command` の書き方

- **推奨**: JSON 配列（例: `"command": ["node","server.js"]`）。シェル解釈を避け、PATH 解決を CLI に任せます。
- **互換**: 文字列形式も受理しますが、将来 deprecate 予定です。`pack.profile` の方針に従い、起動時の `npm install` やビルド処理は記述しないでください。
- CLI は非ログインシェルで実行し、`health.url` は loopback (127.0.0.1) のみを想定します。

### ネットワークポリシー（Fail-Closed）

- `network.egress_id_allow` は **ID + scheme/port** の合致が必須。1つでも合致しない通信は **403 / `EGRESS-403`** で拒否され、監査ログに一致/不一致が出力されます。
- `x-cas` はローカル CAS の index/blobs 参照ヒントです。`adep deps verify` はここで宣言したポリシー（圧縮種別、信頼ドメイン）を使用し、ハッシュ不一致や ZipSlip を検知すると `E_ADEP_DEPS_*` を返します。

### 主な診断コード

| コード                     | 説明 / 対処                                                                 |
| ------------------------- | --------------------------------------------------------------------------- |
| `RUN-CACHE-MISS`          | 依存カプセルが CAS に存在しない → `adep deps pull/resolve/install` を実行 |
| `RUN-ENGINE-NOTFOUND`     | `pack.engines` のバージョン範囲を満たす実行環境が PATH に無い              |
| `E_ADEP_DEPS_ZIPSLIP` 等 | 依存展開時の安全性違反（ZipSlip/展開比率） → 依存カプセルを再生成         |
| `EGRESS-403`              | 許可されていない送信先にアクセス → `network.egress_id_allow` を確認        |

## セキュリティ機能

### Content Security Policy (CSP)
`adep run` は自動的に以下のCSPを適用します：
- `script-src 'self'` – 外部スクリプト禁止
- `connect-src` – manifest.jsonの`egress_allow`に基づく制限
- パストラバーサル攻撃の防止

### 署名検証
実行前に以下を検証：
1. `role="runtime"` ファイルの先行検証
2. パッケージ全体のSHA-256一致
3. 開発者署名の正当性

### パッケージダイジェスト計算アルゴリズム

ADEPパッケージの同一性検証に使用される`package_sha256`は、以下のアルゴリズムで計算されます：

#### 計算対象ファイル
- `manifest.json`
- `manifest.json.sha256`
- `dist/` 配下の全ファイル
- `src/` 配下の全ファイル（存在する場合）
- `sbom.json`（存在する場合）
- **除外**: `_sig/` ディレクトリ（署名自体は計算対象外）

#### 計算手順

```
1. 対象ファイルを走査し、各ファイルについて以下を記録：
   - 相対パス（Unix形式: "/"区切り）
   - 個別ファイルのSHA-256ハッシュ（hex形式）

2. 全エントリをパス名でソート（辞書順）

3. SHA-256ハッシュ関数に以下を順次入力：
   for each (path, hash) in sorted_entries:
     hasher.update(path_bytes)
     hasher.update(0x00)          // NULL区切り
     hasher.update(hash_hex_bytes)
     hasher.update(0x00)          // NULL区切り

4. 最終ハッシュ値をhex文字列として出力
```

#### 具体例

```
パッケージ構成:
  manifest.json          (SHA-256: abc123...)
  manifest.json.sha256   (SHA-256: def456...)
  dist/index.html        (SHA-256: 789abc...)
  dist/app.js            (SHA-256: bcd890...)

ソート後のエントリ:
  1. "dist/app.js" → "bcd890..."
  2. "dist/index.html" → "789abc..."
  3. "manifest.json" → "abc123..."
  4. "manifest.json.sha256" → "def456..."

入力データ（バイト列）:
  "dist/app.js" 0x00 "bcd890..." 0x00
  "dist/index.html" 0x00 "789abc..." 0x00
  "manifest.json" 0x00 "abc123..." 0x00
  "manifest.json.sha256" 0x00 "def456..." 0x00

最終ハッシュ:
  SHA-256(上記バイト列) → package_sha256
```

#### 相互運用性のための注意点

1. **パス区切り**: 必ず `/` を使用（Windows環境でも `\` は使わない）
2. **パスのエンコーディング**: UTF-8
3. **ハッシュ値の形式**: 小文字16進数文字列（64文字）
4. **ソート順**: バイト単位の辞書順（UTF-8バイト列として比較）
5. **NULL区切り**: パスとハッシュの間、およびエントリ間に `0x00` を挿入

#### 実装参考

```rust
// src/package.rs の compute_package_digest() を参照
pub fn compute_package_digest(root: &Path) -> Result<String> {
    let mut entries: Vec<(String, String)> = Vec::new();
    
    // 1. ファイル走査
    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() { continue; }
        let rel_path = /* 相対パス計算 */;
        if !should_include_in_digest(rel_path)? { continue; }
        
        let hash = hash_file_hex(full_path)?;
        entries.push((to_unix_path(rel_path), hash));
    }
    
    // 2. ソート
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    
    // 3. ハッシュ計算
    let mut hasher = Sha256::new();
    for (path, hash) in entries {
        hasher.update(path.as_bytes());
        hasher.update(&[0u8]);
        hasher.update(hash.as_bytes());
        hasher.update(&[0u8]);
    }
    
    Ok(hex::encode(hasher.finalize()))
}
```

この方式により、同一のファイルセットを持つパッケージは、プラットフォームや実装に関わらず同じ`package_sha256`を生成します。

## テスト

```bash
cargo test
```

### テストカバレッジ

- **ユニットテスト (6)**: パス検証・ロール推定・署名ラウンドトリップなど
- **統合テスト (11)**: 
  - 完全ワークフロー（init → keygen → build → sign → verify → pack）
  - role推論の正確性（.wasm, *.worker.js → runtime）
  - manifest.jsonデフォルト値生成
  - ファイル改ざん検出
  - 署名破壊検出
  - manifest hash不整合検出
  - developer_key不一致検出
  - SBOM必須チェック
  - key_rotation検証
  - capabilities構文検証
  - manifest メタデータ整合性検証

全17テストが実行時間0.5秒以内で完了します。

## トラブルシューティング

### "dist directory does not exist"
→ `npm run build` などでビルド成果物を生成してから `adep build` を実行してください。

### "manifest.json already exists"
→ `--force` フラグで上書き: `adep init --force`

### 署名検証失敗
→ ファイルを変更した場合は再度 `adep sign` を実行してください。

### CSP違反（ブラウザコンソール）
→ `manifest.json` の `egress_allow` に必要なドメインを追加し、再署名してください。

## コマンド一覧

| コマンド | 説明 | 実行時間 |
|---------|------|----------|
| `init` | 新規パッケージ初期化 | < 1秒 |
| `keygen` | 開発者鍵生成 | < 1秒 |
| `build` | files配列更新 | 1-3秒 |
| `sign` | パッケージ署名 | 1-2秒 |
| `verify` | 検証（runtime優先） | 1-3秒 |
| `pack` | .adepアーカイブ作成 | 2-5秒 |
| `run` | ローカルHTTPサーバーで実行 | 即時起動 |
| `doctor` | システム要件チェック | < 1秒 |

**総工数**: 既存アプリのADEP化は **5-10分以内** に完了します。

---

## Week 4 - Container Runtime (NEW!)

### システム要件

**必須**:
- Container Engine: Podman 3.0+ or Docker 20.10+
- Python: 3.9-3.12 (コンテナアプリ実行時)
- OS: macOS 11+, Ubuntu 20.04+, Fedora 35+

### 環境診断

```bash
# システム要件をチェック
adep doctor

# 出力例:
# 🏥 ADEP Environment Check
# 
# Container Engines:
#   ✅ podman    (preferred)
#   ✅ docker    (available)
# 
# Runtime Tools:
#   ✅ python3   (3.11.0)
# 
# Cache Directory:
#   ✅ ~/.adep/  (exists)
# 
# Port Availability:
#   ✅ Ports      11/11 available (8000-8010)
# 
# ✅ Your system is ready to run ADEP containers!

# Podmanインストール（macOS）
brew install podman
podman machine init
podman machine start

# Podmanインストール（Linux）
sudo apt install podman  # Ubuntu/Debian
sudo dnf install podman  # Fedora/RHEL
```

### Python コンテナアプリの実行

```bash
# サンプルアプリで動作確認
cd sample-apps/python-weather-api
adep run

# 別ターミナルでテスト
curl http://localhost:8000/
curl http://localhost:8000/weather?city=Tokyo

# 監査ログ確認（外部通信が記録される）
cat ~/.adep/audit.log
```

### 機能一覧

| 機能 | 状態 | 説明 |
|------|------|------|
| 署名検証統合 | ✅ | `run` 時に自動検証（デフォルトON） |
| runtime/platform分離 | ✅ | manifest.json構造の将来互換設計 |
| エンジン自動検出 | ✅ | Podman優先、Docker自動フォールバック |
| ポート自動リトライ | ✅ | 8000-8010の範囲で自動検出 |
| 監査ログ記録 | ✅ | ~/.adep/audit.log にJSON Lines形式 |
| pip cache/wheels優先 | ✅ | ~/.adep/pip-cache, dist/wheels/ 優先利用 |
| エラー三点セット | ✅ | ID/原因/対処/Docs を全エラーに付与 |
| Linux環境対応 | ✅ | --add-host で監査プロキシ到達可能 |

### トラブルシューティング

#### "ADEP-ENGINE-NOTFOUND"
→ `adep doctor` でエンジンの状態を確認  
→ Podman または Docker をインストール

#### "ADEP-PORT-UNAVAILABLE"
→ ポート 8000-8010 がすべて使用中  
→ `lsof -i :8000-8010` で使用中のプロセスを確認

#### 監査ログが記録されない
→ manifest.json の `network.http_proxy_dev` が `true` か確認  
→ Linux環境の場合は最新版（--add-host対応）を使用

---

## 実装状況

### ✅ ADEP v1.2仕様準拠

| 機能 | 状態 | 備考 |
|------|------|------|
| manifest.json生成 | ✅ | UUIDv4, semver, files[] |
| Ed25519署名 | ✅ | developer.sig (99B) |
| role推論 | ✅ | .wasm, *.worker.js → runtime |
| runtime先行検証 | ✅ | Progressive Verify基礎 |
| パッケージSHA-256 | ✅ | Content-addressed |
| CSP自動適用 | ✅ | egress_allow連携 |
| パストラバーサル防御 | ✅ | セキュリティ対策 |

### 📊 パフォーマンス

- **ビルド時間**: < 3秒（Rust 2021）
- **署名生成**: < 1秒（Ed25519）
- **検証時間**: < 3秒（SHA-256 + 署名検証）
- **HTTPサーバー起動**: 即時（tiny_http）
- **メモリ使用量**: < 20MB（実行時）

### 🧪 テスト

- **ユニットテスト**: 6/6 pass
- **統合テスト**: 11/11 pass
- **実アプリ検証**: Next.js/React/Vue対応準備完了

### 🔍 バリデーション機能

- **capabilities構文検証**: `?` 接尾辞チェック、不正文字検出
- **egress_allow URL検証**: https://, wss:// プロトコル強制
- **version.channel検証**: stable/beta/canary のみ許可
- **key_rotation検証**: previous_key形式チェック
- **SBOM要件**: src/ 存在時の sbom.json 必須チェック

---

## Week 3準備完了

### 完成した機能
- ✅ パッケージ作成→実行の完全パイプライン
- ✅ ADEP v1.2 Final Draft完全準拠
- ✅ セキュリティ機能実装（署名検証+CSP+防御）
- ✅ 統合テスト実装
- ✅ egress_allow → CSP連携

### 次のマイルストーン
1. **実アプリ移植実験**（Next.js/React/Vue 3種）
2. **Beta開発者10名での検証**
3. **工数計測とフィードバック収集**

---

## Multi-ADEP Phase 1A (NEW!)

複数の独立したADEPパッケージが、開発環境において安全にlocalhost経由で通信できる機能を提供します。

### 主な機能

#### 1. Dev Mode（開発モード）

localhost通信を許可する明示的な開発モード：

```bash
# Dev mode を有効化（環境変数で明示的にオプトイン）
export ADEP_ALLOW_DEV_MODE=1

# manifest.json に egress_mode を追加
{
  "network": {
    "egress_mode": "dev",
    "egress_allow": [
      "http://localhost:8001",
      "https://api.example.com"
    ]
    }
}
```

## CI / テスト

```bash
# フォーマッタと静的解析
cargo fmt
cargo clippy --all-targets -- -D warnings

# 単体テスト + 統合テスト
cargo test

# 依存カプセルの OCI ラウンドトリップ統合テストのみ実行
cargo test --test integration_test -- --nocapture
```

- `integration_test` にはローカルメモリ上に立ち上がるダミー ORAS レジストリが含まれており、ネットワークに依存しません。
- `ADEP_REGISTRY_ALLOW_INSECURE=1` を付与すると `http://` ベースのテストレジストリに push/pull できます。CI では安全のため `https://` を利用してください。
- 追加でクラスタ向けの自動テストを行う場合は `--features e2e` など機能フラグを組み合わせて実行してください。

**セキュリティ設計**:
- デフォルトでブロック（環境変数 `ADEP_ALLOW_DEV_MODE=1` 必須）
- ブロックリストポート: `[22, 25, 80, 443]`（SSH, SMTP, HTTP, HTTPS）
- 使用記録: `~/.adep/dev_mode_usage.log` に自動記録

#### 2. 依存関係管理

manifest.json で他のADEPパッケージへの依存を宣言：

```json
{
  "dependencies": {
    "adep": [
      {
        "name": "python-api",
        "family_id": "550e8400-e29b-41d4-a716-446655440000",
        "port": 8001
      }
    ]
  }
}
```

**自動環境変数注入**:
```bash
# adep run 実行時に自動設定
ADEP_DEP_PYTHON_API_PORT=8001
ADEP_DEP_PYTHON_API_URL=http://localhost:8001
```

#### 3. Compose コマンド

Docker Composeライクな複数ADEPの統合管理：

```bash
# compose.yaml 作成
cat > compose.yaml << 'EOF'
version: "1.0"

services:
  python-api:
    adep_root: ./sample-apps/python-weather-api
    healthcheck:
      port: 8000
      timeout_secs: 10
  
  weather-dashboard:
    adep_root: ./sample-apps/weather-dashboard-adep
    depends_on:
      - python-api
    healthcheck:
      port: 3000
      timeout_secs: 10
EOF

# 全サービスを起動（依存関係順）
export ADEP_ALLOW_DEV_MODE=1
adep compose up

# 停止
adep compose down

# 状態確認
adep compose ps
```

**機能**:
- ✅ トポロジカルソートによる依存関係順起動
- ✅ ヘルスチェック（ポート待機）
- ✅ プロセス管理（Ctrl+C で一括停止）
- ✅ 循環依存検出

#### 4. レジストリ（プロジェクトローカル）

実行中のADEPを追跡：

```bash
# レジストリファイル: .adep/local-registry.json
# プロジェクトごとに独立（マルチユーザー安全）

{
  "running_adeps": [
    {
      "name": "python-api",
      "family_id": "uuid-v4",
      "version": "1.0.0",
      "pid": 12345,
      "ports": {
        "primary": 8001
      },
      "started_at": "2025-10-02T12:34:56Z",
      "manifest_path": "./python-api/manifest.json"
    }
  ]
}
```

**特徴**:
- アトミック保存（一時ファイル → rename）
- PID検証（kill -0）
- 起動時のデッドプロセス自動削除

### 検証機能

#### localhost URL検証

```rust
// ✅ 許可される形式
"http://localhost:3000"
"http://127.0.0.1:8080"

// ❌ ポート必須
"http://localhost"        // エラー

// ❌ ブロックリスト
"http://localhost:22"     // SSH
"http://localhost:80"     // HTTP（システム衝突）
```

#### egress_mode 検証

```json
// ✅ localhost通信にはdev mode必須
{
  "network": {
    "egress_mode": "dev",
    "egress_allow": ["http://localhost:8000"]
  }
}

// ❌ egress_mode なしではlocalhost URL拒否
{
  "network": {
    "egress_allow": ["http://localhost:8000"]
  }
}
// → ADEP-LOCALHOST-REQUIRES-DEV-MODE エラー
```

### Phase 2 で予定されている機能

以下は Phase 1A の試用結果を見て実装を判断：

- **WebSocket プロキシ**: ws://, wss:// サポート（550-600行）
- **ファイルロック**: 同時書き込み保護（fs2 クレート使用）
- **PID検証強化**: タイムスタンプチェック（/proc, sysctl）
- **循環依存検出**: manifest レベルの検証

### トラブルシューティング

#### "ADEP-DEV-MODE-BLOCKED"
```bash
# Dev mode が有効化されていない
export ADEP_ALLOW_DEV_MODE=1
```

#### "ADEP-DEP-NOT-RUNNING"
```bash
# 依存ADEPが起動していない
cd ../dependency-adep && adep run
```

#### "ADEP-WEBSOCKET-NOT-SUPPORTED"
```bash
# WebSocket は Phase 2 で実装予定
# 現在は HTTP/HTTPS のみサポート
```

### 実装量

- **Day 1-2**: 基礎構造 + 検証ロジック（200行）
- **Day 3**: レジストリ + 依存検証（130行）
- **Day 4**: Compose + プロセス管理（280行）
- **Day 5**: テスト + ドキュメント（120行）
- **合計**: 約730行

### テスト

```bash
# ユニットテスト実行
cargo test

# Phase 1A のテスト
# - localhost URL検証
# - ブロックリストポート
# - egress_mode 検証
# - 環境変数注入
# - レジストリ操作
# - 依存関係解決（トポロジカルソート）
```

---
