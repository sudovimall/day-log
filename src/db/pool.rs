use crate::config::app_config::AppConfig;
use sqlx::{Pool, SqlitePool};
use std::sync::Arc;

pub async fn init(config: &AppConfig) -> Option<Pool<sqlx::Sqlite>> {
    let path = config.get_db_path();

    if let Ok(pool) = SqlitePool::connect(&path).await {
        Some(pool)
    } else {
        panic!("连不上 {} 数据库", path);
    }
}
