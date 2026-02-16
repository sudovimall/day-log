use crate::app_state::AppState;
use crate::http::resp::{ApiCode, ApiResponse, ApiResult};
use crate::util;
use axum::Json;
use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use sqlx::FromRow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

static FILE_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
struct SaveTarget {
    kind: String,
    path: String,
    uri_prefix: &'static str,
}

#[derive(Debug, FromRow)]
struct FileBlobRow {
    uri: String,
}

pub async fn upload_file(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<String> {
    let mut uploaded_uris = Vec::new();

    loop {
        let next = multipart.next_field().await.map_err(|e| {
            warn!("read multipart field failed: {}", e);
            ApiResponse::<String>::err(ApiCode::BadRequest, "invalid multipart data")
        })?;

        let Some(field) = next else {
            break;
        };

        let Some(file_name_raw) = field.file_name().map(|s| s.to_string()) else {
            continue;
        };

        let original_name = sanitize_file_name(&file_name_raw);
        if original_name.is_empty() {
            continue;
        }

        let mime = field
            .content_type()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let target = resolve_target(&state, Some(mime.as_str()));

        let bytes = field.bytes().await.map_err(|_| {
            ApiResponse::<String>::err(ApiCode::BadRequest, "read upload bytes failed")
        })?;

        let oid = util::file_util::file_hash(&bytes);

        if let Some(uri) = find_existing_uri(&state, &target.kind, &oid)
            .await
            .map_err(|_| {
                ApiResponse::<String>::err(ApiCode::DbQueryFailed, "query file hash failed")
            })?
        {
            uploaded_uris.push(uri);
            continue;
        }

        let file_name = unique_file_name(&original_name);
        let mut full_path = PathBuf::from(&target.path);
        full_path.push(&file_name);
        util::file_util::create_file(&full_path, &bytes)
            .await
            .map_err(|_| {
                ApiResponse::<String>::err(ApiCode::FileWriteFailed, "save file failed")
            })?;

        let uri = format!("{}/{}", target.uri_prefix, file_name);

        let ts = now_ts();
        let file_path = full_path.to_string_lossy().to_string();
        let insert_result = sqlx::query(
            r#"
            insert into file_blob (
                kind, algo, oid, mime, size, original_name, uri, file_path, create_time, update_time
            ) values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&target.kind)
        .bind("sha256")
        .bind(&oid)
        .bind(&mime)
        .bind(bytes.len() as i64)
        .bind(&original_name)
        .bind(&uri)
        .bind(file_path)
        .bind(ts)
        .bind(ts)
        .execute(&state.db)
        .await;

        if insert_result.is_err() {
            if let Some(existing_uri) = find_existing_uri(&state, &target.kind, &oid)
                .await
                .map_err(|_| {
                    ApiResponse::<String>::err(ApiCode::DbQueryFailed, "query file hash failed")
                })?
            {
                uploaded_uris.push(existing_uri);
                continue;
            }
            return Err(ApiResponse::<String>::err(
                ApiCode::DbInsertFailed,
                "save file metadata failed",
            ));
        }

        uploaded_uris.push(uri);
    }

    if uploaded_uris.is_empty() {
        return Err(ApiResponse::<String>::err(
            ApiCode::FileMissing,
            "file required",
        ));
    }

    let first_uri = uploaded_uris[0].clone();
    let msg = uploaded_uris.join(",");
    info!("上传文件地址: {}", msg);
    Ok((
        StatusCode::OK,
        Json(ApiResponse {
            data: Some(first_uri),
            code: 200,
            msg,
        }),
    ))
}

fn resolve_target(state: &AppState, content_type: Option<&str>) -> SaveTarget {
    match content_type {
        Some(v) if v.starts_with("image/") => SaveTarget {
            kind: "picture".to_string(),
            path: state.config.get_picture_path(),
            uri_prefix: "/files/picture",
        },
        Some(v) if v.starts_with("video/") => SaveTarget {
            kind: "media".to_string(),
            path: state.config.get_media_path(),
            uri_prefix: "/files/media",
        },
        _ => SaveTarget {
            kind: "file".to_string(),
            path: state.config.get_file_path(),
            uri_prefix: "/files/file",
        },
    }
}

async fn find_existing_uri(
    state: &AppState,
    kind: &str,
    oid: &str,
) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query_as::<_, FileBlobRow>(
        "select uri from file_blob where kind = ? and algo = 'sha256' and oid = ? limit 1",
    )
    .bind(kind)
    .bind(oid)
    .fetch_optional(&state.db)
    .await?;
    Ok(row.map(|v| v.uri))
}

fn sanitize_file_name(name: &str) -> String {
    let normalized = name.replace('\\', "/");
    let base = normalized
        .split('/')
        .last()
        .unwrap_or("")
        .replace("..", "")
        .trim()
        .to_string();
    base.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn unique_file_name(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }

    let (base, ext) = match name.rsplit_once('.') {
        Some((b, e)) if !b.is_empty() && !e.is_empty() => (b.to_string(), format!(".{}", e)),
        _ => (name.to_string(), String::new()),
    };

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = FILE_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}{}", base, ts, seq, ext)
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
