use crate::app_state::AppState;
use crate::http::resp::{ApiCode, ApiResponse, ApiResult};
use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub const KEY_IMPORT_PATTERNS: &str = "import_patterns";
pub const KEY_SYNC_OUTPUT_PATH: &str = "sync_output_path";
pub const KEY_SYNC_COMMIT_MESSAGE: &str = "sync_commit_message";
pub const KEY_DATE_PLACEHOLDERS: &str = "date_placeholders";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatePlaceholders {
    pub yyyy: String,
    pub mm: String,
    pub m: String,
    pub dd: String,
    pub d: String,
    pub date: String,
    pub timestamp: String,
    pub count: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsResp {
    pub import_patterns: Vec<String>,
    pub sync_output_path: String,
    pub sync_commit_message: String,
    pub date_placeholders: DatePlaceholders,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsReq {
    pub import_patterns: Option<Vec<String>>,
    pub sync_output_path: Option<String>,
    pub sync_commit_message: Option<String>,
    pub date_placeholders: Option<DatePlaceholders>,
}

#[derive(Debug, FromRow)]
struct SettingRow {
    value: String,
}

pub async fn get_settings(State(state): State<AppState>) -> ApiResult<AppSettingsResp> {
    let date_placeholders = load_date_placeholders(&state)
        .await
        .unwrap_or_else(default_date_placeholders);
    let import_patterns = load_import_patterns(&state)
        .await
        .unwrap_or_else(|| default_import_patterns_by(&date_placeholders));
    let sync_output_path = load_sync_output_path(&state)
        .await
        .unwrap_or_else(|| state.config.sync.output_path.clone());
    let sync_commit_message = load_sync_commit_message(&state)
        .await
        .unwrap_or_else(|| state.config.sync.commit_message.clone());

    Ok(ApiResponse::ok(AppSettingsResp {
        import_patterns,
        sync_output_path,
        sync_commit_message,
        date_placeholders,
    }))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(req): Json<UpdateSettingsReq>,
) -> ApiResult<AppSettingsResp> {
    if let Some(placeholders) = req.date_placeholders {
        let normalized = normalize_date_placeholders(placeholders)
            .map_err(|msg| ApiResponse::<AppSettingsResp>::err(ApiCode::BadRequest, &msg))?;
        let value = serde_json::to_string(&normalized).map_err(|_| {
            ApiResponse::<AppSettingsResp>::err(ApiCode::BadRequest, "invalid datePlaceholders")
        })?;
        save_setting(&state, KEY_DATE_PLACEHOLDERS, &value)
            .await
            .map_err(|_| {
                ApiResponse::<AppSettingsResp>::err(ApiCode::DbUpdateFailed, "save settings failed")
            })?;
    }

    if let Some(patterns) = req.import_patterns {
        let mut cleaned = patterns
            .into_iter()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>();
        cleaned.sort();
        cleaned.dedup();
        if cleaned.is_empty() {
            return Err(ApiResponse::<AppSettingsResp>::err(
                ApiCode::BadRequest,
                "importPatterns cannot be empty",
            ));
        }
        let value = serde_json::to_string(&cleaned).map_err(|_| {
            ApiResponse::<AppSettingsResp>::err(ApiCode::BadRequest, "invalid importPatterns")
        })?;
        save_setting(&state, KEY_IMPORT_PATTERNS, &value)
            .await
            .map_err(|_| {
                ApiResponse::<AppSettingsResp>::err(ApiCode::DbUpdateFailed, "save settings failed")
            })?;
    }

    if let Some(path) = req.sync_output_path {
        let value = path.trim().to_string();
        if value.is_empty() {
            return Err(ApiResponse::<AppSettingsResp>::err(
                ApiCode::BadRequest,
                "syncOutputPath cannot be empty",
            ));
        }
        save_setting(&state, KEY_SYNC_OUTPUT_PATH, &value)
            .await
            .map_err(|_| {
                ApiResponse::<AppSettingsResp>::err(ApiCode::DbUpdateFailed, "save settings failed")
            })?;
    }
    if let Some(msg) = req.sync_commit_message {
        let value = msg.trim().to_string();
        if value.is_empty() {
            return Err(ApiResponse::<AppSettingsResp>::err(
                ApiCode::BadRequest,
                "syncCommitMessage cannot be empty",
            ));
        }
        save_setting(&state, KEY_SYNC_COMMIT_MESSAGE, &value)
            .await
            .map_err(|_| {
                ApiResponse::<AppSettingsResp>::err(ApiCode::DbUpdateFailed, "save settings failed")
            })?;
    }

    get_settings(State(state)).await
}

pub async fn load_sync_output_path(state: &AppState) -> Option<String> {
    load_setting(state, KEY_SYNC_OUTPUT_PATH).await
}
pub async fn load_sync_commit_message(state: &AppState) -> Option<String> {
    load_setting(state, KEY_SYNC_COMMIT_MESSAGE).await
}
pub async fn load_date_placeholders(state: &AppState) -> Option<DatePlaceholders> {
    let value = load_setting(state, KEY_DATE_PLACEHOLDERS).await?;
    let parsed = serde_json::from_str::<DatePlaceholders>(&value).ok()?;
    normalize_date_placeholders(parsed).ok()
}

pub async fn load_import_patterns(state: &AppState) -> Option<Vec<String>> {
    let value = load_setting(state, KEY_IMPORT_PATTERNS).await?;
    let arr = serde_json::from_str::<Vec<String>>(&value).ok()?;
    let cleaned = arr
        .into_iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

pub fn default_import_patterns_by(placeholders: &DatePlaceholders) -> Vec<String> {
    vec![
        format!(
            "{}/{}/{}.md",
            placeholders.yyyy, placeholders.mm, placeholders.dd
        ),
        format!(
            "{}/{}-{}.md",
            placeholders.yyyy, placeholders.mm, placeholders.dd
        ),
        format!(
            "{}-{}-{}.md",
            placeholders.yyyy, placeholders.mm, placeholders.dd
        ),
        format!(
            "{}_{}_{}.md",
            placeholders.yyyy, placeholders.mm, placeholders.dd
        ),
    ]
}

pub fn default_date_placeholders() -> DatePlaceholders {
    DatePlaceholders {
        yyyy: "{yyyy}".to_string(),
        mm: "{MM}".to_string(),
        m: "{M}".to_string(),
        dd: "{dd}".to_string(),
        d: "{d}".to_string(),
        date: "{date}".to_string(),
        timestamp: "{timestamp}".to_string(),
        count: "{count}".to_string(),
    }
}

fn normalize_date_placeholders(input: DatePlaceholders) -> Result<DatePlaceholders, String> {
    let normalized = DatePlaceholders {
        yyyy: input.yyyy.trim().to_string(),
        mm: input.mm.trim().to_string(),
        m: input.m.trim().to_string(),
        dd: input.dd.trim().to_string(),
        d: input.d.trim().to_string(),
        date: input.date.trim().to_string(),
        timestamp: input.timestamp.trim().to_string(),
        count: input.count.trim().to_string(),
    };

    let fields = [
        ("yyyy", normalized.yyyy.as_str()),
        ("MM", normalized.mm.as_str()),
        ("M", normalized.m.as_str()),
        ("dd", normalized.dd.as_str()),
        ("d", normalized.d.as_str()),
        ("date", normalized.date.as_str()),
        ("timestamp", normalized.timestamp.as_str()),
        ("count", normalized.count.as_str()),
    ];

    for (name, token) in fields {
        if token.is_empty() {
            return Err(format!("datePlaceholders.{} cannot be empty", name));
        }
        if !(token.starts_with('{') && token.ends_with('}') && token.len() >= 3) {
            return Err(format!(
                "datePlaceholders.{} must use brace format like {{xxx}}",
                name
            ));
        }
    }

    let mut uniq = HashSet::new();
    for (_, token) in fields {
        if !uniq.insert(token.to_string()) {
            return Err(format!("duplicate placeholder token '{}'", token));
        }
    }

    Ok(normalized)
}

async fn load_setting(state: &AppState, key: &str) -> Option<String> {
    let row =
        sqlx::query_as::<_, SettingRow>("select value from app_setting where key = ? limit 1")
            .bind(key)
            .fetch_optional(&state.db)
            .await
            .ok()?;
    row.map(|v| v.value)
}

async fn save_setting(state: &AppState, key: &str, value: &str) -> Result<(), sqlx::Error> {
    let ts = now_ts();
    sqlx::query(
        r#"
        insert into app_setting (key, value, update_time)
        values (?, ?, ?)
        on conflict(key) do update set value = excluded.value, update_time = excluded.update_time
        "#,
    )
    .bind(key)
    .bind(value)
    .bind(ts)
    .execute(&state.db)
    .await?;
    Ok(())
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
