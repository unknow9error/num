use crate::connectors::ConnectorExecutor;
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
        self.tables.borrow_mut().insert(
            name.into(),
            InMemoryTable {
                primary_key: primary_key.map(Into::into),
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

    pub fn find_by(&self, table: &str, column: &str, key: &Value) -> Result<Value, String> {
        let tables = self.tables.borrow();
        let table_state = tables
            .get(table)
            .ok_or_else(|| format!("unknown database table '{table}'"))?;
        if let Some(primary_key) = &table_state.primary_key {
            if primary_key != column {
                return Err(format!(
                    "database.find_{table}_by_{column} does not match primary key '{primary_key}'"
                ));
            }
        }
        Ok(table_state
            .rows
            .iter()
            .find(|row| struct_field(row, column) == Some(key))
            .cloned()
            .unwrap_or(Value::Null))
    }
}

impl ConnectorExecutor for InMemoryDatabaseConnector {
    fn call(&self, name: &str, args: &[Value]) -> Option<Result<Value, String>> {
        let ("database", method) = name.split_once('.')? else {
            return None;
        };
        if let Some(table) = method.strip_prefix("list_") {
            if !args.is_empty() {
                return Some(Err(format!("database.{method} expects no arguments")));
            }
            return Some(self.list(table));
        }
        if let Some(table) = method.strip_prefix("insert_") {
            let Some(row) = args.first().cloned() else {
                return Some(Err(format!("database.{method} expects one row argument")));
            };
            return Some(self.insert(table, row));
        }
        if let Some(rest) = method.strip_prefix("find_") {
            let Some((table, column)) = rest.rsplit_once("_by_") else {
                return Some(Err(format!("invalid database finder method '{method}'")));
            };
            let Some(key) = args.first() else {
                return Some(Err(format!("database.{method} expects one key argument")));
            };
            return Some(self.find_by(table, column, key));
        }
        None
    }
}

#[derive(Debug, Clone, Default)]
struct InMemoryTable {
    primary_key: Option<String>,
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
    fn database_connector_rejects_unknown_tables() {
        let database = InMemoryDatabaseConnector::new();
        let error = database
            .call("database.list_users", &[])
            .unwrap()
            .unwrap_err();

        assert!(error.contains("unknown database table"));
    }

    fn user_row(id: &str, name: &str) -> Value {
        let mut fields = HashMap::new();
        fields.insert("id".to_string(), Value::String(id.to_string()));
        fields.insert("name".to_string(), Value::String(name.to_string()));
        Value::Struct("Users".to_string(), fields)
    }
}
