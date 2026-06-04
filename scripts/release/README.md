# num release package

This archive contains:

- `bin/num` or `bin/num.exe` - the num CLI and language server.
- `vscode-extension/*.vsix` - the VS Code extension.
- `install.sh` - macOS/Linux installer.
- `install.ps1` - Windows PowerShell installer.

## macOS/Linux

```bash
./install.sh
```

The installer copies `num` to `$HOME/.local/bin` by default, installs zsh
completion when zsh is available, and installs the VS Code extension when the
`code` CLI is available.

Override the install directory:

```bash
NUM_INSTALL_DIR="$HOME/bin" ./install.sh
```

Also try to install Visual Studio Code when it is missing:

```bash
./install.sh --with-vscode
```

## Windows

Run PowerShell from the extracted package directory:

```powershell
.\install.ps1
```

The installer copies `num.exe` to `%LOCALAPPDATA%\num\bin`, adds that directory
to the user PATH, and installs the VS Code extension when the `code` CLI is
available.

Also try to install Visual Studio Code when it is missing:

```powershell
.\install.ps1 -WithVSCode
```
