use crate::error::{SqlError, SqlResult};
use crate::parser::parse_sql;
use crate::types::{
    EqPredicate, QueryResult, Row, SqlValue, Statement, TableSchema,
};
use consensus_dag::KvStore;
use serde::{Deserialize, Serialize};

const META_PREFIX: &str = "sql:meta:";
const ROW_PREFIX: &str = "sql:row:";
#[derive(Clone, Debug, Serialize, Deserialize)]
struct TableMeta {
    schema: TableSchema,
    next_id: u64,
}

fn meta_key(table: &str) -> String {
    format!("{META_PREFIX}{table}")
}

fn row_key(table: &str, id: u64) -> String {
    format!("{ROW_PREFIX}{table}:{id}")
}

fn kv_set(kv: &KvStore, key: &str, value: &[u8]) -> SqlResult<()> {
    kv.set(key, value.to_vec())
        .map_err(|e| SqlError::Kv(e.to_string()))
}

fn kv_get(kv: &KvStore, key: &str) -> Option<Vec<u8>> {
    kv.get(key)
}

fn load_meta(kv: &KvStore, table: &str) -> SqlResult<TableMeta> {
    let raw = kv_get(kv, &meta_key(table)).ok_or_else(|| SqlError::NoTable(table.to_string()))?;
    serde_json::from_slice(&raw).map_err(|e| SqlError::Kv(e.to_string()))
}

fn save_meta(kv: &KvStore, meta: &TableMeta) -> SqlResult<()> {
    let raw = serde_json::to_vec(meta).map_err(|e| SqlError::Kv(e.to_string()))?;
    kv_set(kv, &meta_key(&meta.schema.name), &raw)
}

fn parse_value(raw: &str, ty: &crate::types::SqlType) -> SqlResult<SqlValue> {
    if raw.eq_ignore_ascii_case("null") {
        return Ok(SqlValue::Null);
    }
    match ty {
        crate::types::SqlType::Int => raw
            .parse::<i64>()
            .map(SqlValue::Int)
            .map_err(|_| SqlError::TypeMismatch(raw.to_string())),
        crate::types::SqlType::Text => Ok(SqlValue::Text(raw.to_string())),
    }
}

fn value_matches(pred: &EqPredicate, schema: &TableSchema, row: &Row) -> bool {
    let Some(idx) = schema.columns.iter().position(|c| c.name == pred.column) else {
        return false;
    };
    match &row.values[idx] {
        SqlValue::Int(n) => pred.value == n.to_string(),
        SqlValue::Text(s) => pred.value == *s,
        SqlValue::Null => pred.value.eq_ignore_ascii_case("null"),
    }
}

fn row_to_strings(row: &Row) -> Vec<String> {
    row.values
        .iter()
        .map(|v| match v {
            SqlValue::Int(n) => n.to_string(),
            SqlValue::Text(s) => s.clone(),
            SqlValue::Null => "NULL".into(),
        })
        .collect()
}

/// SQL engine backed by a `KvStore` with table-scoped key prefixes.
pub struct SqlEngine {
    kv: KvStore,
}

impl SqlEngine {
    pub fn new(kv: KvStore) -> Self {
        Self { kv }
    }

    pub fn kv(&self) -> &KvStore {
        &self.kv
    }

