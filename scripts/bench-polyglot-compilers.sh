#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export BENCH_ROOT="$ROOT"
export BENCH_RUNS="${BENCH_RUNS:-3}"
export BENCH_WARMUP_RUNS="${BENCH_WARMUP_RUNS:-1}"
v -gc none run "$ROOT/scripts/bench_polyglot_compilers.v"
