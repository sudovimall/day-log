use crate::app_state::AppState;
use crate::config::app_config::SyncConfig;
use crate::http::resp::{ApiCode, ApiResponse, ApiResult};
use crate::http::settings;
use crate::http::settings::DatePlaceholders;
use axum::extract::State;
use git2::{
    BranchType, Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository, Signature,
    build::CheckoutBuilder, build::RepoBuilder,
};
use serde::Serialize;
use sqlx::FromRow;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task;
use tracing::{error, info, warn};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResp {
    pub pushed: bool,
    pub commit_id: String,
    pub file_path: String,
    pub format: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct JournalRow {
    id: i64,
    content: String,
    date: String,
    create_time: i64,
    update_time: i64,
}

#[derive(Clone)]
struct SyncTaskInput {
    cfg: SyncConfig,
    repo_path: PathBuf,
    output_files: Vec<SyncOutputFile>,
    commit_message: String,
}

struct SyncTaskOutput {
    pushed: bool,
    commit_id: String,
}

#[derive(Clone)]
struct SyncOutputFile {
    rel_path: PathBuf,
    content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthMode {
    Password,
    Ssh,
}

#[derive(Debug)]
struct StartupImportEntry {
    path: String,
    date: String,
    content: String,
}

#[derive(Debug)]
struct StartupImportScanResult {
    total_markdown_files: usize,
    matched_files: usize,
    skipped_count: usize,
}

#[derive(Debug)]
struct StartupImportParseResult {
    total_markdown_files: usize,
    matched_files: usize,
    skipped_count: usize,
    entries: Vec<StartupImportEntry>,
}

pub async fn startup_sync_to_db(state: &AppState) -> Result<(), String> {
    let cfg = state.config.sync.clone();
    if !cfg.enabled {
        info!("startup sync skipped: sync.enabled=false");
        return Ok(());
    }
    if cfg.repo_url.trim().is_empty() {
        info!("startup sync skipped: sync.repo_url is empty");
        return Ok(());
    }
    let auth_mode = resolve_auth_mode(&cfg)?;
    validate_auth_config(&cfg, auth_mode)?;

    let date_placeholders = settings::default_date_placeholders();
    let mut patterns = cfg
        .import_patterns
        .iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>();
    if patterns.is_empty() {
        patterns.push(cfg.output_path.clone());
    }
    for p in &patterns {
        validate_startup_import_pattern(p, &date_placeholders)?;
    }

    let repo_path = PathBuf::from(state.config.get_sync_repo_path());
    let cfg_for_task = cfg.clone();
    let repo_path_for_task = repo_path.clone();
    task::spawn_blocking(move || prepare_repo_for_import(&cfg_for_task, &repo_path_for_task))
        .await
        .map_err(|_| "startup sync task join failed".to_string())??;

    let patterns_for_task = patterns.clone();
    let placeholders_for_task = date_placeholders.clone();
    let repo_path_for_scan = repo_path.clone();
    let parse_result = task::spawn_blocking(move || {
        scan_repo_markdown_entries(
            repo_path_for_scan.as_path(),
            &patterns_for_task,
            &placeholders_for_task,
        )
    })
    .await
    .map_err(|_| "startup import scan task join failed".to_string())??;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut imported_count = 0usize;
    for entry in parse_result.entries {
        let exist_id =
            sqlx::query_scalar::<_, i64>("select id from journal where date = ? limit 1")
                .bind(&entry.date)
                .fetch_optional(&state.db)
                .await
                .map_err(|_| "startup import query failed".to_string())?;

        let result = match exist_id {
            Some(id) => {
                sqlx::query("update journal set content = ?, update_time = ? where id = ?")
                    .bind(&entry.content)
                    .bind(ts)
                    .bind(id)
                    .execute(&state.db)
                    .await
            }
            None => sqlx::query(
                "insert into journal (content, date, create_time, update_time) values (?, ?, ?, ?)",
            )
            .bind(&entry.content)
            .bind(&entry.date)
            .bind(ts)
            .bind(ts)
            .execute(&state.db)
            .await,
        };

        if result.is_ok() {
            imported_count += 1;
        } else {
            warn!(
                "startup import failed to upsert journal: date={}, path={}",
                entry.date, entry.path
            );
        }
    }

    let summary = StartupImportScanResult {
        total_markdown_files: parse_result.total_markdown_files,
        matched_files: parse_result.matched_files,
        skipped_count: parse_result.skipped_count,
    };
    info!(
        "startup sync done: total_md={}, matched={}, imported={}, skipped={}, repo_path={}",
        summary.total_markdown_files,
        summary.matched_files,
        imported_count,
        summary.skipped_count,
        repo_path.display()
    );
    Ok(())
}

fn prepare_repo_for_import(cfg: &SyncConfig, repo_path: &Path) -> Result<(), String> {
    if let Some(parent) = repo_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let repo = if repo_path.join(".git").exists() {
        Repository::open(repo_path).map_err(|e| e.message().to_string())?
    } else {
        clone_repo(cfg, repo_path)?
    };
    checkout_and_fast_forward(&repo, cfg)
}

fn scan_repo_markdown_entries(
    repo_root: &Path,
    patterns: &[String],
    placeholders: &DatePlaceholders,
) -> Result<StartupImportParseResult, String> {
    let mut markdown_files = Vec::new();
    collect_markdown_files(repo_root, repo_root, &mut markdown_files)?;

    let mut entries = Vec::new();
    let mut skipped_count = 0usize;
    let mut dates = HashSet::new();

    for rel_path in markdown_files {
        let rel = rel_path.to_string_lossy().replace('\\', "/");
        let date = match extract_date_from_path(&rel, patterns, placeholders) {
            Ok(v) => v,
            Err(reason) => {
                skipped_count += 1;
                warn!("startup import skip path={} reason={}", rel, reason);
                continue;
            }
        };

        if !dates.insert(date.clone()) {
            skipped_count += 1;
            warn!("startup import skip duplicate date={} path={}", date, rel);
            continue;
        }

        let full_path = repo_root.join(&rel_path);
        let content = fs::read_to_string(&full_path)
            .map_err(|e| format!("read markdown failed: {} ({})", full_path.display(), e))?;
        entries.push(StartupImportEntry {
            path: rel,
            date,
            content,
        });
    }

    Ok(StartupImportParseResult {
        total_markdown_files: entries.len() + skipped_count,
        matched_files: entries.len(),
        skipped_count,
        entries,
    })
}

fn collect_markdown_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let rd = fs::read_dir(current)
        .map_err(|e| format!("read dir failed: {} ({})", current.display(), e))?;
    for item in rd {
        let item = item.map_err(|e| e.to_string())?;
        let path = item.path();
        if path.is_dir() {
            if path.file_name().and_then(|v| v.to_str()) == Some(".git") {
                continue;
            }
            collect_markdown_files(root, &path, out)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let is_md = path
            .extension()
            .and_then(|v| v.to_str())
            .map(|v| v.eq_ignore_ascii_case("md"))
            .unwrap_or(false);
        if !is_md {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|_| format!("strip prefix failed: {}", path.display()))?;
        out.push(rel.to_path_buf());
    }
    Ok(())
}

fn validate_startup_import_pattern(
    pattern: &str,
    placeholders: &DatePlaceholders,
) -> Result<(), String> {
    let has_year = pattern.contains(&placeholders.yyyy);
    let has_month = pattern.contains(&placeholders.mm) || pattern.contains(&placeholders.m);
    let has_day = pattern.contains(&placeholders.dd) || pattern.contains(&placeholders.d);
    let has_ymd = has_year && has_month && has_day;
    let has_date = pattern.contains(&placeholders.date);
    if !has_ymd && !has_date {
        return Err(format!(
            "invalid import pattern '{}' , required placeholders: {}+{}|{}+{}|{} or {}",
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
        _ if key == mm_key || key == m_key => {
            let Some(m) = normalize_month_or_day(val, 1, 12) else {
                return Err("month out of range (1..12)".to_string());
            };
            if merge_or_check(mm, m) {
                Ok(())
            } else {
                Err("month conflict with another placeholder".to_string())
            }
        }
        _ if key == dd_key || key == d_key => {
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

pub async fn sync_journal(State(state): State<AppState>) -> ApiResult<SyncResp> {
    let cfg = state.config.sync.clone();
    let sync_output_path = settings::load_sync_output_path(&state)
        .await
        .unwrap_or_else(|| cfg.output_path.clone());
    let sync_commit_template = settings::load_sync_commit_message(&state)
        .await
        .unwrap_or_else(|| cfg.commit_message.clone());
    let date_placeholders = settings::load_date_placeholders(&state)
        .await
        .unwrap_or_else(settings::default_date_placeholders);
    info!(
        "journal sync start: enabled={}, branch={}, output_path={}, format={}",
        cfg.enabled, cfg.branch, sync_output_path, cfg.output_format
    );
    if !cfg.enabled {
        info!("journal sync skipped: disabled in config");
        return Err(ApiResponse::<SyncResp>::err(
            ApiCode::BadRequest,
            "sync disabled in config",
        ));
    }
    if cfg.repo_url.trim().is_empty() {
        return Err(ApiResponse::<SyncResp>::err(
            ApiCode::BadRequest,
            "sync.repo_url is required",
        ));
    }
    let auth_mode = resolve_auth_mode(&cfg)
        .map_err(|msg| ApiResponse::<SyncResp>::err(ApiCode::BadRequest, &msg))?;
    validate_auth_config(&cfg, auth_mode)
        .map_err(|msg| ApiResponse::<SyncResp>::err(ApiCode::BadRequest, &msg))?;

    let journals = sqlx::query_as::<_, JournalRow>(
        "select id, content, date, create_time, update_time from journal order by date asc, id asc",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| ApiResponse::<SyncResp>::err(ApiCode::DbListFailed, "db query failed"))?;
    info!("journal sync query done: rows={}", journals.len());

    let output_format = normalize_format(&cfg.output_format).map_err(|msg| {
        ApiResponse::<SyncResp>::err(
            ApiCode::BadRequest,
            &format!("invalid output_format: {}", msg),
        )
    })?;
    let output_files = build_output_files(
        &sync_output_path,
        &output_format,
        &journals,
        &date_placeholders,
    )
    .map_err(|msg| ApiResponse::<SyncResp>::err(ApiCode::BadRequest, &msg))?;
    let commit_message =
        resolve_commit_message(&sync_commit_template, journals.len(), &date_placeholders);
    let repo_path = PathBuf::from(state.config.get_sync_repo_path());
    info!(
        "journal sync prepared: repo_path={}, output_files={}, commit_message={}",
        repo_path.display(),
        output_files.len(),
        commit_message
    );

    let task_input = SyncTaskInput {
        cfg: cfg.clone(),
        repo_path,
        output_files,
        commit_message,
    };

    let task_result = task::spawn_blocking(move || execute_sync(task_input))
        .await
        .map_err(|_| {
            error!("journal sync failed: sync task join failed");
            ApiResponse::<SyncResp>::err(ApiCode::SyncFailed, "sync task join failed")
        })?;

    let result = task_result.map_err(|msg| {
        error!("journal sync failed: {}", msg);
        ApiResponse::<SyncResp>::err(ApiCode::SyncFailed, &msg)
    })?;

    let resp = SyncResp {
        pushed: result.pushed,
        commit_id: result.commit_id,
        file_path: sync_output_path,
        format: output_format,
        message: if result.pushed {
            "sync success".to_string()
        } else {
            "no changes to push".to_string()
        },
    };
    info!(
        "journal sync result: pushed={}, path={}",
        resp.pushed, resp.file_path
    );
    Ok(ApiResponse::ok(resp))
}

fn validate_rel_path(input: &str) -> Result<PathBuf, String> {
    let p = Path::new(input.trim());
    if input.trim().is_empty() {
        return Err("path is empty".to_string());
    }
    if p.is_absolute() {
        return Err("absolute path is not allowed".to_string());
    }
    for c in p.components() {
        if matches!(c, Component::ParentDir) {
            return Err("parent dir is not allowed".to_string());
        }
    }
    Ok(p.to_path_buf())
}

fn normalize_format(s: &str) -> Result<String, String> {
    let v = s.trim().to_ascii_lowercase();
    match v.as_str() {
        "md" | "markdown" => Ok("markdown".to_string()),
        _ => Err("supported: markdown only".to_string()),
    }
}

fn render_journals(format: &str, journals: &[JournalRow]) -> Result<String, String> {
    match format {
        "markdown" => {
            let mut out = String::from("# DayLog Journals\n\n");
            for j in journals {
                out.push_str(&format!("## {}\n\n", j.date));
                out.push_str(&j.content);
                out.push_str("\n\n---\n\n");
            }
            Ok(out)
        }
        _ => Err("unsupported format".to_string()),
    }
}

fn render_single_markdown(j: &JournalRow) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", j.date));
    out.push_str(&j.content);
    out.push('\n');
    out
}

fn build_output_files(
    output_path: &str,
    format: &str,
    journals: &[JournalRow],
    placeholders: &DatePlaceholders,
) -> Result<Vec<SyncOutputFile>, String> {
    if format == "markdown" && contains_date_placeholder(output_path, placeholders) {
        let mut files = Vec::new();
        for j in journals {
            let path = resolve_output_path_template(output_path, &j.date, placeholders)?;
            let rel_path =
                validate_rel_path(&path).map_err(|e| format!("invalid output_path: {}", e))?;
            ensure_md_path(rel_path.as_path())?;
            files.push(SyncOutputFile {
                rel_path,
                content: render_single_markdown(j),
            });
        }
        if files.is_empty() {
            return Err("no journals to sync for markdown template output".to_string());
        }
        return Ok(files);
    }

    let rel_path =
        validate_rel_path(output_path).map_err(|e| format!("invalid output_path: {}", e))?;
    ensure_md_path(rel_path.as_path())?;
    let content = render_journals(format, journals)?;
    Ok(vec![SyncOutputFile { rel_path, content }])
}

fn ensure_md_path(path: &Path) -> Result<(), String> {
    let ok = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("md"))
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(format!("output path must end with .md: {}", path.display()))
    }
}

fn contains_date_placeholder(path: &str, placeholders: &DatePlaceholders) -> bool {
    [
        placeholders.yyyy.as_str(),
        placeholders.mm.as_str(),
        placeholders.m.as_str(),
        placeholders.dd.as_str(),
        placeholders.d.as_str(),
        placeholders.date.as_str(),
    ]
    .iter()
    .any(|k| path.contains(k))
}

fn resolve_output_path_template(
    template: &str,
    date: &str,
    placeholders: &DatePlaceholders,
) -> Result<String, String> {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("invalid journal date: {}", date));
    }
    let yyyy = parts[0];
    let mm = parts[1];
    let dd = parts[2];
    if yyyy.len() != 4
        || mm.len() != 2
        || dd.len() != 2
        || !yyyy.chars().all(|c| c.is_ascii_digit())
        || !mm.chars().all(|c| c.is_ascii_digit())
        || !dd.chars().all(|c| c.is_ascii_digit())
    {
        return Err(format!("invalid journal date: {}", date));
    }
    let m = mm
        .parse::<u32>()
        .map_err(|_| format!("invalid month: {}", mm))?;
    let d = dd
        .parse::<u32>()
        .map_err(|_| format!("invalid day: {}", dd))?;
    let mut out = template.to_string();
    out = out.replace(&placeholders.yyyy, yyyy);
    out = out.replace(&placeholders.mm, mm);
    out = out.replace(&placeholders.m, &m.to_string());
    out = out.replace(&placeholders.dd, dd);
    out = out.replace(&placeholders.d, &d.to_string());
    out = out.replace(&placeholders.date, date);
    Ok(out)
}

fn resolve_commit_message(template: &str, count: usize, placeholders: &DatePlaceholders) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (yyyy, mm, dd, m, d, date) = now_date_tokens();
    template
        .replace(&placeholders.timestamp, &ts.to_string())
        .replace(&placeholders.count, &count.to_string())
        .replace(&placeholders.yyyy, &yyyy)
        .replace(&placeholders.mm, &mm)
        .replace(&placeholders.m, &m)
        .replace(&placeholders.dd, &dd)
        .replace(&placeholders.d, &d)
        .replace(&placeholders.date, &date)
}

fn now_date_tokens() -> (String, String, String, String, String, String) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    const DAY: i64 = 86_400;
    let days = secs.div_euclid(DAY);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    let yyyy = year.to_string();
    let mm = format!("{:02}", m);
    let dd = format!("{:02}", d);
    let m_plain = m.to_string();
    let d_plain = d.to_string();
    let date = format!("{}-{}-{}", yyyy, mm, dd);
    (yyyy, mm, dd, m_plain, d_plain, date)
}

