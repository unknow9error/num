use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub fn import_sql_schema(path: &Path, module_name: Option<&str>) -> Result<String, String> {
    let source = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok(render_sql_schema(&source, module_name))
}

pub fn sql_migration_plan_from_files(
    old_path: &Path,
    new_path: &Path,
) -> Result<SqlMigrationPlan, String> {
    let old_source = fs::read_to_string(old_path)
        .map_err(|err| format!("failed to read {}: {err}", old_path.display()))?;
    let new_source = fs::read_to_string(new_path)
        .map_err(|err| format!("failed to read {}: {err}", new_path.display()))?;
    Ok(plan_sql_migration(&old_source, &new_source))
}

pub fn plan_sql_migration(old_source: &str, new_source: &str) -> SqlMigrationPlan {
    let old_tables = parsed_schema_tables(old_source);
    let new_tables = parsed_schema_tables(new_source);
    let mut changes = Vec::new();

    for (key, old_table) in &old_tables {
        let Some(new_table) = new_tables.get(key) else {
            changes.push(SqlMigrationChange::removed_table(&old_table.name));
            continue;
        };

        compare_table_columns(old_table, new_table, &mut changes);
        let old_primary_key = primary_key_names(old_table);
        let new_primary_key = primary_key_names(new_table);
        if old_primary_key != new_primary_key {
            changes.push(SqlMigrationChange::primary_key_changed(
                &old_table.name,
                old_primary_key,
                new_primary_key,
            ));
        }
    }

    for (key, new_table) in &new_tables {
        if !old_tables.contains_key(key) {
            changes.push(SqlMigrationChange::added_table(&new_table.name));
        }
    }

    changes.sort_by(|left, right| {
        left.table
            .cmp(&right.table)
            .then(left.column.cmp(&right.column))
            .then(left.kind.cmp(&right.kind))
    });
    SqlMigrationPlan { changes }
}

