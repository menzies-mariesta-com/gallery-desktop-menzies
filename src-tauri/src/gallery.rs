//! Gallery filesystem listing and settings under Menzies paths.

use crate::paths::{self, PathError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "svg"];
const SETTINGS_FILE: &str = "settings.json";
const MAX_RECENT: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum GalleryError {
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("pictures directory unavailable")]
    NoPicturesDir,
    #[error("not a directory: {0}")]
    NotDirectory(String),
    #[error("path is outside the allowed gallery root")]
    PathEscape,
    #[error("invalid settings json")]
    InvalidSettings,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryEntry {
    pub kind: String,
    pub path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GallerySettings {
    pub last_folder: Option<String>,
    pub recent_folders: Vec<String>,
    pub onboarded: bool,
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
    #[serde(default = "default_sort_dir")]
    pub sort_dir: String,
    #[serde(default = "default_type_filter")]
    pub type_filter: String,
}

fn default_sort_by() -> String {
    "name".into()
}

fn default_sort_dir() -> String {
    "asc".into()
}

fn default_type_filter() -> String {
    "all".into()
}

impl Default for GallerySettings {
    fn default() -> Self {
        Self {
            last_folder: None,
            recent_folders: Vec::new(),
            onboarded: false,
            sort_by: default_sort_by(),
            sort_dir: default_sort_dir(),
            type_filter: default_type_filter(),
        }
    }
}

fn settings_path() -> Result<PathBuf, GalleryError> {
    let dirs = paths::ensure_app_dirs()?;
    Ok(PathBuf::from(dirs.config_dir).join(SETTINGS_FILE))
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        })
        .unwrap_or(false)
}

fn modified_ms(meta: &fs::Metadata) -> Option<u64> {
    meta.modified().ok().and_then(|time| {
        time.duration_since(SystemTime::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_millis() as u64)
    })
}

fn canonicalize_dir(root: &Path) -> Result<PathBuf, GalleryError> {
    let canonical = fs::canonicalize(root).map_err(GalleryError::Io)?;
    if !canonical.is_dir() {
        return Err(GalleryError::NotDirectory(
            canonical.to_string_lossy().into_owned(),
        ));
    }
    Ok(canonical)
}

fn pictures_root() -> Result<PathBuf, GalleryError> {
    let pictures = dirs::picture_dir().ok_or(GalleryError::NoPicturesDir)?;
    if !pictures.exists() {
        fs::create_dir_all(&pictures)?;
    }
    canonicalize_dir(&pictures)
}

/// Resolve an allowed gallery root: Pictures, or a prior root still under Pictures.
fn resolve_allowed_root(preferred: Option<&str>) -> Result<PathBuf, GalleryError> {
    let pictures = pictures_root()?;
    if let Some(pref) = preferred {
        let candidate = PathBuf::from(pref);
        if let Ok(canonical) = canonicalize_dir(&candidate) {
            if canonical == pictures || canonical.starts_with(&pictures) {
                // Preferred is inside Pictures; the boundary stays Pictures.
                return Ok(pictures);
            }
            // Legacy recent root outside Pictures: allow browsing that tree only.
            return Ok(canonical);
        }
    }
    Ok(pictures)
}

fn ensure_under_allowed(dir: &Path, allowed_root: &Path) -> Result<PathBuf, GalleryError> {
    let canonical = canonicalize_dir(dir)?;
    if canonical != *allowed_root && !canonical.starts_with(allowed_root) {
        return Err(GalleryError::PathEscape);
    }
    Ok(canonical)
}

fn entry_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn list_immediate(dir: &Path) -> Result<Vec<GalleryEntry>, GalleryError> {
    let mut folders = Vec::new();
    let mut images = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if meta.is_dir() {
            folders.push(GalleryEntry {
                kind: "folder".into(),
                path: path.to_string_lossy().into_owned(),
                name: entry_name(&path),
                size: None,
                modified_ms: modified_ms(&meta),
            });
        } else if meta.is_file() && is_image(&path) {
            images.push(GalleryEntry {
                kind: "image".into(),
                path: path.to_string_lossy().into_owned(),
                name: entry_name(&path),
                size: Some(meta.len()),
                modified_ms: modified_ms(&meta),
            });
        }
    }

    folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    images.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    folders.extend(images);
    Ok(folders)
}

#[tauri::command]
pub fn get_default_pictures_dir() -> Result<String, String> {
    let canonical = pictures_root().map_err(|e| e.to_string())?;
    Ok(canonical.to_string_lossy().into_owned())
}

/// List immediate children (subfolders then images) of `dir`.
/// Paths must stay under Pictures, or under a legacy allowed root from settings.
#[tauri::command]
pub fn list_gallery_entries(
    dir: String,
    allowed_root: Option<String>,
) -> Result<Vec<GalleryEntry>, String> {
    let boundary = resolve_allowed_root(allowed_root.as_deref()).map_err(|e| e.to_string())?;
    let dir_path = PathBuf::from(&dir);
    let canonical = ensure_under_allowed(&dir_path, &boundary).map_err(|e| e.to_string())?;
    list_immediate(&canonical).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_gallery_settings() -> Result<GallerySettings, String> {
    let path = settings_path().map_err(|e| e.to_string())?;
    if !path.exists() {
        return Ok(GallerySettings::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut settings: GallerySettings =
        serde_json::from_str(&raw).map_err(|_| GalleryError::InvalidSettings.to_string())?;
    if settings.sort_by.is_empty() {
        settings.sort_by = default_sort_by();
    }
    if settings.sort_dir.is_empty() {
        settings.sort_dir = default_sort_dir();
    }
    if settings.type_filter.is_empty() {
        settings.type_filter = default_type_filter();
    }
    Ok(settings)
}

#[tauri::command]
pub fn save_gallery_settings(mut settings: GallerySettings) -> Result<(), String> {
    settings.recent_folders.truncate(MAX_RECENT);
    if let Some(ref last) = settings.last_folder {
        settings.recent_folders.retain(|p| p != last);
        settings.recent_folders.insert(0, last.clone());
        settings.recent_folders.truncate(MAX_RECENT);
    }
    let path = settings_path().map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())
}