fn execute_sync(input: SyncTaskInput) -> Result<SyncTaskOutput, String> {
    info!(
        "execute sync: repo_path={}, branch={}, output_files={}",
        input.repo_path.display(),
        input.cfg.branch,
        input.output_files.len()
    );
    if let Some(parent) = input.repo_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let repo = if input.repo_path.join(".git").exists() {
        info!(
            "execute sync: opening existing repo {}",
            input.repo_path.display()
        );
        Repository::open(&input.repo_path).map_err(|e| e.message().to_string())?
    } else {
        info!(
            "execute sync: cloning repo {} -> {}",
            input.cfg.repo_url,
            input.repo_path.display()
        );
        clone_repo(&input.cfg, &input.repo_path)?
    };

    info!("execute sync: fetch + fast-forward branch");
    checkout_and_fast_forward(&repo, &input.cfg)?;

    for f in &input.output_files {
        let full_output_path = input.repo_path.join(&f.rel_path);
        if let Some(parent) = full_output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        info!(
            "execute sync: writing output file {}",
            full_output_path.display()
        );
        fs::write(&full_output_path, f.content.as_bytes()).map_err(|e| e.to_string())?;
    }

    let mut index = repo.index().map_err(|e| e.message().to_string())?;
    for f in &input.output_files {
        index
            .add_path(f.rel_path.as_path())
            .map_err(|e| e.message().to_string())?;
    }
    index.write().map_err(|e| e.message().to_string())?;

    let tree_id = index.write_tree().map_err(|e| e.message().to_string())?;
    let tree = repo
        .find_tree(tree_id)
        .map_err(|e| e.message().to_string())?;

    let mut parents = Vec::new();
    if let Ok(head) = repo.head() {
        let commit = head.peel_to_commit().map_err(|e| e.message().to_string())?;
        if commit.tree_id() == tree_id {
            info!("execute sync: no file changes detected, skip commit/push");
            return Ok(SyncTaskOutput {
                pushed: false,
                commit_id: "".to_string(),
            });
        }
        parents.push(commit);
    }

    let sig = Signature::now(&input.cfg.author_name, &input.cfg.author_email)
        .map_err(|e| e.message().to_string())?;
    let parent_refs = parents.iter().collect::<Vec<_>>();
    let commit_id = repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            &input.commit_message,
            &tree,
            &parent_refs,
        )
        .map_err(|e| e.message().to_string())?;
    info!("execute sync: commit created {}", commit_id);

    info!("execute sync: pushing branch {}", input.cfg.branch);
    push_branch(&repo, &input.cfg)?;
    info!("execute sync: push success");

    Ok(SyncTaskOutput {
        pushed: true,
        commit_id: commit_id.to_string(),
    })
}

