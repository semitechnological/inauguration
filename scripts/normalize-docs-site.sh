#!/usr/bin/env bash
set -euo pipefail
DIST="${1:?usage: normalize-docs-site.sh <dist-dir>}"
[[ -d "$DIST" ]] || { echo "error: not a directory: $DIST" >&2; exit 1; }

export DIST
python3 -S <<'PY'
import os
from pathlib import Path

for path in Path(os.environ["DIST"]).rglob("*.html"):
    text = path.read_text(encoding="utf-8")
    path.write_text(text.replace("<br></br>", "<br>"), encoding="utf-8")
PY
