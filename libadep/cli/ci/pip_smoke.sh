#!/usr/bin/env bash
set -euo pipefail

cargo test --test depsd_smoke -- depsd_autostart_python_pnpm_success
