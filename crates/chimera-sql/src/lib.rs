//! Minimal SQL-over-KV engine for Chimera mesh nodes.
//!
//! Executes a small SQL subset (`CREATE TABLE`, `INSERT`, `SELECT` with `WHERE`/`LIMIT`,
//! `UPDATE`, `DELETE`) on top of `chimera-consensus-dag` [`KvStore`] key prefixes.

mod engine;
mod error;
mod parser;
mod types;

pub use engine::SqlEngine;
pub use error::{SqlError, SqlResult};
pub use parser::parse_sql;
pub use types::{
    ColumnDef, EqPredicate, QueryResult, Row, SqlType, SqlValue, Statement, TableSchema,
};
