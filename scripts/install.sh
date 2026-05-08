#!/usr/bin/env bash
set -euo pipefail

REPO="${REPO:-semitechnological/inauguration}"
BIN_NAME="${BIN_NAME:-in}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 1
  }
}

need_cmd curl
need_cmd tar
need_cmd uname

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin) os_slug="macos" ;;
  Linux) os_slug="linux" ;;
  *)
    echo "unsupported OS: $OS" >&2
    exit 1
    ;;
esac

case "$ARCH" in
  arm64|aarch64) arch_slug="aarch64" ;;
  x86_64|amd64) arch_slug="x86_64" ;;
  *)
    echo "unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

TAG="${TAG:-}"
if [[ -z "$TAG" ]]; then
  TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name": "\(v[^"]*\)".*/\1/p' | head -n 1)"
fi

if [[ -z "$TAG" ]]; then
  echo "could not determine release tag (set TAG=vX.Y.Z)" >&2
  exit 1
fi

ASSET="${BIN_NAME}-${os_slug}-${arch_slug}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${TAG}/${ASSET}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "downloading ${URL}"
curl -fL "$URL" -o "$TMP_DIR/$ASSET"
tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"

mkdir -p "$INSTALL_DIR"
install -m 0755 "$TMP_DIR/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"

echo "installed: $INSTALL_DIR/$BIN_NAME"
echo "run: $BIN_NAME --help"
