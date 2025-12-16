# Deploy / Launch / Share 100 点実装計画 (onescluster + capsuled)

**最終更新:** 2025-12-16  
**目的:** onescluster（capsuled）上で、

- **Deploy**: Drag&Drop（ソース/アーティファクト）→ Build → Registry → Deploy
- **Launch**: Scale-to-Zero / On-demand 復帰 / Router 連携
- **Share**: Marketplace / Fork / Access Control
  を「最短で動く」形で段階的に実装する。

この計画は **既にリポジトリ内に存在する実装**（Caddy Admin API クライアント、`libadep` の packager+publish、Engine 側の deploy 受け口、署名/SBOM 基盤、OCI client 等）を前提に、外部 OSS は **境界が明確に切れる箇所にのみ**導入する。

---

## 0. いま既に使える「土台」

### 0.1 Router (Caddy)

- Caddy Admin API クライアント（Go）: [client/pkg/networking/caddy/caddy.go](client/pkg/networking/caddy/caddy.go)
  - `EnsureBaseConfig()` で `apps/http/servers/srv0` を初期化
  - `AddRoute(capsuleID, host, upstreamPort)` で `@id=capsule-<id>` の route を作成/更新
  - `RemoveRoute(capsuleID)` で削除

一次情報（Caddy 公式）では `/config` の同時更新に ETag/If-Match を使うべき、と明記されている。現状コードは簡易 existence check で更新するため、並行 deploy 時に衝突しうる（後述の改善タスクで吸収）。

### 0.2 Deploy Control-plane→Engine

- Control plane 側 deploy handler（Go）: [client/pkg/api/deploy_handler.go](client/pkg/api/deploy_handler.go)
  - VRAM 予約 → Engine gRPC（DeployCapsule） → DB 記録
- Engine 側 gRPC 受け口（Rust）: [engine/src/coordinator_service.rs](engine/src/coordinator_service.rs)
  - `deploy_workload` が `adep_json` を受け取り `CapsuleManager` に委譲
  - `manifest.metadata["digest"]` を参照（digest が来たら使える状態）

### 0.3 Build+Push（既存実装）

- `libadep` の packager が **Dockerfile を自動生成**し、タグ/キャッシュ/push を計画: [libadep/core/src/packager.rs](libadep/core/src/packager.rs)
- `libadep` CLI が `docker buildx build --push` を実行（registry cache も設定）: [libadep/cli/src/commands/capsule.rs](libadep/cli/src/commands/capsule.rs)

このため、MVP の Build は「CNB/Buildpacks/BuildKit 直叩き」を今すぐ導入しなくても成立する（将来の差し替えは可能）。

### 0.4 Supply-chain（下地）

- 署名/検証（Ed25519）や SBOM 必須の考え方が `libadep` 側に存在する（tests もある）。
- 将来的に Sigstore/cosign へ寄せる余地もある（外部運用/ポリシーと接続しやすい）。

---

## 1. 目標状態（MVP→100 点）

### Deploy（MVP の「動く」定義）

1. Web からソース（zip/tar）をアップロードできる（再開可能）
2. アップロード完了で Build Job が作られ、ビルド&push が完了する
3. push されたイメージ（**digest 正本**）を指定して Engine に deploy できる
4. Deploy 完了後にアクセス URL が発行される

### Launch（MVP の「動く」定義）

1. deploy 直後に URL へアクセスできる（Caddy 経由）
2. 停止（scale-to-zero 相当）→ URL アクセスで復帰、までの道筋がある

### Share（MVP の「動く」定義）

1. アプリ（= OCI image digest + metadata）を「公開/非公開」で管理できる
2. 他ユーザーが fork（= 自分の名前空間にコピー）できる
3. Deploy はタグではなく digest を参照する（改ざん/再タグ付けの影響を受けにくい）

---

## 2. 外部 OSS 採用（最短で効くものだけ）

### 2.1 Upload: tus + tusd + Uppy

理由:

- tus は HTTP ベースの再開可能アップロード仕様（オフセット/競合のルールが明確）
- `tusd` は tus の公式 Go 実装で、Hooks/ストレージ拡張が前提
- Uppy はブラウザ側の実装が短く済み、tus plugin が成熟している

採用方針:

- **サーバ**: `tusd` を独立プロセスとして動かし、アップロード先ストレージは最初はローカルディスク（将来 S3 互換も可）
- **クライアント**: web 側は `@uppy/tus` を利用して tusd にアップロード

セキュリティ注意（仕様/ドキュメントより）:

- tus の `Upload-Metadata` はヘッダ値なので、ヘッダ注入等を避けるため「許可するメタデータキー」を制限する
- アップロード完了後に build へ渡す入力（ファイルパス、サイズ、SHA256 等）を Coordinator が検証する

