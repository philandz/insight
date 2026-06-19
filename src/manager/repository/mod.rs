use sqlx::MySqlPool;

/// InsightRepository — MVP has no tables; all data comes from Entry service via gRPC.
/// This struct exists to satisfy the DB initialization pattern in main.rs.
#[allow(dead_code)]
pub struct InsightRepository {
    pool: MySqlPool,
}

impl InsightRepository {
    /// Connects to the shared DB and runs migrations (none for MVP, empty migration file).
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        let pool = sqlx::MySqlPool::connect(database_url).await?;
        let mut migrator = sqlx::migrate::Migrator::new(std::path::Path::new("./migrations")).await?;
        migrator.set_ignore_missing(true);
        migrator.run(&pool).await?;
        Ok(Self { pool })
    }
}