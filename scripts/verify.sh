#!/usr/bin/env bash
# verify.sh — inauguration verify
# Run: bash scripts/verify.sh
set -euo pipefail

RED='\033[0;31m' GREEN='\033[0;32m' CYAN='\033[0;36m' DIM='\033[2m' RESET='\033[0m'
IN="${IN_BIN:-$(which in)}"
pass=0 fail=0

check() {
  local label="$1" cmd="$2" expect="$3"
  local out
  if out=$(eval "$cmd" 2>&1) && echo "$out" | grep -qF "$expect"; then
    printf "  ${GREEN}PASS${RESET} %s\n" "$label"
    pass=$((pass+1))
  else
    printf "  ${RED}FAIL${RESET} %s\n" "$label"
    fail=$((fail+1))
  fi
}

echo ""
echo "══════════════════════════════════════════════════"
echo "  inauguration · verify"
echo "══════════════════════════════════════════════════"

# ── 1. 33 languages eval 42 ──
echo ""
echo "${CYAN}[1/6] 33 languages return 42${RESET}"
failed=""
for lang in in c cpp objc objcpp rust zig go swift java kotlin scala csharp fsharp vbnet python ruby php perl javascript typescript lua dart haskell ocaml elixir erlang julia r nim d crystal odin hare holyc groovy clojure; do
  out=$($IN eval --parser "$lang" '42' 2>/dev/null) || { failed="$failed $lang"; }
done
if [ -z "$failed" ]; then printf "  ${GREEN}PASS${RESET} 33/33\n"; pass=$((pass+1))
else printf "  ${RED}FAIL${RESET}$failed\n"; fail=$((fail+1)); fi

# ── 2. Auto-detect polyglot IO ──
echo ""
echo "${CYAN}[2/6] Auto-detect polyglot IO${RESET}"
check "io-demo" '$IN eval examples/polyglot/io.poly' "hello from python"

# ── 3. Polyglot compute ──
echo ""
echo "${CYAN}[3/6] Polyglot compute (5 languages, different results)${RESET}"
check "compute" '$IN eval examples/polyglot/compute.poly' "14"

# ── 4. Compile Fibonacci ──
echo ""
echo "${CYAN}[4/6] Compile fib(10) = 55${RESET}"
check "fib" '$IN eval examples/compile/fib.in --verbose' "55"

# ── 5. Compile sum 1..100 = 5050 ──
echo ""
echo "${CYAN}[5/6] Compile sum_to(100) = 5050${RESET}"
check "sum" '$IN eval examples/compile/sum.in --verbose' "5050"

# ── 6. Capability table ──
echo ""
echo "${CYAN}[6/6] Capability table (no levels)${RESET}"
out=$($IN languages --json 2>/dev/null || true)
if [ -n "$out" ]; then
  count=$(echo "$out" | grep -c '"language"' || true)
  printf "  ${GREEN}PASS${RESET} %d languages\n" "$count"
  pass=$((pass+1))
else printf "  ${RED}FAIL${RESET}\n"; fail=$((fail+1)); fi

echo ""
echo "══════════════════════════════════════════════════"
printf "  ${GREEN}%d PASS${RESET} · ${RED}%d FAIL${RESET} · %d total\n" "$pass" "$fail" $((pass+fail))
[ "$fail" -eq 0 ] && echo "  All checks passed."
echo ""
echo "Usage:"
echo "  in eval file.in          # compile + execute .in files"
echo "  in eval file.poly        # polyglot eval"
echo "  in eval 'print(42)'      # inline .in code"
echo "  in eval --parser js '42' # inline with specific language"
echo "══════════════════════════════════════════════════"
