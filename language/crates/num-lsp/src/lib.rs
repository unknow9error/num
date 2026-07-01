use num_compiler::{
    ast::*,
    builtins::{self, BuiltinKind},
    check, compile, compile_program, formatter,
    span::Span,
    token::{Token, TokenKind},
    SourceFile,
};
mod json;

use json::{JsonParser, JsonValue};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspDiagnostic {
    pub code: &'static str,
    pub severity: LspSeverity,
    pub message: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspSeverity {
    Error,
    Warning,
    Information,
}

pub fn diagnostics(source_name: &str, source: &str) -> Vec<LspDiagnostic> {
    check(source_name, source)
        .into_iter()
        .map(lsp_diagnostic)
        .collect()
}

pub fn program_diagnostics(
    source_name: &str,
    source: &str,
    open_documents: &HashMap<String, String>,
) -> Vec<LspDiagnostic> {
    compile_lsp_program(source_name, source, open_documents)
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.span.source == source_name)
        .map(lsp_diagnostic)
        .collect()
}

fn compile_lsp_program(
    source_name: &str,
    source: &str,
    open_documents: &HashMap<String, String>,
) -> num_compiler::ProgramCompilation {
    let files = lsp_program_files(source_name, source, open_documents);
    compile_program(files, Some(source_name))
}

fn lsp_diagnostic(diagnostic: num_compiler::diagnostic::Diagnostic) -> LspDiagnostic {
    let mut message = diagnostic.message.clone();
    if let Some(reason) = &diagnostic.reason {
        message.push_str(&format!("\n\nReason: {reason}"));
    }
    if let Some(help) = &diagnostic.help {
        message.push_str(&format!("\nHelp: {help}"));
    }
    LspDiagnostic {
        code: diagnostic.code,
        severity: match diagnostic.severity {
            num_compiler::diagnostic::Severity::Error => LspSeverity::Error,
            num_compiler::diagnostic::Severity::Warning => LspSeverity::Warning,
            num_compiler::diagnostic::Severity::Info => LspSeverity::Information,
        },
        message,
        line: diagnostic.span.line,
        column: diagnostic.span.column,
    }
}

fn lsp_program_files(
    source_name: &str,
    source: &str,
    open_documents: &HashMap<String, String>,
) -> Vec<SourceFile> {
    let entry_path = Path::new(source_name);
    let root = entry_path.parent().unwrap_or_else(|| Path::new("."));
    let mut paths = Vec::new();
    if should_scan_lsp_root(root) {
        collect_num_file_paths(root, &mut paths);
    }
    paths.sort();
    paths.dedup();

    let mut files = Vec::new();
    let mut included_entry = false;
    for path in paths {
        let name = path.display().to_string();
        let text = if name == source_name {
            included_entry = true;
            Some(source.to_string())
        } else {
            let uri = format!("file://{name}");
            open_documents
                .get(&uri)
                .cloned()
                .or_else(|| fs::read_to_string(&path).ok())
        };

        if let Some(text) = text {
            files.push(SourceFile::new(name, text));
        }
    }

    if !included_entry {
        files.push(SourceFile::new(source_name.to_string(), source.to_string()));
    }

    files
}

fn should_scan_lsp_root(root: &Path) -> bool {
    root.parent().is_some() || root == Path::new(".")
}

fn collect_num_file_paths(path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_num_file_paths(&entry_path, files);
        } else if entry_path
            .extension()
            .is_some_and(|extension| extension == "num")
        {
            files.push(entry_path);
        }
    }
}

pub fn completions(prefix: &str) -> Vec<&'static str> {
    const KEYWORDS: &[&str] = &[
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
        "from",
        "let",
        "var",
        "if",
        "else",
        "return",
        "transaction",
        "saga",
        "audit",
        "requires",
        "require",
        "rollback",
        "risk",
        "timeout",
        "cost",
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
        "Uncertain",
        "Text",
        "Int",
        "Bool",
        "Money",
        "Secret",
        "Permission",
        "KZT",
        "USD",
        "EUR",
        "GBP",
        "RUB",
        "CNY",
    ];

    KEYWORDS
        .iter()
        .copied()
        .filter(|keyword| keyword.starts_with(prefix))
        .collect()
}

// ==========================================
// LSP Server Loop and Command Handlers
// ==========================================

