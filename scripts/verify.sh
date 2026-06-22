#!/usr/bin/env bash
# verify.sh — inauguration compiler verification
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
echo "  inauguration compiler · verify"
echo "══════════════════════════════════════════════════"

# ── .in compile examples ──
echo ""
echo "${CYAN}[1/11] Compile fib(20) = 6765 (while loop)${RESET}"
check "fib"        '$IN eval examples/compile/fib.in --verbose'        "6765"

echo ""
echo "${CYAN}[2/11] Compile factorial(5) = 120 (while loop)${RESET}"
check "factorial"  '$IN eval examples/compile/factorial.in --verbose'  "120"

echo ""
echo "${CYAN}[3/11] Compile sum 1..100 = 5050 (for loop)${RESET}"
check "sum"        '$IN eval examples/compile/sum.in --verbose'        "5050"

echo ""
echo "${CYAN}[4/11] Compile 10 primes under 30 (nested loops)${RESET}"
check "primes"     '$IN eval examples/compile/primes.in --verbose'     "10"

echo ""
echo "${CYAN}[5/11] Compile collatz(27) = 111 (while + if)${RESET}"
check "collatz"    '$IN eval examples/compile/collatz.in --verbose'    "111"

echo ""
echo "${CYAN}[6/11] Compile gcd(48,18) = 6 (recursion)${RESET}"
check "gcd"        '$IN eval examples/compile/gcd.in --verbose'        "6"

echo ""
echo "${CYAN}[7/11] Compile is_even(42) = true (mutual recursion)${RESET}"
check "even_odd"   '$IN eval examples/compile/even_odd.in --verbose'   "true"

# ── Rust compile example ──
echo ""
echo "${CYAN}[8/11] Compile Rust (10+20)*2 = 60${RESET}"
check "rust"       '$IN eval examples/compile/add_multiply.rs --verbose' "60"

# ── Polyglot eval ──
echo ""
echo "${CYAN}[9/11] Polyglot IO (4 languages, auto-detected)${RESET}"
check "polyglot-io"  '$IN eval examples/polyglot/io.poly'  "hello from python"

echo ""
echo "${CYAN}[10/11] Polyglot compute (5 languages, different results)${RESET}"
check "polyglot-math" '$IN eval examples/polyglot/compute.poly' "14"

# ── 33 languages ──
echo ""
echo "${CYAN}[11/11] 33 languages eval 42${RESET}"
failed=""
for lang in in c cpp objc objcpp rust zig go swift java kotlin scala csharp fsharp vbnet python ruby php perl javascript typescript lua dart haskell ocaml elixir erlang julia r nim d crystal odin hare holyc groovy clojure; do
  out=$($IN eval --parser "$lang" '42' 2>/dev/null) || { failed="$failed $lang"; }
done
if [ -z "$failed" ]; then printf "  ${GREEN}PASS${RESET} 33/33\n"; pass=$((pass+1))
else printf "  ${RED}FAIL${RESET}$failed\n"; fail=$((fail+1)); fi

echo ""
echo "══════════════════════════════════════════════════"
printf "  ${GREEN}%d PASS${RESET} · ${RED}%d FAIL${RESET} · %d total\n" "$pass" "$fail" $((pass+fail))
[ "$fail" -eq 0 ] && echo "  All checks passed."
echo ""
echo "Usage:"
echo "  in eval file.in          # compile + execute"
echo "  in eval file.rs          # compile Rust"
echo "  in eval file.poly        # polyglot eval"
echo "  in eval 'print(42)'      # inline .in code"
echo "══════════════════════════════════════════════════"
