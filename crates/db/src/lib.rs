use sqlx::PgPool;

pub mod users;
pub mod pages;
pub mod submissions;
pub mod workspaces;

pub async fn create_pool(database_url: &str) -> sqlx::Result<PgPool> {
    PgPool::connect(database_url).await
}

pub async fn run_migrations(pool: &PgPool, embedding_dim: u32) -> sqlx::Result<()> {
    let sql = include_str!("migrations/001_init.sql")
        .replace("__EMBEDDING_DIM__", &embedding_dim.to_string());
    sqlx::raw_sql(&sql).execute(pool).await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;
    let sql2 = include_str!("migrations/002_workspaces.sql");
    sqlx::raw_sql(sql2).execute(pool).await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;
    let sql3 = include_str!("migrations/003_workspace_visibility.sql");
    sqlx::raw_sql(sql3).execute(pool).await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;
    let sql4 = include_str!("migrations/004_role_update.sql");
    sqlx::raw_sql(sql4).execute(pool).await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;
    Ok(())
}
