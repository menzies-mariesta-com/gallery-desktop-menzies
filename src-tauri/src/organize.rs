//! Duplicate, rename, delete, clipboard copy, and bounded manual image order.
//! Duplicate and rename never overwrite an existing file. Delete removes only gated images.

use crate::gallery::{self, GalleryError};
use crate::hashutil;
use crate::paths;
use arboard::{Clipboard, ImageData};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_ORDER: usize = 2000;
const MAX_ORDER_FILES: usize = 200;
const MAX_ORDER_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredImage {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderFile {
    version: u32,
    paths: Vec<String>,
}

fn parent_file_name(path: &Path) -> Result<(PathBuf, String, String), GalleryError> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| GalleryError::NotFile(path.to_string_lossy().into_owned()))?
        .to_string();
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("image")
        .to_string();
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| GalleryError::NotFile(name.clone()))?
        .to_string();
    let parent = path
        .parent()
        .ok_or_else(|| GalleryError::NotDirectory(path.to_string_lossy().into_owned()))?
        .to_path_buf();
    Ok((parent, stem, ext))
}

fn unique_copy(dir: &Path, stem: &str, ext: &str) -> Result<PathBuf, String> {
    for index in 0..100 {
        let name = if index == 0 {
            format!("{stem}-copy.{ext}")
        } else {
            format!("{stem}-copy-{}.{ext}", index + 1)
        };
        let candidate = dir.join(&name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(gallery::command_err(
        "target_exists",
        "too many copies already exist",
    ))
}

fn valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 || name == "." || name == ".." {
        return false;
    }
    !name.contains('/') && !name.contains('\\') && !name.chars().any(|ch| ch.is_control())
}

#[tauri::command]
pub fn duplicate_gallery_image(
    source_path: String,
    allowed_root: Option<String>,
) -> Result<StoredImage, String> {
    let source = gallery::gate_file(Path::new(&source_path), allowed_root.as_deref())
        .map_err(gallery::gallery_command_err)?;
    if !gallery::is_image(&source) {
        return Err(gallery::command_err(
            "not_raster",
            "only gallery images can be copied",
        ));
    }
    let (parent, stem, ext) = parent_file_name(&source).map_err(gallery::gallery_command_err)?;
    let dest = unique_copy(&parent, &stem, &ext)?;
    fs::copy(&source, &dest).map_err(|err| gallery::command_err("io", err))?;
    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("copy")
        .to_string();
    Ok(StoredImage {
        path: dest.to_string_lossy().into_owned(),
        name,
    })
}

#[tauri::command]
pub fn delete_gallery_image(
    source_path: String,
    allowed_root: Option<String>,
) -> Result<(), String> {
    let source = gallery::gate_file(Path::new(&source_path), allowed_root.as_deref())
        .map_err(gallery::gallery_command_err)?;
    if !gallery::is_image(&source) {
        return Err(gallery::command_err(
            "not_raster",
            "only gallery images can be deleted",
        ));
    }
    fs::remove_file(&source).map_err(|err| gallery::command_err("io", err))?;
    Ok(())
}

#[tauri::command]
pub fn rename_gallery_image(
    source_path: String,
    allowed_root: Option<String>,
    new_name: String,
) -> Result<StoredImage, String> {
    let source = gallery::gate_file(Path::new(&source_path), allowed_root.as_deref())
        .map_err(gallery::gallery_command_err)?;
    if !gallery::is_image(&source) {
        return Err(gallery::command_err(
            "not_raster",
            "only gallery images can be renamed",
        ));
    }
    let source_ext = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // Strip any user-provided extension and join the locked original format.
    let stem = Path::new(&new_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(new_name.as_str())
        .trim()
        .to_string();
    if stem.is_empty() || stem.contains('/') || stem.contains('\\') || stem.contains("..") {
        return Err(gallery::command_err("bad_name", "file name is not allowed"));
    }
    let locked_name = if source_ext.is_empty() {
        stem
    } else {
        format!("{stem}.{source_ext}")
    };
    if !valid_name(&locked_name) {
        return Err(gallery::command_err("bad_name", "file name is not allowed"));
    }
    let parent = source
        .parent()
        .ok_or_else(|| gallery::command_err("not_directory", "image has no parent folder"))?;
    let dest = parent.join(&locked_name);
    if dest == source {
        return Ok(StoredImage {
            path: source.to_string_lossy().into_owned(),
            name: locked_name,
        });
    }
    if dest.exists() {
        return Err(gallery::command_err(
            "target_exists",
            "a file with that name already exists",
        ));
    }
    fs::rename(&source, &dest).map_err(|err| gallery::command_err("io", err))?;
    Ok(StoredImage {
        path: dest.to_string_lossy().into_owned(),
        name: locked_name,
    })
}

/// Copy image pixels to the system clipboard so the user can paste in other apps.
#[tauri::command]
pub fn copy_image_to_clipboard(
    source_path: String,
    allowed_root: Option<String>,
) -> Result<(), String> {
    let source = gallery::gate_file(Path::new(&source_path), allowed_root.as_deref())
        .map_err(gallery::gallery_command_err)?;
    if !gallery::is_image(&source) {
        return Err(gallery::command_err(
            "not_raster",
            "only gallery images can be copied",
        ));
    }
    let decoded = image::open(&source).map_err(|err| gallery::command_err("decode", err))?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    let bytes = rgba.into_raw();
    let mut clipboard = Clipboard::new().map_err(|err| gallery::command_err("clipboard", err))?;
    clipboard
        .set_image(ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Owned(bytes),
        })
        .map_err(|err| gallery::command_err("clipboard", err))?;
    Ok(())
}

fn order_dir() -> Result<PathBuf, String> {
    let dirs = paths::ensure_app_dirs().map_err(|err| gallery::command_err("path", err))?;
    let dir = PathBuf::from(dirs.data_dir).join("cache/order");
    fs::create_dir_all(&dir).map_err(|err| gallery::command_err("io", err))?;
    Ok(dir)
}

fn order_path(dir: &Path) -> Result<PathBuf, String> {
    let key = hashutil::sha256_hex(dir.to_string_lossy().as_bytes());
    Ok(order_dir()?.join(format!("{key}.json")))
}

#[tauri::command]
pub fn load_image_order(dir: String, allowed_root: Option<String>) -> Result<Vec<String>, String> {
    let canonical = gallery::gate_dir(Path::new(&dir), allowed_root.as_deref())
        .map_err(gallery::gallery_command_err)?;
    let path = order_path(&canonical)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let meta = fs::metadata(&path).map_err(|err| gallery::command_err("io", err))?;
    if meta.len() > MAX_ORDER_BYTES {
        return Err(gallery::command_err(
            "order_too_large",
            "order file is too large",
        ));
    }
    let raw = fs::read_to_string(&path).map_err(|err| gallery::command_err("io", err))?;
    let parsed: OrderFile = serde_json::from_str(&raw)
        .map_err(|_| gallery::command_err("invalid_order", "order file is invalid"))?;
    let mut kept = Vec::new();
    for entry in parsed.paths.into_iter().take(MAX_ORDER) {
        let candidate = PathBuf::from(&entry);
        let Ok(file) = gallery::gate_file(&candidate, Some(canonical.to_string_lossy().as_ref()))
        else {
            continue;
        };
        if file.parent() == Some(canonical.as_path()) && gallery::is_image(&file) {
            kept.push(file.to_string_lossy().into_owned());
        }
    }
    Ok(kept)
}

#[tauri::command]
pub fn save_image_order(
    dir: String,
    allowed_root: Option<String>,
    paths: Vec<String>,
) -> Result<(), String> {
    if paths.len() > MAX_ORDER {
        return Err(gallery::command_err(
            "order_too_large",
            "too many images in the manual order",
        ));
    }
    let canonical = gallery::gate_dir(Path::new(&dir), allowed_root.as_deref())
        .map_err(gallery::gallery_command_err)?;
    let mut clean = Vec::with_capacity(paths.len());
    for entry in &paths {
        let file = gallery::gate_file(Path::new(entry), Some(canonical.to_string_lossy().as_ref()))
            .map_err(gallery::gallery_command_err)?;
        if file.parent() != Some(canonical.as_path()) || !gallery::is_image(&file) {
            return Err(gallery::command_err(
                "path_escape",
                "order entries must be images in this folder",
            ));
        }
        let text = file.to_string_lossy().into_owned();
        if !clean.iter().any(|existing: &String| existing == &text) {
            clean.push(text);
        }
    }
    let payload = OrderFile {
        version: 1,
        paths: clean,
    };
    let raw = serde_json::to_string(&payload).map_err(|err| gallery::command_err("io", err))?;
    let path = order_path(&canonical)?;
    fs::write(&path, raw).map_err(|err| gallery::command_err("io", err))?;
    if let Ok(cache) = order_dir() {
        let _ = crate::image_edit::evict_cache_dir(&cache, MAX_ORDER_FILES);
    }
    Ok(())
}
