#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SWIFT_ROOT="${IN_VENDOR_SWIFT_ROOT:-$ROOT/vendor/swift}"
PATCH="$ROOT/patches/vendor-swift/inauguration-emit-sil-stdout.patch"
if [[ ! -d "$SWIFT_ROOT" ]]; then
	echo "error: Swift checkout not found at $SWIFT_ROOT" >&2
	echo "Clone github.com/swiftlang/swift into vendor/swift (gitignored) or set IN_VENDOR_SWIFT_ROOT." >&2
	exit 1
fi
if [[ ! -f "$PATCH" ]]; then
	echo "error: missing patch $PATCH" >&2
	exit 1
fi
git -C "$SWIFT_ROOT" apply "$PATCH"
echo "Applied inauguration FrontendTool patch to $SWIFT_ROOT"
