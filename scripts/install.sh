#!/usr/bin/env bash
# Prefer the repo-root installer (thin wrapper).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec "${ROOT}/install.sh" "$@"
