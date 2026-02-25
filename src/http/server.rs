use crate::app_state::AppState;
use crate::http::{file, import_zip, journal, repo_sync, settings};
use axum::routing::{get, get_service, post};
use axum::{Router, extract::DefaultBodyLimit};
use std::io;
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};
use tracing::{debug, info};

pub async fn run(app_state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    if let Err(e) = repo_sync::startup_sync_to_db(&app_state).await {
        tracing::error!("启动同步失败: {}", e);
    }

    let port = app_state.config.port;
    let max_switch_time = app_state.config.auto_switch_port_time;
    let mut switch_time = 0;
    let mut current_port = port;

    let router = Router::new()
        .route_service(
            "/",
            get_service(ServeFile::new(&app_state.config.get_index_path())),
        )
        .nest_service(
            "/static",
            ServeDir::new(&app_state.config.get_static_path()),
        )
        .nest_service(
            "/files/picture",
            ServeDir::new(&app_state.config.get_picture_path()),
        )
        .nest_service(
            "/files/media",
            ServeDir::new(&app_state.config.get_media_path()),
        )
        .nest_service(
            "/files/file",
            ServeDir::new(&app_state.config.get_file_path()),
        )
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
        .route("/journal/import/zip", post(import_zip::import_journal_zip))
        .route(
            "/settings",
            get(settings::get_settings).put(settings::update_settings),
        )
        .route("/upload", post(file::upload_file))
        .route("/sync/journal", post(repo_sync::sync_journal))
        .layer(DefaultBodyLimit::max(app_state.config.upload_file_limit))
        .with_state(app_state);

    loop {
        if switch_time >= max_switch_time {
            return Err(Box::new(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!(
                    "尝试使用端口 {} 至 {} 失败，已超过最大尝试次数 {}",
                    port, current_port, max_switch_time
                ),
            )));
        }
        match TcpListener::bind(format!("0.0.0.0:{}", current_port)).await {
            Ok(listener) => {
                info!("服务已启动 http://127.0.0.1:{}", current_port);
                let _ = axum::serve(listener, router.into_make_service()).await?;
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                debug!("端口 {} 被占用，尝试使用下一个端口", current_port);

                current_port += 1;
                switch_time += 1;
            }
            Err(err) => {
                return Err(Box::new(err));
            }
        }
    }

    Ok(())
}
