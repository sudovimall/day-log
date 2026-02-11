use crate::app_state::AppState;
use crate::http::resp::{ApiCode, ApiResponse, ApiResult};
use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;
#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Journal {
    pub id: i64,
    pub content: String,
    pub date: String,
    pub create_time: i64,
    pub update_time: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateJournalReq {
    pub content: String,
    pub date: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateJournalReq {
    pub content: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub date: Option<String>,
    pub page: Option<i64>,
    pub size: Option<i64>,
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

pub async fn create_journal(
    State(state): State<AppState>,
    Json(req): Json<CreateJournalReq>,
) -> ApiResult<Journal> {
    let ts = now_ts();
    let result = sqlx::query(
        "insert into journal (content, date, create_time, update_time) values (?, ?, ?, ?)",
    )
    .bind(req.content)
    .bind(req.date)
    .bind(ts)
    .bind(ts)
    .execute(&state.db)
    .await
    .map_err(|_| ApiResponse::<Journal>::err(ApiCode::DbInsertFailed, "db insert failed"))?;

    let id = result.last_insert_rowid();
    let journal = sqlx::query_as::<_, Journal>(
        "select id, content, date, create_time, update_time from journal where id = ?",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| ApiResponse::<Journal>::err(ApiCode::DbQueryFailed, "db query failed"))?;

    Ok(ApiResponse::ok(journal))
}

pub async fn list_journals(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Vec<Journal>> {
    let page = query.page.unwrap_or(1).clamp(1, 1000);
    let size = query.size.unwrap_or(10).clamp(1, 100);
    info!("获取日记 page: {}, size: {}", page, size);

    let journals = if let Some(date) = query.date {
        sqlx::query_as::<_, Journal>(
            "select id, content, date, create_time, update_time from journal where date = ? order by id desc limit ? offset ?",
        )
            .bind(date)
            .bind(size)
            .bind((page - 1) * size)
            .fetch_all(&state.db)
            .await
    } else {
        sqlx::query_as::<_, Journal>(
            "select id, content, date, create_time, update_time from journal order by id  limit ? offset ?",
        )
            .bind(size)
            .bind((page - 1) * size)
            .fetch_all(&state.db)
            .await
    }
        .map_err(|_| ApiResponse::<Vec<Journal>>::err(ApiCode::DbListFailed, "db query failed"))?;

    Ok(ApiResponse::ok(journals))
}

pub async fn get_journal(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Journal> {
    info!("获取日记 id: {}", id);
    let journal = sqlx::query_as::<_, Journal>(
        "select id, content, date, create_time, update_time from journal where id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| ApiResponse::<Journal>::err(ApiCode::DbGetFailed, "db query failed"))?;

    match journal {
        Some(journal) => Ok(ApiResponse::ok(journal)),
        None => Err(ApiResponse::err(ApiCode::NotFound, "not found")),
    }
}

pub async fn update_journal(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateJournalReq>,
) -> ApiResult<Journal> {
    if req.content.is_none() && req.date.is_none() {
        return Err(ApiResponse::<Journal>::err(
            ApiCode::BadRequest,
            "content or date required",
        ));
    }

    let ts = now_ts();
    let result = sqlx::query(
        "update journal set content = coalesce(?, content), date = coalesce(?, date), update_time = ? where id = ?",
    )
        .bind(req.content)
        .bind(req.date)
        .bind(ts)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| ApiResponse::<Journal>::err(ApiCode::DbUpdateFailed, "db update failed"))?;

    if result.rows_affected() == 0 {
        return Err(ApiResponse::<Journal>::err(ApiCode::NotFound, "not found"));
    }

    let journal = sqlx::query_as::<_, Journal>(
        "select id, content, date, create_time, update_time from journal where id = ?",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| ApiResponse::<Journal>::err(ApiCode::DbUpdateGetFailed, "db query failed"))?;

    Ok(ApiResponse::ok(journal))
}

pub async fn delete_journal(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<()> {
    let result = sqlx::query("delete from journal where id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| ApiResponse::<()>::err(ApiCode::DbDeleteFailed, "db delete failed"))?;

    if result.rows_affected() == 0 {
        return Err(ApiResponse::<()>::err(ApiCode::NotFound, "not found"));
    }

    Ok(ApiResponse::ok(()))
}
