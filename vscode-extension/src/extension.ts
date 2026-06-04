import * as vscode from "vscode";

import { registerCommands } from "./commands";
import { registerCompletionProvider } from "./completions";
import { NumLanguageClientManager } from "./languageClient";
import { createNumUi } from "./status";

let clientManager: NumLanguageClientManager | undefined;

export function activate(context: vscode.ExtensionContext) {
  const ui = createNumUi(context);
  clientManager = new NumLanguageClientManager(context, ui);

  registerCommands(context, ui, clientManager);
  registerCompletionProvider(context);
  context.subscriptions.push(clientManager);

  void clientManager.start();
}

export function deactivate(): Thenable<void> | undefined {
  return clientManager?.stop();
}
