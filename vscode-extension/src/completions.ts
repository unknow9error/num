import * as vscode from "vscode";

import { LANGUAGE_ID } from "./constants";

const KEYWORD_COMPLETIONS = [
  "module",
  "use",
  "permission",
  "role",
  "policy",
  "type",
  "enum",
  "fn",
  "workflow",
  "action",
  "test",
  "allow",
  "deny",
  "requires",
  "require",
  "transaction",
  "saga",
  "rollback",
  "risk",
  "timeout",
  "cost",
  "let",
  "var",
  "if",
  "else",
  "match",
  "return",
  "audit",
  "assert",
  "expect_deny",
  "expect_allow",
  "expect_workflow_success",
  "expect_workflow_failure",
  "expect_audit",
  "mock_ai",
  "mock_connector",
  "confidence",
  "public",
  "internal",
  "private",
  "sensitive",
  "secret",
  "regulated",
  "trusted",
  "untrusted",
  "verified",
];

const TYPE_COMPLETIONS = [
  "Text",
  "Int",
  "Float",
  "Decimal",
  "Bool",
  "Date",
  "DateTime",
  "Duration",
  "Uuid",
  "Email",
  "PhoneNumber",
  "Url",
  "Json",
  "Bytes",
  "Result",
  "Option",
  "List",
  "Map",
  "Set",
  "Money",
  "Secret",
  "Uncertain",
];

const BUILTIN_COMPLETIONS = [
  {
    label: "Permission",
    kind: vscode.CompletionItemKind.Module,
    detail: "Built-in namespace",
    documentation: "References permissions declared with `permission <Name>`.",
  },
  {
    label: "KZT",
    kind: vscode.CompletionItemKind.Constant,
    detail: "Currency code",
    documentation: "Kazakhstani tenge. Used as a type argument in `Money<KZT>`.",
  },
  {
    label: "USD",
    kind: vscode.CompletionItemKind.Constant,
    detail: "Currency code",
    documentation: "United States dollar. Used as a type argument in `Money<USD>`.",
  },
  {
    label: "EUR",
    kind: vscode.CompletionItemKind.Constant,
    detail: "Currency code",
    documentation: "Euro. Used as a type argument in `Money<EUR>`.",
  },
];

export function registerCompletionProvider(context: vscode.ExtensionContext): void {
  const provider = vscode.languages.registerCompletionItemProvider(
    { language: LANGUAGE_ID, scheme: "file" },
    new NumCompletionProvider(),
    "."
  );

  context.subscriptions.push(provider);
}

class NumCompletionProvider implements vscode.CompletionItemProvider {
  provideCompletionItems(
    document: vscode.TextDocument,
    position: vscode.Position
  ): vscode.ProviderResult<vscode.CompletionItem[]> {
    const context = getDocumentContext(document);
    const linePrefix = document.lineAt(position).text.slice(0, position.character);
    const memberMatch = /([A-Za-z_][A-Za-z0-9_]*)\.\s*([A-Za-z_][A-Za-z0-9_]*)?$/.exec(
      linePrefix
    );

    if (memberMatch?.[1] === "Permission") {
      const prefix = memberMatch[2] ?? "";
      return context.permissions
        .filter((name) => name.startsWith(prefix))
        .map((name) => completion(name, vscode.CompletionItemKind.Constant, "Permission"));
    }

    return [
      ...KEYWORD_COMPLETIONS.map((name) =>
        completion(name, vscode.CompletionItemKind.Keyword, "Keyword")
      ),
      ...TYPE_COMPLETIONS.map((name) =>
        completion(name, vscode.CompletionItemKind.TypeParameter, "Type")
      ),
      ...BUILTIN_COMPLETIONS.map((item) =>
        completion(item.label, item.kind, item.detail, item.documentation)
      ),
      ...context.declarations.map((decl) =>
        completion(decl.name, decl.kind, decl.detail)
      ),
    ];
  }
}

interface DocumentContext {
  permissions: string[];
  declarations: Array<{
    name: string;
    detail: string;
    kind: vscode.CompletionItemKind;
  }>;
}

function getDocumentContext(document: vscode.TextDocument): DocumentContext {
  const text = document.getText();
  const permissions = new Set<string>();
  const declarations: DocumentContext["declarations"] = [];
  const declarationPattern =
    /^\s*(permission|role|policy|type|enum|fn|workflow|action|connector|service|test)\s+([A-Za-z_][A-Za-z0-9_]*|"[^"]+")/gm;

  for (const match of text.matchAll(declarationPattern)) {
    const kind = match[1];
    const name = match[2]?.replace(/^"|"$/g, "");
    if (!name) {
      continue;
    }

    if (kind === "permission") {
      permissions.add(name);
    }

    declarations.push({
      name,
      detail: toTitleCase(kind),
      kind: completionKind(kind),
    });
  }

  return {
    permissions: [...permissions],
    declarations,
  };
}

function completion(
  label: string,
  kind: vscode.CompletionItemKind,
  detail: string,
  documentation?: string
): vscode.CompletionItem {
  const item = new vscode.CompletionItem(label, kind);
  item.detail = detail;
  if (documentation) {
    item.documentation = new vscode.MarkdownString(documentation);
  }
  return item;
}

function completionKind(kind: string): vscode.CompletionItemKind {
  switch (kind) {
    case "permission":
      return vscode.CompletionItemKind.Constant;
    case "fn":
    case "workflow":
    case "action":
      return vscode.CompletionItemKind.Function;
    case "type":
    case "role":
      return vscode.CompletionItemKind.Class;
    case "enum":
      return vscode.CompletionItemKind.Enum;
    default:
      return vscode.CompletionItemKind.Module;
  }
}

function toTitleCase(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}
