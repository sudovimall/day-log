mod app_state;
mod config;
mod db;
mod http;
mod util;

use std::sync::Arc;
use tracing::error;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=trace", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    // 不配置 有default
    let app_config = match config::app_config::AppConfig::load_from_file("config.toml") {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("读取配置失败: {}", e);
            return;
        }
    };
    app_config.init().await;

    let pool = match db::init(&app_config).await {
        Ok(pool) => pool,
        Err(e) => {
            error!("初始化数据库失败: {}", e);
            return;
        }
    };

    let state = app_state::AppState {
        db: pool,
        config: Arc::new(app_config),
    };

    if let Err(e) = http::server::run(state).await {
        error!("服务启动失败: {}", e);
    }
}