pub fn run_server() -> Result<(), String> {
    let mut documents: HashMap<String, String> = HashMap::new();
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    loop {
        let msg = match read_message(&mut handle) {
            Ok(m) => m,
            Err(e) => {
                if e == "EOF" {
                    break;
                }
                return Err(e);
            }
        };

        let mut parser = JsonParser::new(&msg);
        let request = match parser.parse() {
            Ok(val) => val,
            Err(err) => {
                eprintln!("[LSP Server] JSON Parse Error: {}", err);
                continue;
            }
        };

        let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let id = request.get("id").unwrap_or(&JsonValue::Null);

        match method {
            "initialize" => {
                let response = r#"{
                    "capabilities": {
                        "textDocumentSync": {
                            "openClose": true,
                            "change": 1,
                            "save": {
                                "includeText": false
                            }
                        },
                        "completionProvider": {
                            "resolveProvider": false,
                            "triggerCharacters": [".", " ", "<"]
                        },
                        "hoverProvider": true,
                        "definitionProvider": true,
                        "renameProvider": true,
                        "documentFormattingProvider": true,
                        "documentSymbolProvider": true
                    }
                }"#;
                send_response(id, response);
            }
            "shutdown" => {
                send_response(id, "null");
            }
            "textDocument/completion" => {
                if let Some(params) = request.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let Some(uri) = doc.get("uri").and_then(|v| v.as_str()) {
                            if let Some(pos) = params.get("position") {
                                let line = pos.get("line").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                    as usize;
                                let character =
                                    pos.get("character").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                        as usize;

                                let completion_res = if let Some(text) = documents.get(uri) {
                                    handle_completion(uri, text, line, character, &documents)
                                } else {
                                    "[]".to_string()
                                };
                                send_response(id, &completion_res);
                                continue;
                            }
                        }
                    }
                }
                send_response(id, "[]");
            }
            "textDocument/didOpen" => {
                if let Some(params) = request.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let (Some(uri), Some(text)) = (
                            doc.get("uri").and_then(|v| v.as_str()),
                            doc.get("text").and_then(|v| v.as_str()),
                        ) {
                            documents.insert(uri.to_string(), text.to_string());
                            publish_diagnostics(uri, text, &documents);
                        }
                    }
                }
            }
            "textDocument/didChange" => {
                if let Some(params) = request.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let Some(uri) = doc.get("uri").and_then(|v| v.as_str()) {
                            if let Some(changes) =
                                params.get("contentChanges").and_then(|v| match v {
                                    JsonValue::Array(a) => Some(a),
                                    _ => None,
                                })
                            {
                                if let Some(change) = changes.first() {
                                    if let Some(text) = change.get("text").and_then(|v| v.as_str())
                                    {
                                        documents.insert(uri.to_string(), text.to_string());
                                        publish_diagnostics(uri, text, &documents);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "textDocument/didSave" => {
                if let Some(params) = request.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let Some(uri) = doc.get("uri").and_then(|v| v.as_str()) {
                            if let Some(text) = documents.get(uri) {
                                publish_diagnostics(uri, text, &documents);
                            }
                        }
                    }
                }
            }
            "textDocument/hover" => {
                if let Some(params) = request.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let Some(uri) = doc.get("uri").and_then(|v| v.as_str()) {
                            if let Some(pos) = params.get("position") {
                                let line = pos.get("line").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                    as usize;
                                let character =
                                    pos.get("character").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                        as usize;

                                let hover_res = if let Some(text) = documents.get(uri) {
                                    handle_hover(uri, text, line, character, &documents)
                                } else {
                                    "null".to_string()
                                };
                                send_response(id, &hover_res);
                                continue;
                            }
                        }
                    }
                }
                send_response(id, "null");
            }
            "textDocument/definition" => {
                if let Some(params) = request.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let Some(uri) = doc.get("uri").and_then(|v| v.as_str()) {
                            if let Some(pos) = params.get("position") {
                                let line = pos.get("line").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                    as usize;
                                let character =
                                    pos.get("character").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                        as usize;

                                let def_res = if let Some(text) = documents.get(uri) {
                                    handle_definition(uri, text, line, character, &documents)
                                } else {
                                    "null".to_string()
                                };
                                send_response(id, &def_res);
                                continue;
                            }
                        }
                    }
                }
                send_response(id, "null");
            }
            "textDocument/rename" => {
                if let Some(params) = request.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let Some(uri) = doc.get("uri").and_then(|v| v.as_str()) {
                            if let (Some(pos), Some(new_name)) = (
                                params.get("position"),
                                params.get("newName").and_then(|v| v.as_str()),
                            ) {
                                let line = pos.get("line").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                    as usize;
                                let character =
                                    pos.get("character").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                        as usize;

                                let rename_res = if let Some(text) = documents.get(uri) {
                                    handle_rename(uri, text, line, character, new_name, &documents)
                                } else {
                                    "null".to_string()
                                };
                                send_response(id, &rename_res);
                                continue;
                            }
                        }
                    }
                }
                send_response(id, "null");
            }
            "textDocument/formatting" => {
                if let Some(params) = request.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let Some(uri) = doc.get("uri").and_then(|v| v.as_str()) {
                            let formatting_res = if let Some(text) = documents.get(uri) {
                                handle_formatting(uri, text)
                            } else {
                                "[]".to_string()
                            };
                            send_response(id, &formatting_res);
                            continue;
                        }
                    }
                }
                send_response(id, "[]");
            }
            "textDocument/documentSymbol" => {
                if let Some(params) = request.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let Some(uri) = doc.get("uri").and_then(|v| v.as_str()) {
                            let symbols_res = if let Some(text) = documents.get(uri) {
                                handle_document_symbols(uri, text)
                            } else {
                                "[]".to_string()
                            };
                            send_response(id, &symbols_res);
                            continue;
                        }
                    }
                }
                send_response(id, "[]");
            }
            _ => {
                // Return default response for unhandled requests to avoid blocking VS Code client
                if id != &JsonValue::Null {
                    send_response(id, "null");
                }
            }
        }
    }

    Ok(())
}

fn read_message<R: Read>(reader: &mut R) -> Result<String, String> {
    let mut content_length = None;
    let mut line = String::new();
    loop {
        line.clear();
        let mut byte = [0u8; 1];
        loop {
            if reader.read_exact(&mut byte).is_err() {
                return Err("EOF".to_string());
            }
            line.push(byte[0] as char);
            if byte[0] == b'\n' {
                break;
            }
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if line.to_lowercase().starts_with("content-length:") {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 {
                let len_str = parts[1].trim();
                content_length = len_str.parse::<usize>().ok();
            }
        }
    }

    let len = content_length.ok_or_else(|| "Missing Content-Length header".to_string())?;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).map_err(|e| e.to_string())?;
    String::from_utf8(buf).map_err(|e| e.to_string())
}

fn send_response(id: &JsonValue, result_json: &str) {
    let id_str = match id {
        JsonValue::Number(n) => n.to_string(),
        JsonValue::String(s) => format!("\"{}\"", json_escape(s)),
        _ => "null".to_string(),
    };
    let response = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{}}}",
        id_str, result_json
    );
    print!("Content-Length: {}\r\n\r\n{}", response.len(), response);
    io::stdout().flush().unwrap();
}

fn send_notification(method: &str, params_json: &str) {
    let response = format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"{}\",\"params\":{}}}",
        method, params_json
    );
    print!("Content-Length: {}\r\n\r\n{}", response.len(), response);
    io::stdout().flush().unwrap();
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn publish_diagnostics(uri: &str, text: &str, documents: &HashMap<String, String>) {
    let path = uri_to_path(uri);
    let ds = program_diagnostics(&path, text, documents);

    let mut ds_json = String::new();
    ds_json.push('[');
    for (i, d) in ds.iter().enumerate() {
        if i > 0 {
            ds_json.push_str(", ");
        }
        let severity_code = match d.severity {
            LspSeverity::Error => 1,
            LspSeverity::Warning => 2,
            LspSeverity::Information => 3,
        };
        // Normalize 1-indexed compiler line/col to 0-indexed LSP coordinates
        let l = d.line.saturating_sub(1);
        let c = d.column.saturating_sub(1);
        ds_json.push_str(&format!(
            "{{\"range\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}},\"severity\":{},\"code\":\"{}\",\"message\":\"{}\",\"source\":\"num\"}}",
            l, c, l, c + 1, severity_code, d.code, json_escape(&d.message)
        ));
    }
    ds_json.push(']');

    let params = format!("{{\"uri\":\"{}\",\"diagnostics\":{}}}", uri, ds_json);
    send_notification("textDocument/publishDiagnostics", &params);
}

