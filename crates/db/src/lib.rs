use sqlx::PgPool;

pub mod users;
pub mod pages;
pub mod submissions;
pub mod workspaces;

pub async fn create_pool(database_url: &str) -> sqlx::Result<PgPool> {
    PgPool::connect(database_url).await
}

pub async fn run_migrations(pool: &PgPool) -> sqlx::Result<()> {
    let sql = include_str!("migrations/001_init.sql");
    sqlx::raw_sql(sql).execute(pool).await?;
    let sql2 = include_str!("migrations/002_workspaces.sql");
    sqlx::raw_sql(sql2).execute(pool).await?;
    Ok(())
}
