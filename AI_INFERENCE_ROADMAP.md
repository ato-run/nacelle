# Capsuled AI Inference Evolution Roadmap

**バージョン:** 0.1.0 (Draft)  
**作成日:** 2025-11-19  
**ステータス:** 提案段階 (R&D Track)

---

## 1. ビジョン: "The Distributed AI OS"

Capsuled を単なるコンテナオーケストレーターから、**コンシューマー GPU リソースを活用した、世界で最も効率的で低コストな分散 AI 推論プラットフォーム**へと進化させます。
既存の `CAPSULED_ROADMAP.md` (v1.0 到達計画) と並行して、AI ワークロードに特化した機能拡張を行うための技術ロードマップです。

## 2. 戦略的柱 (Strategic Pillars)

### A. Inference Optimization (推論最適化)

汎用的なコンテナ実行だけでなく、LLM (Large Language Models) の推論に特化したランタイム統合を行います。

- **目標**: メモリ断片化の解消、スループットの最大化、コールドスタート時間の短縮。
- **主要技術**: vLLM (PagedAttention), Speculative Decoding, Model Hot-swapping.

### B. GPU Security & Virtualization (GPU セキュリティと仮想化)

不特定多数のノード（Personal Rigs）で他者のワークロードを実行するための、軍事レベルのセキュリティと公平性を担保します。

- **目標**: テナント間のデータ漏洩防止 (VRAM 残留データ)、GPU リソースの公平な分割。
- **主要技術**: VRAM Scrubbing (Sanitization), Software-based MPS (Multi-Process Service), Time-slicing.

### C. Distributed Intelligence (分散インテリジェンス)

単一の GPU に収まらない巨大モデル（405B+）を、コンシューマーインターネット経由で分割実行します。

- **目標**: RTX 3090/4090 を束ねて H100 クラスタに匹敵する推論能力を実現（レイテンシは許容）。
- **主要技術**: Pipeline Parallelism over QUIC, Geo-aware Scheduling, P2P Model Registry.

---

## 3. フェーズ別実装計画

既存の `CAPSULED_ROADMAP.md` のフェーズ進行と並行、またはその後の拡張として実施します。

### Phase 2.5: AI Runtime Integration (AI ランタイム統合)

**時期目安**: Phase 2 (GPU 機能完成) 完了後

| コンポーネント | タスク                   | 技術詳細                                                                                                      |
| -------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------- |
| **Engine**     | **Native vLLM Support**  | OCI コンテナとしてではなく、Engine が管理するサイドカープロセスとして `vLLM` を起動・制御する機能。           |
| **Client**     | **Model Registry (P2P)** | HuggingFace からのダウンロードを最適化し、ノード間でモデルキャッシュを共有する P2P (Torrent/IPFS-like) 機構。 |
| **API**        | **Inference API**        | OpenAI 互換の `/v1/completions` エンドポイントを Engine がプロキシし、認証と計量を行う。                      |

### Phase 3.5: Advanced GPU Security (高度 GPU セキュリティ)

**時期目安**: Phase 3 (運用機能実装) と並行

| コンポーネント | タスク                      | 技術詳細                                                                                                                |
| -------------- | --------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| **Engine**     | **VRAM Scrubbing**          | Capsule 終了時に `cudarc` 等を用いて GPU VRAM にランダムパターン/ゼロを書き込み、前テナントのデータを物理的に消去する。 |
| **Engine**     | **Software MPS**            | NVIDIA MPS (Multi-Process Service) を Engine 経由で設定し、複数の軽量モデルを 1 つの GPU で効率的に並列実行させる。     |
| **Logic**      | **Secure Boot Attestation** | (将来検討) ノードの改竄検知。                                                                                           |

### Phase 4.5: Distributed Inference (分散推論)

**時期目安**: Phase 4 (HA・スケーラビリティ) 以降

| コンポーネント | タスク                        | 技術詳細                                                                                               |
| -------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------ |
| **Network**    | **Tensor Stream over QUIC**   | ノード間の Tensor 転送を TCP ではなく QUIC (HTTP/3) で行い、Head-of-Line blocking を回避して低遅延化。 |
| **Scheduler**  | **Topology-aware Scheduling** | ネットワークレイテンシの低いノード群（近隣 ISP など）を動的にグループ化し、モデル分割配置を行う。      |
| **Client**     | **Exo-like Orchestration**    | [Exo](https://github.com/exo-explore/exo) のような分散推論ロジックの統合。                             |

---

## 4. 技術調査・R&D トピック

### CXL (Compute Express Link) 対応

- **現状**: 2025 年後半より CXL 2.0 対応メモリ拡張モジュールが普及開始。
- **計画**: VRAM 不足時にシステムメモリ（CXL 経由）へスワップする際のパフォーマンス劣化を最小限に抑えるスケジューリングポリシーの研究。

### Unikernel / MicroVM for AI

- **現状**: コンテナのオーバーヘッド（特に Python ランタイム）が課題。
- **計画**: 推論エンジンだけを含んだ極小の Unikernel (Unikraft 等) を `youki` 経由で起動し、起動時間とメモリフットプリントを極小化する研究。

### Kernel-Bypass Networking

- **現状**: Linux Kernel のネットワークスタックが高速な推論通信のボトルネックになる可能性。
- **計画**: `io_uring` や `DPDK` を活用した Rust 製ユーザー空間ネットワークスタックの導入検討。

---

## 5. 次のアクション

1.  **PoC 作成**: `capsuled-engine` に `vLLM` を組み込み、API 経由で推論を行うプロトタイプの実装。
2.  **ベンチマーク**: コンシューマー GPU (RTX 3090/4090) における VRAM Scrubbing の所要時間測定。
3.  **設計更新**: `ARCHITECTURE.md` に "AI Inference Layer" を追加。
