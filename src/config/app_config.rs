use crate::util;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use tracing::error;

fn default_base_path() -> String {
    ".daylog".to_string()
}
fn default_port() -> u16 {
    9999
}
fn default_db_path() -> String {
    "db/daylog.sqlite".to_string()
}
fn default_picture_path() -> String {
    "picture".to_string()
}
fn default_media_path() -> String {
    "media".to_string()
}
fn default_file_path() -> String {
    "file".to_string()
}
fn default_index_path() -> String {
    "dist/index.html".to_string()
}
fn default_static_path() -> String {
    "dist/static".to_string()
}
fn default_upload_file_limit() -> usize {
    1024 * 1024 * 100
}
fn default_auto_switch_port_time() -> i16 {
    100
}
fn default_sync_enabled() -> bool {
    false
}
fn default_sync_repo_url() -> String {
    "".to_string()
}
fn default_sync_branch() -> String {
    "main".to_string()
}
fn default_sync_username() -> String {
    "".to_string()
}
fn default_sync_password() -> String {
    "".to_string()
}
fn default_sync_auth_method() -> String {
    "auto".to_string()
}
fn default_sync_ssh_username() -> String {
    "git".to_string()
}
fn default_sync_ssh_private_key_path() -> String {
    "".to_string()
}
fn default_sync_ssh_public_key_path() -> String {
    "".to_string()
}
fn default_sync_ssh_passphrase() -> String {
    "".to_string()
}
fn default_sync_author_name() -> String {
    "day-log-bot".to_string()
}
fn default_sync_author_email() -> String {
    "day-log-bot@example.com".to_string()
}
fn default_sync_commit_message() -> String {
    "sync journals {timestamp} count={count}".to_string()
}
fn default_sync_output_format() -> String {
    "markdown".to_string()
}
fn default_sync_output_path() -> String {
    "journals/{yyyy}/{MM}-{dd}/{d}.md".to_string()
}
fn default_sync_repo_local_path() -> String {
    "sync-repo".to_string()
}
fn default_sync_import_patterns() -> Vec<String> {
    Vec::new()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyncConfig {
    #[serde(default = "default_sync_enabled")]
    pub enabled: bool,
    #[serde(default = "default_sync_repo_url")]
    pub repo_url: String,
    #[serde(default = "default_sync_branch")]
    pub branch: String,
    #[serde(default = "default_sync_username")]
    pub username: String,
    #[serde(default = "default_sync_password")]
    pub password: String,
    #[serde(default = "default_sync_auth_method")]
    pub auth_method: String,
    #[serde(default = "default_sync_ssh_username")]
    pub ssh_username: String,
    #[serde(default = "default_sync_ssh_private_key_path")]
    pub ssh_private_key_path: String,
    #[serde(default = "default_sync_ssh_public_key_path")]
    pub ssh_public_key_path: String,
    #[serde(default = "default_sync_ssh_passphrase")]
    pub ssh_passphrase: String,
    #[serde(default = "default_sync_author_name")]
    pub author_name: String,
    #[serde(default = "default_sync_author_email")]
    pub author_email: String,
    #[serde(default = "default_sync_commit_message")]
    pub commit_message: String,
    #[serde(default = "default_sync_output_format")]
    pub output_format: String,
    #[serde(default = "default_sync_output_path")]
    pub output_path: String,
    #[serde(default = "default_sync_repo_local_path")]
    pub repo_local_path: String,
    #[serde(default = "default_sync_import_patterns")]
    pub import_patterns: Vec<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: default_sync_enabled(),
            repo_url: default_sync_repo_url(),
            branch: default_sync_branch(),
            username: default_sync_username(),
            password: default_sync_password(),
            auth_method: default_sync_auth_method(),
            ssh_username: default_sync_ssh_username(),
            ssh_private_key_path: default_sync_ssh_private_key_path(),
            ssh_public_key_path: default_sync_ssh_public_key_path(),
            ssh_passphrase: default_sync_ssh_passphrase(),
            author_name: default_sync_author_name(),
            author_email: default_sync_author_email(),
            commit_message: default_sync_commit_message(),
            output_format: default_sync_output_format(),
            output_path: default_sync_output_path(),
            repo_local_path: default_sync_repo_local_path(),
            import_patterns: default_sync_import_patterns(),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_base_path")]
    pub base_path: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default = "default_picture_path")]
    pub picture_path: String,
    #[serde(default = "default_media_path")]
    pub media_path: String,
    #[serde(default = "default_file_path")]
    pub file_path: String,
    #[serde(default = "default_index_path")]
    pub index_path: String,
    #[serde(default = "default_static_path")]
    pub static_path: String,
    #[serde(default = "default_upload_file_limit")]
    pub upload_file_limit: usize,
    #[serde(default = "default_auto_switch_port_time")]
    pub auto_switch_port_time: i16,
    #[serde(default)]
    pub sync: SyncConfig,
}

impl AppConfig {
    pub fn load_from_file(path: &str) -> Result<AppConfig, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(path)?;
        let config = toml::from_str::<AppConfig>(&contents)?;
        Ok(config)
    }

    pub async fn init(&self) {
        self.init_db().await;
        self.init_picture_dir().await;
        self.init_media_dir().await;
        self.init_file_dir().await;
    }
    async fn init_db(&self) {
        let mut path = self.base_path.clone() + "/" + self.db_path.as_str();
        path = path.replace("//", "/");

        if let Err(e) = util::file_util::ensure_file_path(&path).await {
            error!("{}", e)
        }
    }
    async fn init_picture_dir(&self) {
        let mut path = self.base_path.clone() + "/" + self.picture_path.as_str() + "/";
        path = path.replace("//", "/");
        if let Err(e) = util::file_util::ensure_path(&path).await {
            error!("{}", e)
        }
    }
    async fn init_media_dir(&self) {
        let mut path = self.base_path.clone() + "/" + self.media_path.as_str() + "/";
        path = path.replace("//", "/");
        if let Err(e) = util::file_util::ensure_path(&path).await {
            error!("{}", e)
        }
    }
    async fn init_file_dir(&self) {
        let mut path = self.base_path.clone() + "/" + self.file_path.as_str() + "/";
        path = path.replace("//", "/");
        if let Err(e) = util::file_util::ensure_path(&path).await {
            error!("{}", e)
        }
    }
    pub fn get_db_path(&self) -> String {
        (self.base_path.clone() + "/" + self.db_path.as_str()).replace("//", "/")
    }

    pub fn get_index_path(&self) -> String {
        self.index_path.clone()
    }

    pub fn get_static_path(&self) -> String {
        self.static_path.clone()
    }

    pub fn get_media_path(&self) -> String {
        let path = self.base_path.clone() + "/" + self.media_path.as_str() + "/";
        path.replace("//", "/")
    }

    pub fn get_picture_path(&self) -> String {
        let path = self.base_path.clone() + "/" + self.picture_path.as_str() + "/";
        path.replace("//", "/")
    }

    pub fn get_file_path(&self) -> String {
        let path = self.base_path.clone() + "/" + self.file_path.as_str() + "/";
        path.replace("//", "/")
    }

    pub fn get_sync_repo_path(&self) -> String {
        let p = Path::new(&self.sync.repo_local_path);
        if p.is_absolute() {
            return p.to_string_lossy().to_string();
        }
        (self.base_path.clone() + "/" + self.sync.repo_local_path.as_str()).replace("//", "/")
    }
}