pub fn render_sql_schema(source: &str, module_name: Option<&str>) -> String {
    let module_name = module_name.unwrap_or("generated.database");
    let tables = parse_schema(source);
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
        for relation in &table.foreign_keys {
            out.push_str("    // ");
            out.push_str(&relation.comment(&table.name, &table.columns));
            out.push('\n');
        }
        out.push_str("    list_");
        out.push_str(&table_ident);
        out.push_str("() -> List<");
        out.push_str(&table_type);
        out.push_str(">\n");

        if let Some(primary_keys) = table.primary_key_columns() {
            out.push_str("    find_");
            out.push_str(&table_ident);
            out.push_str("_by_");
            out.push_str(
                &primary_keys
                    .iter()
                    .map(|column| to_identifier(&column.name))
                    .collect::<Vec<_>>()
                    .join("_and_"),
            );
            out.push('(');
            for (index, primary_key) in primary_keys.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                out.push_str(&to_identifier(&primary_key.name));
                out.push_str(": ");
                out.push_str(&primary_key.ty);
            }
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
pub struct SqlMigrationPlan {
    changes: Vec<SqlMigrationChange>,
}

impl SqlMigrationPlan {
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str("SQL migration plan\n");
        out.push_str(&format!(
            "Summary: {} additive, {} breaking, {} review\n",
            self.changes
                .iter()
                .filter(|change| change.severity == "additive")
                .count(),
            self.changes
                .iter()
                .filter(|change| change.severity == "breaking")
                .count(),
            self.changes
                .iter()
                .filter(|change| change.severity == "review")
                .count()
        ));
        if self.changes.is_empty() {
            out.push_str("No supported table, column, or primary-key changes detected.\n");
        } else {
            for change in &self.changes {
                out.push_str("- ");
                out.push_str(&change.render_text());
                out.push('\n');
            }
        }
        out.push_str("Planning only: no database migration SQL is generated or executed.\n");
        out
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema_version": "num.sql_migration_plan.v1",
            "changes": self.changes.iter().map(SqlMigrationChange::to_json).collect::<Vec<_>>()
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SqlMigrationChange {
    kind: String,
    severity: String,
    table: String,
    column: Option<String>,
    from: Option<String>,
    to: Option<String>,
}

impl SqlMigrationChange {
    fn added_table(table: &str) -> Self {
        Self {
            kind: "table_added".to_string(),
            severity: "additive".to_string(),
            table: table.to_string(),
            column: None,
            from: None,
            to: None,
        }
    }

    fn removed_table(table: &str) -> Self {
        Self {
            kind: "table_removed".to_string(),
            severity: "breaking".to_string(),
            table: table.to_string(),
            column: None,
            from: None,
            to: None,
        }
    }

    fn added_column(table: &str, column: &Column) -> Self {
        Self {
            kind: "column_added".to_string(),
            severity: if column.nullable {
                "additive".to_string()
            } else {
                "breaking".to_string()
            },
            table: table.to_string(),
            column: Some(column.name.clone()),
            from: None,
            to: Some(column_signature(column)),
        }
    }

    fn removed_column(table: &str, column: &Column) -> Self {
        Self {
            kind: "column_removed".to_string(),
            severity: "breaking".to_string(),
            table: table.to_string(),
            column: Some(column.name.clone()),
            from: Some(column_signature(column)),
            to: None,
        }
    }

    fn changed_column(table: &str, old_column: &Column, new_column: &Column) -> Self {
        Self {
            kind: "column_changed".to_string(),
            severity: "review".to_string(),
            table: table.to_string(),
            column: Some(old_column.name.clone()),
            from: Some(column_signature(old_column)),
            to: Some(column_signature(new_column)),
        }
    }

    fn primary_key_changed(table: &str, from: Vec<String>, to: Vec<String>) -> Self {
        Self {
            kind: "primary_key_changed".to_string(),
            severity: "breaking".to_string(),
            table: table.to_string(),
            column: None,
            from: Some(render_column_list(&from)),
            to: Some(render_column_list(&to)),
        }
    }

    fn render_text(&self) -> String {
        let subject = match &self.column {
            Some(column) => format!(
                "`{}.{}`",
                sanitize_comment_text(&self.table),
                sanitize_comment_text(column)
            ),
            None => format!("`{}`", sanitize_comment_text(&self.table)),
        };
        match self.kind.as_str() {
            "table_added" => format!("additive table added: {subject}"),
            "table_removed" => format!("breaking table removed: {subject}"),
            "column_added" => format!(
                "{} column added: {subject} as {}",
                self.severity,
                self.to.as_deref().unwrap_or("unknown")
            ),
            "column_removed" => format!(
                "breaking column removed: {subject} was {}",
                self.from.as_deref().unwrap_or("unknown")
            ),
            "column_changed" => format!(
                "review column changed: {subject} from {} to {}",
                self.from.as_deref().unwrap_or("unknown"),
                self.to.as_deref().unwrap_or("unknown")
            ),
            "primary_key_changed" => format!(
                "breaking primary key changed on {subject}: {} -> {}",
                self.from.as_deref().unwrap_or("(none)"),
                self.to.as_deref().unwrap_or("(none)")
            ),
            _ => format!("{} change on {subject}", self.severity),
        }
    }

    fn to_json(&self) -> serde_json::Value {
        let mut value = serde_json::json!({
            "kind": self.kind,
            "severity": self.severity,
            "table": self.table,
        });
        if let Some(object) = value.as_object_mut() {
            if let Some(column) = &self.column {
                object.insert("column".to_string(), serde_json::json!(column));
            }
            if let Some(from) = &self.from {
                object.insert("from".to_string(), serde_json::json!(from));
            }
            if let Some(to) = &self.to {
                object.insert("to".to_string(), serde_json::json!(to));
            }
        }
        value
    }
}

fn parse_schema(source: &str) -> Vec<Table> {
    let mut tables = parse_tables(source);
    attach_indexes(&mut tables, parse_indexes(source));
    tables
}

fn parsed_schema_tables(source: &str) -> BTreeMap<String, Table> {
    parse_schema(source)
        .into_iter()
        .map(|table| (table.name.to_ascii_lowercase(), table))
        .collect()
}

fn compare_table_columns(
    old_table: &Table,
    new_table: &Table,
    changes: &mut Vec<SqlMigrationChange>,
) {
    let old_columns = table_columns(old_table);
    let new_columns = table_columns(new_table);

    for (key, old_column) in &old_columns {
        let Some(new_column) = new_columns.get(key) else {
            changes.push(SqlMigrationChange::removed_column(
                &old_table.name,
                old_column,
            ));
            continue;
        };
        if column_signature(old_column) != column_signature(new_column) {
            changes.push(SqlMigrationChange::changed_column(
                &old_table.name,
                old_column,
                new_column,
            ));
        }
    }

    for (key, new_column) in &new_columns {
        if !old_columns.contains_key(key) {
            changes.push(SqlMigrationChange::added_column(
                &new_table.name,
                new_column,
            ));
        }
    }
}

fn table_columns(table: &Table) -> BTreeMap<String, Column> {
    table
        .columns
        .iter()
        .cloned()
        .map(|column| (column.name.to_ascii_lowercase(), column))
        .collect()
}

fn primary_key_names(table: &Table) -> Vec<String> {
    table
        .columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| column.name.clone())
        .collect()
}

fn column_signature(column: &Column) -> String {
    let nullability = if column.nullable {
        "nullable"
    } else {
        "required"
    };
    format!("{} {nullability}", column.ty)
}

fn render_column_list(columns: &[String]) -> String {
    if columns.is_empty() {
        "(none)".to_string()
    } else {
        columns
            .iter()
            .map(|column| format!("`{}`", sanitize_comment_text(column)))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Table {
    name: String,
    columns: Vec<Column>,
    foreign_keys: Vec<ForeignKey>,
    indexes: Vec<SqlIndex>,
}

impl Table {
    fn primary_key_columns(&self) -> Option<Vec<&Column>> {
        let primary_keys = self
            .columns
            .iter()
            .filter(|column| column.primary_key)
            .collect::<Vec<_>>();
        (!primary_keys.is_empty()).then_some(primary_keys)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Column {
    name: String,
    ty: String,
    nullable: bool,
    primary_key: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ForeignKey {
    columns: Vec<String>,
    referenced_table: String,
    referenced_columns: Vec<String>,
}

impl ForeignKey {
    fn comment(&self, table: &str, columns: &[Column]) -> String {
        let source_columns = self
            .columns
            .iter()
            .map(|column| {
                format!(
                    "`{}.{}`",
                    sanitize_comment_text(table),
                    sanitize_comment_text(column)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let referenced_columns = if self.referenced_columns.is_empty() {
            format!("`{}`", sanitize_comment_text(&self.referenced_table))
        } else {
            self.referenced_columns
                .iter()
                .map(|column| {
                    format!(
                        "`{}.{}`",
                        sanitize_comment_text(&self.referenced_table),
                        sanitize_comment_text(column)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let nullable = if self.columns.iter().any(|foreign_key_column| {
            columns
                .iter()
                .find(|column| column.name.eq_ignore_ascii_case(foreign_key_column))
                .is_some_and(|column| column.nullable)
        }) {
            "nullable"
        } else {
            "required"
        };

        format!("SQL relation {source_columns} references {referenced_columns}; {nullable} foreign-key hint only, runtime relation loading is not generated yet")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SqlIndex {
    name: String,
    table: String,
    columns: Vec<String>,
    unique: bool,
}

impl SqlIndex {
    fn comment(&self) -> String {
        let columns = self
            .columns
            .iter()
            .map(|column| {
                format!(
                    "`{}.{}`",
                    sanitize_comment_text(&self.table),
                    sanitize_comment_text(column)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let uniqueness = if self.unique { "unique" } else { "non-unique" };

        format!(
            "SQL {uniqueness} index `{}` covers {columns}; index metadata only, runtime query planning is not generated yet",
            sanitize_comment_text(&self.name)
        )
    }
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
        let items = split_sql_items(body);
        let columns = parse_columns(&items);
        let foreign_keys = parse_foreign_keys(&items, &columns);
        tables.push(Table {
            name,
            columns,
            foreign_keys,
            indexes: Vec::new(),
        });
        rest = after_open[close + 1..].to_string();
    }

    tables.sort_by(|left, right| left.name.cmp(&right.name));
    tables
}

fn attach_indexes(tables: &mut [Table], indexes: Vec<SqlIndex>) {
    let mut by_table = BTreeMap::<String, Vec<SqlIndex>>::new();
    for index in indexes {
        by_table
            .entry(index.table.to_ascii_lowercase())
            .or_default()
            .push(index);
    }

    for table in tables {
        if let Some(mut indexes) = by_table.remove(&table.name.to_ascii_lowercase()) {
            indexes.sort_by(|left, right| {
                left.name
                    .cmp(&right.name)
                    .then(left.columns.cmp(&right.columns))
                    .then(left.unique.cmp(&right.unique))
            });
            table.indexes = indexes;
        }
    }
}

fn parse_indexes(source: &str) -> Vec<SqlIndex> {
    let mut indexes = Vec::new();
    let mut rest = strip_sql_comments(source);

    while let Some(start) = find_create_index(&rest) {
        let statement_start = &rest[start..];
        let end = find_statement_end(statement_start).unwrap_or(statement_start.len());
        let statement = &statement_start[..end];
        if let Some(index) = parse_index_statement(statement) {
            indexes.push(index);
        }
        rest = statement_start
            .get(end.saturating_add(1)..)
            .unwrap_or_default()
            .to_string();
    }

    indexes.sort_by(|left, right| {
        left.table
            .cmp(&right.table)
            .then(left.name.cmp(&right.name))
            .then(left.columns.cmp(&right.columns))
    });
    indexes
}

fn find_create_index(source: &str) -> Option<usize> {
    let lower = source.to_ascii_lowercase();
    [
        lower.find("create index"),
        lower.find("create unique index"),
    ]
    .into_iter()
    .flatten()
    .min()
}

fn parse_index_statement(statement: &str) -> Option<SqlIndex> {
    let mut rest = statement.trim_start();
    rest = strip_keyword(rest, "create")?;
    let (unique, after_unique) = if let Some(rest) = strip_keyword(rest, "unique") {
        (true, rest)
    } else {
        (false, rest)
    };
    rest = strip_keyword(after_unique, "index")?;
    if let Some(after_if_not_exists) = strip_keyword(rest, "if")
        .and_then(|rest| strip_keyword(rest, "not"))
        .and_then(|rest| strip_keyword(rest, "exists"))
    {
        rest = after_if_not_exists;
    }

    let (name, after_name) = take_sql_word(rest)?;
    rest = strip_keyword(after_name, "on")?;
    let (table, after_table) = take_sql_word(rest)?;
    let table = clean_sql_ident(table.rsplit('.').next()?)?;
    let after_table = after_table.trim_start();
    let after_open = after_table.strip_prefix('(')?;
    let close = find_matching_paren(after_open)?;
    let columns = split_sql_items(&after_open[..close])
        .into_iter()
        .filter_map(|column| parse_index_column(&column))
        .collect::<Vec<_>>();

    (!columns.is_empty()).then_some(SqlIndex {
        name: clean_sql_ident(name)?,
        table,
        columns,
        unique,
    })
}

fn parse_index_column(column: &str) -> Option<String> {
    let raw = column.trim();
    if raw.contains('(') || raw.contains(')') {
        return None;
    }
    let first = raw.split_whitespace().next()?;
    clean_sql_ident(first)
}

fn strip_keyword<'a>(source: &'a str, keyword: &str) -> Option<&'a str> {
    let source = source.trim_start();
    let rest = source.strip_prefix(keyword).or_else(|| {
        source
            .get(..keyword.len())
            .filter(|head| head.eq_ignore_ascii_case(keyword))
            .and_then(|_| source.get(keyword.len()..))
    })?;
    if rest
        .chars()
        .next()
        .is_none_or(|ch| ch.is_whitespace() || matches!(ch, '(' | ';'))
    {
        Some(rest.trim_start())
    } else {
        None
    }
}

fn take_sql_word(source: &str) -> Option<(&str, &str)> {
    let source = source.trim_start();
    let mut end = 0usize;
    for (index, ch) in source.char_indices() {
        if ch.is_whitespace() || ch == '(' || ch == ';' {
            break;
        }
        end = index + ch.len_utf8();
    }
    (end > 0).then_some((&source[..end], &source[end..]))
}

fn parse_columns(items: &[String]) -> Vec<Column> {
    let primary_key_names = table_primary_key_names(&items);
    items
        .iter()
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

fn parse_foreign_keys(items: &[String], columns: &[Column]) -> Vec<ForeignKey> {
    let mut foreign_keys = Vec::new();

    for item in items {
        if let Some(foreign_key) = table_foreign_key_from_constraint(item) {
            foreign_keys.push(foreign_key);
            continue;
        }

        if let Some(foreign_key) = inline_foreign_key_from_column(item, columns) {
            foreign_keys.push(foreign_key);
        }
    }

    foreign_keys.sort_by(|left, right| {
        left.columns
            .cmp(&right.columns)
            .then(left.referenced_table.cmp(&right.referenced_table))
            .then(left.referenced_columns.cmp(&right.referenced_columns))
    });
    foreign_keys
}

fn inline_foreign_key_from_column(item: &str, columns: &[Column]) -> Option<ForeignKey> {
    let item = item.trim();
    if item.is_empty() || is_table_constraint(item) {
        return None;
    }

    let column_name = clean_sql_ident(item.split_whitespace().next()?)?;
    let lower = item.to_ascii_lowercase();
    let references_start = lower.find("references")?;
    let after_references = &item[references_start + "references".len()..];
    let (referenced_table, referenced_columns) = parse_reference_target(after_references)?;
    columns
        .iter()
        .any(|column| column.name.eq_ignore_ascii_case(&column_name))
        .then_some(ForeignKey {
            columns: vec![column_name],
            referenced_table,
            referenced_columns,
        })
}

fn table_foreign_key_from_constraint(item: &str) -> Option<ForeignKey> {
    let lower = item.to_ascii_lowercase();
    let foreign_key_start = lower.find("foreign key")?;
    let after_foreign_key = &item[foreign_key_start + "foreign key".len()..];
    let open = after_foreign_key.find('(')?;
    let after_open = &after_foreign_key[open + 1..];
    let close = find_matching_paren(after_open)?;
    let columns = split_sql_items(&after_open[..close])
        .into_iter()
        .filter_map(|column| clean_sql_ident(&column))
        .collect::<Vec<_>>();
    if columns.is_empty() {
        return None;
    }

    let after_columns = &after_open[close + 1..];
    let lower_after_columns = after_columns.to_ascii_lowercase();
    let references_start = lower_after_columns.find("references")?;
    let after_references = &after_columns[references_start + "references".len()..];
    let (referenced_table, referenced_columns) = parse_reference_target(after_references)?;

    Some(ForeignKey {
        columns,
        referenced_table,
        referenced_columns,
    })
}

fn parse_reference_target(source: &str) -> Option<(String, Vec<String>)> {
    let source = source.trim_start();
    let mut table_end = 0usize;
    for (index, ch) in source.char_indices() {
        if ch.is_whitespace() || ch == '(' {
            break;
        }
        table_end = index + ch.len_utf8();
    }
    if table_end == 0 {
        return None;
    }

    let referenced_table = clean_sql_ident(source[..table_end].rsplit('.').next()?)?;
    let after_table = source[table_end..].trim_start();
    let referenced_columns = if let Some(after_open) = after_table.strip_prefix('(') {
        let close = find_matching_paren(after_open)?;
        split_sql_items(&after_open[..close])
            .into_iter()
            .filter_map(|column| clean_sql_ident(&column))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    Some((referenced_table, referenced_columns))
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

fn find_statement_end(source: &str) -> Option<usize> {
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
        if ch == ';' {
            return Some(index);
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

fn sanitize_comment_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\r' | '\n' | '\t' => ' ',
            '`' => '\'',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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
    for index in &table.indexes {
        out.push_str("    // ");
        out.push_str(&index.comment());
        out.push('\n');
    }
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
    fn preserves_sql_index_metadata_comments() {
        let source = r#"
CREATE TABLE refunds (
    id UUID PRIMARY KEY,
    payment_id UUID NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL
);

CREATE INDEX idx_refunds_payment_id ON refunds (payment_id);
CREATE UNIQUE INDEX idx_refunds_status_created_at ON refunds (status, created_at);
"#;

        let rendered = render_sql_schema(source, Some("generated.db"));
        let rendered_again = render_sql_schema(source, Some("generated.db"));

        assert_eq!(rendered, rendered_again);
        assert!(rendered.contains(
            "// SQL non-unique index `idx_refunds_payment_id` covers `refunds.payment_id`; index metadata only, runtime query planning is not generated yet"
        ));
        assert!(rendered.contains(
            "// SQL unique index `idx_refunds_status_created_at` covers `refunds.status`, `refunds.created_at`; index metadata only, runtime query planning is not generated yet"
        ));
        assert!(rendered.contains("type Refunds"));
        assert!(num_compiler::check("generated_sql.num", &rendered).is_empty());
    }

    #[test]
    fn preserves_if_not_exists_and_sorted_index_metadata_comments() {
        let source = r#"
CREATE INDEX idx_refunds_payment_id ON public.refunds (payment_id DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_refunds_external_id ON refunds (external_id);

CREATE TABLE refunds (
    id UUID PRIMARY KEY,
    payment_id UUID NOT NULL,
    external_id TEXT NOT NULL
);
"#;

        let rendered = render_sql_schema(source, Some("generated.db"));

        assert!(rendered.contains(
            "type Refunds {\n    // SQL unique index `idx_refunds_external_id` covers `refunds.external_id`; index metadata only, runtime query planning is not generated yet\n    // SQL non-unique index `idx_refunds_payment_id` covers `refunds.payment_id`; index metadata only, runtime query planning is not generated yet"
        ));
        assert!(num_compiler::check("generated_sql.num", &rendered).is_empty());
    }

    #[test]
    fn renders_sql_migration_plan_for_additive_changes() {
        let old_source = include_str!("../tests/fixtures/sql_migration/additive_old.sql");
        let new_source = include_str!("../tests/fixtures/sql_migration/additive_new.sql");

        let plan = plan_sql_migration(old_source, new_source);
        let text = plan.render_text();
        let json = plan.to_json();

        assert!(text.contains("Summary: 2 additive, 0 breaking, 0 review"));
        assert!(text.contains("additive column added: `refunds.note` as Option<Text> nullable"));
        assert!(text.contains("additive table added: `refund_events`"));
        assert_eq!(json["schema_version"], "num.sql_migration_plan.v1");
        assert_eq!(json["changes"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn renders_sql_migration_plan_for_breaking_and_review_changes() {
        let old_source = include_str!("../tests/fixtures/sql_migration/breaking_old.sql");
        let new_source = include_str!("../tests/fixtures/sql_migration/breaking_new.sql");

        let plan = plan_sql_migration(old_source, new_source);
        let text = plan.render_text();
        let json = plan.to_json();

        assert!(text.contains("Summary: 0 additive, 4 breaking, 1 review"));
        assert!(text.contains("breaking table removed: `audit_logs`"));
        assert!(text.contains("breaking column added: `refunds.tenant_id` as Uuid required"));
        assert!(text.contains("breaking column removed: `refunds.note` was Option<Text> nullable"));
        assert!(text.contains(
            "review column changed: `refunds.amount` from Decimal required to Int required"
        ));
        assert!(
            text.contains("breaking primary key changed on `refunds`: `id` -> `tenant_id`, `id`")
        );
        assert_eq!(json["changes"][0]["kind"], "table_removed");
        assert_eq!(json["changes"][0]["severity"], "breaking");
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
    fn renders_composite_table_level_primary_key_finder() {
        let source = r#"
CREATE TABLE ledger_entries (
    account_id UUID,
    sequence_no INTEGER,
    amount NUMERIC(12,2) NOT NULL,
    PRIMARY KEY (account_id, sequence_no)
);
"#;

        let rendered = render_sql_schema(source, Some("generated.db"));
        let rendered_again = render_sql_schema(source, Some("generated.db"));

        assert_eq!(rendered, rendered_again);
        assert!(rendered.contains("accountId: Uuid"));
        assert!(rendered.contains("sequenceNo: Int"));
        assert!(rendered.contains(
            "find_ledgerEntries_by_accountId_and_sequenceNo(accountId: Uuid, sequenceNo: Int) -> Option<LedgerEntries>"
        ));
        assert!(num_compiler::check("generated_sql.num", &rendered).is_empty());
    }

    #[test]
    fn renders_three_column_primary_key_finder() {
        let source = r#"
CREATE TABLE entitlement_grants (
    tenant_id UUID,
    actor_id UUID,
    permission_code TEXT,
    granted_at TIMESTAMP NOT NULL,
    PRIMARY KEY (tenant_id, actor_id, permission_code)
);
"#;

        let rendered = render_sql_schema(source, Some("generated.db"));

        assert!(rendered.contains(
            "find_entitlementGrants_by_tenantId_and_actorId_and_permissionCode(tenantId: Uuid, actorId: Uuid, permissionCode: Text) -> Option<EntitlementGrants>"
        ));
        assert!(num_compiler::check("generated_sql.num", &rendered).is_empty());
    }

    #[test]
    fn renders_inline_foreign_key_relation_hints() {
        let source = r#"
CREATE TABLE payments (
    id UUID PRIMARY KEY
);

CREATE TABLE refunds (
    id UUID PRIMARY KEY,
    payment_id UUID NOT NULL REFERENCES payments(id),
    note TEXT
);
"#;

        let rendered = render_sql_schema(source, Some("generated.db"));

        assert!(rendered.contains(
            "// SQL relation `refunds.payment_id` references `payments.id`; required foreign-key hint only, runtime relation loading is not generated yet"
        ));
        assert!(rendered.contains("list_refunds() -> List<Refunds>"));
        assert!(rendered.contains("insert_refunds(row: Refunds) -> Refunds"));
        assert!(num_compiler::check("generated_sql.num", &rendered).is_empty());
    }

    #[test]
    fn renders_table_level_foreign_key_relation_hints() {
        let source = r#"
CREATE TABLE customers (
    id UUID PRIMARY KEY
);

CREATE TABLE orders (
    id UUID PRIMARY KEY,
    customer_id UUID,
    CONSTRAINT orders_customer_fk FOREIGN KEY (customer_id) REFERENCES customers(id)
);
"#;

        let rendered = render_sql_schema(source, Some("generated.db"));

        assert!(rendered.contains(
            "// SQL relation `orders.customer_id` references `customers.id`; nullable foreign-key hint only, runtime relation loading is not generated yet"
        ));
        assert!(rendered.contains("customerId: Option<Uuid>"));
        assert!(num_compiler::check("generated_sql.num", &rendered).is_empty());
    }

    #[test]
    fn renders_composite_foreign_key_relation_hints_deterministically() {
        let source = r#"
CREATE TABLE order_lines (
    order_id UUID,
    line_no INTEGER,
    sku TEXT NOT NULL,
    PRIMARY KEY (order_id, line_no)
);

CREATE TABLE shipment_lines (
    shipment_id UUID,
    order_id UUID NOT NULL,
    line_no INTEGER NOT NULL,
    FOREIGN KEY (order_id, line_no) REFERENCES order_lines(order_id, line_no)
);
"#;

        let rendered = render_sql_schema(source, Some("generated.db"));
        let rendered_again = render_sql_schema(source, Some("generated.db"));

        assert_eq!(rendered, rendered_again);
        assert!(rendered.contains(
            "// SQL relation `shipment_lines.order_id`, `shipment_lines.line_no` references `order_lines.order_id`, `order_lines.line_no`; required foreign-key hint only, runtime relation loading is not generated yet"
        ));
        assert!(num_compiler::check("generated_sql.num", &rendered).is_empty());
    }
}
