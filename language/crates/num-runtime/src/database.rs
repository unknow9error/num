use crate::connectors::{ConnectorError, ConnectorExecutor};
use crate::interpreter::Value;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct InMemoryDatabaseConnector {
    tables: RefCell<HashMap<String, InMemoryTable>>,
}

impl InMemoryDatabaseConnector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_table(
        &mut self,
        name: impl Into<String>,
        primary_key: Option<impl Into<String>>,
    ) {
        self.register_table_with_primary_key(
            name,
            primary_key
                .map(|primary_key| vec![primary_key.into()])
                .unwrap_or_default(),
        );
    }

    pub fn register_table_with_primary_key(
        &mut self,
        name: impl Into<String>,
        primary_key: impl IntoIterator<Item = impl Into<String>>,
    ) {
        self.tables.borrow_mut().insert(
            name.into(),
            InMemoryTable {
                primary_key: primary_key.into_iter().map(Into::into).collect(),
                rows: Vec::new(),
            },
        );
    }

    pub fn insert(&self, table: &str, row: Value) -> Result<Value, String> {
        let mut tables = self.tables.borrow_mut();
        let table_state = tables
            .get_mut(table)
            .ok_or_else(|| format!("unknown database table '{table}'"))?;
        if !matches!(row, Value::Struct(_, _)) {
            return Err(format!("database.insert_{table} expects a structured row"));
        }
        table_state.rows.push(row.clone());
        Ok(row)
    }

    pub fn list(&self, table: &str) -> Result<Value, String> {
        let tables = self.tables.borrow();
        let table_state = tables
            .get(table)
            .ok_or_else(|| format!("unknown database table '{table}'"))?;
        Ok(Value::List(table_state.rows.clone()))
    }

    pub fn find_by(
        &self,
        table: &str,
        columns: &[String],
        keys: &[Value],
    ) -> Result<Value, String> {
        let tables = self.tables.borrow();
        let table_state = tables
            .get(table)
            .ok_or_else(|| format!("unknown database table '{table}'"))?;
        if !table_state.primary_key.is_empty() && table_state.primary_key != columns {
            return Err(format!(
                "database.find_{table}_by_{} does not match primary key '{}'",
                columns.join("_and_"),
                table_state.primary_key.join(", ")
            ));
        }
        if columns.len() != keys.len() {
            return Err(format!(
                "database.find_{table}_by_{} expects {} key arguments",
                columns.join("_and_"),
                columns.len()
            ));
        }
        Ok(table_state
            .rows
            .iter()
            .find(|row| {
                columns
                    .iter()
                    .zip(keys.iter())
                    .all(|(column, key)| struct_field(row, column) == Some(key))
            })
            .cloned()
            .unwrap_or(Value::Null))
    }
}

impl ConnectorExecutor for InMemoryDatabaseConnector {
    fn call(&self, name: &str, args: &[Value]) -> Option<Result<Value, ConnectorError>> {
        let ("database", method) = name.split_once('.')? else {
            return None;
        };
        if let Some(table) = method.strip_prefix("list_") {
            if !args.is_empty() {
                return Some(Err(ConnectorError::execution(format!(
                    "database.{method} expects no arguments"
                ))));
            }
            return Some(self.list(table).map_err(ConnectorError::execution));
        }
        if let Some(table) = method.strip_prefix("insert_") {
            let Some(row) = args.first().cloned() else {
                return Some(Err(ConnectorError::execution(format!(
                    "database.{method} expects one row argument"
                ))));
            };
            return Some(self.insert(table, row).map_err(ConnectorError::execution));
        }
        if let Some(rest) = method.strip_prefix("find_") {
            let Some((table, column)) = rest.rsplit_once("_by_") else {
                return Some(Err(ConnectorError::execution(format!(
                    "invalid database finder method '{method}'"
                ))));
            };
            let columns = column
                .split("_and_")
                .map(str::to_string)
                .collect::<Vec<_>>();
            if args.len() != columns.len() {
                return Some(Err(ConnectorError::execution(format!(
                    "database.{method} expects {} key arguments",
                    columns.len()
                ))));
            }
            return Some(
                self.find_by(table, &columns, args)
                    .map_err(ConnectorError::execution),
            );
        }
        None
    }
}

#[derive(Debug, Clone, Default)]
struct InMemoryTable {
    primary_key: Vec<String>,
    rows: Vec<Value>,
}