fn uri_to_path(uri: &str) -> String {
    if uri.starts_with("file://") {
        #[cfg(windows)]
        {
            uri.trim_start_matches("file:///").replace("/", "\\")
        }
        #[cfg(not(windows))]
        {
            uri.trim_start_matches("file://").to_string()
        }
    } else {
        uri.to_string()
    }
}

// Convert 0-indexed line/char to byte offset
fn lsp_pos_to_byte_offset(source: &str, line: usize, character: usize) -> usize {
    let mut offset = 0;
    for (i, l) in source.lines().enumerate() {
        if i == line {
            return offset + utf16_character_to_byte_offset(l, character);
        }
        // Count both line content and standard trailing newline
        offset += l.len() + 1;
    }
    offset
}

fn utf16_character_to_byte_offset(line: &str, character: usize) -> usize {
    if character == 0 {
        return 0;
    }

    let mut utf16_units = 0usize;
    for (byte_index, ch) in line.char_indices() {
        if utf16_units >= character {
            return byte_index;
        }
        utf16_units += ch.len_utf16();
        if utf16_units > character {
            return byte_index;
        }
    }

    line.len()
}

fn byte_offset_to_utf16_character(line: &str, byte_offset: usize) -> usize {
    let byte_offset = byte_offset.min(line.len());
    line[..byte_offset]
        .chars()
        .map(char::len_utf16)
        .sum::<usize>()
}

fn word_prefix_at(source: &str, line: usize, character: usize) -> String {
    let prefix = line_prefix_at(source, line, character);
    prefix
        .chars()
        .rev()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn line_prefix_at(source: &str, line: usize, character: usize) -> String {
    let Some(line_text) = source.lines().nth(line) else {
        return String::new();
    };
    let byte_offset = utf16_character_to_byte_offset(line_text, character);
    line_text[..byte_offset].to_string()
}

fn member_context_at(source: &str, line: usize, character: usize) -> Option<(String, String)> {
    let line_prefix = line_prefix_at(source, line, character);
    let trimmed = line_prefix.trim_end();
    let mut member_prefix = String::new();

    for ch in trimmed.chars().rev() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            member_prefix.push(ch);
        } else {
            break;
        }
    }

    member_prefix = member_prefix.chars().rev().collect();
    let qualifier_end = trimmed.len().saturating_sub(member_prefix.len());
    let before_member = trimmed[..qualifier_end].trim_end();

    if !before_member.ends_with('.') {
        return None;
    }

    let before_dot = before_member[..before_member.len().saturating_sub(1)].trim_end();
    let qualifier = before_dot
        .chars()
        .rev()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();

    if qualifier.is_empty() {
        None
    } else {
        Some((qualifier, member_prefix))
    }
}

fn completion_item(label: &str, kind: u8, detail: &str, insert_text: &str) -> String {
    format!(
        "{{\"label\":\"{}\",\"kind\":{},\"detail\":\"{}\",\"insertText\":\"{}\"}}",
        json_escape(label),
        kind,
        json_escape(detail),
        json_escape(insert_text)
    )
}

fn declaration_detail(decl: &Declaration) -> &'static str {
    match decl {
        Declaration::Permission(_) => "Permission",
        Declaration::Role(_) => "Role",
        Declaration::Policy(_) => "Policy",
        Declaration::Type(_) => "Type",
        Declaration::Enum(_) => "Enum",
        Declaration::Function(_) => "Function",
        Declaration::Workflow(_) => "Workflow",
        Declaration::Action(_) => "Action",
        Declaration::Connector(_) => "Connector",
        Declaration::Service(_) => "Service",
        Declaration::Test(_) => "Test",
        Declaration::Impl(_) => "Impl",
    }
}

fn document_end_position(text: &str) -> (usize, usize) {
    let mut lines = text.lines();
    let Some(first) = lines.next() else {
        return (0, 0);
    };

    let mut line_index = 0usize;
    let mut line_text = first;
    for line in lines {
        line_index += 1;
        line_text = line;
    }

    if text.ends_with('\n') {
        (line_index + 1, 0)
    } else {
        (
            line_index,
            byte_offset_to_utf16_character(line_text, line_text.len()),
        )
    }
}

fn document_symbol(decl: &Declaration) -> String {
    let span = decl.span();
    let start_line = span.line.saturating_sub(1);
    let start_col = span.column.saturating_sub(1);
    let end_col = start_col + decl.name().len();
    let kind = match decl {
        Declaration::Permission(_) => 14,
        Declaration::Role(_) => 5,
        Declaration::Policy(_) => 19,
        Declaration::Type(_) => 23,
        Declaration::Enum(_) => 10,
        Declaration::Function(_) => 12,
        Declaration::Workflow(_) => 12,
        Declaration::Action(_) => 12,
        Declaration::Connector(_) => 2,
        Declaration::Service(_) => 2,
        Declaration::Test(_) => 12,
        Declaration::Impl(_) => 11,
    };

    format!(
        "{{\"name\":\"{}\",\"detail\":\"{}\",\"kind\":{},\"range\":{},\"selectionRange\":{}}}",
        json_escape(decl.name()),
        declaration_detail(decl),
        kind,
        lsp_range(start_line, start_col, start_line, end_col),
        lsp_range(start_line, start_col, start_line, end_col)
    )
}

