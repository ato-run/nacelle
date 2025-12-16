#!/usr/bin/env bash
set -euo pipefail

cargo test --test depsd_smoke -- depsd_command_failure_surfaces_error_code
