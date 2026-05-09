#!/usr/bin/env bash
# Post-process crepus web build output: doc shells hardcode Inter; swap to Instrument Sans
# (https://fonts.google.com/specimen/Instrument+Sans). UI patterns reference:
# https://vercel.com/design/guidelines
set -euo pipefail
DIST="${1:?usage: patch-docs-site-instrument-sans.sh <dist-dir>}"
[[ -d "$DIST" ]] || { echo "error: not a directory: $DIST" >&2; exit 1; }

OLD='family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap'
NEW='family=Instrument+Sans:ital,wght@0,400..700;1,400..700&family=JetBrains+Mono:ital,wght@0,400..600;1,400..600&display=swap'

export DIST OLD NEW
python3 -S <<'PY'
import os
from pathlib import Path

dist = Path(os.environ["DIST"])
old = os.environ["OLD"]
new = os.environ["NEW"]

for path in dist.rglob("*.html"):
    text = path.read_text(encoding="utf-8")
    text2 = text.replace(old, new)
    text2 = text2.replace("font-family: Inter,", 'font-family: "Instrument Sans",')
    if text2 != text:
        path.write_text(text2, encoding="utf-8")
PY
