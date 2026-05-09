#!/usr/bin/env bash
# Mirror sil_emit's swiftc -typecheck inputs for SwiftPM packages: Sources/ + Generated/,
# .build/**/debug/Modules, local generated/ + FFI/, and file-system deps' generated/ from
# .build/workspace-state.json (same idea as in-cli sil_emit).
set -euo pipefail
PKG="${1:?package root}"
cd "$PKG"

if [[ "${SKIP_SWIFT_BUILD:-}" != 1 ]]; then
  swift build -q
fi

shopt -s nullglob
swift_files=()
for d in Sources Generated; do
  if [[ -d $d ]]; then
    while IFS= read -r -d '' f; do swift_files+=("$f"); done < <(find "$d" -name '*.swift' -print0 | sort -z)
  fi
done
if ((${#swift_files[@]} == 0)); then
  echo "swiftc-bench-typecheck: no Swift files under Sources/ or Generated/ in $PKG" >&2
  exit 1
fi

mods_dir=""
while IFS= read -r d; do
  mods_dir=$d
  break
done < <(find .build -path '*/debug/Modules' -type d 2>/dev/null | sort)

sdk=()
if [[ "$(uname -s)" == Darwin ]]; then
  if p=$(xcrun --sdk macosx --show-sdk-path 2>/dev/null) && [[ -n $p ]]; then
    sdk=(-sdk "$p")
  fi
fi

xcc_args=()
append_maps_in_dir() {
  local dir=$1
  local skip_umbrella=${2:-0}
  [[ -d $dir ]] || return 0
  while IFS= read -r m; do
    [[ -n $m ]] || continue
    local base
    base=$(basename "$m")
    if [[ $skip_umbrella -eq 1 && $base == module.modulemap ]]; then
      continue
    fi
    xcc_args+=(-Xcc "-fmodule-map-file=$m")
  done < <(find "$dir" -maxdepth 1 -name '*.modulemap' 2>/dev/null | sort)
  xcc_args+=(-Xcc "-I$dir")
}

append_maps_in_dir generated 1
append_maps_in_dir FFI 0

if [[ -f .build/workspace-state.json ]]; then
  while IFS= read -r root; do
    [[ -n $root ]] || continue
    append_maps_in_dir "$root/generated" 1
    append_maps_in_dir "$root/Generated" 1
  done < <(python3 -c 'import json, pathlib, sys
pkg = pathlib.Path(sys.argv[1])
raw = (pkg / ".build" / "workspace-state.json").read_text(encoding="utf-8")
data = json.loads(raw)
for dep in data.get("object", {}).get("dependencies", []) or []:
    st = dep.get("state") or {}
    p = st.get("path")
    if isinstance(p, str) and p:
        print(p)
' "$PKG")
fi

cmd=(swiftc -typecheck "${sdk[@]}")
if [[ -n ${mods_dir:-} ]]; then
  cmd+=(-I "$mods_dir")
fi
cmd+=("${xcc_args[@]}")
cmd+=("${swift_files[@]}")
exec "${cmd[@]}"
