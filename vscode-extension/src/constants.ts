import * as vscode from "vscode";

export const LANGUAGE_ID = "num";
export const EXTENSION_NAME = "num";

export const NUM_DOCUMENT_SELECTOR: vscode.DocumentSelector = [
  { scheme: "file", language: LANGUAGE_ID },
  { scheme: "untitled", language: LANGUAGE_ID },
];

export const NUM_FILE_PATTERN = "**/*.num";
