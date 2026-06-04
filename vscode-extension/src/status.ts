import * as vscode from "vscode";

export interface NumUi {
  output: vscode.OutputChannel;
  status: vscode.StatusBarItem;
}

export function createNumUi(context: vscode.ExtensionContext): NumUi {
  const output = vscode.window.createOutputChannel("num");
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  status.name = "num";
  status.command = "num.checkFile";
  status.text = "$(check) num";
  status.tooltip = "Run num check on the current file";
  status.show();

  context.subscriptions.push(output, status);

  return { output, status };
}

export function setStatus(ui: NumUi, text: string, tooltip?: string): void {
  ui.status.text = text;
  ui.status.tooltip = tooltip;
}
