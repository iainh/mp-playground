use crate::error::AppError;
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use mp_config::Config;
use mp_config_sqlx::Datasources;
use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Clone, Datasources)]
#[datasources(prefix = "datasource")]
pub(crate) struct AppDatabases {
    #[datasource(default)]
    pub(crate) primary: SqlitePool,
    pub(crate) audit: sqlx::SqlitePool,
    #[datasource(name = "events")]
    pub(crate) event_log: SqlitePool,
}

pub(crate) async fn connect_databases(
    config_source: &Config,
) -> Result<AppDatabases, Box<dyn std::error::Error>> {
    let databases = AppDatabases::connect(config_source).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS app_events (
            id INTEGER PRIMARY KEY,
            message TEXT NOT NULL
        )",
    )
    .execute(&databases.primary)
    .await?;

    sqlx::query("INSERT INTO app_events (message) VALUES (?)")
        .bind("in-memory sqlite datasource initialized")
        .execute(&databases.primary)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS audit_events (
            id INTEGER PRIMARY KEY,
            message TEXT NOT NULL
        )",
    )
    .execute(&databases.audit)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS event_log (
            id INTEGER PRIMARY KEY,
            message TEXT NOT NULL
        )",
    )
    .execute(&databases.event_log)
    .await?;

    Ok(databases)
}

pub(crate) async fn database_report(
    State(state): State<AppState>,
) -> Result<Json<DatabaseReport>, AppError> {
    let event_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM app_events")
        .fetch_one(&state.databases.primary)
        .await
        .map_err(AppError::Database)?;
    let audit_table_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'audit_events'",
    )
    .fetch_one(&state.databases.audit)
    .await
    .map_err(AppError::Database)?;
    let event_table_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'event_log'",
    )
    .fetch_one(&state.databases.event_log)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(DatabaseReport {
        kind: "sqlite",
        location: "memory",
        event_count,
        named_pools: NamedPoolReport {
            audit_ready: audit_table_count == 1,
            events_ready: event_table_count == 1,
        },
    }))
}

#[derive(Debug, Serialize)]
pub(crate) struct DatabaseReport {
    kind: &'static str,
    location: &'static str,
    event_count: i64,
    named_pools: NamedPoolReport,
}

#[derive(Debug, Serialize)]
pub(crate) struct NamedPoolReport {
    audit_ready: bool,
    events_ready: bool,
}
