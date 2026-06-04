#!/usr/bin/env bash
set -euo pipefail

PACKAGE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR="${NUM_INSTALL_DIR:-$HOME/.local/bin}"
COMPLETION_DIR="${NUM_ZSH_COMPLETION_DIR:-$HOME/.zsh/completions}"
BIN_PATH="$PACKAGE_DIR/bin/num"
VSIX_PATH="$(find "$PACKAGE_DIR/vscode-extension" -maxdepth 1 -name '*.vsix' | head -n 1)"
WITH_VSCODE=0

for arg in "$@"; do
  case "$arg" in
    --with-vscode) WITH_VSCODE=1 ;;
    --help|-h)
      cat <<'EOF'
Usage: ./install.sh [--with-vscode]

Installs:
  - num CLI/LSP binary
  - zsh completion when zsh is available
  - VS Code extension when the VS Code CLI is available

Options:
  --with-vscode  Try to install Visual Studio Code if it is missing.
EOF
      exit 0
      ;;
    *) echo "unknown option: $arg" >&2; exit 1 ;;
  esac
done

if [[ ! -x "$BIN_PATH" ]]; then
  echo "missing executable: $BIN_PATH" >&2
  exit 1
fi

find_code_cli() {
  if command -v code >/dev/null 2>&1; then
    command -v code
    return 0
  fi

  local mac_code="/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"
  if [[ -x "$mac_code" ]]; then
    echo "$mac_code"
    return 0
  fi

  return 1
}

install_vscode_if_requested() {
  if [[ "$WITH_VSCODE" != "1" ]] || find_code_cli >/dev/null 2>&1; then
    return 0
  fi

  echo "VS Code was not found. Trying to install it..."

  if [[ "$(uname -s)" == "Darwin" ]] && command -v brew >/dev/null 2>&1; then
    brew install --cask visual-studio-code
    return 0
  fi

  if [[ "$(uname -s)" == "Linux" ]]; then
    if command -v snap >/dev/null 2>&1; then
      sudo snap install code --classic
      return 0
    fi
    if command -v flatpak >/dev/null 2>&1; then
      flatpak install -y flathub com.visualstudio.code
      return 0
    fi
  fi

  echo "Could not install VS Code automatically on this machine."
  echo "Install VS Code from https://code.visualstudio.com/ and run this installer again."
}

install_vscode_if_requested

mkdir -p "$INSTALL_DIR"
cp "$BIN_PATH" "$INSTALL_DIR/num"
chmod +x "$INSTALL_DIR/num"

if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  echo "num installed to $INSTALL_DIR, which is not currently in PATH."
  echo "Add this to your shell profile:"
  echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
fi

if command -v zsh >/dev/null 2>&1; then
  mkdir -p "$COMPLETION_DIR"
  "$INSTALL_DIR/num" completions zsh > "$COMPLETION_DIR/_num"

  ZSHRC="$HOME/.zshrc"
  if [[ -f "$ZSHRC" ]] && ! grep -q 'num completions' "$ZSHRC"; then
    {
      echo ''
      echo '# num completions'
      echo 'fpath=("$HOME/.zsh/completions" $fpath)'
      echo 'autoload -Uz compinit'
      echo 'compinit'
    } >> "$ZSHRC"
  fi
fi

CODE_CLI="$(find_code_cli || true)"
if [[ -n "$VSIX_PATH" ]] && [[ -n "$CODE_CLI" ]]; then
  "$CODE_CLI" --install-extension "$VSIX_PATH" --force
else
  echo "VS Code CLI 'code' was not found; install the VSIX manually if needed:"
  echo "  $VSIX_PATH"
fi

echo "Installed num:"
"$INSTALL_DIR/num" --help
