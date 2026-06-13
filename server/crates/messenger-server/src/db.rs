use std::str::FromStr;
use std::time::Duration;

use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

use crate::config::AppConfig;
use crate::error::AppError;

/// Подключается к БД с настройками пула.
///
/// # Errors
///
/// Возвращает `AppError::Db` при ошибке подключения или настройки прагм.
pub async fn connect(config: &AppConfig) -> Result<DatabaseConnection, AppError> {
    if config.database_url.starts_with("sqlite") {
        return connect_sqlite(&config.database_url).await;
    }

    let mut opt = ConnectOptions::new(&config.database_url);
    opt.max_connections(20)
        .min_connections(2)
        .connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .sqlx_logging(false); // не светим SQL в логах

    Database::connect(opt).await.map_err(AppError::Db)
}

/// Connect to SQLite with the pragmas applied to **every** pooled connection.
///
/// `busy_timeout` and `foreign_keys` are per-connection settings. Applying them
/// once via `execute_unprepared` only configured a single connection out of the
/// pool, so the rest used `busy_timeout = 0` and failed instantly with
/// "database is locked" under any write contention (e.g. an attachment finalize
/// racing the sync loop). Setting them on `SqliteConnectOptions` applies them on
/// connect for each connection, so writers wait for the lock instead of erroring.
async fn connect_sqlite(database_url: &str) -> Result<DatabaseConnection, AppError> {
    use sqlx::sqlite::{
        SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
    };

    let map_err = |e: sqlx::Error| AppError::Db(DbErr::Custom(format!("sqlite connect: {e}")));

    let opts = SqliteConnectOptions::from_str(database_url)
        .map_err(map_err)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(8))
        .connect_with(opts)
        .await
        .map_err(map_err)?;

    Ok(sea_orm::SqlxSqliteConnector::from_sqlx_sqlite_pool(pool))
}

/// Запускает миграции при старте сервера.
///
/// # Errors
///
/// Возвращает `AppError::Db` при ошибке применения миграций.
pub async fn run_migrations(db: &DatabaseConnection) -> Result<(), AppError> {
    use messenger_migration::MigratorTrait;
    messenger_migration::Migrator::up(db, None)
        .await
        .map_err(|e| AppError::Db(sea_orm::DbErr::Migration(e.to_string())))?;
    Ok(())
}
