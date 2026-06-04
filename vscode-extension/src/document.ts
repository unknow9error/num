import * as vscode from "vscode";

import { LANGUAGE_ID } from "./constants";

export function activeNumDocument(): vscode.TextDocument | undefined {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== LANGUAGE_ID) {
    return undefined;
  }

  return editor.document;
}

export function fullDocumentRange(document: vscode.TextDocument): vscode.Range {
  return new vscode.Range(
    document.positionAt(0),
    document.positionAt(document.getText().length)
  );
}

export async function replaceDocumentText(
  document: vscode.TextDocument,
  text: string
): Promise<boolean> {
  const edit = new vscode.WorkspaceEdit();
  edit.replace(document.uri, fullDocumentRange(document), text);
  return vscode.workspace.applyEdit(edit);
}
