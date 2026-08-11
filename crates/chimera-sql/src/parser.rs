use crate::error::{SqlError, SqlResult};
use crate::types::{ColumnDef, EqPredicate, SqlType, Statement};

fn trim_semi(s: &str) -> &str {
    s.trim().trim_end_matches(';').trim()
}

fn split_cols(s: &str) -> Vec<&str> {
    s.split(',')
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .collect()
}

fn parse_type(raw: &str) -> SqlResult<SqlType> {
    match raw.to_uppercase().as_str() {
        "INT" | "INTEGER" => Ok(SqlType::Int),
        "TEXT" | "STRING" => Ok(SqlType::Text),
        other => Err(SqlError::Syntax(format!("unknown type `{other}`"))),
    }
}

fn parse_where_eq(rest: &str) -> SqlResult<Option<EqPredicate>> {
    let upper = rest.to_uppercase();
    let Some(idx) = upper.find(" WHERE ") else {
        return Ok(None);
    };
    let clause = rest[idx + 7..].trim();
    let parts: Vec<&str> = clause.splitn(3, ' ').collect();
    if parts.len() != 3 || !parts[1].eq_ignore_ascii_case("=") {
        return Err(SqlError::Syntax(
            "WHERE supports only `col = value`".into(),
        ));
    }
    let val = parts[2].trim_matches('\'').trim_matches('"').to_string();
    Ok(Some(EqPredicate {
        column: parts[0].to_string(),
        value: val,
    }))
}

fn strip_where_limit(rest: &str) -> (&str, Option<EqPredicate>, Option<usize>) {
    let mut tail = rest;
    let mut limit = None;
    if let Some(li) = tail.to_uppercase().rfind(" LIMIT ") {
        let (before, after) = tail.split_at(li);
        tail = before;
        if let Some(n) = after
            .trim()
            .strip_prefix("LIMIT")
            .or_else(|| after.trim().strip_prefix("limit"))
        {
            limit = n.trim().parse().ok();
        } else if let Some(n) = after.split_whitespace().nth(1) {
            limit = n.parse().ok();
        }
    }
    let where_eq = parse_where_eq(tail).ok().flatten();
    let base = if let Some(idx) = tail.to_uppercase().find(" WHERE ") {
        &tail[..idx]
    } else {
        tail
    };
    (base.trim(), where_eq, limit)
}

/// Parse a minimal SQL statement (single statement only).
pub fn parse_sql(input: &str) -> SqlResult<Statement> {
    let sql = trim_semi(input);
    let upper = sql.to_uppercase();

    if upper.starts_with("CREATE TABLE ") {
        let rest = sql[13..].trim();
        let (name, cols_raw) = rest
            .split_once('(')
            .ok_or_else(|| SqlError::Syntax("expected CREATE TABLE name (...)".into()))?;
        let cols_raw = cols_raw
            .trim_end_matches(')')
            .trim();
        let mut columns = Vec::new();
        for part in split_cols(cols_raw) {
            let mut it = part.split_whitespace();
            let col_name = it
                .next()
                .ok_or_else(|| SqlError::Syntax("empty column".into()))?
                .to_string();
            let ty_raw = it
                .next()
                .ok_or_else(|| SqlError::Syntax(format!("missing type for `{col_name}`")))?;
            columns.push(ColumnDef {
                name: col_name,
                ty: parse_type(ty_raw)?,
            });
        }
        return Ok(Statement::CreateTable {
            name: name.trim().to_string(),
            columns,
        });
    }

    if upper.starts_with("INSERT INTO ") {
        let rest = sql[12..].trim();
        let (table, values_raw) = rest
            .split_once(" VALUES ")
            .or_else(|| rest.split_once(" values "))
            .ok_or_else(|| SqlError::Syntax("expected INSERT INTO t VALUES (...)".into()))?;
        let values_raw = values_raw
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim();
        let values: Vec<String> = split_cols(values_raw)
            .into_iter()
            .map(|v| v.trim_matches('\'').trim_matches('"').to_string())
            .collect();
        return Ok(Statement::Insert {
            table: table.trim().to_string(),
            values,
        });
    }

    if upper.starts_with("SELECT ") {
        if !upper.contains(" FROM ") {
            return Err(SqlError::Syntax("SELECT requires FROM".into()));
        }
        let from_idx = upper.find(" FROM ").unwrap();
        let table_part = sql[from_idx + 6..].trim();
        let (table, where_eq, limit) = strip_where_limit(table_part);
        return Ok(Statement::Select {
            table: table.to_string(),
            where_eq,
            limit,
        });
    }

    if upper.starts_with("UPDATE ") {
        let rest = sql[7..].trim();
        let set_idx = rest.to_uppercase().find(" SET ").ok_or_else(|| {
            SqlError::Syntax("UPDATE requires SET col = val".into())
        })?;
        let table = rest[..set_idx].trim().to_string();
        let after_set = rest[set_idx + 5..].trim();
        let (assign, where_eq) = {
            let (base, w, _) = strip_where_limit(after_set);
            (base, w)
        };
        let mut parts = assign.splitn(3, ' ');
        let set_col = parts
            .next()
            .ok_or_else(|| SqlError::Syntax("missing SET column".into()))?
            .to_string();
        let eq = parts
            .next()
            .ok_or_else(|| SqlError::Syntax("missing = in SET".into()))?;
        if !eq.eq_ignore_ascii_case("=") {
            return Err(SqlError::Syntax("SET expects col = val".into()));
        }
        let set_val = parts
            .next()
            .ok_or_else(|| SqlError::Syntax("missing SET value".into()))?
            .trim_matches('\'')
            .trim_matches('"')
            .to_string();
        return Ok(Statement::Update {
            table,
            set_col,
            set_val,
            where_eq,
        });
    }

    if upper.starts_with("DELETE FROM ") {
        let table_part = sql[12..].trim();
        let (table, where_eq, _) = strip_where_limit(table_part);
        return Ok(Statement::Delete {
            table: table.to_string(),
            where_eq,
        });
    }

    Err(SqlError::Syntax(format!(
        "unsupported statement: {}",
        sql.chars().take(40).collect::<String>()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_create() {
        let s = parse_sql("CREATE TABLE users (id INT, name TEXT)").unwrap();
        match s {
            Statement::CreateTable { name, columns } => {
                assert_eq!(name, "users");
                assert_eq!(columns.len(), 2);
            }
            _ => panic!("expected create"),
        }
    }

    #[test]
    fn parse_select_limit() {
        let s = parse_sql("SELECT * FROM users WHERE id = 1 LIMIT 5").unwrap();
        match s {
            Statement::Select {
                table,
                where_eq,
                limit,
            } => {
                assert_eq!(table, "users");
                assert_eq!(where_eq.unwrap().column, "id");
                assert_eq!(limit, Some(5));
            }
            _ => panic!("expected select"),
        }
    }
}
