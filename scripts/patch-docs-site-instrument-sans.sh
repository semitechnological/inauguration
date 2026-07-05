#!/usr/bin/env bash
# Post-process crepus web build output: doc shells hardcode Inter; swap to Chivo Mono
# (https://fonts.google.com/specimen/Chivo+Mono)
set -euo pipefail
DIST="${1:?usage: patch-docs-site-instrument-sans.sh <dist-dir>}"
[[ -d "$DIST" ]] || { echo "error: not a directory: $DIST" >&2; exit 1; }

export DIST
python3 -S <<'PY'
import os
from pathlib import Path

dist = Path(os.environ["DIST"])

google_fonts = """<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Chivo+Mono:ital,wght@0,100..900;1,100..900&family=JetBrains+Mono:ital,wght@0,100..800;1,100..800&display=swap" rel="stylesheet">"""

for path in dist.rglob("*.html"):
    text = path.read_text(encoding="utf-8")
    
    # Inject Google Fonts if not already present
    if "fonts.googleapis.com" not in text and "<head>" in text:
        text = text.replace("<head>", f"<head>\n{google_fonts}")
        
    # Replace Inter font references
    text = text.replace("font-family:Inter,system-ui,sans-serif", 'font-family:"Chivo Mono","JetBrains Mono",ui-monospace,SFMono-Regular,monospace')
    text = text.replace("font-family: Inter,", 'font-family: "Chivo Mono",')
    text = text.replace("font-family: monospace", 'font-family: "JetBrains Mono", monospace')
    
    path.write_text(text, encoding="utf-8")
PY
