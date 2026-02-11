use crate::util;
use serde::Deserialize;
use std::fs;
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
fn default_index_path() -> String {
    "dist/index.html".to_string()
}
fn default_static_path() -> String {
    "dist/static".to_string()
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
    #[serde(default = "default_index_path")]
    pub index_path: String,
    #[serde(default = "default_static_path")]
    pub static_path: String,
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
    pub fn get_db_path(&self) -> String {
        (self.base_path.clone() + "/" + self.db_path.as_str()).replace("//", "/")
    }

    pub fn get_index_path(&self) -> String {
        self.index_path.clone()
    }

    pub fn get_static_path(&self) -> String {
        self.static_path.clone()
    }
}
