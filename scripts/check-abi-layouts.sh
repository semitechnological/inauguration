#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "abi-layouts check: boundary verify + icore v3 sample"
(
  cd in-cli
  cargo test -q boundary -- --nocapture
)
python3 - "$ROOT/apps/icore-sample/boundary-v3.icore" <<'PY'
import json
import subprocess
import sys
from pathlib import Path

path = Path(sys.argv[1])
data = json.loads(path.read_text())
boundary = data.get("boundary")
if not boundary:
    raise SystemExit("boundary-v3.icore missing boundary section")
if boundary.get("abi_version") != 1:
    raise SystemExit("unexpected abi_version")
layouts = boundary.get("layouts") or []
if not layouts:
    raise SystemExit("boundary layouts missing")
person = layouts[0]
if person.get("name") != "Person":
    raise SystemExit("expected Person layout")
fields = {field["name"]: field for field in person.get("fields", [])}
if fields["name"]["type"] != "InSliceU8":
    raise SystemExit("expected InSliceU8 name field")
if fields["age"]["offset"] != 16:
    raise SystemExit("expected age offset 16")
PY
echo "abi-layouts checks passed"