fn struct_field<'a>(row: &'a Value, field: &str) -> Option<&'a Value> {
    let Value::Struct(_, fields) = row else {
        return None;
    };
    fields.get(field)
}

#[cfg(test)]
mod tests {
    use super::InMemoryDatabaseConnector;
    use crate::connectors::ConnectorExecutor;
    use crate::interpreter::Value;
    use std::collections::HashMap;

    #[test]
    fn database_connector_lists_inserts_and_finds_rows() {
        let mut database = InMemoryDatabaseConnector::new();
        database.register_table("users", Some("id"));
        let row = user_row("user_1", "Ada");

        let inserted = database.insert("users", row.clone()).unwrap();
        assert_eq!(inserted, row);

        let listed = database.call("database.list_users", &[]).unwrap().unwrap();
        assert_eq!(listed, Value::List(vec![row.clone()]));

        let found = database
            .call(
                "database.find_users_by_id",
                &[Value::String("user_1".to_string())],
            )
            .unwrap()
            .unwrap();
        assert_eq!(found, row);
    }

    #[test]
    fn database_connector_returns_null_for_missing_row() {
        let mut database = InMemoryDatabaseConnector::new();
        database.register_table("users", Some("id"));

        let found = database
            .call(
                "database.find_users_by_id",
                &[Value::String("missing".to_string())],
            )
            .unwrap()
            .unwrap();

        assert_eq!(found, Value::Null);
    }

    #[test]
    fn database_connector_finds_rows_by_composite_primary_key() {
        let mut database = InMemoryDatabaseConnector::new();
        database.register_table_with_primary_key("ledgerEntries", ["accountId", "sequenceNo"]);
        let row = ledger_entry_row("acct_1", 42, "refund");

        database.insert("ledgerEntries", row.clone()).unwrap();

        let found = database
            .call(
                "database.find_ledgerEntries_by_accountId_and_sequenceNo",
                &[Value::String("acct_1".to_string()), Value::Int(42)],
            )
            .unwrap()
            .unwrap();
        assert_eq!(found, row);

        let missing = database
            .call(
                "database.find_ledgerEntries_by_accountId_and_sequenceNo",
                &[Value::String("acct_1".to_string()), Value::Int(43)],
            )
            .unwrap()
            .unwrap();
        assert_eq!(missing, Value::Null);
    }

    #[test]
    fn database_connector_rejects_wrong_composite_primary_key_method() {
        let mut database = InMemoryDatabaseConnector::new();
        database.register_table_with_primary_key("ledgerEntries", ["accountId", "sequenceNo"]);

        let error = database
            .call(
                "database.find_ledgerEntries_by_sequenceNo_and_accountId",
                &[Value::Int(42), Value::String("acct_1".to_string())],
            )
            .unwrap()
            .unwrap_err();

        assert_eq!(error.code, "execution_failed");
        assert!(error.message.contains("does not match primary key"));
    }

    #[test]
    fn database_connector_rejects_wrong_composite_key_arity() {
        let mut database = InMemoryDatabaseConnector::new();
        database.register_table_with_primary_key("ledgerEntries", ["accountId", "sequenceNo"]);

        let error = database
            .call(
                "database.find_ledgerEntries_by_accountId_and_sequenceNo",
                &[Value::String("acct_1".to_string())],
            )
            .unwrap()
            .unwrap_err();

        assert_eq!(error.code, "execution_failed");
        assert!(error.message.contains("expects 2 key arguments"));
    }

    #[test]
    fn database_connector_rejects_unknown_tables() {
        let database = InMemoryDatabaseConnector::new();
        let error = database
            .call("database.list_users", &[])
            .unwrap()
            .unwrap_err();

        assert_eq!(error.code, "execution_failed");
        assert!(error.message.contains("unknown database table"));
    }

    fn user_row(id: &str, name: &str) -> Value {
        let mut fields = HashMap::new();
        fields.insert("id".to_string(), Value::String(id.to_string()));
        fields.insert("name".to_string(), Value::String(name.to_string()));
        Value::Struct("Users".to_string(), fields)
    }

    fn ledger_entry_row(account_id: &str, sequence_no: i64, memo: &str) -> Value {
        let mut fields = HashMap::new();
        fields.insert(
            "accountId".to_string(),
            Value::String(account_id.to_string()),
        );
        fields.insert("sequenceNo".to_string(), Value::Int(sequence_no));
        fields.insert("memo".to_string(), Value::String(memo.to_string()));
        Value::Struct("LedgerEntries".to_string(), fields)
    }
}
