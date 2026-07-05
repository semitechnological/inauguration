#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IN_CMD=("${IN_BIN:-in}")
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/check-jit.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

cat > "$tmp_dir/lib.in" <<'EOF'
fn ready(flag: Bool) -> String {
  if flag == true {
    return "ready";
  } else {
    return "wait";
  }
}
EOF

cat > "$tmp_dir/sample.in" <<'EOF'
import "./lib.in";
fn answer() -> Int { return 42; }
fn main() -> Int {
  print(ready(true));
  return answer();
}
EOF

echo 'jit compile ok: polyglot sample'
output="$("${IN_CMD[@]}" execute --verbose "$tmp_dir/sample.in" --module-id App 2>&1)"
printf '%s\n' "$output" | grep -q 'result: Int(42)'

echo 'jit compile ok: agent-native sample'
printf '%s\n' "$output" | grep -q '^ready'
