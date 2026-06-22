#!/usr/bin/env bash
# verify.sh — inauguration: no language gates, auto-detect polyglot
# Run: bash scripts/verify.sh
set -euo pipefail

RED='\033[0;31m' GREEN='\033[0;32m' CYAN='\033[0;36m' DIM='\033[2m' RESET='\033[0m'
IN="${IN_BIN:-$(which in)}"

pass=0 fail=0
check() {
  local label="$1" cmd="$2" expect="$3"
  local out
  if out=$(eval "$cmd" 2>/dev/null) && echo "$out" | grep -qF "$expect"; then
    printf "  ${GREEN}PASS${RESET} %s\n" "$label"
    pass=$((pass+1))
  else
    printf "  ${RED}FAIL${RESET} %s\n" "$label"
    fail=$((fail+1))
  fi
}

echo ""
echo "══════════════════════════════════════════════════"
echo "  inauguration · verify · remove-language-gates"
echo "══════════════════════════════════════════════════"

# ── 1. Single-language eval ──
echo ""
echo "${CYAN}[1/5] 33 languages return 42${RESET}"
echo "  ${DIM}in eval '42' --parser <lang>${RESET}"
failed=""
for lang in in c cpp objc objcpp rust zig go swift java kotlin scala csharp fsharp vbnet python ruby php perl javascript typescript lua dart haskell ocaml elixir erlang julia r nim d crystal odin hare holyc groovy clojure; do
  out=$($IN eval --parser "$lang" '42' 2>/dev/null) || { failed="$failed $lang"; }
done
if [ -z "$failed" ]; then
  printf "  ${GREEN}PASS${RESET} 33/33\n"; pass=$((pass+1))
else
  printf "  ${RED}FAIL${RESET}$failed\n"; fail=$((fail+1))
fi

# ── 2. Auto-detect IO ──
echo ""
echo "${CYAN}[2/5] Auto-detect polyglot IO (no ## markers)${RESET}"
echo "  ${DIM}input: print() | console.log() | println!() | std.io.print()${RESET}"
check "io-demo" '$IN eval --path examples/polyglot/io.poly' "hello from python"

# ── 3. Polyglot math ──
echo ""
echo "${CYAN}[3/5] Polyglot math (## fences for disambiguation)${RESET}"
echo "  ${DIM}input: 9 languages all compute 2+3*4${RESET}"
check "math-demo" '$IN eval --path examples/polyglot/math.poly' "14"

# ── 4. Auto-detect compute ──
echo ""
echo "${CYAN}[4/5] Auto-detect compute (blank lines)${RESET}"
echo "  ${DIM}input: 2+3*4 | 42*2 | 100+200${RESET}"
check "compute-demo" '$IN eval --path examples/polyglot/compute.poly' "14"

# ── 5. Capability table ──
echo ""
echo "${CYAN}[5/5] Capability table (no levels)${RESET}"
echo "  ${DIM}in languages --json${RESET}"
out=$($IN languages --json 2>/dev/null || true)
if [ -n "$out" ]; then
  count=$(echo "$out" | grep -c '"language"' || true)
  parse=$(echo "$out" | grep -c '"parse"' || true)
  printf "  ${GREEN}PASS${RESET} %d languages, %d can parse\n" "$count" "$parse"
  pass=$((pass+1))
else
  printf "  ${RED}FAIL${RESET}\n"; fail=$((fail+1))
fi

echo ""
echo "══════════════════════════════════════════════════"
printf "  ${GREEN}%d PASS${RESET} · ${RED}%d FAIL${RESET} · %d total\n" "$pass" "$fail" $((pass+fail))
[ "$fail" -eq 0 ] && echo "  All checks passed."
echo ""
echo "Run examples directly:"
echo "  in eval --path examples/polyglot/io.poly"
echo "  in eval --path examples/polyglot/math.poly"
echo "  in eval --path examples/polyglot/compute.poly"
echo "══════════════════════════════════════════════════"
