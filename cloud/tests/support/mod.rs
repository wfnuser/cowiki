use cowiki_cloud::db;
use sqlx::PgPool;
use uuid::Uuid;

pub struct TestDatabase {
    pub pool: PgPool,
    admin_url: String,
    database_name: String,
}

impl TestDatabase {
    pub async fn create() -> Option<Self> {
        let admin_url = std::env::var("TEST_DATABASE_URL").ok()?;
        let admin = PgPool::connect(&admin_url).await.unwrap();
        let database_name = format!("cowiki_test_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE \"{database_name}\""))
            .execute(&admin)
            .await
            .unwrap();
        admin.close().await;

        let mut database_url = url::Url::parse(&admin_url).unwrap();
        database_url.set_path(&format!("/{database_name}"));
        let pool = db::connect_and_migrate(database_url.as_str())
            .await
            .unwrap();
        Some(Self {
            pool,
            admin_url,
            database_name,
        })
    }

    pub async fn finish(self) {
        self.pool.close().await;
        let admin = PgPool::connect(&self.admin_url).await.unwrap();
        sqlx::query(&format!(
            "DROP DATABASE \"{}\" WITH (FORCE)",
            self.database_name
        ))
        .execute(&admin)
        .await
        .unwrap();
        admin.close().await;
    }
}
