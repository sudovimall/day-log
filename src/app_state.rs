use std::sync::Arc;
use sqlx::Pool;
use crate::config::app_config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub db: Pool<sqlx::Sqlite>,
    pub config: Arc<AppConfig>
}