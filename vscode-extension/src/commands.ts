import * as path from "node:path";
import * as vscode from "vscode";

import { runNumCli } from "./cli";
import { getNumConfiguration } from "./configuration";
import { activeNumDocument, replaceDocumentText } from "./document";
import { NumLanguageClientManager } from "./languageClient";
import { NumUi, setStatus } from "./status";

export function registerCommands(
  context: vscode.ExtensionContext,
  ui: NumUi,
  clientManager: NumLanguageClientManager
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("num.checkFile", () => checkCurrentFile(ui)),
    vscode.commands.registerCommand("num.formatFile", () => formatCurrentFile(ui)),
    vscode.commands.registerCommand("num.restartLanguageServer", () =>
      clientManager.restart()
    ),
    vscode.commands.registerCommand("num.newProject", () => createProject(ui))
  );
}

async function checkCurrentFile(ui: NumUi): Promise<void> {
  const document = activeNumDocument();
  if (!document) {
    vscode.window.showWarningMessage("Open a .num file before running num check.");
    return;
  }

  await document.save();

  const { cliPath } = getNumConfiguration();
  const relativeName = vscode.workspace.asRelativePath(document.uri);
  setStatus(ui, "$(sync~spin) num checking", `Checking ${relativeName}`);

  try {
    const result = await runNumCli(cliPath, ["check", document.uri.fsPath]);
    writeCommandOutput(ui, "check", document.uri.fsPath, result.stdout, result.stderr);
    setStatus(ui, "$(check) num", `num check passed: ${relativeName}`);
    vscode.window.setStatusBarMessage(`num check passed: ${relativeName}`, 3000);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    writeCommandOutput(ui, "check", document.uri.fsPath, "", message);
    setStatus(ui, "$(error) num", `num check failed: ${relativeName}`);
    ui.output.show(true);
    vscode.window.showErrorMessage(`num check failed: ${message}`);
  }
}

async function formatCurrentFile(ui: NumUi): Promise<void> {
  const document = activeNumDocument();
  if (!document) {
    vscode.window.showWarningMessage("Open a .num file before running num format.");
    return;
  }

  const { cliPath } = getNumConfiguration();
  const relativeName = vscode.workspace.asRelativePath(document.uri);
  setStatus(ui, "$(sync~spin) num formatting", `Formatting ${relativeName}`);

  try {
    const result = await runNumCli(cliPath, ["fmt", document.uri.fsPath]);
    if (result.stdout !== document.getText()) {
      await replaceDocumentText(document, result.stdout);
    }
    setStatus(ui, "$(check) num", `num format applied: ${relativeName}`);
    vscode.window.setStatusBarMessage(`num format applied: ${relativeName}`, 3000);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    writeCommandOutput(ui, "fmt", document.uri.fsPath, "", message);
    setStatus(ui, "$(error) num", `num format failed: ${relativeName}`);
    ui.output.show(true);
    vscode.window.showErrorMessage(`num format failed: ${message}`);
  }
}

async function createProject(ui: NumUi): Promise<void> {
  const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const defaultPath = workspaceRoot ? path.join(workspaceRoot, "num-app") : "num-app";
  const target = await vscode.window.showInputBox({
    prompt: "Project directory",
    value: defaultPath,
    ignoreFocusOut: true,
  });

  if (!target) {
    return;
  }

  const { cliPath } = getNumConfiguration();
  setStatus(ui, "$(sync~spin) num creating", `Creating ${target}`);

  try {
    const result = await runNumCli(cliPath, ["new", target]);
    writeCommandOutput(ui, "new", target, result.stdout, result.stderr);
    setStatus(ui, "$(check) num", `Created ${target}`);
    vscode.window.showInformationMessage(`num project created: ${target}`);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    writeCommandOutput(ui, "new", target, "", message);
    setStatus(ui, "$(error) num", `Failed to create ${target}`);
    ui.output.show(true);
    vscode.window.showErrorMessage(`num project creation failed: ${message}`);
  }
}

function writeCommandOutput(
  ui: NumUi,
  command: string,
  target: string,
  stdout: string,
  stderr: string
): void {
  ui.output.appendLine(`> num ${command} ${target}`);
  if (stdout.trim()) {
    ui.output.append(stdout.endsWith("\n") ? stdout : `${stdout}\n`);
  }
  if (stderr.trim()) {
    ui.output.append(stderr.endsWith("\n") ? stderr : `${stderr}\n`);
  }
  ui.output.appendLine("");
}
