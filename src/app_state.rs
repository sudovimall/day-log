use crate::config::app_config::AppConfig;
use sqlx::Pool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: Pool<sqlx::Sqlite>,
    pub config: Arc<AppConfig>,
}
