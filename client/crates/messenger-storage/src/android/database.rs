//! SQLite-backed local database for Android (no encryption at DB level for MVP).
//! DB-level encryption (SQLCipher) requires OpenSSL cross-compilation for Android,
//! which is deferred. Secrets are protected via the file-based secret store.

use crate::{
    error::StorageError,
    traits::{LocalDatabase, Row, StorageValue},
};
use async_trait::async_trait;
use rusqlite::{Connection, params_from_iter, ToSql, types::ValueRef};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// A stock SQLite database for Android (no encryption).
pub struct AndroidDatabase {
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for AndroidDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AndroidDatabase")
            .field("conn", &"<Mutex<Connection>>")
            .finish()
    }
}

impl AndroidDatabase {
    /// Open (or create) a database at `path`.
    pub fn open(path: PathBuf) -> Result<Self, StorageError> {
        let conn = Connection::open(&path)
            .map_err(|e| StorageError::Database(e.to_string()))?;

        // Enable WAL and foreign keys
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
            .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

#[async_trait(?Send)]
impl LocalDatabase for AndroidDatabase {
    async fn execute(&self, sql: &str, params: &[StorageValue]) -> Result<u64, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let params: Vec<&dyn ToSql> = params.iter().map(value_to_sql).collect();
        let affected = stmt
            .execute(params_from_iter(params))
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(affected as u64)
    }

    async fn query(&self, sql: &str, params: &[StorageValue]) -> Result<Vec<Row>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let params: Vec<&dyn ToSql> = params.iter().map(value_to_sql).collect();
        let column_names: Vec<String> = stmt
            .column_names()
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let rows = stmt
            .query_map(params_from_iter(params), |row| {
                let mut columns = Vec::with_capacity(column_names.len());
                for (idx, name) in column_names.iter().enumerate() {
                    let val = row.get_ref(idx)?;
                    columns.push((name.clone(), sql_to_value(val)?));
                }
                Ok(Row { columns })
            })
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r.map_err(|e| StorageError::Database(e.to_string()))?);
        }
        Ok(result)
    }

    async fn close(&self) {
        // Connection is owned by the mutex; dropping the struct closes it.
    }
}

fn value_to_sql(v: &StorageValue) -> &dyn ToSql {
    match v {
        StorageValue::Null => &rusqlite::types::Null,
        StorageValue::Int(i) => i,
        StorageValue::Real(f) => f,
        StorageValue::Text(s) => s,
        StorageValue::Blob(b) => b,
    }
}

fn sql_to_value(v: ValueRef<'_>) -> Result<StorageValue, rusqlite::Error> {
    match v {
        ValueRef::Null => Ok(StorageValue::Null),
        ValueRef::Integer(i) => Ok(StorageValue::Int(i)),
        ValueRef::Real(f) => Ok(StorageValue::Real(f)),
        ValueRef::Text(t) => Ok(StorageValue::Text(
            String::from_utf8_lossy(t).into_owned(),
        )),
        ValueRef::Blob(b) => Ok(StorageValue::Blob(b.to_vec())),
    }
}
