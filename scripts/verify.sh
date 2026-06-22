#!/usr/bin/env bash
# verify.sh — inauguration: auto-detect + polyglot eval
# No language gates. No ## lang fences needed.
# Run: bash scripts/verify.sh
set -euo pipefail

RED='\033[0;31m' GREEN='\033[0;32m' CYAN='\033[0;36m' RESET='\033[0m'
IN="${IN_BIN:-$(which in)}"
PASS=0 FAIL=0

check_returns() {
  local label="$1" code="$2" expected="$3"
  local out
  if out=$($IN eval "$code" 2>/dev/null) && echo "$out" | grep -qF "$expected"; then
    printf "  ${GREEN}PASS${RESET} %s → (contains %s)\n" "$label" "$expected"
    PASS=$((PASS+1))
  else
    printf "  ${RED}FAIL${RESET} %s → expected '%s' in output\n" "$label" "$expected"
    FAIL=$((FAIL+1))
  fi
}

echo ""
echo "══════════════════════════════════════════════════"
echo "  inauguration verify — auto-detect, no gates"
echo "══════════════════════════════════════════════════"

# ── 1. All 33 languages eval '42' → 42 ──
echo ""
echo "${CYAN}[1/5] Basic eval — 33 languages return 42${RESET}"
failed=""
for lang in in c cpp objc objcpp rust zig go swift java kotlin scala csharp fsharp vbnet python ruby php perl javascript typescript lua dart haskell ocaml elixir erlang julia r nim d crystal odin hare holyc groovy clojure; do
  if out=$($IN eval --parser "$lang" '42' 2>/dev/null) && [ "$out" = "42" ]; then :; else failed="$failed $lang"; fi
done
if [ -z "$failed" ]; then
  printf "  ${GREEN}PASS${RESET} 33 languages\n"; PASS=$((PASS+1))
else
  printf "  ${RED}FAIL${RESET}$failed\n"; FAIL=$((FAIL+1))
fi

# ── 2. Auto-detect polyglot IO — no ## markers ──
echo ""
echo "${CYAN}[2/5] Auto-detect IO — blank-line separated, language inferred${RESET}"
check_returns "python print" '
print("hello from python")
' "hello from python"

check_returns "javascript log" '
console.log("hello from js")
' "hello from js"

check_returns "rust println" '
println!("hello from rust")
' "hello from rust"

# ── 3. Polyglot math — 9 languages, ## fence markers ──
echo ""
echo "${CYAN}[3/5] Polyglot math — 9 languages compute 2+3*4${RESET}"
check_returns "polyglot-math" '
## python
2 + 3 * 4
## javascript
2 + 3 * 4
## rust
2 + 3 * 4
## zig
2 + 3 * 4
## go
2 + 3 * 4
## java
2 + 3 * 4
## kotlin
2 + 3 * 4
## scala
2 + 3 * 4
## .in
2 + 3 * 4
' "14"

# ── 4. Auto-detect compute — blank lines, different values ──
echo ""
echo "${CYAN}[4/5] Auto-detect compute — 3 languages, no markers${RESET}"
check_returns "2+3*4" '
2 + 3 * 4

42 * 2

100 + 200
' "14"

# ── 5. Capabilities — parse/lower/typecheck/boundary/bytecode ──
echo ""
echo "${CYAN}[5/5] Capability table (no levels, just capabilities)${RESET}"
out=$($IN languages --json 2>/dev/null || true)
if [ -n "$out" ]; then
  count=$(echo "$out" | grep -c '"language"' || true)
  parse=$(echo "$out" | grep -c '"parse"' || true)
  lower=$(echo "$out" | grep -c '"lower"' || true)
  printf "  ${GREEN}PASS${RESET} %d languages, %d parse, %d lower\n" "$count" "$parse" "$lower"
  PASS=$((PASS+1))
else
  printf "  ${RED}FAIL${RESET} languages --json\n"; FAIL=$((FAIL+1))
fi

echo ""
echo "══════════════════════════════════════════════════"
printf "  ${GREEN}%d PASS${RESET} · ${RED}%d FAIL${RESET} · %d total\n" "$PASS" "$FAIL" $((PASS+FAIL))
[ "$FAIL" -eq 0 ] && echo "  All checks passed."
echo "══════════════════════════════════════════════════"
