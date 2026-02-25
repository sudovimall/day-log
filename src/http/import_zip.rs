use crate::app_state::AppState;
use crate::http::resp::{ApiCode, ApiResponse, ApiResult};
use crate::http::settings;
use crate::http::settings::DatePlaceholders;
use axum::extract::{Multipart, State};
use serde::Serialize;
use std::collections::HashSet;
use std::io::{Cursor, Read};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task;
use tracing::{info, warn};
use zip::ZipArchive;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportJournalResp {
    pub total_markdown_files: usize,
    pub matched_files: usize,
    pub imported_count: usize,
    pub skipped_count: usize,
    pub skipped_paths: Vec<String>,
    pub skipped_details: Vec<SkipDetail>,
    pub patterns: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SkipDetail {
    pub path: String,
    pub reason: String,
}

#[derive(Debug)]
struct ParsedEntry {
    path: String,
    date: String,
    content: String,
}

#[derive(Debug)]
struct ParseZipResult {
    total_markdown_files: usize,
    matched_files: usize,
    entries: Vec<ParsedEntry>,
    skipped_details: Vec<SkipDetail>,
}

pub async fn import_journal_zip(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<ImportJournalResp> {
    let mut zip_file: Option<Vec<u8>> = None;
    let mut patterns_raw: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|_| {
        ApiResponse::<ImportJournalResp>::err(ApiCode::BadRequest, "invalid multipart data")
    })? {
        let name = field.name().unwrap_or("").to_string();

        if name == "file" || field.file_name().is_some() {
            zip_file = Some(
                field
                    .bytes()
                    .await
                    .map_err(|_| {
                        ApiResponse::<ImportJournalResp>::err(
                            ApiCode::BadRequest,
                            "read zip file failed",
                        )
                    })?
                    .to_vec(),
            );
        } else if name == "patterns" {
            patterns_raw = Some(field.text().await.map_err(|_| {
                ApiResponse::<ImportJournalResp>::err(ApiCode::BadRequest, "read patterns failed")
            })?);
        }
    }

    let zip_file = zip_file.ok_or_else(|| {
        ApiResponse::<ImportJournalResp>::err(ApiCode::FileMissing, "zip file required")
    })?;

    let date_placeholders = settings::load_date_placeholders(&state)
        .await
        .unwrap_or_else(settings::default_date_placeholders);
    let default_patterns = settings::load_import_patterns(&state)
        .await
        .unwrap_or_else(|| settings::default_import_patterns_by(&date_placeholders));
    let patterns = normalize_patterns(
        patterns_raw.as_deref(),
        default_patterns,
        &date_placeholders,
    )
    .map_err(|msg| ApiResponse::<ImportJournalResp>::err(ApiCode::BadRequest, &msg))?;

    let patterns_for_parse = patterns.clone();
    let placeholders_for_parse = date_placeholders.clone();
    let parse_result = task::spawn_blocking(move || {
        parse_zip(zip_file, &patterns_for_parse, &placeholders_for_parse)
    })
    .await
    .map_err(|_| {
        ApiResponse::<ImportJournalResp>::err(ApiCode::BadRequest, "parse zip task failed")
    })?
    .map_err(|msg| ApiResponse::<ImportJournalResp>::err(ApiCode::BadRequest, &msg))?;

    let mut imported_count = 0usize;
    let mut skipped_details = parse_result.skipped_details;
    let ts = now_ts();

    for entry in parse_result.entries {
        let exist_id =
            sqlx::query_scalar::<_, i64>("select id from journal where date = ? limit 1")
                .bind(&entry.date)
                .fetch_optional(&state.db)
                .await;

        let result = match exist_id {
            Ok(Some(id)) => {
                sqlx::query("update journal set content = ?, update_time = ? where id = ?")
                    .bind(entry.content)
                    .bind(ts)
                    .bind(id)
                    .execute(&state.db)
                    .await
            }
            Ok(None) => sqlx::query(
                "insert into journal (content, date, create_time, update_time) values (?, ?, ?, ?)",
            )
            .bind(entry.content)
            .bind(entry.date)
            .bind(ts)
            .bind(ts)
            .execute(&state.db)
            .await,
            Err(e) => Err(e),
        };

        if result.is_ok() {
            imported_count += 1;
        } else {
            let detail = SkipDetail {
                path: entry.path,
                reason: "db insert failed".to_string(),
            };
            warn!("zip import skipped: {} => {}", detail.path, detail.reason);
            skipped_details.push(detail);
        }
    }

    let skipped_paths = skipped_details
        .iter()
        .map(|v| format!("{} ({})", v.path, v.reason))
        .collect::<Vec<_>>();

    let resp = ImportJournalResp {
        total_markdown_files: parse_result.total_markdown_files,
        matched_files: parse_result.matched_files,
        imported_count,
        skipped_count: skipped_details.len(),
        skipped_paths,
        skipped_details,
        patterns,
    };

    info!(
        "导入日记完成 total_md={}, matched={}, imported={}, skipped={}",
        resp.total_markdown_files, resp.matched_files, resp.imported_count, resp.skipped_count
    );
    for detail in &resp.skipped_details {
        info!("导入跳过 path='{}' reason='{}'", detail.path, detail.reason);
    }
    Ok(ApiResponse::ok(resp))
}

fn normalize_patterns(
    input: Option<&str>,
    default_patterns: Vec<String>,
    placeholders: &DatePlaceholders,
) -> Result<Vec<String>, String> {
    let mut patterns = if let Some(raw) = input {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            default_patterns
        } else if let Ok(arr) = serde_json::from_str::<Vec<String>>(trimmed) {
            arr
        } else {
            trimmed
                .split(|c| c == '\n' || c == ',' || c == ';')
                .map(|v| v.trim().to_string())
                .collect()
        }
    } else {
        default_patterns
    };

    patterns.retain(|v| !v.trim().is_empty());
    let mut uniq = HashSet::new();
    patterns.retain(|v| uniq.insert(v.clone()));

    if patterns.is_empty() {
        return Err("patterns required".to_string());
    }

    for p in &patterns {
        validate_pattern(p, placeholders)?;
    }

    Ok(patterns)
}

