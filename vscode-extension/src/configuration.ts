import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";

export interface NumConfiguration {
  cliPath: string;
  traceServer: boolean;
}

export function getNumConfiguration(): NumConfiguration {
  const config = vscode.workspace.getConfiguration("num");

  return {
    cliPath: resolveCliPath(config.get<string>("cliPath")),
    traceServer: config.get<boolean>("lsp.trace.server") ?? false,
  };
}

function resolveCliPath(configuredPath: string | undefined): string {
  const explicitPath = configuredPath?.trim();
  if (explicitPath) {
    return explicitPath;
  }

  for (const candidate of workspaceCliCandidates()) {
    if (isExecutableFile(candidate)) {
      return candidate;
    }
  }

  return "num";
}

function workspaceCliCandidates(): string[] {
  const folders = vscode.workspace.workspaceFolders ?? [];
  const candidates: string[] = [];

  for (const folder of folders) {
    candidates.push(path.join(folder.uri.fsPath, "target", "debug", executableName("num")));
    candidates.push(
      path.join(folder.uri.fsPath, "language", "target", "debug", executableName("num"))
    );
  }

  return candidates;
}

function executableName(name: string): string {
  return process.platform === "win32" ? `${name}.exe` : name;
}

function isExecutableFile(filePath: string): boolean {
  try {
    const stat = fs.statSync(filePath);
    return stat.isFile();
  } catch {
    return false;
  }
}