fn clone_repo(cfg: &SyncConfig, repo_path: &Path) -> Result<Repository, String> {
    let auth_mode = resolve_auth_mode(cfg)?;
    let cb = remote_callbacks(cfg, auth_mode);
    let mut fetch = FetchOptions::new();
    fetch.remote_callbacks(cb);

    let mut builder = RepoBuilder::new();
    builder.fetch_options(fetch);
    builder.branch(cfg.branch.trim());
    builder
        .clone(cfg.repo_url.trim(), repo_path)
        .map_err(|e| e.message().to_string())
}

fn checkout_and_fast_forward(repo: &Repository, cfg: &SyncConfig) -> Result<(), String> {
    let branch_name = cfg.branch.trim();
    let remote_branch = format!("refs/remotes/origin/{}", branch_name);
    let local_branch = format!("refs/heads/{}", branch_name);

    let auth_mode = resolve_auth_mode(cfg)?;
    let cb = remote_callbacks(cfg, auth_mode);
    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(cb);

    let mut remote = repo
        .find_remote("origin")
        .map_err(|e| e.message().to_string())?;
    remote
        .fetch(&[branch_name], Some(&mut fetch_opts), None)
        .map_err(|e| e.message().to_string())?;

    let oid = repo
        .refname_to_id(&remote_branch)
        .map_err(|e| e.message().to_string())?;
    let target = repo.find_commit(oid).map_err(|e| e.message().to_string())?;

    if repo.find_branch(branch_name, BranchType::Local).is_err() {
        repo.branch(branch_name, &target, true)
            .map_err(|e| e.message().to_string())?;
    }

    let mut local_ref = repo
        .find_reference(&local_branch)
        .map_err(|e| e.message().to_string())?;
    local_ref
        .set_target(target.id(), "fast-forward")
        .map_err(|e| e.message().to_string())?;

    repo.set_head(&local_branch)
        .map_err(|e| e.message().to_string())?;
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout))
        .map_err(|e| e.message().to_string())?;
    Ok(())
}

