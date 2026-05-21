use std::time::Duration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};

use crate::config::AppConfig;
use crate::error::AppError;

/// Подключается к БД с настройками пула.
///
/// # Errors
///
/// Возвращает `AppError::Db` при ошибке подключения или настройки прагм.
pub async fn connect(config: &AppConfig) -> Result<DatabaseConnection, AppError> {
    let mut opt = ConnectOptions::new(&config.database_url);
    opt.max_connections(20)
        .min_connections(2)
        .connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .sqlx_logging(false); // не светим SQL в логах

    let db = Database::connect(opt).await.map_err(AppError::Db)?;

    // Для SQLite — настройки прагмы
    if config.database_url.starts_with("sqlite") {
        db.execute_unprepared("PRAGMA journal_mode = WAL")
            .await
            .map_err(AppError::Db)?;
        db.execute_unprepared("PRAGMA synchronous = NORMAL")
            .await
            .map_err(AppError::Db)?;
        db.execute_unprepared("PRAGMA foreign_keys = ON")
            .await
            .map_err(AppError::Db)?;
        db.execute_unprepared("PRAGMA busy_timeout = 5000")
            .await
            .map_err(AppError::Db)?;
    }

    Ok(db)
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
