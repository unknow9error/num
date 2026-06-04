#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT_DIR/language/crates/num-cli/Cargo.toml" | head -n 1)"

if [[ -z "$VERSION" ]]; then
  echo "failed to read num version" >&2
  exit 1
fi

case "$(uname -s)" in
  Darwin) OS_NAME="macos" ;;
  Linux) OS_NAME="linux" ;;
  MINGW*|MSYS*|CYGWIN*) OS_NAME="windows" ;;
  *) echo "unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  arm64|aarch64) ARCH_NAME="arm64" ;;
  x86_64|amd64) ARCH_NAME="x64" ;;
  *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

PACKAGE_NAME="num-${VERSION}-${OS_NAME}-${ARCH_NAME}"
DIST_DIR="$ROOT_DIR/dist"
RELEASE_DIR="$DIST_DIR/releases"
STAGE_DIR="$RELEASE_DIR/$PACKAGE_NAME"
BIN_NAME="num"

if [[ "$OS_NAME" == "windows" ]]; then
  BIN_NAME="num.exe"
fi

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR/bin" "$STAGE_DIR/vscode-extension"

echo "building num CLI..."
cargo build --release -p num --manifest-path "$ROOT_DIR/Cargo.toml"

echo "building VS Code extension..."
npm --prefix "$ROOT_DIR/vscode-extension" ci
npm --prefix "$ROOT_DIR/vscode-extension" run compile
(
  cd "$ROOT_DIR/vscode-extension"
  npx --yes @vscode/vsce@3.9.1 package \
    --allow-missing-repository \
    --out "$STAGE_DIR/vscode-extension/num-lang-${VERSION}.vsix"
)

cp "$ROOT_DIR/target/release/$BIN_NAME" "$STAGE_DIR/bin/$BIN_NAME"
cp "$ROOT_DIR/scripts/release/install.sh" "$STAGE_DIR/install.sh"
cp "$ROOT_DIR/scripts/release/install.ps1" "$STAGE_DIR/install.ps1"
cp "$ROOT_DIR/scripts/release/README.md" "$STAGE_DIR/README.md"
chmod +x "$STAGE_DIR/install.sh" "$STAGE_DIR/bin/$BIN_NAME"

(
  cd "$RELEASE_DIR"
  rm -f "$PACKAGE_NAME.tar.gz" "$PACKAGE_NAME.zip"
  if [[ "$OS_NAME" == "windows" ]]; then
    if command -v powershell.exe >/dev/null 2>&1; then
      powershell.exe -NoProfile -Command \
        "Compress-Archive -Path '$PACKAGE_NAME' -DestinationPath '$PACKAGE_NAME.zip' -Force"
    elif command -v pwsh >/dev/null 2>&1; then
      pwsh -NoProfile -Command \
        "Compress-Archive -Path '$PACKAGE_NAME' -DestinationPath '$PACKAGE_NAME.zip' -Force"
    else
      tar -a -cf "$PACKAGE_NAME.zip" "$PACKAGE_NAME"
    fi
    echo "created $RELEASE_DIR/$PACKAGE_NAME.zip"
  else
    tar -czf "$PACKAGE_NAME.tar.gz" "$PACKAGE_NAME"
    echo "created $RELEASE_DIR/$PACKAGE_NAME.tar.gz"
  fi
)
