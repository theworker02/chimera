use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SqlType {
    Int,
    Text,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub ty: SqlType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSchema {
    pub name: String,
    pub columns: Vec<ColumnDef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SqlValue {
    Int(i64),
    Text(String),
    Null,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub id: u64,
    pub values: Vec<SqlValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EqPredicate {
    pub column: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    CreateTable {
        name: String,
        columns: Vec<ColumnDef>,
    },
    Insert {
        table: String,
        values: Vec<String>,
    },
    Select {
        table: String,
        where_eq: Option<EqPredicate>,
        limit: Option<usize>,
    },
    Update {
        table: String,
        set_col: String,
        set_val: String,
        where_eq: Option<EqPredicate>,
    },
    Delete {
        table: String,
        where_eq: Option<EqPredicate>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum QueryResult {
    Ok,
    Rows(Vec<Row>),
    Affected(usize),
}
