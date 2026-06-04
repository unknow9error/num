$ErrorActionPreference = "Stop"

param(
  [switch]$WithVSCode
)

$PackageDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$InstallDir = if ($env:NUM_INSTALL_DIR) { $env:NUM_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "num\bin" }
$BinPath = Join-Path $PackageDir "bin\num.exe"
$VsixPath = Get-ChildItem -Path (Join-Path $PackageDir "vscode-extension") -Filter "*.vsix" | Select-Object -First 1

if (!(Test-Path $BinPath)) {
  throw "missing executable: $BinPath"
}

function Find-CodeCli {
  $command = Get-Command code -ErrorAction SilentlyContinue
  if ($command) {
    return $command.Source
  }

  $candidates = @(
    (Join-Path $env:LOCALAPPDATA "Programs\Microsoft VS Code\bin\code.cmd"),
    (Join-Path $env:ProgramFiles "Microsoft VS Code\bin\code.cmd"),
    (Join-Path ${env:ProgramFiles(x86)} "Microsoft VS Code\bin\code.cmd")
  )

  foreach ($candidate in $candidates) {
    if ($candidate -and (Test-Path $candidate)) {
      return $candidate
    }
  }

  return $null
}

if ($WithVSCode -and !(Find-CodeCli)) {
  Write-Host "VS Code was not found. Trying to install it..."
  if (Get-Command winget -ErrorAction SilentlyContinue) {
    winget install --id Microsoft.VisualStudioCode --source winget --accept-package-agreements --accept-source-agreements
  } elseif (Get-Command choco -ErrorAction SilentlyContinue) {
    choco install vscode -y
  } else {
    Write-Host "Could not install VS Code automatically."
    Write-Host "Install VS Code from https://code.visualstudio.com/ and run this installer again."
  }
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Force $BinPath (Join-Path $InstallDir "num.exe")

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (($UserPath -split ";") -notcontains $InstallDir) {
  [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
  Write-Host "Added $InstallDir to the user PATH. Open a new terminal to use num."
}

$CodeCli = Find-CodeCli
if ($VsixPath -and $CodeCli) {
  & $CodeCli --install-extension $VsixPath.FullName --force
} elseif ($VsixPath) {
  Write-Host "VS Code CLI 'code' was not found; install this VSIX manually:"
  Write-Host "  $($VsixPath.FullName)"
}

Write-Host "Installed num:"
& (Join-Path $InstallDir "num.exe") --help
