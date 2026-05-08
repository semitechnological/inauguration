#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BENCH_ROOT="$ROOT" v -gc none run "$ROOT/scripts/bench_swift.v"
