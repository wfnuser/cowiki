use sqlx::PgPool;

pub mod api_keys;
pub mod audit;
pub mod pages;
pub mod submissions;
pub mod users;
pub mod workspaces;

pub async fn create_pool(database_url: &str) -> sqlx::Result<PgPool> {
    PgPool::connect(database_url).await
}

pub async fn run_migrations(pool: &PgPool, embedding_dim: u32) -> sqlx::Result<()> {
    let sql = include_str!("migrations/001_init.sql")
        .replace("__EMBEDDING_DIM__", &embedding_dim.to_string());
    sqlx::raw_sql(&sql).execute(pool).await.map_err(|e| {
        tracing::error!("DB error: {e}");
        e
    })?;
    let sql2 = include_str!("migrations/002_workspaces.sql");
    sqlx::raw_sql(sql2).execute(pool).await.map_err(|e| {
        tracing::error!("DB error: {e}");
        e
    })?;
    let sql3 = include_str!("migrations/003_workspace_visibility.sql");
    sqlx::raw_sql(sql3).execute(pool).await.map_err(|e| {
        tracing::error!("DB error: {e}");
        e
    })?;
    let sql4 = include_str!("migrations/004_role_update.sql");
    sqlx::raw_sql(sql4).execute(pool).await.map_err(|e| {
        tracing::error!("DB error: {e}");
        e
    })?;
    let sql5 = include_str!("migrations/005_fts.sql");
    sqlx::raw_sql(sql5).execute(pool).await.map_err(|e| {
        tracing::error!("DB error: {e}");
        e
    })?;
    let sql6 = include_str!("migrations/006_api_keys.sql");
    sqlx::raw_sql(sql6).execute(pool).await.map_err(|e| {
        tracing::error!("DB error: {e}");
        e
    })?;
    let sql7 = include_str!("migrations/007_team_permissions.sql");
    sqlx::raw_sql(sql7).execute(pool).await.map_err(|e| {
        tracing::error!("DB error: {e}");
        e
    })?;
    let sql8 = include_str!("migrations/008_submission_workspace.sql");
    sqlx::raw_sql(sql8).execute(pool).await.map_err(|e| {
        tracing::error!("DB error: {e}");
        e
    })?;
    Ok(())
}
