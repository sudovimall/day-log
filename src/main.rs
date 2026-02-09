mod app_state;
mod config;
mod db;
mod http;
mod util;

use std::sync::Arc;
use tracing::debug;
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
    let app_config = config::app_config::AppConfig::load_from_file("config.toml").unwrap();
    debug!("config: {:?}", app_config);
    app_config.init().await;

    let pool = db::init(&app_config).await.unwrap();

    let state = app_state::AppState {
        db: pool,
        config: Arc::new(app_config),
    };

    let _ = http::server::run(state).await;
}
