# Kubernetes (deprecated)

`capsuled` / Gumball は配布戦略として Docker Compose を採用しており、Kubernetes マニフェストは提供しません。

過去に検討した manifest は削除しました。Kubernetes 対応が必要になった場合は、まず方針（ADR/README）を更新した上で、最小権限の設計から再検討してください。

## Notes

- この方針の正本は `docs/REPO_STRUCTURE.md`（Kubernetes Policy）に記載する。
- `capsuled/k8s/` 配下には「deprecated である」説明のみを置き、manifest の再追加はしない。
