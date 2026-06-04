use std::fs;
use std::path::Path;

pub fn import_sql_schema(path: &Path, module_name: Option<&str>) -> Result<String, String> {
    let source = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok(render_sql_schema(&source, module_name))
}

pub fn render_sql_schema(source: &str, module_name: Option<&str>) -> String {
    let module_name = module_name.unwrap_or("generated.database");
    let tables = parse_tables(source);
    let mut out = String::new();

    out.push_str("module ");
    out.push_str(module_name);
    out.push_str("\n\n");

    for table in &tables {
        render_table_type(&mut out, table);
        out.push('\n');
    }

    out.push_str("connector database {\n");
    for table in &tables {
        let table_ident = to_identifier(&table.name);
        let table_type = to_type_name(&table.name);
        out.push_str("    list_");
        out.push_str(&table_ident);
        out.push_str("() -> List<");
        out.push_str(&table_type);
        out.push_str(">\n");

        if let Some(primary_key) = table.primary_key_column() {
            out.push_str("    find_");
            out.push_str(&table_ident);
            out.push_str("_by_");
            out.push_str(&to_identifier(&primary_key.name));
            out.push('(');
            out.push_str(&to_identifier(&primary_key.name));
            out.push_str(": ");
            out.push_str(&primary_key.ty);
            out.push_str(") -> Option<");
            out.push_str(&table_type);
            out.push_str(">\n");
        }

        out.push_str("    insert_");
        out.push_str(&table_ident);
        out.push_str("(row: ");
        out.push_str(&table_type);
        out.push_str(") -> ");
        out.push_str(&table_type);
        out.push('\n');
    }
    out.push_str("}\n");

    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Table {
    name: String,
    columns: Vec<Column>,
}

impl Table {
    fn primary_key_column(&self) -> Option<&Column> {
        let mut primary_keys = self.columns.iter().filter(|column| column.primary_key);
        let primary_key = primary_keys.next()?;
        primary_keys.next().is_none().then_some(primary_key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Column {
    name: String,
    ty: String,
    nullable: bool,
    primary_key: bool,
}

fn parse_tables(source: &str) -> Vec<Table> {
    let mut tables = Vec::new();
    let mut rest = strip_sql_comments(source);

    while let Some(start) = find_create_table(&rest) {
        rest = rest[start + "create table".len()..]
            .trim_start()
            .to_string();
        if rest.to_ascii_lowercase().starts_with("if not exists") {
            rest = rest["if not exists".len()..].trim_start().to_string();
        }

        let Some((name, after_name)) = parse_table_name(&rest) else {
            break;
        };
        let Some(open) = after_name.find('(') else {
            break;
        };
        let after_open = &after_name[open + 1..];
        let Some(close) = find_matching_paren(after_open) else {
            break;
        };
        let body = &after_open[..close];
        tables.push(Table {
            name,
            columns: parse_columns(body),
        });
        rest = after_open[close + 1..].to_string();
    }

    tables.sort_by(|left, right| left.name.cmp(&right.name));
    tables
}

fn parse_columns(body: &str) -> Vec<Column> {
    let items = split_sql_items(body);
    let primary_key_names = table_primary_key_names(&items);
    items
        .into_iter()
        .filter_map(|item| parse_column(&item))
        .map(|mut column| {
            if primary_key_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&column.name))
            {
                column.primary_key = true;
                column.nullable = false;
                column.ty = unwrap_option_type(&column.ty).to_string();
            }
            column
        })
        .collect()
}

fn parse_column(item: &str) -> Option<Column> {
    let item = item.trim();
    if item.is_empty() || is_table_constraint(item) {
        return None;
    }

    let mut parts = item.split_whitespace();
    let name = clean_sql_ident(parts.next()?)?;
    let raw_ty = parts.next()?;
    let lower = item.to_ascii_lowercase();
    let ty = sql_type(raw_ty);
    let primary_key = lower.contains("primary key");
    let nullable = !primary_key && !lower.contains("not null");

    Some(Column {
        name,
        ty: if nullable {
            format!("Option<{ty}>")
        } else {
            ty
        },
        nullable,
        primary_key,
    })
}

fn table_primary_key_names(items: &[String]) -> Vec<String> {
    items
        .iter()
        .filter(|item| is_table_constraint(item))
        .filter_map(|item| primary_key_columns_from_constraint(item))
        .next()
        .unwrap_or_default()
}

fn primary_key_columns_from_constraint(item: &str) -> Option<Vec<String>> {
    let lower = item.to_ascii_lowercase();
    let primary_key_start = lower.find("primary key")?;
    let after_primary_key = &item[primary_key_start + "primary key".len()..];
    let open = after_primary_key.find('(')?;
    let after_open = &after_primary_key[open + 1..];
    let close = find_matching_paren(after_open)?;
    let columns = split_sql_items(&after_open[..close])
        .into_iter()
        .filter_map(|column| clean_sql_ident(&column))
        .collect::<Vec<_>>();
    (!columns.is_empty()).then_some(columns)
}

fn unwrap_option_type(ty: &str) -> &str {
    ty.strip_prefix("Option<")
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(ty)
}

fn is_table_constraint(item: &str) -> bool {
    let lower = item.trim_start().to_ascii_lowercase();
    lower.starts_with("primary key")
        || lower.starts_with("foreign key")
        || lower.starts_with("unique")
        || lower.starts_with("constraint")
        || lower.starts_with("check")
}

fn sql_type(raw: &str) -> String {
    let base = raw
        .split_once('(')
        .map(|(base, _)| base)
        .unwrap_or(raw)
        .trim_matches('"')
        .trim_matches('`')
        .trim_matches('[')
        .trim_matches(']')
        .to_ascii_lowercase();

    match base.as_str() {
        "uuid" => "Uuid".to_string(),
        "bool" | "boolean" => "Bool".to_string(),
        "int" | "integer" | "smallint" | "bigint" | "serial" | "bigserial" => "Int".to_string(),
        "real" | "double" | "float" | "float4" | "float8" => "Float".to_string(),
        "decimal" | "numeric" => "Decimal".to_string(),
        "date" => "Date".to_string(),
        "timestamp" | "timestamptz" | "datetime" => "DateTime".to_string(),
        "json" | "jsonb" => "Json".to_string(),
        "bytea" | "blob" | "binary" | "varbinary" => "Bytes".to_string(),
        _ => "Text".to_string(),
    }
}

fn find_create_table(source: &str) -> Option<usize> {
    source.to_ascii_lowercase().find("create table")
}

fn parse_table_name(source: &str) -> Option<(String, &str)> {
    let source = source.trim_start();
    let mut end = 0usize;
    for (index, ch) in source.char_indices() {
        if ch.is_whitespace() || ch == '(' {
            break;
        }
        end = index + ch.len_utf8();
    }
    if end == 0 {
        return None;
    }
    let name = source[..end].rsplit('.').next().and_then(clean_sql_ident)?;
    Some((name, &source[end..]))
}

fn clean_sql_ident(value: &str) -> Option<String> {
    let value = value
        .trim()
        .trim_matches('"')
        .trim_matches('`')
        .trim_matches('[')
        .trim_matches(']');
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn find_matching_paren(source: &str) -> Option<usize> {
    let mut depth = 1usize;
    let mut in_string = false;
    let mut quote = '\0';
    for (index, ch) in source.char_indices() {
        if in_string {
            if ch == quote {
                in_string = false;
            }
            continue;
        }
        if matches!(ch, '\'' | '"') {
            in_string = true;
            quote = ch;
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_sql_items(body: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut quote = '\0';

    for ch in body.chars() {
        if in_string {
            current.push(ch);
            if ch == quote {
                in_string = false;
            }
            continue;
        }
        if matches!(ch, '\'' | '"') {
            in_string = true;
            quote = ch;
            current.push(ch);
            continue;
        }
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                let item = current.trim();
                if !item.is_empty() {
                    items.push(item.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let item = current.trim();
    if !item.is_empty() {
        items.push(item.to_string());
    }
    items
}

fn strip_sql_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split("--").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn to_type_name(value: &str) -> String {
    let mut output = String::new();
    for part in identifier_parts(value) {
        output.push_str(&capitalize_identifier_part(&part));
    }
    if output.is_empty() {
        "GeneratedTable".to_string()
    } else {
        output
    }
}

fn to_identifier(value: &str) -> String {
    let mut parts = identifier_parts(value).into_iter();
    let mut output = parts
        .next()
        .map(|part| lower_first(&part))
        .unwrap_or_else(|| "value".to_string());
    for part in parts {
        output.push_str(&capitalize_identifier_part(&part));
    }
    if output.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        output.insert(0, '_');
    }
    output
}

fn identifier_parts(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .flat_map(|part| part.split('_'))
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn capitalize_identifier_part(part: &str) -> String {
    let mut chars = part.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut output = String::new();
    output.extend(first.to_uppercase());
    output.push_str(chars.as_str());
    output
}

fn lower_first(part: &str) -> String {
    let mut chars = part.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut output = String::new();
    output.extend(first.to_lowercase());
    output.push_str(chars.as_str());
    output
}

fn render_table_type(out: &mut String, table: &Table) {
    out.push_str("type ");
    out.push_str(&to_type_name(&table.name));
    out.push_str(" {\n");
    for column in &table.columns {
        out.push_str("    ");
        out.push_str(&to_identifier(&column.name));
        out.push_str(": ");
        out.push_str(&column.ty);
        out.push('\n');
    }
    out.push_str("}\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_tables_and_database_connector() {
        let source = r#"
CREATE TABLE refunds (
    id UUID PRIMARY KEY,
    payment_id VARCHAR(64) NOT NULL,
    amount NUMERIC(12,2) NOT NULL,
    approved BOOLEAN NOT NULL,
    note TEXT
);
"#;

        let rendered = render_sql_schema(source, Some("generated.db"));

        assert!(rendered.contains("module generated.db"));
        assert!(rendered.contains("type Refunds"));
        assert!(rendered.contains("id: Uuid"));
        assert!(rendered.contains("paymentId: Text"));
        assert!(rendered.contains("note: Option<Text>"));
        assert!(rendered.contains("connector database"));
        assert!(rendered.contains("list_refunds() -> List<Refunds>"));
        assert!(rendered.contains("find_refunds_by_id(id: Uuid) -> Option<Refunds>"));
        assert!(rendered.contains("insert_refunds(row: Refunds) -> Refunds"));
        assert!(num_compiler::check("generated_sql.num", &rendered).is_empty());
    }

    #[test]
    fn renders_table_level_primary_key_finder() {
        let source = r#"
CREATE TABLE users (
    id UUID,
    email TEXT NOT NULL,
    PRIMARY KEY (id)
);
"#;

        let rendered = render_sql_schema(source, Some("generated.db"));

        assert!(rendered.contains("id: Uuid"));
        assert!(rendered.contains("email: Text"));
        assert!(rendered.contains("find_users_by_id(id: Uuid) -> Option<Users>"));
        assert!(num_compiler::check("generated_sql.num", &rendered).is_empty());
    }

    #[test]
    fn renders_named_table_level_primary_key_constraint() {
        let source = r#"
CREATE TABLE users (
    id UUID,
    email TEXT NOT NULL,
    CONSTRAINT users_pkey PRIMARY KEY (id)
);
"#;

        let rendered = render_sql_schema(source, Some("generated.db"));

        assert!(rendered.contains("id: Uuid"));
        assert!(rendered.contains("find_users_by_id(id: Uuid) -> Option<Users>"));
        assert!(num_compiler::check("generated_sql.num", &rendered).is_empty());
    }

    #[test]
    fn composite_table_level_primary_key_does_not_generate_single_key_finder() {
        let source = r#"
CREATE TABLE ledger_entries (
    account_id UUID,
    sequence_no INTEGER,
    amount NUMERIC(12,2) NOT NULL,
    PRIMARY KEY (account_id, sequence_no)
);
"#;

        let rendered = render_sql_schema(source, Some("generated.db"));

        assert!(rendered.contains("accountId: Uuid"));
        assert!(rendered.contains("sequenceNo: Int"));
        assert!(!rendered.contains("find_ledgerEntries_by_"));
        assert!(num_compiler::check("generated_sql.num", &rendered).is_empty());
    }
}
