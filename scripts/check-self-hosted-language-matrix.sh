#!/usr/bin/env bash
set -euo pipefail

# Self-hosted language matrix gate.
# Runs each mandatory language example through the owned pipeline:
#   in build, in graph --json, in agent, in backend --target bytecode --json
# Fails if any mandatory language requires an external compiler/runtime.
# Skips languages gracefully when their sample file doesn't exist.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IN_CMD=("${IN_BIN:-in}")
POLYGLOT_DIR="apps/polyglot-sample"

# Language -> sample file mapping (mandatory languages only).
# Each entry: "lang|sample_path|extra_env"
declare -a LANGS=(
  "in|sample.in|"
  "icore|sample.icore|"
  "c|sample.c|"
  "cpp|sample.cpp|"
  "objc|sample.m|"           # no sample exists → skipped gracefully
  "objcpp|sample.mm|"        # no sample exists → skipped gracefully
  "java|Sample.java|"
  "kotlin|Sample.kt|"
  "cs|Program.cs|"
  "swift|sample.swift|IN_NATIVE_SWIFT_SIL=only"
  "rust|sample.rs|"
  "go|sample.go|"
  "v|sample.v|"
  "js|sample.js|"
  "ts|sample.ts|"
  "python|sample.py|"
)

FAILED=0
SKIPPED=0
PASSED=0

check_json_no_external() {
  local json_path="$1"
  local label="$2"
  python3 - "$json_path" "$label" <<'PY'
import json, sys
data = json.loads(open(sys.argv[1]).read())
label = sys.argv[2]

def require(condition, message):
    if not condition:
        raise SystemExit(f"[{label}] {message}")

external = data.get("external_invocations")
require(external is not None, "missing external_invocations field")
require(external == [], f"external_invocations was not empty: {external}")

owned = data.get("owned")
if owned is not None:
    require(owned is True, f"owned was not true: {owned}")
PY
}

check_agent_no_error() {
  local json_path="$1"
  local label="$2"
  python3 - "$json_path" "$label" <<'PY'
import json, sys
data = json.loads(open(sys.argv[1]).read())
label = sys.argv[2]

diags = data.get("diagnostics", [])
if diags:
    errors = [d for d in diags if d.get("severity") == "error"]
    if errors:
        raise SystemExit(f"[{label}] agent diagnostics contain errors: {errors}")
PY
}

run_matrix_for_lang() {
  local lang="$1"
  local sample_file="$2"
  local extra_env="$3"

  local path="$POLYGLOT_DIR/$sample_file"

  if [[ ! -f "$path" ]]; then
    echo "SKIP [$lang]: sample file $path not found"
    SKIPPED=$((SKIPPED + 1))
    return 0
  fi

  local status="PASS"
  local tmp_json
  tmp_json="$(mktemp "${TMPDIR:-/tmp}/in-matrix-${lang}.XXXXXX")"

  local env_prefix=""
  if [[ -n "$extra_env" ]]; then
    env_prefix="env $extra_env"
  fi

  # 1. in build
  if ! $env_prefix "${IN_CMD[@]}" build --path "$path" --module-id App >/dev/null 2>&1; then
    echo "FAIL [$lang]: in build"
    status="FAIL"
  fi

  # 2. in graph --json
  if [[ "$status" == "PASS" ]]; then
    if $env_prefix "${IN_CMD[@]}" graph --json --path "$path" --module-id App >"$tmp_json" 2>/dev/null; then
      if ! python3 -c "import json; json.load(open('$tmp_json'))" 2>/dev/null; then
        echo "FAIL [$lang]: graph output is not valid JSON"
        status="FAIL"
      fi
    else
      echo "FAIL [$lang]: in graph --json"
      status="FAIL"
    fi
  fi

  # 3. in agent
  if [[ "$status" == "PASS" ]]; then
    if $env_prefix "${IN_CMD[@]}" agent --path "$path" --module-id App >"$tmp_json" 2>/dev/null; then
      check_agent_no_error "$tmp_json" "$lang" || status="FAIL"
    else
      echo "FAIL [$lang]: in agent"
      status="FAIL"
    fi
  fi

  # 4. in backend --target bytecode --json
  if [[ "$status" == "PASS" ]]; then
    if $env_prefix "${IN_CMD[@]}" backend --target bytecode --json --path "$path" --module-id App >"$tmp_json" 2>/dev/null; then
      check_json_no_external "$tmp_json" "$lang" || status="FAIL"
    else
      echo "FAIL [$lang]: in backend --target bytecode --json"
      status="FAIL"
    fi
  fi

  rm -f "$tmp_json"

  if [[ "$status" == "PASS" ]]; then
    echo "PASS [$lang]: $path"
    PASSED=$((PASSED + 1))
  else
    FAILED=$((FAILED + 1))
  fi

  return 0
}

echo "=== self-hosted language matrix ==="
echo ""

for entry in "${LANGS[@]}"; do
  IFS='|' read -r lang sample_file extra_env <<<"$entry"
  run_matrix_for_lang "$lang" "$sample_file" "$extra_env"
done

echo ""
echo "=== matrix summary ==="
echo "PASS:  $PASSED"
echo "SKIP:  $SKIPPED"
echo "FAIL:  $FAILED"

if [[ $FAILED -gt 0 ]]; then
  echo ""
  echo "Some language fronts failed the self-hosted gate."
  exit 1
fi
