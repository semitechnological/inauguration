#!/usr/bin/env bash
#
# check-component-metadata.sh — Validate that component metadata sidecar
# is emitted with the expected keys for a freestanding .in component.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
INAUG_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="/tmp/component-metadata-check"
IN_CLI="$INAUG_DIR/in-cli/target/release/in"

echo "=== Component Metadata Check ==="

# Build the in CLI if not present
if [ ! -f "$IN_CLI" ]; then
    echo "Building in CLI..."
    cargo build -q --release --manifest-path "$INAUG_DIR/in-cli/Cargo.toml"
fi

rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

# Create a minimal test component
cat > "$BUILD_DIR/test-component.in" << 'EOF'
package test.metadata

component MetadataTest {
  target "x86_64-test"
  deterministic true
  checkpoint full

  import dep: HelperInterface
  export api: MainInterface
  capability log: DebugConsole(write, read)
  capability store: ObjectStore(read, write, delete)
}

interface HelperInterface {
  fn help(x: Int) -> Int
}

interface MainInterface {
  fn run() -> Int
}

struct Config {
  Int version
  String name
  Bool enabled
}

fn run() -> Int {
  return 42
}
EOF

# Compile as freestanding static-lib
echo "[1] Compiling test component..."
OUTPUT=$("$IN_CLI" compile \
    --path "$BUILD_DIR/test-component.in" \
    --target native \
    --target-triple x86_64-unknown-none \
    --linkage static-lib \
    --entry run \
    --out "$BUILD_DIR/test-component.o" \
    --json 2>&1)

SUCCESS=$(echo "$OUTPUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('success', False))" 2>/dev/null || echo "false")
if [ "$SUCCESS" != "True" ]; then
    echo "FAIL: compilation not successful"
    echo "$OUTPUT"
    exit 1
fi
echo "  compilation OK"

# Check metadata sidecar exists
META="$BUILD_DIR/test-component.component-metadata.json"
if [ ! -f "$META" ]; then
    echo "FAIL: no component-metadata.json emitted"
    exit 1
fi
echo "  metadata sidecar exists: $META"

# Validate JSON keys
echo "[2] Validating metadata keys..."
python3 -c "
import json, sys
with open('$META') as f:
    d = json.load(f)

required = ['component', 'target', 'entry', 'code_sections', 'imports',
            'exports', 'capabilities_required', 'object_schemas',
            'memory', 'checkpoint', 'deterministic', 'provenance']

for key in required:
    if key not in d:
        print(f'FAIL: missing key \"{key}\"')
        sys.exit(1)

# Check specific values
assert 'MetadataTest' in d['component'], 'component name missing'
assert d['target'] == 'x86_64-test', 'wrong target'
assert d['entry'] == 'run', 'wrong entry'
assert d['deterministic'] == True, 'not deterministic'
assert d['checkpoint'] == 'full', 'wrong checkpoint'

# Check capabilities
caps = d['capabilities_required']
assert len(caps) == 2, f'expected 2 capabilities, got {len(caps)}'
cap_names = {c['name'] for c in caps}
assert 'log' in cap_names, 'missing log capability'
assert 'store' in cap_names, 'missing store capability'

# Check imports/exports
assert len(d['imports']) == 1, 'expected 1 import'
assert len(d['exports']) == 1, 'expected 1 export'
assert d['imports'][0]['interface'] == 'HelperInterface'
assert d['exports'][0]['interface'] == 'MainInterface'

# Check object schemas
schemas = d['object_schemas']
assert len(schemas) == 1, f'expected 1 schema, got {len(schemas)}'
assert schemas[0]['name'] == 'Config'
fields = schemas[0]['fields']
assert len(fields) == 3, f'expected 3 fields, got {len(fields)}'

# Check provenance
assert d['provenance']['compiler'] == 'inauguration'

print('  all keys present and valid')
print(f'  component: {d[\"component\"]}')
print(f'  target: {d[\"target\"]}')
print(f'  capabilities: {len(caps)}')
print(f'  schemas: {len(schemas)}')
print(f'  deterministic: {d[\"deterministic\"]}')
print(f'  checkpoint: {d[\"checkpoint\"]}')
"

echo ""
echo "=== Component metadata check PASSED ==="
