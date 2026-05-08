#!/usr/bin/env bash
# inauguration `in` installer — clone: cargo build release in in-cli; else GitHub tarballs.
# Usage:
#   ./install.sh                    # checkout → cargo build --release (in-cli)
#   IN_USE_RELEASE=1 ./install.sh   # checkout → still fetch release binary
#
set -euo pipefail

REPO="${IN_REPO:-semitechnological/inauguration}"
INSTALL_DIR="${IN_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${IN_VERSION:-}"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'; CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

info()  { printf '%b\n' "${CYAN}${*}${NC}"; }
ok()    { printf '%b\n' "${GREEN}✓ ${*}${NC}"; }
warn()  { printf '%b\n' "${YELLOW}! ${*}${NC}" >&2; }
die()   { printf '%b\n' "${RED}error: ${*}${NC}" >&2; exit 1; }

path_hint() {
  if command -v in &>/dev/null 2>&1; then
    return
  fi
  printf '\n%sAdd `in` to your PATH:%s\n' "$BOLD" "$NC"
  case "${SHELL:-}" in
    */fish) printf '  fish_add_path %s\n' "$INSTALL_DIR" ;;
    *)      printf '  echo '\''export PATH="%s:$PATH"'\'' >> ~/.bashrc  # or ~/.zshrc\n' "$INSTALL_DIR" ;;
  esac
}

install_from_repo() {
  local root="$1"
  if ! command -v cargo &>/dev/null; then
    die "cargo not in PATH — install Rust from https://rustup.rs/ or set IN_USE_RELEASE=1 to download a binary."
  fi
  info "Building \`in\` from local checkout (${root})…"
  (cd "${root}/in-cli" && cargo build --release)
  mkdir -p "$INSTALL_DIR"
  cp "${root}/in-cli/target/release/in" "${INSTALL_DIR}/in"
  chmod +x "${INSTALL_DIR}/in"
  ok "Built and installed ${INSTALL_DIR}/in"
  path_hint
}

install_from_release() {
  local OS ARCH os arch_suffix ASSET BASE_URL TMP TMP_SHA HAVE_SHA EXPECTED ACTUAL BIN_NAME

  BIN_NAME=in
  OS="$(uname -s)"
  ARCH="$(uname -m)"

  case "$OS-$ARCH" in
    Darwin-arm64|Darwin-aarch64) os="macos"; arch_suffix="aarch64" ;;
    Linux-x86_64|Linux-amd64)   os="linux"; arch_suffix="x86_64" ;;
    *)
      die "No prebuilt release for ${OS}/${ARCH}; build from source with ./install.sh in a checkout (see README)."
      ;;
  esac

  ASSET="${BIN_NAME}-${os}-${arch_suffix}.tar.gz"

  if [[ -z "$VERSION" ]]; then
    info "Fetching latest GitHub release tag…"
    VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
      | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)"
    [[ -n "$VERSION" ]] || die "Could not determine latest release (set IN_VERSION=vX.Y.Z)."
  fi

  info "Installing ${BIN_NAME} ${VERSION} (${os}-${arch_suffix}) from GitHub Releases…"

  BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
  TMP="$(mktemp)"
  TMP_SHA="$(mktemp)"
  trap 'rm -f "${TMP:-}" "${TMP_SHA:-}"' EXIT

  HAVE_SHA=0
  if command -v curl &>/dev/null; then
    curl -fsSL --progress-bar "${BASE_URL}/${ASSET}" -o "$TMP"
    if curl -fsSL "${BASE_URL}/${ASSET}.sha256" -o "$TMP_SHA" 2>/dev/null; then
      HAVE_SHA=1
    fi
  elif command -v wget &>/dev/null; then
    wget -q --show-progress "${BASE_URL}/${ASSET}" -O "$TMP"
    if wget -q "${BASE_URL}/${ASSET}.sha256" -O "$TMP_SHA" 2>/dev/null; then
      HAVE_SHA=1
    fi
  else
    die "curl or wget is required"
  fi

  if [[ "$HAVE_SHA" -eq 1 ]] && [[ -s "$TMP_SHA" ]]; then
    EXPECTED="$(head -n1 "$TMP_SHA" | awk '{print $1}' | tr -d '[:space:]')"
    if command -v sha256sum &>/dev/null; then
      ACTUAL="$(sha256sum "$TMP" | awk '{print $1}')"
    elif command -v shasum &>/dev/null; then
      ACTUAL="$(shasum -a 256 "$TMP" | awk '{print $1}')"
    else
      die "sha256sum or shasum is required for checksum verification"
    fi

    [[ "$ACTUAL" = "$EXPECTED" ]] || die "SHA256 mismatch
  expected: $EXPECTED
  actual:   $ACTUAL"
    ok "Checksum verified"
  elif [[ "${IN_NO_VERIFY:-}" = "1" ]]; then
    warn "IN_NO_VERIFY=1 — skipping tarball integrity verification"
  else
    die "Missing ${ASSET}.sha256 for ${VERSION}; set IN_NO_VERIFY=1 to bypass (not recommended)."
  fi

  mkdir -p "$INSTALL_DIR"
  local extract
  extract="$(mktemp -d)"
  trap 'rm -rf "${extract:-}" "${TMP:-}" "${TMP_SHA:-}"' EXIT
  tar -xzf "$TMP" -C "$extract"

  if [[ ! -f "${extract}/${BIN_NAME}" ]]; then
    die "Release tarball missing \`${BIN_NAME}\` binary at archive root."
  fi
  install -m 0755 "${extract}/${BIN_NAME}" "${INSTALL_DIR}/${BIN_NAME}"

  ok "${BIN_NAME} ${VERSION} → ${INSTALL_DIR}/${BIN_NAME}"
  path_hint
}

# Checkout → build (matches .github/workflows/release.yml layout)
_src="${BASH_SOURCE[0]:-}"
if [[ -n "${_src}" ]] && [[ "$(basename -- "${_src}")" == "install.sh" ]] && [[ "${IN_USE_RELEASE:-}" != "1" ]]; then
  _root="$(cd "$(dirname -- "${_src}")" && pwd)"
  if [[ -f "${_root}/in-cli/Cargo.toml" ]] && grep -q 'name = "inauguration"' "${_root}/in-cli/Cargo.toml"; then
    install_from_repo "${_root}"
    exit 0
  fi
fi

install_from_release
