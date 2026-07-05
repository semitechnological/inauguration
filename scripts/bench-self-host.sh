#!/usr/bin/env bash
# Self-host vs rustc: compile time, binary size, cold exec — docs/self-host-vs-native.json
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IN="${IN_BIN:-$ROOT/in-cli/target/release/in}"
CLI_MANIFEST="$ROOT/in-cli/Cargo.toml"
MAIN="$ROOT/in-cli/src/main.rs"
OUT_JSON="${OUT_JSON:-$ROOT/docs/self-host-vs-native.json}"
RUNS="${BENCH_RUNS:-3}"

if [[ ! -x "$IN" ]]; then
  cargo build --release --manifest-path "$CLI_MANIFEST" --features extended -q
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

# --- rustc (cargo) comparison: same crate, release binary ---
RUSTC_BIN="$ROOT/in-cli/target/release/in"
rustc_build_ms=0
rustc_build_runs=()
for ((i = 1; i <= RUNS; i++)); do
  touch "$MAIN"
  log="$(mktemp)"
  set +e
  /usr/bin/time -p cargo build --release --manifest-path "$CLI_MANIFEST" --features extended 2>&1 | tee "$log"
  set -e
  r="$(grep '^real' "$log" | awk '{print $2 * 1000}')"
  rustc_build_runs+=("$r")
  rm -f "$log"
done
rustc_build_ms="$(printf '%s\n' "${rustc_build_runs[@]}" | awk '{s+=$1; n++} END { if (n) printf "%.3f", s/n; else print "0" }')"

in_bin_bytes="$(stat -f%z "$IN" 2>/dev/null || stat -c%s "$IN")"
rustc_bin_bytes="$(stat -f%z "$RUSTC_BIN" 2>/dev/null || stat -c%s "$RUSTC_BIN")"

exec_ms_avg() {
  python3 - "$1" <<'PY'
import subprocess, sys, time
bin = sys.argv[1]
samples = []
for _ in range(5):
    t0 = time.perf_counter()
    subprocess.run([bin, "--version"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    samples.append((time.perf_counter() - t0) * 1000)
print(f"{sum(samples)/len(samples):.3f}")
PY
}
in_exec_avg="$(exec_ms_avg "$IN")"
rustc_exec_avg="$(exec_ms_avg "$RUSTC_BIN")"

mkdir -p "$(dirname "$OUT_JSON")"
WALL_CSV=$(IFS=,; echo "${wall_ms[*]}")
RUSTC_WALL_CSV=$(IFS=,; echo "${rustc_build_runs[*]}")
export OUT_JSON version host cpu avg_wall parsed typed edges jit_us ok native_note RUNS WALL_CSV \
  rustc_build_ms rustc_build_runs RUSTC_WALL_CSV in_bin_bytes rustc_bin_bytes in_exec_avg rustc_exec_avg
python3 - <<'PY'
import json, os, datetime

def human_bytes(n):
    n = float(n)
    if n >= 1024**3:
        return f"{n / 1024**3:.2f} GiB"
    if n >= 1024**2:
        return f"{n / 1024**2:.2f} MiB"
    if n >= 1024:
        return f"{n / 1024:.2f} KiB"
    return f"{int(n)} B"

walls = [float(x) for x in os.environ.get("WALL_CSV", "").split(",") if x]
rustc_walls = [float(x) for x in os.environ.get("RUSTC_WALL_CSV", "").split(",") if x]
in_b = int(os.environ["in_bin_bytes"])
rustc_b = int(os.environ["rustc_bin_bytes"])
in_wall = float(os.environ["avg_wall"])
rustc_wall = float(os.environ["rustc_build_ms"])
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
        "wall_ms_avg": in_wall,
        "wall_ms_runs": walls,
        "jit_compile_us": int(os.environ["jit_us"] or 0),
        "front_ok": int(os.environ.get("parsed") or 0) > 0,
        "backend": "owned Core IR → JIT (no --out); full JIT link often blocked on stdlib",
    },
    "native_self_build": {
        "command": "in build --path in-cli/src/main.rs --out /tmp/in",
        "status": "blocked" if os.environ["native_note"] != "native --out succeeded" else "ok",
        "note": os.environ["native_note"],
    },
    "rustc_release": {
        "command": "cargo build --release --manifest-path in-cli/Cargo.toml --features extended",
        "wall_ms_avg": rustc_wall,
        "wall_ms_runs": rustc_walls,
        "artifact": "in-cli/target/release/in",
        "binary_bytes": rustc_b,
        "binary_human": human_bytes(rustc_b),
        "notes": "Incremental rebuild after touch main.rs; true cold clean build is higher",
    },
    "comparison": {
        "subject": "Same inauguration CLI crate (in-cli/src/main.rs + full dependency graph)",
        "in_front_wall_ms_avg": in_wall,
        "rustc_release_wall_ms_avg": rustc_wall,
        "compile_speed_ratio_in_over_rustc": round(in_wall / rustc_wall, 4) if rustc_wall else None,
        "binary_bytes_in": in_b,
        "binary_bytes_rustc": rustc_b,
        "binary_size_ratio": round(in_b / rustc_b, 4) if rustc_b else None,
        "binary_human": human_bytes(in_b),
        "cold_exec_version_ms_avg": {
            "in": float(os.environ["in_exec_avg"]),
            "rustc_binary": float(os.environ["rustc_exec_avg"]),
        },
        "interpretation": {
            "compile": "in measures owned front+JIT attempt on main.rs only; rustc links full crate + deps",
            "binary": "Single shipped `in` binary (rustc output); in native self-build not yet comparable",
            "exec": "`in --version` process startup; not compiler throughput",
        },
    },
}
with open(os.environ["OUT_JSON"], "w") as f:
    json.dump(doc, f, indent=2)
    f.write("\n")
print(os.environ["OUT_JSON"])
PY