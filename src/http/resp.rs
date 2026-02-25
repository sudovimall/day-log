use axum::{Json, http::StatusCode};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T> {
    pub code: i32,
    pub msg: String,
    pub data: Option<T>,
}

#[derive(Debug, Clone, Copy)]
pub enum ApiCode {
    Ok = 200,
    BadRequest = 400,
    NotFound = 404,
    DbInsertFailed = 1001,
    DbQueryFailed = 1002,
    DbListFailed = 1003,
    DbGetFailed = 1004,
    DbUpdateFailed = 1005,
    DbUpdateGetFailed = 1006,
    DbDeleteFailed = 1007,
    FileMissing = 2001,
    FileWriteFailed = 2002,
    SyncFailed = 3001,
}

impl ApiCode {
    pub fn code(self) -> i32 {
        self as i32
    }
}

pub type ApiResult<T> =
    Result<(StatusCode, Json<ApiResponse<T>>), (StatusCode, Json<ApiResponse<T>>)>;

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> (StatusCode, Json<ApiResponse<T>>) {
        (
            StatusCode::OK,
            Json(ApiResponse {
                code: ApiCode::Ok.code(),
                msg: "ok".to_string(),
                data: Some(data),
            }),
        )
    }

    pub fn err(code: ApiCode, msg: &str) -> (StatusCode, Json<ApiResponse<T>>) {
        (
            StatusCode::OK,
            Json(ApiResponse {
                code: code.code(),
                msg: msg.to_string(),
                data: None,
            }),
        )
    }
}