fn push_branch(repo: &Repository, cfg: &SyncConfig) -> Result<(), String> {
    let auth_mode = resolve_auth_mode(cfg)?;
    let cb = remote_callbacks(cfg, auth_mode);
    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(cb);

    let mut remote = repo
        .find_remote("origin")
        .map_err(|e| e.message().to_string())?;
    let spec = format!("refs/heads/{0}:refs/heads/{0}", cfg.branch.trim());
    remote
        .push(&[&spec], Some(&mut push_opts))
        .map_err(|e| e.message().to_string())
}

fn remote_callbacks(cfg: &SyncConfig, auth_mode: AuthMode) -> RemoteCallbacks<'static> {
    let username = cfg.username.clone();
    let password = cfg.password.clone();
    let ssh_username = cfg.ssh_username.clone();
    let ssh_private_key = cfg.ssh_private_key_path.clone();
    let ssh_public_key = cfg.ssh_public_key_path.clone();
    let ssh_passphrase = cfg.ssh_passphrase.clone();
    let mut cb = RemoteCallbacks::new();
    cb.credentials(move |_url, user, _allowed| match auth_mode {
        AuthMode::Password => Cred::userpass_plaintext(&username, &password),
        AuthMode::Ssh => {
            let user_name = if !ssh_username.trim().is_empty() {
                ssh_username.as_str()
            } else {
                user.unwrap_or("git")
            };
            let private_key = expand_tilde_path(ssh_private_key.trim())?;
            let public_key = if ssh_public_key.trim().is_empty() {
                None
            } else {
                Some(expand_tilde_path(ssh_public_key.trim())?)
            };
            let passphrase = if ssh_passphrase.trim().is_empty() {
                None
            } else {
                Some(ssh_passphrase.as_str())
            };
            Cred::ssh_key(
                user_name,
                public_key.as_deref(),
                Path::new(&private_key),
                passphrase,
            )
        }
    });
    cb
}

