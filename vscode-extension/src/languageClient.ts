import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

import { getNumConfiguration } from "./configuration";
import { LANGUAGE_ID, NUM_FILE_PATTERN } from "./constants";
import { NumUi, setStatus } from "./status";

export class NumLanguageClientManager implements vscode.Disposable {
  private client: LanguageClient | undefined;

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly ui: NumUi
  ) {}

  async start(): Promise<void> {
    if (this.client) {
      return;
    }

    const config = getNumConfiguration();
    const serverOptions: ServerOptions = {
      command: config.cliPath,
      args: ["lsp"],
    };
    const clientOptions: LanguageClientOptions = {
      documentSelector: [
        { scheme: "file", language: LANGUAGE_ID },
        { scheme: "untitled", language: LANGUAGE_ID },
      ],
      outputChannel: this.ui.output,
      traceOutputChannel: config.traceServer ? this.ui.output : undefined,
      synchronize: {
        configurationSection: "num",
        fileEvents: vscode.workspace.createFileSystemWatcher(NUM_FILE_PATTERN),
      },
    };

    this.client = new LanguageClient(
      "num",
      "num Language Server",
      serverOptions,
      clientOptions
    );

    setStatus(this.ui, "$(sync~spin) num", "Starting num language server");

    try {
      await this.client.start();
      setStatus(this.ui, "$(check) num", `num language server: ${config.cliPath}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.client = undefined;
      setStatus(this.ui, "$(error) num", "num language server failed to start");
      vscode.window.showErrorMessage(`Failed to start num language server: ${message}`);
    }
  }

  async restart(): Promise<void> {
    setStatus(this.ui, "$(sync~spin) num", "Restarting num language server");
    await this.stop();
    await this.start();
    vscode.window.setStatusBarMessage("num language server restarted", 3000);
  }

  async stop(): Promise<void> {
    const client = this.client;
    this.client = undefined;
    if (client) {
      await client.stop();
    }
  }

  dispose(): void {
    void this.stop();
  }
}
