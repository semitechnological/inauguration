#!/usr/bin/env bash
# Hermetic contract: release producer checksum asset name must match
# install.sh consumer request (${tarball}.sha256). No network.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_YML="${ROOT}/.github/workflows/release.yml"
INSTALL_SH="${ROOT}/install.sh"

die() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$RELEASE_YML" ]] || die "missing $RELEASE_YML"
[[ -f "$INSTALL_SH" ]] || die "missing $INSTALL_SH"

# Producer: shasum redirect target for the packaged tarball.
# Expect basename form in-<suffix>.tar.gz.sha256 (same path stem as the tarball).
producer_line="$(
  grep -E 'shasum[[:space:]]+-a[[:space:]]+256[[:space:]]+"dist/in-\$\{\{[[:space:]]*matrix\.asset_suffix[[:space:]]*\}\}\.tar\.gz"' \
    "$RELEASE_YML" | head -n1 || true
)"
[[ -n "$producer_line" ]] || die "release.yml missing shasum of dist/in-\${{ matrix.asset_suffix }}.tar.gz"

producer_checksum="$(
  printf '%s\n' "$producer_line" \
    | sed -n 's/.*[[:space:]]>[[:space:]]*"\(dist\/in-\${{ matrix\.asset_suffix }}\.[^"]*\)".*/\1/p' \
    | head -n1
)"
# Fallback: allow unquoted redirect target
if [[ -z "$producer_checksum" ]]; then
  producer_checksum="$(
    printf '%s\n' "$producer_line" \
      | sed -n 's/.*[[:space:]]>[[:space:]]*\(dist\/in-\${{ matrix\.asset_suffix }}\.[^[:space:]]*\).*/\1/p' \
      | head -n1
  )"
fi
[[ -n "$producer_checksum" ]] || die "could not parse producer checksum path from: $producer_line"

# Consumer: ASSET is in-${os}-${arch}.tar.gz; checksum URL is ${ASSET}.sha256
consumer_asset_line="$(
  grep -E 'ASSET=.*\$\{BIN_NAME\}-\$\{os\}-\$\{arch_suffix\}\.tar\.gz' "$INSTALL_SH" | head -n1 || true
)"
[[ -n "$consumer_asset_line" ]] || die "install.sh missing ASSET=\${BIN_NAME}-\${os}-\${arch_suffix}.tar.gz"

if ! grep -qE '\$\{BASE_URL\}/\$\{ASSET\}\.sha256' "$INSTALL_SH"; then
  die "install.sh does not request \${ASSET}.sha256"
fi

# Canonical expected producer path: tarball path + .sha256
expected_producer="dist/in-\${{ matrix.asset_suffix }}.tar.gz.sha256"

# Normalize whitespace in matrix expression for comparison
norm() {
  printf '%s' "$1" | sed -E 's/[[:space:]]+//g'
}

got="$(norm "$producer_checksum")"
want="$(norm "$expected_producer")"

if [[ "$got" != "$want" ]]; then
  die "checksum asset filename contract mismatch
  producer writes: ${producer_checksum}
  consumer expects: \${ASSET}.sha256 where ASSET=in-\${os}-\${arch}.tar.gz
  i.e. producer must write: ${expected_producer}
  (install would look for in-<os>-<arch>.tar.gz.sha256; release currently names differently)"
fi

echo "OK: release checksum asset matches install.sh (\${tarball}.sha256)"