fn resolve_auth_mode(cfg: &SyncConfig) -> Result<AuthMode, String> {
    let method = cfg.auth_method.trim().to_ascii_lowercase();
    match method.as_str() {
        "password" | "userpass" | "https" => Ok(AuthMode::Password),
        "ssh" => Ok(AuthMode::Ssh),
        "auto" | "" => {
            if looks_like_github_repo(&cfg.repo_url) {
                return Ok(AuthMode::Ssh);
            }
            if !cfg.username.trim().is_empty() && !cfg.password.trim().is_empty() {
                return Ok(AuthMode::Password);
            }
            if !cfg.ssh_private_key_path.trim().is_empty() {
                return Ok(AuthMode::Ssh);
            }
            Ok(AuthMode::Password)
        }
        _ => Err("sync.auth_method must be one of: auto, password, ssh".to_string()),
    }
}

fn validate_auth_config(cfg: &SyncConfig, mode: AuthMode) -> Result<(), String> {
    match mode {
        AuthMode::Password => {
            if cfg.username.trim().is_empty() || cfg.password.trim().is_empty() {
                if looks_like_github_repo(&cfg.repo_url) {
                    return Err(
                        "GitHub repo should use ssh auth. set sync.auth_method='ssh' and sync.ssh_private_key_path"
                            .to_string(),
                    );
                }
                return Err(
                    "sync.username and sync.password are required for password auth".to_string(),
                );
            }
            Ok(())
        }
        AuthMode::Ssh => {
            if cfg.ssh_private_key_path.trim().is_empty() {
                return Err("sync.ssh_private_key_path is required for ssh auth".to_string());
            }
            let key_path = expand_tilde_path(cfg.ssh_private_key_path.trim())
                .map_err(|e| e.message().to_string())?;
            if !Path::new(&key_path).exists() {
                return Err(format!("ssh private key not found: {}", key_path.display()));
            }
            Ok(())
        }
    }
}

fn looks_like_github_repo(repo_url: &str) -> bool {
    let lower = repo_url.trim().to_ascii_lowercase();
    lower.contains("github.com")
}

fn expand_tilde_path(input: &str) -> Result<PathBuf, git2::Error> {
    if !input.starts_with('~') {
        return Ok(PathBuf::from(input));
    }
    let home = env::var("HOME")
        .map_err(|_| git2::Error::from_str("HOME env is required when using ~ in ssh key path"))?;
    if input == "~" {
        return Ok(PathBuf::from(home));
    }
    if let Some(rest) = input.strip_prefix("~/") {
        return Ok(Path::new(&home).join(rest));
    }
    Err(git2::Error::from_str(
        "unsupported ~ path form, use ~/xxx for ssh key path",
    ))
}
