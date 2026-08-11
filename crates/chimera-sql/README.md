# chimera-sql

Minimal SQL engine over `chimera-consensus-dag` `KvStore` key prefixes.

Supported statements:

- `CREATE TABLE name (col TYPE, ...)`
- `INSERT INTO name VALUES (...)`
- `SELECT * FROM name [WHERE col = val] [LIMIT n]`
- `UPDATE name SET col = val [WHERE col = val]`
- `DELETE FROM name [WHERE col = val]`

Pure Rust — no external SQL parser dependencies.
