#!/usr/bin/env bash
# Emit docs/benchmarks/self-host-vs-native.json from live `in build` (Rust self-host front).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IN="${IN_BIN:-$ROOT/in-cli/target/release/in}"
MAIN="$ROOT/in-cli/src/main.rs"
OUT_JSON="${OUT_JSON:-$ROOT/docs/benchmarks/self-host-vs-native.json}"
RUNS="${BENCH_RUNS:-3}"

if [[ ! -x "$IN" ]]; then
  cargo build --release --manifest-path "$ROOT/in-cli/Cargo.toml" --features extended -q
fi
export IN_COMPILE_CACHE=0

version="$("$IN" --version 2>/dev/null | head -1 || echo unknown)"
host="$(uname -s)"
cpu="$(sysctl -n machdep.cpu.brand_string 2>/dev/null || grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2 | xargs || echo unknown)"

wall_ms=()
parsed=0
typed=0
edges=0
jit_us=0
ok=false
native_note=""

for ((i = 1; i <= RUNS; i++)); do
  log="$(mktemp)"
  set +e
  "$IN" build --path "$MAIN" --verbose 2>&1 | tee "$log"
  build_rc=${PIPESTATUS[0]}
  set -e
  w="$(grep 'in.build_wall_ms=' "$log" | tail -1 | sed 's/.*=//')"
  [[ -n "$w" ]] && wall_ms+=("$w")
  if grep -q 'functions:.*parsed' "$log"; then
    ok=true
    parsed="$(grep 'functions:.*parsed' "$log" | tail -1 | sed -E 's/.* ([0-9]+) parsed.*/\1/')"
    typed="$(grep 'functions:.*parsed' "$log" | tail -1 | sed -E 's/.*, ([0-9]+) typed.*/\1/')"
  elif grep -q 'in parse:' "$log"; then
    parsed="$(grep 'in parse:' "$log" | tail -1 | sed -E 's/.*in parse: ([0-9]+).*/\1/')"
    typed="$parsed"
  elif [[ "$build_rc" -eq 0 ]]; then
    ok=true
  fi
  jit_us="$(grep 'compile took' "$log" | tail -1 | grep -oE '[0-9]+' | head -1 || echo 0)"
  rm -f "$log"
done

set +e
"$IN" build --path "$MAIN" --out /tmp/in-bench-native-check --verbose 2>&1 | tee /tmp/in-native-bench.log
native_rc=${PIPESTATUS[0]}
set -e
if [[ "$native_rc" -eq 0 ]]; then
  native_note="native --out succeeded"
else
  native_note="$(grep -E 'compile failed|build: native' /tmp/in-native-bench.log | tail -1 | sed 's/^in: //' | head -c 240)"
fi

edges=0
stats_json="/tmp/in-bench-compile-stats.json"
if "$IN" compile --path "$MAIN" --target jit --out /tmp/in-bench-stats --json 2>/dev/null >"$stats_json"; then
  read -r parsed typed edges < <(python3 - <<PY
import json
with open("$stats_json") as f:
    r = json.load(f)
print(r.get("parsed_function_count", 0), r.get("typed_function_count", 0), r.get("call_edge_count", 0))
PY
)
  [[ "${parsed:-0}" -gt 0 ]] && ok=true
fi

avg_wall="$(printf '%s\n' "${wall_ms[@]}" | awk '{s+=$1; n++} END { if (n) printf "%.3f", s/n; else print "0" }')"

mkdir -p "$(dirname "$OUT_JSON")"
WALL_CSV=$(IFS=,; echo "${wall_ms[*]}")
export OUT_JSON version host cpu avg_wall parsed typed edges jit_us ok native_note RUNS WALL_CSV
python3 - <<'PY'
import json, os, datetime
walls = [float(x) for x in os.environ.get("WALL_CSV", "").split(",") if x]
doc = {
    "generated_at_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "in_version": os.environ["version"],
    "host_os": os.environ["host"],
    "cpu": os.environ["cpu"],
    "bench_runs": int(os.environ["RUNS"]),
    "self_host_parse": {
        "command": "in build --path in-cli/src/main.rs",
        "functions_parsed": int(os.environ["parsed"] or 0),
        "functions_typed": int(os.environ["typed"] or 0),
        "call_edges": int(os.environ["edges"] or 0),
        "wall_ms_avg": float(os.environ["avg_wall"]),
        "wall_ms_runs": walls,
        "jit_compile_us": int(os.environ["jit_us"] or 0),
        "front_ok": int(os.environ.get("parsed") or 0) > 0,
        "backend": "owned Core IR → JIT (no --out)",
    },
    "native_self_build": {
        "command": "in build --path in-cli/src/main.rs --out /tmp/in",
        "status": "blocked" if "blocked" in os.environ["native_note"] or os.environ["native_note"] != "native --out succeeded" else "ok",
        "note": os.environ["native_note"],
    },
}
with open(os.environ["OUT_JSON"], "w") as f:
    json.dump(doc, f, indent=2)
    f.write("\n")
print(os.environ["OUT_JSON"])
PY