fn validate_pattern(pattern: &str, placeholders: &DatePlaceholders) -> Result<(), String> {
    let has_year = pattern.contains(&placeholders.yyyy);
    let has_month = pattern.contains(&placeholders.mm) || pattern.contains(&placeholders.m);
    let has_day = pattern.contains(&placeholders.dd) || pattern.contains(&placeholders.d);
    let has_ymd = has_year && has_month && has_day;
    let has_date = pattern.contains(&placeholders.date);
    if !has_ymd && !has_date {
        return Err(format!(
            "invalid pattern '{}' , required placeholders: {}+{}|{}+{}|{} or {}",
            pattern,
            placeholders.yyyy,
            placeholders.mm,
            placeholders.m,
            placeholders.dd,
            placeholders.d,
            placeholders.date
        ));
    }
    Ok(())
}

fn parse_zip(
    zip_file: Vec<u8>,
    patterns: &[String],
    placeholders: &DatePlaceholders,
) -> Result<ParseZipResult, String> {
    let mut archive =
        ZipArchive::new(Cursor::new(zip_file)).map_err(|_| "invalid zip file".to_string())?;

    let mut entries = Vec::new();
    let mut skipped_details = Vec::new();
    let mut total_markdown_files = 0usize;

    for idx in 0..archive.len() {
        let mut file = archive
            .by_index(idx)
            .map_err(|_| "read zip entry failed".to_string())?;
        if !file.is_file() {
            continue;
        }

        let path = file.name().replace('\\', "/");
        if !path.to_ascii_lowercase().ends_with(".md") {
            continue;
        }
        total_markdown_files += 1;

        let date = match extract_date_from_path(&path, patterns, placeholders) {
            Ok(v) => v,
            Err(reason) => {
                let detail = SkipDetail {
                    path: path.clone(),
                    reason,
                };
                warn!("zip import skipped: {} => {}", detail.path, detail.reason);
                skipped_details.push(detail);
                continue;
            }
        };

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|_| "read markdown content failed".to_string())?;
        let content = String::from_utf8_lossy(&buf).to_string();

        entries.push(ParsedEntry {
            path,
            date,
            content,
        });
    }

    Ok(ParseZipResult {
        total_markdown_files,
        matched_files: entries.len(),
        entries,
        skipped_details,
    })
}