fn lsp_range(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> String {
    format!(
        "{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}}",
        start_line, start_col, end_line, end_col
    )
}

fn handle_hover(
    uri: &str,
    text: &str,
    line: usize,
    character: usize,
    documents: &HashMap<String, String>,
) -> String {
    let path = uri_to_path(uri);
    let compilation = compile_lsp_program(&path, text, documents);
    let offset = lsp_pos_to_byte_offset(text, line, character);

    // Lex file to find token under cursor
    let lexed = num_compiler::lexer::lex(&path, text);
    let token = match lexed
        .tokens
        .iter()
        .find(|t| offset >= t.span.start && offset <= t.span.end)
    {
        Some(t) => t,
        None => return "null".to_string(),
    };

    if let TokenKind::Ident(_) = &token.kind {
        if let Some(hover_text) = get_hover_info(&compilation.module, token) {
            return format!(
                "{{\"contents\":{{\"kind\":\"markdown\",\"value\":\"{}\"}}}}",
                json_escape(&hover_text)
            );
        }
    }

    "null".to_string()
}

fn handle_completion(
    uri: &str,
    text: &str,
    line: usize,
    character: usize,
    documents: &HashMap<String, String>,
) -> String {
    let path = uri_to_path(uri);
    let compilation = compile_lsp_program(&path, text, documents);
    if let Some((qualifier, member_prefix)) = member_context_at(text, line, character) {
        let mut items = Vec::new();
        if qualifier == "Permission" {
            for decl in &compilation.module.declarations {
                if let Declaration::Permission(_) = decl {
                    if decl.name().starts_with(&member_prefix) {
                        items.push(completion_item(decl.name(), 21, "Permission", decl.name()));
                    }
                }
            }
            items.sort();
            items.dedup();
            return format!("[{}]", items.join(","));
        }
    }

    let prefix = word_prefix_at(text, line, character);
    let mut items = Vec::new();

    for keyword in completions(&prefix) {
        items.push(completion_item(keyword, 14, "Keyword", keyword));
    }

    for symbol in builtins::symbols() {
        if symbol.name.starts_with(&prefix) {
            let kind = match symbol.kind {
                BuiltinKind::Namespace => 9,
                BuiltinKind::Type => 25,
                BuiltinKind::Function => 3,
                BuiltinKind::Currency => 21,
            };
            items.push(completion_item(
                symbol.name,
                kind,
                symbol.signature,
                symbol.name,
            ));
        }
    }

    for decl in &compilation.module.declarations {
        let kind = match decl {
            Declaration::Permission(_) => 21,
            Declaration::Role(_) => 7,
            Declaration::Policy(_) => 6,
            Declaration::Type(_) => 7,
            Declaration::Enum(_) => 13,
            Declaration::Function(_) => 3,
            Declaration::Workflow(_) => 3,
            Declaration::Action(_) => 3,
            Declaration::Connector(_) => 6,
            Declaration::Service(_) => 6,
            Declaration::Test(_) => 3,
            Declaration::Impl(_) => 11,
        };
        let detail = declaration_detail(decl);
        if decl.name().starts_with(&prefix) {
            items.push(completion_item(decl.name(), kind, detail, decl.name()));
        }
    }

    items.sort();
    items.dedup();

    format!("[{}]", items.join(","))
}

fn handle_formatting(uri: &str, text: &str) -> String {
    let path = uri_to_path(uri);
    let compilation = compile(&path, text);
    let formatted = formatter::format_module(&compilation.module);
    let (end_line, end_character) = document_end_position(text);

    format!(
        "[{{\"range\":{{\"start\":{{\"line\":0,\"character\":0}},\"end\":{{\"line\":{},\"character\":{}}}}},\"newText\":\"{}\"}}]",
        end_line,
        end_character,
        json_escape(&formatted)
    )
}

fn handle_document_symbols(uri: &str, text: &str) -> String {
    let path = uri_to_path(uri);
    let compilation = compile(&path, text);
    let mut symbols = Vec::new();

    for decl in &compilation.module.declarations {
        symbols.push(document_symbol(decl));
    }

    format!("[{}]", symbols.join(","))
}

fn handle_definition(
    uri: &str,
    text: &str,
    line: usize,
    character: usize,
    documents: &HashMap<String, String>,
) -> String {
    let path = uri_to_path(uri);
    let compilation = compile_lsp_program(&path, text, documents);
    let offset = lsp_pos_to_byte_offset(text, line, character);

    let lexed = num_compiler::lexer::lex(&path, text);
    let token = match lexed
        .tokens
        .iter()
        .find(|t| offset >= t.span.start && offset <= t.span.end)
    {
        Some(t) => t,
        None => return "null".to_string(),
    };

    if let TokenKind::Ident(_) = &token.kind {
        if let Some(def_span) = get_definition_span(&compilation.module, token) {
            // Span lines/cols are 1-indexed, convert to LSP 0-indexed
            let start_line = def_span.line.saturating_sub(1);
            let start_col = def_span.column.saturating_sub(1);
            // End coordinates: highlight token name length on the same line
            let end_line = start_line;
            let end_col = start_col + token.lexeme.len();

            let target_uri = if def_span.source.starts_with("file://") {
                def_span.source.clone()
            } else {
                format!("file://{}", def_span.source)
            };

            return format!(
                "{{\"uri\":\"{}\",\"range\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}}}}",
                target_uri, start_line, start_col, end_line, end_col
            );
        }
    }

    "null".to_string()
}

fn handle_rename(
    uri: &str,
    text: &str,
    line: usize,
    character: usize,
    new_name: &str,
    documents: &HashMap<String, String>,
) -> String {
    if !is_valid_module_path(new_name) {
        return "null".to_string();
    }

    let path = uri_to_path(uri);
    let offset = lsp_pos_to_byte_offset(text, line, character);
    let Some((old_name, _range)) = module_declaration_at_offset(text, offset) else {
        return "null".to_string();
    };
    if old_name == new_name {
        return "{\"changes\":{}}".to_string();
    }

    let files = lsp_program_files(&path, text, documents);
    if module_path_exists(&files, new_name) {
        return "null".to_string();
    }

    let mut changes = Vec::new();
    for file in files {
        let mut edits = Vec::new();
        for range in module_reference_ranges(&file.source, &old_name) {
            edits.push(format!(
                "{{\"range\":{},\"newText\":\"{}\"}}",
                range.range.to_lsp_json(),
                json_escape(new_name)
            ));
        }
        if !edits.is_empty() {
            changes.push(format!(
                "\"{}\": [{}]",
                json_escape(&path_to_uri(&file.name)),
                edits.join(",")
            ));
        }
    }

    changes.sort();
    format!("{{\"changes\":{{{}}}}}", changes.join(","))
}

fn module_path_exists(files: &[SourceFile], module_name: &str) -> bool {
    files
        .iter()
        .any(|file| compile(&file.name, &file.source).module.name.as_deref() == Some(module_name))
}

fn module_declaration_at_offset(source: &str, offset: usize) -> Option<(String, TextRange)> {
    module_reference_ranges(source, "")
        .into_iter()
        .find_map(|range| {
            if range.kind == ModuleReferenceKind::Declaration
                && offset >= range.start_offset
                && offset <= range.end_offset
            {
                Some((range.text.clone(), range.as_text_range()))
            } else {
                None
            }
        })
}

fn module_reference_ranges(source: &str, module_name: &str) -> Vec<ModuleReferenceRange> {
    let mut ranges = Vec::new();
    let mut line_start = 0usize;
    for (line_index, line_text) in source.lines().enumerate() {
        if let Some(range) = module_path_range_in_line(
            line_text,
            line_start,
            line_index,
            "module",
            module_name,
            ModuleReferenceKind::Declaration,
        ) {
            ranges.push(range);
        }
        if let Some(range) = module_path_range_in_line(
            line_text,
            line_start,
            line_index,
            "use",
            module_name,
            ModuleReferenceKind::Import,
        ) {
            ranges.push(range);
        }
        line_start += line_text.len() + 1;
    }
    ranges
}

fn module_path_range_in_line(
    line_text: &str,
    line_start: usize,
    line_index: usize,
    keyword: &str,
    expected_module: &str,
    kind: ModuleReferenceKind,
) -> Option<ModuleReferenceRange> {
    let trimmed = line_text.trim_start();
    let leading = line_text.len() - trimmed.len();
    let rest = trimmed.strip_prefix(keyword)?;
    if !rest.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }
    let module_text = rest.trim_start();
    let whitespace_after_keyword = rest.len() - module_text.len();
    let module_len = module_text
        .find(char::is_whitespace)
        .unwrap_or(module_text.len());
    if module_len == 0 {
        return None;
    }
    let module_text = &module_text[..module_len];
    if !expected_module.is_empty() && module_text != expected_module {
        return None;
    }
    let start_col = leading + keyword.len() + whitespace_after_keyword;
    let end_col = start_col + module_text.len();
    Some(ModuleReferenceRange {
        kind,
        text: module_text.to_string(),
        start_offset: line_start + start_col,
        end_offset: line_start + end_col,
        range: TextRange {
            start_line: line_index,
            start_character: byte_offset_to_utf16_character(line_text, start_col),
            end_line: line_index,
            end_character: byte_offset_to_utf16_character(line_text, end_col),
        },
    })
}

