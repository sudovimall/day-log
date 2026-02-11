use crate::app_state::AppState;
use crate::http::journal;
use crate::http::resource;
use axum::routing::{get, get_service, post};
use axum::{extract::DefaultBodyLimit, Router};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;
use tracing::{debug, info};

pub async fn run(app_state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let mut port = app_state.config.port;

    let router
        = Router::new()
        .route("/journal", get(journal::create_journal))
    let router = Router::new()
        .route_service("/", get_service(ServeDir::new(&app_state.config.get_index_path())))
        .nest_service("/static", ServeDir::new(&app_state.config.get_static_path()))
        .route(
            "/journal",
            post(journal::create_journal).get(journal::list_journals),
        )
        .route(
            "/journal/{id}",
            get(journal::get_journal)
                .put(journal::update_journal)
                .delete(journal::delete_journal),
        )
        .with_state(app_state);

    loop {
        match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
            Ok(listener) => {
                info!("服务已启动 http://127.0.0.1:{}", port);
                let _ = axum::serve(listener, router.into_make_service()).await?;
                break;
            },
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                debug!("端口 {} 被占用，尝试使用下一个端口", port);
                port += 1;
            },
            Err(err) => {
                return Err(Box::new(err));
            }
        }
    }

    Ok(())
}


