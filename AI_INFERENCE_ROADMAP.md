# Capsuled AI Inference Evolution Roadmap

**バージョン:** 0.2.0 (Active Development)  
**更新日:** 2025-11-20  
**ステータス:** 実装・検証段階 (Phase 2.5 Active)

---

## 1. ビジョン: "The Distributed AI OS"

Capsuled を単なるコンテナオーケストレーターから、**コンシューマー GPU リソースを活用した、世界で最も効率的で低コストな分散 AI 推論プラットフォーム**へと進化させます。
このロードマップは `CAPSULED_ROADMAP.md` を拡張し、AI 推論特有の課題（モデルロード遅延、VRAM 残留リスク、分散推論）を解決するための技術的マイルストーンを定義します。

## 2. 戦略的柱 (Strategic Pillars)

### A. Inference Optimization (推論最適化)
LLM (Large Language Models) のコールドスタート問題を解決し、推論レイテンシを最小化します。
- **Core Value**: 「推論税 (Inference Tax)」の撤廃。
- **主要技術**: Host Path Volume Mounting (Model Caching), vLLM Integration, Model Pre-fetching.

### B. Sovereign GPU Security (主権型GPUセキュリティ)
不特定多数のノード（Personal Rigs）で他者のワークロードを実行する際の「データ残留リスク」を物理的に排除します。
- **Core Value**: 軍事レベルのセキュリティ証明 (Proof of Scrub)。
- **主要技術**: VRAM Scrubbing (Sanitization), Crypto-signed Audit Logs, Tenant Isolation.

### C. Distributed Intelligence (分散インテリジェンス)
地理的に分散したコンシューマーGPUを、あたかも1つの巨大なクラスタとして扱います。
- **Core Value**: Headscale (VPN) を活用した "Local-Feel" UX。
- **主要技術**: Headscale Integration, Topology-aware Scheduling.

---

## 3. フェーズ別実装計画

### ✅ Phase 2: GPU Foundation (完了/実装済み)

AIワークロードを実行するためのハードウェア抽象化層と、セキュリティの核となる物理洗浄機能の実装。

| コンポーネント | タスク | ステータス | 技術詳細 |
| :--- | :--- | :--- | :--- |
| **Engine** | **GPU Discovery & UUID** | **完了** | `nvml-wrapper` を用いた GPU UUID の特定と管理。不安定な Index 依存からの脱却。 |
| **Engine** | **VRAM Scrubbing (Core)** | **完了** | `cudarc` (CUDA Driver API) を用いた VRAM 物理ゼロ埋め (`cudaMemset`) の実装。コンテナ終了時のフック統合。 |
| **Scheduler** | **GPU-Aware Scheduling** | **完了** | Client が空き GPU UUID を特定し、Engine が OCI Spec (`NVIDIA_VISIBLE_DEVICES`) に注入するパイプライン。 |

### 🚀 Phase 2.5: AI Runtime Integration (現在進行中)

**目標**: モデルロード時間を「20分」から「1秒」に短縮し、実用的な推論基盤とする。

| コンポーネント | タスク | 優先度 | 技術詳細 |
| :--- | :--- | :--- | :--- |
| **Engine** | **Host Path Volume Mounting** | **MUST** | ホスト側のキャッシュ済みモデル（例: `/opt/models/llama-3`）をコンテナへ高速マウントする機能。Path Traversal 防止バリデーション付き。 |
| **Client** | **Model Fetcher** | **SHOULD** | HuggingFace 等からモデルを事前にホストへダウンロード・キャッシュする管理機能（P2Pの前段階）。 |
| **Manifest** | **Volume Schema Definition** | **MUST** | `adep.json` に `volumes` フィールドを追加し、読み取り専用マウントを定義可能にする。 |
| **Engine** | **vLLM Sidecar Pattern** | **LATER** | vLLM をコンテナ内で起動し、Unix Domain Socket 経由で制御するパターンの確立。 |

### Phase 3.5: Advanced Security & Operations (高度セキュリティ)

**目標**: 「VRAMを消した」ことを数学的に証明し、エンタープライズ利用に耐える信頼性を確立する。

| コンポーネント | タスク | 技術詳細 |
| :--- | :--- | :--- |
| **Engine** | **Proof of Scrub (Audit)** | VRAM 洗浄実行時のログ（タイムスタンプ、対象UUID、書き込みパターン）を記録し、Engine の秘密鍵でデジタル署名する。 |
| **Engine** | **Zombie Process Hunting** | VRAM 洗浄前に、対象 GPU を掴んでいる残留プロセスを確実に特定し `kill -9` する堅牢なロジック。 |
| **Network** | **Headscale 1-Click VPN** | 各ノードを自動的にプライベート VPN メッシュに参加させ、`http://my-gpu.local` のようなアクセスを実現する。 |

### Phase 4.5: Distributed Inference (分散推論)

**目標**: 単一 GPU に収まらない巨大モデルの分割実行。

| コンポーネント | タスク | 技術詳細 |
| :--- | :--- | :--- |
| **Network** | **Headscale Optimization** | VPN 経由での Tensor 転送パフォーマンスの最適化（MTU調整、WireGuardチューニング）。 |
| **Scheduler** | **Geo-aware Scheduling** | ネットワークレイテンシ（RTT）に基づき、物理的に近いノード群にパイプライン並列化ジョブを配置する。 |

---

## 4. 技術的決定事項 (Decision Records)

### 1. ネットワークスタックの方針転換
- **変更前**: 自前で `io_uring` や `QUIC` ベースのオーバーレイネットワークを実装。
- **変更後**: **Headscale (Tailscale Control Plane)** を全面的に採用。
- **理由**: 自前実装はコストが高く、セキュリティリスクも増大するため。OSS として成熟した VPN 技術を利用し、UX（Local-Feel）の向上にリソースを集中する。

### 2. モデル配布戦略
- **変更前**: 独自の P2P Model Registry を初期から実装。
- **変更後**: **Host Path Mounting + Simple Fetcher** を優先。
- **理由**: 推論のコールドスタート問題を最短で解決するため。P2P はノード数が増加した後の最適化手段として位置づける。

---

## 5. 直近のアクションアイテム

1.  **実装**: `engine` 側の `OciSpecBuilder` を拡張し、`mounts` 設定の生成ロジックを追加する。
2.  **定義**: `adep-logic` (Wasm) を更新し、マニフェストのスキーマに `volumes` を追加する。
3.  **検証**: 実際に Llama-3 などの巨大モデルをホストに置き、コンテナからマウントして推論サーバー (vLLM/llama.cpp) が即座に起動することを確認する。