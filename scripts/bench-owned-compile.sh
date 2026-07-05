#!/usr/bin/env bash
set -euo pipefail

echo "=== owned-compile benchmark ==="

RUNS="${IN_BENCH_RUNS:-5}"
SRC_DIR=$(mktemp -d)
SRC="$SRC_DIR/bench.in"
OUT="$SRC_DIR/bench.bc"
trap 'rm -rf "$SRC_DIR"' EXIT

cat > "$SRC" << 'ENDIN'
fn answer() -> Int { return 42; }
fn helper(x: Int) -> Int { return x; }
fn main() -> void { let v: Int = helper(1); return; }
ENDIN

IN_BIN="${IN_BIN:-in}"
if ! command -v "$IN_BIN" &>/dev/null; then
  IN_BIN="cargo run --manifest-path in-cli/Cargo.toml --"
fi

echo "--- cold start ---"
$IN_BIN compile --path "$SRC" --out "$OUT" --target native --entry main 2>&1

echo "--- warm ($RUNS runs) ---"
declare -a TIMINGS=()
for i in $(seq 1 "$RUNS"); do
  start=$(python3 -c 'import time; print(int(time.time() * 1_000_000))')
  $IN_BIN compile --path "$SRC" --out "$OUT" --target native --entry main 2>&1 >/dev/null
  end=$(python3 -c 'import time; print(int(time.time() * 1_000_000))')
  elapsed=$((end - start))
  TIMINGS+=("$elapsed")
  echo "  run $i: ${elapsed}us"
done

sum=0
min=999999999
max=0
for t in "${TIMINGS[@]}"; do
  sum=$((sum + t))
  (( t < min )) && min=$t
  (( t > max )) && max=$t
done
mean=$((sum / RUNS))

echo "--- results ---"
echo "min: ${min}us"
echo "max: ${max}us"
echo "mean: ${mean}us"

echo "=== owned-compile benchmark done ==="