fn is_valid_module_path(value: &str) -> bool {
    let mut parts = value.split('.');
    parts.clone().next().is_some()
        && parts.all(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
                && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
}

fn path_to_uri(path: &str) -> String {
    if path.starts_with("file://") {
        path.to_string()
    } else {
        format!("file://{path}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleReferenceKind {
    Declaration,
    Import,
}

#[derive(Debug, Clone)]
struct ModuleReferenceRange {
    kind: ModuleReferenceKind,
    text: String,
    start_offset: usize,
    end_offset: usize,
    range: TextRange,
}

impl ModuleReferenceRange {
    fn as_text_range(&self) -> TextRange {
        self.range.clone()
    }
}

#[derive(Debug, Clone)]
struct TextRange {
    start_line: usize,
    start_character: usize,
    end_line: usize,
    end_character: usize,
}

impl TextRange {
    fn to_lsp_json(&self) -> String {
        format!(
            "{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}}",
            self.start_line, self.start_character, self.end_line, self.end_character
        )
    }
}

fn format_privacy(privacy: Option<num_compiler::ast::Privacy>) -> &'static str {
    match privacy {
        Some(num_compiler::ast::Privacy::Public) => "Public",
        Some(num_compiler::ast::Privacy::Internal) => "Internal",
        Some(num_compiler::ast::Privacy::Private) => "Private",
        Some(num_compiler::ast::Privacy::Sensitive) => "Sensitive",
        Some(num_compiler::ast::Privacy::Secret) => "Secret",
        Some(num_compiler::ast::Privacy::Regulated) => "Regulated",
        None => "none",
    }
}

fn format_trust(trust: Option<num_compiler::ast::Trust>) -> &'static str {
    match trust {
        Some(num_compiler::ast::Trust::Untrusted) => "Untrusted",
        Some(num_compiler::ast::Trust::Trusted) => "Trusted",
        Some(num_compiler::ast::Trust::Verified) => "Verified",
        None => "none",
    }
}

fn format_risk(risk: num_compiler::ast::Risk) -> &'static str {
    match risk {
        num_compiler::ast::Risk::Low => "Low",
        num_compiler::ast::Risk::Medium => "Medium",
        num_compiler::ast::Risk::High => "High",
        num_compiler::ast::Risk::Critical => "Critical",
    }
}

fn format_source(source: Option<&str>) -> String {
    match source {
        Some("UserInput") => "UserInput".to_string(),
        Some("Database") => "Database".to_string(),
        Some("AI") => "AI".to_string(),
        Some(other) => other.to_string(),
        None => "none".to_string(),
    }
}