fn extract_date_from_path(
    path: &str,
    patterns: &[String],
    placeholders: &DatePlaceholders,
) -> Result<String, String> {
    let mut reasons = Vec::new();
    for pattern in patterns {
        match match_path_with_pattern(path, pattern, placeholders) {
            Ok(date) => return Ok(date),
            Err(reason) => reasons.push(format!("[{}] {}", pattern, reason)),
        }
    }
    Err(format!("path not match patterns: {}", reasons.join(" | ")))
}

fn match_path_with_pattern(
    path: &str,
    pattern: &str,
    placeholders: &DatePlaceholders,
) -> Result<String, String> {
    let path_tokens: Vec<&str> = path.split('/').collect();
    let pattern_tokens: Vec<&str> = pattern.split('/').collect();

    if path_tokens.len() < pattern_tokens.len() {
        return Err(format!(
            "path segment count too short (path={}, pattern={})",
            path_tokens.len(),
            pattern_tokens.len()
        ));
    }
    let start = path_tokens.len() - pattern_tokens.len();
    let tail_tokens = &path_tokens[start..];

    let mut yyyy: Option<String> = None;
    let mut mm: Option<String> = None;
    let mut dd: Option<String> = None;

    for (actual, template) in tail_tokens.iter().zip(pattern_tokens.iter()) {
        capture_component(actual, template, placeholders, &mut yyyy, &mut mm, &mut dd)?;
    }

    let yyyy = yyyy.ok_or_else(|| "missing yyyy from path".to_string())?;
    let mm = mm.ok_or_else(|| "missing month from path".to_string())?;
    let dd = dd.ok_or_else(|| "missing day from path".to_string())?;

    if !valid_date_parts(&yyyy, &mm, &dd) {
        return Err(format!("invalid date parts: {}-{}-{}", yyyy, mm, dd));
    }

    Ok(format!("{}-{}-{}", yyyy, mm, dd))
}

fn capture_component(
    actual: &str,
    template: &str,
    placeholders: &DatePlaceholders,
    yyyy: &mut Option<String>,
    mm: &mut Option<String>,
    dd: &mut Option<String>,
) -> Result<(), String> {
    let mut i = 0usize;
    let mut j = 0usize;
    let t = template.as_bytes();
    let a = actual.as_bytes();

    while i < t.len() {
        if t[i] == b'{' {
            let end = match template[i..].find('}') {
                Some(v) => i + v,
                None => return Err(format!("invalid template component: {}", template)),
            };
            let key = &template[i + 1..end];
            i = end + 1;

            let next_literal = template[i..].chars().next();
            let value_end = if let Some(ch) = next_literal {
                match actual[j..].find(ch) {
                    Some(pos) => j + pos,
                    None => {
                        return Err(format!(
                            "missing literal '{}' after placeholder {{{}}} in '{}'",
                            ch, key, actual
                        ));
                    }
                }
            } else {
                actual.len()
            };
            if value_end < j {
                return Err("invalid placeholder range".to_string());
            }
            let val = &actual[j..value_end];
            j = value_end;

            assign_placeholder(key, val, placeholders, yyyy, mm, dd)
                .map_err(|e| format!("placeholder {{{}}} parse failed: {}", key, e))?;
        } else {
            if j >= a.len() || t[i] != a[j] {
                return Err(format!(
                    "literal mismatch at '{}' expect '{}'",
                    actual, t[i] as char
                ));
            }
            i += 1;
            j += 1;
        }
    }

    if j == a.len() {
        Ok(())
    } else {
        Err(format!("component length mismatch: '{}'", actual))
    }
}

