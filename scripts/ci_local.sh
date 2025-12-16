#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

RQLITE_IMAGE_DEFAULT="rqlite/rqlite:9.3.5"
RQLITE_CONTAINER_NAME="capsuled-prepush-rqlite"
STARTED_RQLITE_CONTAINER=false

cleanup() {
  if command -v docker >/dev/null 2>&1; then
    if [[ "$STARTED_RQLITE_CONTAINER" == "true" ]]; then
      docker rm -f "$RQLITE_CONTAINER_NAME" >/dev/null 2>&1 || true
    fi
  fi
}
trap cleanup EXIT

cd "$REPO_ROOT"

echo "[ci_local] Rust fmt"
cargo fmt --all --check

echo "[ci_local] Rust clippy (engine, -D warnings)"
(cd engine && cargo clippy -- -D warnings)

echo "[ci_local] Start rqlite (for coordinator tests)"
if ! command -v docker >/dev/null 2>&1; then
  echo "docker not found; cannot run coordinator tests that depend on rqlite" >&2
  exit 1
fi

if ! docker info >/dev/null 2>&1; then
  echo "docker daemon not available (is Docker Desktop running?)" >&2
  exit 1
fi

docker rm -f "$RQLITE_CONTAINER_NAME" >/dev/null 2>&1 || true

RQLITE_HTTP_PORT=4001
RQLITE_RAFT_PORT=4002
RQLITE_ADDR="http://127.0.0.1:${RQLITE_HTTP_PORT}"

# If something (likely rqlite) is already running on 4001, just reuse it.
if curl -fsS "${RQLITE_ADDR}/status" >/dev/null 2>&1; then
  echo "[ci_local] Reusing existing rqlite at ${RQLITE_ADDR}";
else
  get_free_port() {
    python3 - <<'PY'
import socket
s = socket.socket()
s.bind(('127.0.0.1', 0))
print(s.getsockname()[1])
s.close()
PY
  }

  # If 4001/4002 are occupied, pick free ports and set RQLITE_ADDR accordingly.
  RQLITE_HTTP_PORT="$(get_free_port)"
  RQLITE_RAFT_PORT="$(get_free_port)"
  RQLITE_ADDR="http://127.0.0.1:${RQLITE_HTTP_PORT}"

  echo "[ci_local] Starting rqlite container on host ports ${RQLITE_HTTP_PORT}/${RQLITE_RAFT_PORT}";
  docker run -d --name "$RQLITE_CONTAINER_NAME" \
    -p "${RQLITE_HTTP_PORT}:4001" -p "${RQLITE_RAFT_PORT}:4002" \
    "${RQLITE_IMAGE:-$RQLITE_IMAGE_DEFAULT}" \
    -http-addr 0.0.0.0:4001 -raft-addr 0.0.0.0:4002 \
    >/dev/null
  STARTED_RQLITE_CONTAINER=true

  for _ in {1..60}; do
    if curl -fsS "${RQLITE_ADDR}/status" >/dev/null 2>&1; then
      break
    fi
    sleep 0.5
  done

  if ! curl -fsS "${RQLITE_ADDR}/status" >/dev/null 2>&1; then
    echo "rqlite did not become ready on ${RQLITE_ADDR}/status" >&2
    docker logs "$RQLITE_CONTAINER_NAME" || true
    exit 1
  fi
fi

echo "[ci_local] Go lint/test (client)"
GOLANGCI_LINT_BIN="$(go env GOPATH)/bin/golangci-lint"
if [[ ! -x "$GOLANGCI_LINT_BIN" ]]; then
  echo "[ci_local] Installing golangci-lint v1.64.8"
  go install github.com/golangci/golangci-lint/cmd/golangci-lint@v1.64.8
fi

(cd client && "$GOLANGCI_LINT_BIN" run --timeout=5m)

# CI parity: the GitHub Actions workflow does not run the race detector.
# Enable locally via `CI_LOCAL_RACE=1 ./scripts/ci_local.sh`.
GO_RACE_FLAG=""
if [[ "${CI_LOCAL_RACE:-0}" == "1" ]]; then
  GO_RACE_FLAG="-race"
fi

(cd client && RQLITE_ADDR="$RQLITE_ADDR" go test -v $GO_RACE_FLAG ./...)

echo "[ci_local] Rust tests (engine, --all-features)"
(cd engine && cargo test --all-features)

echo "[ci_local] Builds"
(cd client && go build -o coordinator ./cmd/coordinator)
(cd engine && cargo build --release)

echo "[ci_local] OK"