fn get_hover_info(module: &Module, token: &Token) -> Option<String> {
    let name = &token.lexeme;

    if let Some(symbol) = builtins::symbol(name) {
        return Some(format!(
            "```num\n{}\n```\n\n---\n**Built-in {}**\n{}",
            symbol.signature,
            builtin_kind_label(symbol.kind),
            symbol.documentation
        ));
    }

    // Check top-level declarations
    for decl in &module.declarations {
        if decl.name() == name {
            match decl {
                Declaration::Permission(_) => {
                    return Some(format!(
                        "```num\npermission {}\n```\n\n---\n**Permission Declaration**\nAllows guarding operations using roles.",
                        name
                    ));
                }
                Declaration::Role(role) => {
                    let mut allows_list = String::new();
                    for allow in &role.allows {
                        allows_list.push_str(&format!("- `{allow}`\n"));
                    }
                    return Some(format!(
                        "```num\nrole {} {{\n    // allows permissions\n}}\n```\n\n---\n**Role Declaration**\nGroups permission grants.\n\n**Allowed Permissions:**\n{}",
                        name, allows_list
                    ));
                }
                Declaration::Type(ty) => {
                    let generic_params = format_generic_params(&ty.generic_params);
                    return match &ty.body {
                        TypeBody::Struct(fields) => {
                            let mut fields_str = String::new();
                            for field in fields {
                                fields_str
                                    .push_str(&format!("    {}: {}\n", field.name, field.ty.raw));
                            }
                            Some(format!(
                                "```num\ntype {}{} {{\n{}}}\n```\n\n---\n**Type Declaration**\nStructured data schema definition.",
                                name, generic_params, fields_str
                            ))
                        }
                        TypeBody::Alias(alias) => Some(format!(
                            "```num\ntype {}{} = {}\n```\n\n---\n**Type Alias**\nNominal alias or branded type declaration.",
                            name, generic_params, alias.raw
                        )),
                    };
                }
                Declaration::Enum(en) => {
                    let mut variants_str = String::new();
                    for variant in &en.variants {
                        if let Some(payload) = &variant.payload {
                            variants_str
                                .push_str(&format!("- `{}({})`\n", variant.name, payload.raw));
                        } else {
                            variants_str.push_str(&format!("- `{}`\n", variant.name));
                        }
                    }
                    return Some(format!(
                        "```num\nenum {} {{\n    // variants\n}}\n```\n\n---\n**Enum Declaration**\nDiscrete list of states.\n\n**Variants:**\n{}",
                        name, variants_str
                    ));
                }
                Declaration::Action(action) => {
                    let params: Vec<String> = action
                        .params
                        .iter()
                        .map(|p| format!("{}: {}", p.name, p.ty.raw))
                        .collect();
                    let params_str = params.join(", ");
                    let requires = action.requires.join(", ");
                    let rollback_str = action.rollback.as_deref().unwrap_or("none");
                    let timeout_str = action.timeout.as_deref().unwrap_or("none");
                    let cost_str = action.cost.as_deref().unwrap_or("none");

                    return Some(format!(
                        "```num\naction {}({})\n```\n\n---\n**Action Declaration**\nDurable side-effect execution block.\n\n- **Requires:** `{}`\n- **Risk Level:** `{}`\n- **Compensating Action:** `{}`\n- **Timeout:** `{}`\n- **Budget / Cost:** `{}`",
                        name, params_str, requires, format_risk(action.risk), rollback_str, timeout_str, cost_str
                    ));
                }
                Declaration::Workflow(workflow) => {
                    let params: Vec<String> = workflow
                        .params
                        .iter()
                        .map(|p| format!("{}: {}", p.name, p.ty.raw))
                        .collect();
                    let params_str = params.join(", ");
                    return Some(format!(
                        "```num\nworkflow {}({})\n```\n\n---\n**Workflow Declaration**\nDurable workflow orchestration flow.",
                        name, params_str
                    ));
                }
                Declaration::Function(function) => {
                    let params: Vec<String> = function
                        .params
                        .iter()
                        .map(|p| format!("{}: {}", p.name, p.ty.raw))
                        .collect();
                    let params_str = params.join(", ");
                    return Some(format!(
                        "```num\nfn {}({})\n```\n\n---\n**Function Declaration**\nStateless helper logic.",
                        name, params_str
                    ));
                }
                Declaration::Service(service) => {
                    let routes = service
                        .routes
                        .iter()
                        .map(|route| {
                            let input = route
                                .input
                                .as_ref()
                                .map(|input| format!(" input {}: {}", input.name, input.ty.raw))
                                .unwrap_or_default();
                            format!("- `{}` `{}`{}\n", route.method, route.path, input)
                        })
                        .collect::<String>();
                    return Some(format!(
                        "```num\nservice {} {{\n    // routes\n}}\n```\n\n---\n**Service Declaration**\nHTTP-facing API boundary.\n\n**Routes:**\n{}",
                        name, routes
                    ));
                }
                Declaration::Test(test) => {
                    return Some(format!(
                        "```num\ntest {}\"{}\" {{\n    // assertions\n}}\n```\n\n---\n**Test Declaration**\nExecutable `.num` test block checked by the compiler and run with `num test`.",
                        format_test_kind_prefix(test.kind),
                        test.name
                    ));
                }
                _ => {}
            }
        }
    }

    // Check local scope (parameters or variables inside workflow/action/fn)
    for decl in &module.declarations {
        let decl_span = decl.span();
        if token.span.start >= decl_span.start && token.span.end <= decl_span.end {
            match decl {
                Declaration::Impl(imp) => {
                    for method in &imp.methods {
                        let method_span = &method.span;
                        if token.span.start >= method_span.start
                            && token.span.end <= method_span.end
                        {
                            if let Some(param) = method.params.iter().find(|p| p.name == *name) {
                                return Some(format!(
                                    "```num\nlet {}: {}\n```\n\n---\n**Method Parameter**\n\n- **Source:** `{}`\n- **Privacy:** `{}`\n- **Trust:** `{}`",
                                    name, param.ty.raw, format_source(param.labels.source.as_deref()), format_privacy(param.labels.privacy), format_trust(param.labels.trust)
                                ));
                            }
                            if let Some(let_stmt) =
                                find_let_stmt_in_body(&method.body, name, token.span.start)
                            {
                                let ty_str = let_stmt
                                    .ty
                                    .as_ref()
                                    .map(|t| t.raw.as_str())
                                    .unwrap_or("unknown");
                                return Some(format!(
                                    "```num\nlet {}: {}\n```\n\n---\n**Local Variable**\n\n- **Source:** `{}`\n- **Privacy:** `{}`\n- **Trust:** `{}`",
                                    name, ty_str, format_source(let_stmt.labels.source.as_deref()), format_privacy(let_stmt.labels.privacy), format_trust(let_stmt.labels.trust)
                                ));
                            }
                        }
                    }
                }
                Declaration::Workflow(workflow) => {
                    if let Some(param) = workflow.params.iter().find(|p| p.name == *name) {
                        return Some(format!(
                            "```num\nlet {}: {}\n```\n\n---\n**Workflow Parameter**\n\n- **Source:** `{}`\n- **Privacy:** `{}`\n- **Trust:** `{}`",
                            name, param.ty.raw, format_source(param.labels.source.as_deref()), format_privacy(param.labels.privacy), format_trust(param.labels.trust)
                        ));
                    }
                    if let Some(let_stmt) =
                        find_let_stmt_in_body(&workflow.body, name, token.span.start)
                    {
                        let ty_str = let_stmt
                            .ty
                            .as_ref()
                            .map(|t| t.raw.as_str())
                            .unwrap_or("unknown");
                        return Some(format!(
                            "```num\nlet {}: {}\n```\n\n---\n**Local Variable**\n\n- **Source:** `{}`\n- **Privacy:** `{}`\n- **Trust:** `{}`",
                            name, ty_str, format_source(let_stmt.labels.source.as_deref()), format_privacy(let_stmt.labels.privacy), format_trust(let_stmt.labels.trust)
                        ));
                    }
                }
                Declaration::Action(action) => {
                    if let Some(param) = action.params.iter().find(|p| p.name == *name) {
                        return Some(format!(
                            "```num\nlet {}: {}\n```\n\n---\n**Action Parameter**\n\n- **Source:** `{}`\n- **Privacy:** `{}`\n- **Trust:** `{}`",
                            name, param.ty.raw, format_source(param.labels.source.as_deref()), format_privacy(param.labels.privacy), format_trust(param.labels.trust)
                        ));
                    }
                    if let Some(let_stmt) =
                        find_let_stmt_in_body(&action.body, name, token.span.start)
                    {
                        let ty_str = let_stmt
                            .ty
                            .as_ref()
                            .map(|t| t.raw.as_str())
                            .unwrap_or("unknown");
                        return Some(format!(
                            "```num\nlet {}: {}\n```\n\n---\n**Local Variable**\n\n- **Source:** `{}`\n- **Privacy:** `{}`\n- **Trust:** `{}`",
                            name, ty_str, format_source(let_stmt.labels.source.as_deref()), format_privacy(let_stmt.labels.privacy), format_trust(let_stmt.labels.trust)
                        ));
                    }
                }
                Declaration::Function(function) => {
                    if let Some(param) = function.params.iter().find(|p| p.name == *name) {
                        return Some(format!(
                            "```num\nlet {}: {}\n```\n\n---\n**Function Parameter**\n\n- **Source:** `{}`\n- **Privacy:** `{}`\n- **Trust:** `{}`",
                            name, param.ty.raw, format_source(param.labels.source.as_deref()), format_privacy(param.labels.privacy), format_trust(param.labels.trust)
                        ));
                    }
                    if let Some(let_stmt) =
                        find_let_stmt_in_body(&function.body, name, token.span.start)
                    {
                        let ty_str = let_stmt
                            .ty
                            .as_ref()
                            .map(|t| t.raw.as_str())
                            .unwrap_or("unknown");
                        return Some(format!(
                            "```num\nlet {}: {}\n```\n\n---\n**Local Variable**\n\n- **Source:** `{}`\n- **Privacy:** `{}`\n- **Trust:** `{}`",
                            name, ty_str, format_source(let_stmt.labels.source.as_deref()), format_privacy(let_stmt.labels.privacy), format_trust(let_stmt.labels.trust)
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    None
}

fn format_test_kind_prefix(kind: TestKind) -> &'static str {
    match kind {
        TestKind::Unit => "",
        TestKind::Policy => "policy ",
        TestKind::Workflow => "workflow ",
        TestKind::Ai => "ai ",
    }
}

fn format_generic_params(params: &[String]) -> String {
    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

fn builtin_kind_label(kind: BuiltinKind) -> &'static str {
    match kind {
        BuiltinKind::Namespace => "Namespace",
        BuiltinKind::Type => "Type",
        BuiltinKind::Function => "Function",
        BuiltinKind::Currency => "Currency",
    }
}

fn get_definition_span(module: &Module, token: &Token) -> Option<Span> {
    let name = &token.lexeme;

    // Check top-level declarations
    for decl in &module.declarations {
        if decl.name() == name {
            return Some(decl.span().clone());
        }
    }

    // Check local scope (parameters or variables inside workflow/action/fn)
    for decl in &module.declarations {
        let decl_span = decl.span();
        if token.span.start >= decl_span.start && token.span.end <= decl_span.end {
            match decl {
                Declaration::Impl(imp) => {
                    for method in &imp.methods {
                        if token.span.start >= method.span.start
                            && token.span.end <= method.span.end
                        {
                            if let Some(param) = method.params.iter().find(|p| p.name == *name) {
                                return Some(param.span.clone());
                            }
                            if let Some(let_stmt) =
                                find_let_stmt_in_body(&method.body, name, token.span.start)
                            {
                                return Some(let_stmt.span.clone());
                            }
                        }
                    }
                }
                Declaration::Workflow(workflow) => {
                    if let Some(param) = workflow.params.iter().find(|p| p.name == *name) {
                        return Some(param.span.clone());
                    }
                    if let Some(let_stmt) =
                        find_let_stmt_in_body(&workflow.body, name, token.span.start)
                    {
                        return Some(let_stmt.span.clone());
                    }
                }
                Declaration::Action(action) => {
                    if let Some(param) = action.params.iter().find(|p| p.name == *name) {
                        return Some(param.span.clone());
                    }
                    if let Some(let_stmt) =
                        find_let_stmt_in_body(&action.body, name, token.span.start)
                    {
                        return Some(let_stmt.span.clone());
                    }
                }
                Declaration::Function(function) => {
                    if let Some(param) = function.params.iter().find(|p| p.name == *name) {
                        return Some(param.span.clone());
                    }
                    if let Some(let_stmt) =
                        find_let_stmt_in_body(&function.body, name, token.span.start)
                    {
                        return Some(let_stmt.span.clone());
                    }
                }
                _ => {}
            }
        }
    }

    None
}

fn find_let_stmt_in_body<'b>(
    body: &'b [Stmt],
    name: &str,
    target_offset: usize,
) -> Option<&'b LetStmt> {
    for stmt in body {
        match stmt {
            Stmt::Let(let_stmt) => {
                if let_stmt.name == name && let_stmt.span.start < target_offset {
                    return Some(let_stmt);
                }
            }
            Stmt::Transaction(tx) => {
                if let Some(found) = find_let_stmt_in_body(&tx.body, name, target_offset) {
                    return Some(found);
                }
            }
            Stmt::If(if_stmt) => {
                if let Some(found) = find_let_stmt_in_body(&if_stmt.then_body, name, target_offset)
                {
                    return Some(found);
                }
                if let Some(found) = find_let_stmt_in_body(&if_stmt.else_body, name, target_offset)
                {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_lsp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = env::temp_dir().join(format!("num_lsp_{name}_{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn completion_handles_utf16_positions() {
        let source = r#"
module tests.completion

permission IssueRefund

workflow main() {
    requires Permissionо
}
"#;

        let result = handle_completion("file:///test.num", source, 6, 24, &HashMap::new());

        assert!(result.contains("IssueRefund"));
    }

    fn imported_module_fixture(name: &str) -> (PathBuf, PathBuf, String) {
        let dir = temp_lsp_dir(name);
        let domain_path = dir.join("domain.num");
        let main_path = dir.join("main.num");
        fs::write(
            &domain_path,
            r#"
module app.domain

permission IssueRefund

type RefundRequest {
    reason: Text
}
"#,
        )
        .unwrap();
        let main_source = r#"
module app.main
use app.domain

workflow main(request: RefundRequest) {
    require Permission.Issue
    audit(request.reason)
}
"#
        .to_string();
        fs::write(&main_path, &main_source).unwrap();
        (dir, main_path, main_source)
    }

    #[test]
    fn completion_includes_imported_declarations() {
        let (dir, main_path, main_source) = imported_module_fixture("completion_imports");

        let result = handle_completion(
            &format!("file://{}", main_path.display()),
            &main_source,
            4,
            31,
            &HashMap::new(),
        );

        assert!(result.contains("RefundRequest"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn permission_completion_includes_imported_permissions() {
        let (dir, main_path, main_source) = imported_module_fixture("permission_imports");

        let result = handle_completion(
            &format!("file://{}", main_path.display()),
            &main_source,
            5,
            28,
            &HashMap::new(),
        );

        assert!(result.contains("IssueRefund"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hover_resolves_imported_declarations() {
        let (dir, main_path, main_source) = imported_module_fixture("hover_imports");

        let result = handle_hover(
            &format!("file://{}", main_path.display()),
            &main_source,
            4,
            24,
            &HashMap::new(),
        );

        assert!(result.contains("type RefundRequest"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn definition_resolves_imported_declarations() {
        let (dir, main_path, main_source) = imported_module_fixture("definition_imports");

        let result = handle_definition(
            &format!("file://{}", main_path.display()),
            &main_source,
            4,
            24,
            &HashMap::new(),
        );

        assert!(result.contains("domain.num"));
        assert!(result.contains("\"line\":5"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rename_module_updates_declaration_and_imports_across_workspace() {
        let dir = temp_lsp_dir("rename_module");
        let domain_path = dir.join("domain.num");
        let main_path = dir.join("main.num");
        let domain_source = r#"
module app.domain

type RefundRequest {
    reason: Text
}
"#;
        let main_source = r#"
module app.main
use app.domain

workflow main(request: RefundRequest) {
    audit(request.reason)
}
"#;
        fs::write(&domain_path, domain_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let result = handle_rename(
            &format!("file://{}", domain_path.display()),
            domain_source,
            1,
            12,
            "app.billing",
            &HashMap::new(),
        );

        assert!(result.contains(&format!("file://{}", domain_path.display())));
        assert!(result.contains(&format!("file://{}", main_path.display())));
        assert_eq!(result.matches("\"newText\":\"app.billing\"").count(), 2);
        assert!(result.contains("\"line\":1"));
        assert!(result.contains("\"line\":2"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rename_module_rejects_existing_target_module() {
        let dir = temp_lsp_dir("rename_conflict");
        let domain_path = dir.join("domain.num");
        let billing_path = dir.join("billing.num");
        let domain_source = r#"
module app.domain

type RefundRequest {
    reason: Text
}
"#;
        fs::write(&domain_path, domain_source).unwrap();
        fs::write(
            &billing_path,
            r#"
module app.billing

type BillingRecord {
    id: Text
}
"#,
        )
        .unwrap();

        let result = handle_rename(
            &format!("file://{}", domain_path.display()),
            domain_source,
            1,
            12,
            "app.billing",
            &HashMap::new(),
        );

        assert_eq!(result, "null");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn diagnostics_resolve_imported_sibling_modules() {
        let dir = temp_lsp_dir("imports");
        let domain_path = dir.join("domain.num");
        let main_path = dir.join("main.num");
        fs::write(
            &domain_path,
            r#"
module app.domain

type RefundRequest {
    reason: Text
}
"#,
        )
        .unwrap();
        let main_source = r#"
module app.main
use app.domain

workflow main(request: RefundRequest) {
    audit(request.reason)
}
"#;
        fs::write(&main_path, main_source).unwrap();

        let diagnostics = program_diagnostics(
            &main_path.display().to_string(),
            main_source,
            &HashMap::new(),
        );

        assert!(diagnostics.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn diagnostics_use_open_sibling_buffers_before_disk_contents() {
        let dir = temp_lsp_dir("open_buffers");
        let domain_path = dir.join("domain.num");
        let main_path = dir.join("main.num");
        fs::write(
            &domain_path,
            r#"
module app.domain

type OldRequest {
    reason: Text
}
"#,
        )
        .unwrap();
        let main_source = r#"
module app.main
use app.domain

workflow main(request: RefundRequest) {
    audit(request.reason)
}
"#;
        fs::write(&main_path, main_source).unwrap();

        let mut open_documents = HashMap::new();
        open_documents.insert(
            format!("file://{}", domain_path.display()),
            r#"
module app.domain

type RefundRequest {
    reason: Text
}
"#
            .to_string(),
        );

        let diagnostics = program_diagnostics(
            &main_path.display().to_string(),
            main_source,
            &open_documents,
        );

        assert!(diagnostics.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
