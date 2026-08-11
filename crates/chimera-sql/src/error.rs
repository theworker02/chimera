use thiserror::Error;

pub type SqlResult<T> = Result<T, SqlError>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SqlError {
    #[error("syntax error: {0}")]
    Syntax(String),

    #[error("table `{0}` already exists")]
    TableExists(String),

    #[error("table `{0}` does not exist")]
    NoTable(String),

    #[error("column `{0}` not found")]
    NoColumn(String),

    #[error("kv error: {0}")]
    Kv(String),

    #[error("type mismatch for column `{0}`")]
    TypeMismatch(String),
}