    pub fn execute(&self, sql: &str) -> SqlResult<QueryResult> {
        let stmt = parse_sql(sql)?;
        match stmt {
            Statement::CreateTable { name, columns } => {
                if kv_get(&self.kv, &meta_key(&name)).is_some() {
                    return Err(SqlError::TableExists(name));
                }
                let meta = TableMeta {
                    schema: TableSchema { name, columns },
                    next_id: 1,
                };
                save_meta(&self.kv, &meta)?;
                Ok(QueryResult::Ok)
            }
            Statement::Insert { table, values } => {
                let mut meta = load_meta(&self.kv, &table)?;
                if values.len() != meta.schema.columns.len() {
                    return Err(SqlError::Syntax("column count mismatch".into()));
                }
                let parsed: Vec<SqlValue> = values
                    .iter()
                    .zip(meta.schema.columns.iter())
                    .map(|(v, c)| parse_value(v, &c.ty))
                    .collect::<SqlResult<_>>()?;
                let id = meta.next_id;
                meta.next_id += 1;
                let row = Row { id, values: parsed };
                let raw = serde_json::to_vec(&row).map_err(|e| SqlError::Kv(e.to_string()))?;
                kv_set(&self.kv, &row_key(&table, id), &raw)?;
                save_meta(&self.kv, &meta)?;
                Ok(QueryResult::Affected(1))
            }
            Statement::Select {
                table,
                where_eq,
                limit,
            } => {
                let meta = load_meta(&self.kv, &table)?;
                let mut rows = self.scan_rows(&table, &meta)?;
                if let Some(pred) = where_eq {
                    rows.retain(|r| value_matches(&pred, &meta.schema, r));
                }
                if let Some(n) = limit {
                    rows.truncate(n);
                }
                Ok(QueryResult::Rows(rows))
            }
            Statement::Update {
                table,
                set_col,
                set_val,
                where_eq,
            } => {
                let meta = load_meta(&self.kv, &table)?;
                let col_idx = meta
                    .schema
                    .columns
                    .iter()
                    .position(|c| c.name == set_col)
                    .ok_or_else(|| SqlError::NoColumn(set_col))?;
                let ty = meta.schema.columns[col_idx].ty.clone();
                let new_val = parse_value(&set_val, &ty)?;
                let mut affected = 0usize;
                for row in self.scan_rows(&table, &meta)? {
                    if let Some(ref pred) = where_eq {
                        if !value_matches(pred, &meta.schema, &row) {
                            continue;
                        }
                    }
                    let mut updated = row.clone();
                    updated.values[col_idx] = new_val.clone();
                    let raw =
                        serde_json::to_vec(&updated).map_err(|e| SqlError::Kv(e.to_string()))?;
                    kv_set(&self.kv, &row_key(&table, updated.id), &raw)?;
                    affected += 1;
                }
                Ok(QueryResult::Affected(affected))
            }
            Statement::Delete { table, where_eq } => {
                let meta = load_meta(&self.kv, &table)?;
                let mut affected = 0usize;
                for row in self.scan_rows(&table, &meta)? {
                    if let Some(ref pred) = where_eq {
                        if !value_matches(pred, &meta.schema, &row) {
                            continue;
                        }
                    }
                    // Tombstone via empty delete marker — remove row key by overwriting with tomb
                    kv_set(&self.kv, &row_key(&table, row.id), b"__deleted__")?;
                    affected += 1;
                }
                Ok(QueryResult::Affected(affected))
            }
        }
    }

    fn scan_rows(&self, table: &str, meta: &TableMeta) -> SqlResult<Vec<Row>> {
        let mut rows = Vec::new();
        for id in 1..meta.next_id {
            let key = row_key(table, id);
            let Some(raw) = kv_get(&self.kv, &key) else {
                continue;
            };
            if raw == b"__deleted__" {
                continue;
            }
            let row: Row = serde_json::from_slice(&raw).map_err(|e| SqlError::Kv(e.to_string()))?;
            rows.push(row);
        }
        Ok(rows)
    }

    /// Introspection helper for tests.
    pub fn table_row_strings(&self, table: &str) -> SqlResult<Vec<Vec<String>>> {
        let meta = load_meta(&self.kv, table)?;
        Ok(self
            .scan_rows(table, &meta)?
            .into_iter()
            .map(|r| row_to_strings(&r))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> SqlEngine {
        SqlEngine::new(KvStore::leader(1, vec![]))
    }

    #[test]
    fn crud_lifecycle() {
        let eng = engine();
        eng.execute("CREATE TABLE items (id INT, label TEXT)")
            .unwrap();
        eng.execute("INSERT INTO items VALUES (1, 'alpha')").unwrap();
        eng.execute("INSERT INTO items VALUES (2, 'beta')").unwrap();

        match eng
            .execute("SELECT * FROM items WHERE id = 1 LIMIT 1")
            .unwrap()
        {
            QueryResult::Rows(rows) => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].values[0], SqlValue::Int(1));
            }
            _ => panic!("expected rows"),
        }

        eng.execute("UPDATE items SET label = 'gamma' WHERE id = 2")
            .unwrap();
        let rows = eng.table_row_strings("items").unwrap();
        assert_eq!(rows[1][1], "gamma");

        eng.execute("DELETE FROM items WHERE id = 1").unwrap();
        let remaining = eng.table_row_strings("items").unwrap();
        assert_eq!(remaining.len(), 1);
    }
}