### 2.2 Artifact/署名: cosign + ORAS（段階導入）

MVP では既存 Ed25519 署名（`libadep`）で始められるが、

- 署名・アテステーションの外部連携（ポリシー、透過ログ、CI 署名）を視野に
  段階的に cosign/ORAS を導入する。

---

## 3. 責務分離（壊れにくい境界）

### 3.1 コンポーネント

- **Web/UI**: アップロード開始、進捗表示、Deploy/Launch/Share の操作
- **Upload Service**: tusd（アップロード完了イベントを発火）
- **Coordinator (Go)**: Build/Deploy のオーケストレーション、状態管理（rqlite）、認可
- **Builder**: `libadep` packager + `docker buildx build --push`（まずはこれ）
- **Registry**: OCI registry（既存前提）
- **Engine (Rust)**: digest を pull して実行、状態レポート
- **Edge/Router**: Caddy（Admin API で route を管理）

### 3.2 状態の単一正 (SoT)

- rqlite（既存の StateManager）に、下記の状態を格納する:
  - Upload: upload_id, owner, status, artifact_path
  - Build: build_id, source_ref, status, logs_ref, output_image_digest
  - App: app_id, owner, visibility, image_digest, signature_ref
  - Deployment: capsule_id/workload_id, app_id, image_digest, node_id, route_host

---

## 4. 実装フェーズ（最短で縦切り）

### Phase A (Deploy MVP): Upload→Build→Push→Deploy を 1 本通す

**A-1: Upload service の導入（tusd）**

- `tusd` を docker-compose に追加し、アップロード保存先（ローカル）を確保
- tusd の **hook**（HTTP hook か post-finish hook）で Coordinator に「upload completed」を通知

**A-2: Coordinator 側 Build API**

- `POST /api/v1/builds`（内部用）: upload 完了イベントを受けて BuildJob 作成
- BuildJob は「source path」「owner」「capsule name」「namespace」「requested runtime」を持つ

**A-3: Builder 実行（libadep publish を流用）**

- Builder は `libadep` を呼び出し:
  - resolver→packager→docker buildx build --push
  - push 後に registry から digest を取得し、rqlite に保存

**A-4: Deploy で digest 正本を通す**

- Coordinator の Deploy で `manifest.metadata["digest"]` を必須化（tag 指定は非推奨）
- Engine は既に digest を受け取れるため、pull/実行の安定性が上がる

**A-5: Router 連携**

- deploy 成功時に Caddy route を追加（既存 `AddRoute` を使用）
- 衝突回避（後述 B-1）まで、MVP は「単一 Coordinator が更新」前提で運用

### Phase B (Launch): Scale-to-Zero / On-demand

**B-1: Caddy config 更新の衝突対策**

- Caddy 公式の推奨に従い、`GET /config/...` の `Etag` を取り、`If-Match` を付けて更新する（Optimistic Concurrency）
- ルート作成/更新/削除の全操作で適用

**B-2: On-demand 起動（最小）**

- まずは「Stopped→Start API」で復帰できるようにする（明示起動）
- 次に「HTTP アクセス → 復帰」は、
  - Caddy→Coordinator を経由する構成（Coordinator が必要なら起動してプロキシ）
  - もしくは Caddy のハンドラ構成でフォールバック
    のどちらかを採用（最小実装は前者）

### Phase C (Share): Marketplace / Fork / Access Control

**C-1: App モデル（公開/非公開）**

- App = {owner, visibility, image_digest, display_name, tags, description}
- Deploy は App を参照し、digest を確定する

**C-2: Fork**

- Fork = 既存 App の digest を自分の名前空間に複製（同一 digest の参照でもよい／必要なら re-tag）

**C-3: 署名**

- `libadep sign/verify` を足場に、
  - 署名を App に紐付け
  - 将来 cosign（key/identity）へ移行できるように signature_ref を抽象化

---

## 5. 自己批判（100 点化のための補強）

1. **アップロードを“普通の POST”で済ませない**: tus の仕様に沿って再開可能にすることで、UX と信頼性が一気に上がる。
2. **Caddy の config 競合**: 公式に ETag/If-Match が推奨されているため、ここは計画に含めて「衝突しない」運用に寄せる。
3. **tag 正本は危険**: 既に Engine は digest を受け取る前提があるので、SoT を digest に寄せるべき。
4. **Build を早く動かす**: `libadep` が `docker buildx --push` まで実装済みなので、最短はここを本線にする。
5. **Share は“UI”より“権限と参照”が先**: 公開/非公開・署名・digest 参照を先に固めると、後から Marketplace UI を足しても壊れにくい。
