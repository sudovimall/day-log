use sha2::{Digest, Sha256};
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

pub async fn create_file(
    path: impl AsRef<Path>,
    content: impl AsRef<[u8]>,
) -> Result<(), io::Error> {
    let (file_path, _) = ensure_file_path(path).await?;
    fs::write(file_path, content).await?;
    Ok(())
}

pub fn file_hash(bytes: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    Digest::update(&mut hasher, &bytes);
    let result = hasher.finalize();
    let result = format!("{:x}", result);
    result
}
