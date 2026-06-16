#!/usr/bin/env bash
# check-freestanding-x86_64.sh — Verify the freestanding x86_64-unknown-none target.
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
IN="$DIR/../in-cli/target/release/in"
BUILD_DIR="${BUILD_DIR:-/tmp/in-freestanding-check}"

PASS=0
FAIL=0

check() {
    local label="$1"
    if [ "$2" = "true" ]; then
        echo "  ok: $label"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: $label"
        FAIL=$((FAIL + 1))
    fi
}

mkdir -p "$BUILD_DIR"

echo "[1/3] Compiling minimal freestanding kernel..."
cat > "$BUILD_DIR/test.in" << 'EOF'
fn kernel_entry(mb_info: Int) -> Int {
  return 42
}
EOF

"$IN" compile \
    --path "$BUILD_DIR/test.in" --entry kernel_entry --emit boot \
    --trampoline /tmp/trampoline.bin \
    --target native --target-triple x86_64-unknown-none --linkage static-lib \
    --out "$BUILD_DIR/test.bin" 2>/dev/null

check "boot image produced" "$([ -f "$BUILD_DIR/test.bin" ] && echo true)"

SIZE=$(stat -f%z "$BUILD_DIR/test.bin" 2>/dev/null || stat -c%s "$BUILD_DIR/test.bin" 2>/dev/null || echo 0)
check "boot image is >= 8192 bytes" "$([ "$SIZE" -ge 8192 ] && echo true)"

echo "[2/3] Checking multiboot header..."
# The multiboot header is at offset 0, magic 0x1BADB002 (little-endian: 02 b0 ad 1b)
MB_MAGIC=$(xxd -p -l4 -s 0 "$BUILD_DIR/test.bin" 2>/dev/null || od -A n -t x1 -N4 "$BUILD_DIR/test.bin" 2>/dev/null | tr -d ' ')
check "multiboot magic present" "$(echo "$MB_MAGIC" | grep -qi "02b0ad1b" && echo true)"

echo "[3/3] Checking kernel code..."
if command -v objdump &>/dev/null; then
    objdump -D -b binary -m i386 -M x86-64,intel "$BUILD_DIR/test.bin" 2>/dev/null | head -20
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ] && echo "PASS" || { echo "FAIL"; exit 1; }
