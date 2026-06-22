#!/usr/bin/env bash
# bench-self-host.sh — benchmark self-hosted vs native compilation
set -euo pipefail

IN="$(which in)"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUTFILE="/tmp/inauguration-bench-$(date +%s).txt"

echo "# inauguration benchmark report" > "$OUTFILE"
echo "## $(date)" >> "$OUTFILE"
echo "" >> "$OUTFILE"

echo "### Self-hosted compilation (in eval in-cli)" | tee -a "$OUTFILE"
for i in 1 2 3; do
  echo -n "  Run $i: " | tee -a "$OUTFILE"
  /usr/bin/time -p "$IN" eval "$PROJECT_DIR/in-cli" --verbose 2>&1 | grep "timing.total_us" | while read -r line; do
    us=$(echo "$line" | grep -o '[0-9]*')
    ms=$((us / 1000))
    echo "${ms}ms" | tee -a "$OUTFILE"
  done
done

echo "" >> "$OUTFILE"
echo "### Cargo build (incremental)" | tee -a "$OUTFILE"
for i in 1 2 3; do
  echo -n "  Run $i: " | tee -a "$OUTFILE"
  /usr/bin/time -p cargo build --release 2>&1 | grep "real" | while read -r line; do
    echo "$line" | tee -a "$OUTFILE"
  done
done

echo "" >> "$OUTFILE"
echo "### Inline eval performance" | tee -a "$OUTFILE"
for lang in in rust zig; do
  echo -n "  $lang: " | tee -a "$OUTFILE"
  /usr/bin/time -p "$IN" eval --parser "$lang" '42' 2>&1 | grep "real" | tee -a "$OUTFILE"
done

echo "" >> "$OUTFILE"
echo "### Bytecode size" | tee -a "$OUTFILE"
ARTIFACT="$(find /var/folders -name "in-cargo-main.bin" -newer "$IN" 2>/dev/null | head -1 || echo "/tmp/in-cargo-main.bin")"
ls -lh "$ARTIFACT" 2>/dev/null | tee -a "$OUTFILE" || echo "no artifact" | tee -a "$OUTFILE"
echo "" | tee -a "$OUTFILE"
ls -lh "$IN" | tee -a "$OUTFILE"

echo "" | tee -a "$OUTFILE"
echo "Report saved to $OUTFILE" | tee -a "$OUTFILE"