fn assign_placeholder(
    key: &str,
    val: &str,
    placeholders: &DatePlaceholders,
    yyyy: &mut Option<String>,
    mm: &mut Option<String>,
    dd: &mut Option<String>,
) -> Result<(), String> {
    if val.chars().any(|c| !c.is_ascii_digit()) {
        return Err(format!("value '{}' contains non-digit", val));
    }

    let yyyy_key = placeholder_key(&placeholders.yyyy)?;
    let mm_key = placeholder_key(&placeholders.mm)?;
    let m_key = placeholder_key(&placeholders.m)?;
    let dd_key = placeholder_key(&placeholders.dd)?;
    let d_key = placeholder_key(&placeholders.d)?;
    let date_key = placeholder_key(&placeholders.date)?;

    match key {
        _ if key == yyyy_key => {
            if val.len() != 4 {
                return Err("yyyy must be 4 digits".to_string());
            }
            *yyyy = Some(val.to_string());
            Ok(())
        }
        _ if key == mm_key => {
            let Some(m) = normalize_month_or_day(val, 1, 12) else {
                return Err("month out of range (1..12)".to_string());
            };
            if merge_or_check(mm, m) {
                Ok(())
            } else {
                Err("month conflict with another placeholder".to_string())
            }
        }
        _ if key == m_key => {
            let Some(m) = normalize_month_or_day(val, 1, 12) else {
                return Err("month out of range (1..12)".to_string());
            };
            if merge_or_check(mm, m) {
                Ok(())
            } else {
                Err("month conflict with another placeholder".to_string())
            }
        }
        _ if key == dd_key => {
            let Some(d) = normalize_month_or_day(val, 1, 31) else {
                return Err("day out of range (1..31)".to_string());
            };
            if merge_or_check(dd, d) {
                Ok(())
            } else {
                Err("day conflict with another placeholder".to_string())
            }
        }
        _ if key == d_key => {
            let Some(d) = normalize_month_or_day(val, 1, 31) else {
                return Err("day out of range (1..31)".to_string());
            };
            if merge_or_check(dd, d) {
                Ok(())
            } else {
                Err("day conflict with another placeholder".to_string())
            }
        }
        _ if key == date_key => {
            let Some((py, pm, pd)) = parse_date_value(val) else {
                return Err("unsupported date format".to_string());
            };
            if !merge_or_check(yyyy, py) || !merge_or_check(mm, pm) || !merge_or_check(dd, pd) {
                return Err("date conflicts with yyyy/MM/dd placeholders".to_string());
            }
            Ok(())
        }
        _ => Err(format!("unsupported placeholder: {}", key)),
    }
}

fn placeholder_key(token: &str) -> Result<&str, String> {
    if !(token.starts_with('{') && token.ends_with('}') && token.len() >= 3) {
        return Err(format!("invalid placeholder token '{}'", token));
    }
    Ok(&token[1..token.len() - 1])
}

fn merge_or_check(slot: &mut Option<String>, value: String) -> bool {
    if let Some(existing) = slot.as_ref() {
        existing == &value
    } else {
        *slot = Some(value);
        true
    }
}

fn parse_date_value(v: &str) -> Option<(String, String, String)> {
    let s = v.trim();
    if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        let y = &s[0..4];
        let m = &s[4..6];
        let d = &s[6..8];
        if valid_date_parts(y, m, d) {
            return Some((y.to_string(), m.to_string(), d.to_string()));
        }
        return None;
    }

    for sep in ['-', '_', '.'] {
        let parts: Vec<&str> = s.split(sep).collect();
        if parts.len() != 3 {
            continue;
        }
        let y = parts[0];
        let m = parts[1];
        let d = parts[2];
        let normalized_m = normalize_month_or_day(m, 1, 12);
        let normalized_d = normalize_month_or_day(d, 1, 31);
        if y.len() == 4
            && y.chars().all(|c| c.is_ascii_digit())
            && normalized_m.is_some()
            && normalized_d.is_some()
        {
            let mm = normalized_m.unwrap();
            let dd = normalized_d.unwrap();
            if valid_date_parts(y, &mm, &dd) {
                return Some((y.to_string(), mm, dd));
            }
        }
    }

    None
}

fn normalize_month_or_day(v: &str, min: u32, max: u32) -> Option<String> {
    if v.is_empty() || v.len() > 2 || v.chars().any(|c| !c.is_ascii_digit()) {
        return None;
    }
    let n = v.parse::<u32>().ok()?;
    if n < min || n > max {
        return None;
    }
    Some(format!("{:02}", n))
}

fn valid_date_parts(yyyy: &str, mm: &str, dd: &str) -> bool {
    let year = yyyy.parse::<i32>().ok();
    let month = mm.parse::<u32>().ok();
    let day = dd.parse::<u32>().ok();

    if year.is_none() || month.is_none() || day.is_none() {
        return false;
    }

    let month = month.unwrap();
    let day = day.unwrap();
    month >= 1 && month <= 12 && day >= 1 && day <= 31
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
