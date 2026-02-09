use std::io;
use std::path::{Path, PathBuf};
use tokio::fs;

/// 确保 `file_path` 的父目录存在，并确保文件存在
pub async fn ensure_file_path(file_path: impl AsRef<Path>) -> Result<(PathBuf, bool), io::Error> {
    let file_path = file_path.as_ref().to_path_buf();

    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file_path)
        .await
    {
        Ok(_) => Ok((file_path, true)),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok((file_path, false)),
        Err(e) => Err(e),
    }
}

pub async fn ensure_path(path: impl AsRef<Path>) -> Result<PathBuf, io::Error> {
    let path = path.as_ref().to_path_buf();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    fs::create_dir_all(&path).await?;
    Ok(path)
}
