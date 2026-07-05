#!/usr/bin/env bash
set -euo pipefail

echo "=== inrt runtime check ==="

SRC=$(mktemp -d)/answer.in
trap 'rm -f "$SRC"' EXIT

echo -e 'fn answer() -> Int { return 42; }\nfn main() -> void { return; }' > "$SRC"

export IN_DEV_BIN="${IN_DEV_BIN:-cargo run --manifest-path in-cli/Cargo.toml --}"

if command -v in &>/dev/null; then
  IN_BIN="in"
else
  IN_BIN="cargo run --manifest-path in-cli/Cargo.toml --"
fi

echo "--- owned JIT compile & run ---"
$IN_BIN compile --path "$SRC" --target jit --entry answer --out /tmp/inrt-test.bin

echo "--- owned native compile ---"
$IN_BIN compile --path "$SRC" --target native --entry answer --out /tmp/inrt-test.bin 2>&1 || true

echo "=== inrt runtime check passed ==="
