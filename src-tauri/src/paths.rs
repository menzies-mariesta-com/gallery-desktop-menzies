//! Menzies config and data directory helpers.

use serde::Serialize;
use std::fs;
use std::path::PathBuf;

const APP_FOLDER: &str = "com.mariesta.menzies.gallery-desktop-menzies";

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("home config directory unavailable")]
    NoConfigDir,
    #[error("home data directory unavailable")]
    NoDataDir,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub config_dir: String,
    pub data_dir: String,
}

fn config_dir() -> Result<PathBuf, PathError> {
    let base = dirs::config_dir().ok_or(PathError::NoConfigDir)?;
    Ok(base.join("menzies").join(APP_FOLDER))
}

fn data_dir() -> Result<PathBuf, PathError> {
    let base = dirs::data_local_dir().ok_or(PathError::NoDataDir)?;
    Ok(base.join("menzies").join(APP_FOLDER))
}

pub fn ensure_app_dirs() -> Result<AppPaths, PathError> {
    let config = config_dir()?;
    let data = data_dir()?;
    fs::create_dir_all(&config)?;
    fs::create_dir_all(&data)?;
    fs::create_dir_all(data.join("logs"))?;
    Ok(AppPaths {
        config_dir: config.to_string_lossy().into_owned(),
        data_dir: data.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn app_paths() -> Result<AppPaths, String> {
    ensure_app_dirs().map_err(|err| err.to_string())
}
