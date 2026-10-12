//! Cancellable face index for the current gallery folder.
//! One image at a time. No polling. Photos are not uploaded.

use crate::face_db::{self, NewFace, PersonMemory};
use crate::face_ext::{self, FaceError};
use crate::face_infer::{self, FoundFace};
use crate::face_math::{assign_person, centroid, MATCH_COSINE, MATCH_MARGIN};
use crate::gallery::{self, GalleryError};
use crate::hashutil;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;
use tauri::ipc::Channel;

const MAX_FILES: usize = 2000;
const MAX_DEPTH: u32 = 4;
const MAX_IMAGE_BYTES: u64 = 40 * 1024 * 1024;

static CANCEL: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceIndexEvent {
    pub phase: String,
    pub done: u32,
    pub total: u32,
    pub faces: u32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceIndexSummary {
    pub cancelled: bool,
    pub scanned: u32,
    pub skipped: u32,
    pub faces: u32,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacePreviewView {
    pub path: String,
    pub bbox: [f32; 4],
    pub image_width: i64,
    pub image_height: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacePersonView {
    pub id: i64,
    pub label: Option<String>,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub photo_count: i64,
    pub preview: Option<FacePreviewView>,
}

pub fn request_cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

fn index_err(code: &str, detail: impl std::fmt::Display) -> String {
    gallery::command_err(code, detail)
}

fn cancelled() -> bool {
    CANCEL.load(Ordering::SeqCst)
}

fn collect_images(root: &Path, dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    if cancelled() || out.len() >= MAX_FILES || depth > MAX_DEPTH {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    let mut nested = Vec::new();
    for entry in entries.flatten() {
        if cancelled() || out.len() >= MAX_FILES {
            break;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let Ok(canonical) = fs::canonicalize(&path) else {
            continue;
        };
        if canonical != root && !canonical.starts_with(root) {
            continue;
        }
        if file_type.is_dir() {
            nested.push(canonical);
            continue;
        }
        if !gallery::is_indexable_image(&canonical) {
            continue;
        }
        let Ok(meta) = fs::metadata(&canonical) else {
            continue;
        };
        if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_IMAGE_BYTES {
            continue;
        }
        let name = canonical
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        out.push(canonical);
    }
    nested.sort();
    for child in nested {
        collect_images(root, &child, depth + 1, out);
    }
}

fn modified_ms(path: &Path) -> Option<i64> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

struct MemoryPerson {
    id: i64,
    vectors: Vec<Vec<f32>>,
}

fn centroids_of(people: &[MemoryPerson]) -> Vec<(i64, Vec<f32>)> {
    people
        .iter()
        .filter_map(|person| centroid(&person.vectors).map(|center| (person.id, center)))
        .collect()
}

async fn run_index(
    root: PathBuf,
    on_progress: Channel<FaceIndexEvent>,
) -> Result<FaceIndexSummary, String> {
    let installed = face_ext::require_installed().map_err(|err| match err {
        FaceError::ExperimentalOff => index_err("experimental_off", err),
        FaceError::InferenceNotLinked => index_err("inference_not_linked", err),
        other => index_err(other.code(), other),
    })?;
    let send = |phase: &str, done: u32, total: u32, faces: u32, name: &str| {
        let _ = on_progress.send(FaceIndexEvent {
            phase: phase.into(),
            done,
            total,
            faces,
            name: name.into(),
        });
    };
    send("scanning", 0, 0, 0, "");
    let mut files = Vec::new();
    collect_images(&root, &root, 0, &mut files);
    files.sort();
    let truncated = files.len() >= MAX_FILES;
    let total = files.len() as u32;
    let pool = face_db::pool().await.map_err(|err| index_err("io", err))?;
    let mut people = face_db::load_people_memory(&pool, &installed.model_version)
        .await
        .map_err(|err| index_err("io", err))?
        .into_iter()
        .map(|person: PersonMemory| MemoryPerson {
            id: person.id,
            vectors: person.vectors,
        })
        .collect::<Vec<_>>();
    let mut scanned = 0u32;
    let mut skipped = 0u32;
    let mut face_total = 0u32;
    if cancelled() {
        send("cancelled", 0, total, 0, "");
        return Ok(FaceIndexSummary {
            cancelled: true,
            scanned: 0,
            skipped: 0,
            faces: 0,
            truncated,
        });
    }
    for path in &files {
        if cancelled() {
            send("cancelled", scanned, total, face_total, "");
            return Ok(FaceIndexSummary {
                cancelled: true,
                scanned,
                skipped,
                faces: face_total,
                truncated,
            });
        }
        let name = file_name(path);
        send("indexing", scanned, total, face_total, &name);
        let path_text = path.to_string_lossy().into_owned();
        let meta = fs::metadata(path).map_err(|err| index_err("io", err))?;
        let byte_size = i64::try_from(meta.len()).unwrap_or(i64::MAX);
        let mtime = modified_ms(path);
        let stored = face_db::image_by_path(&pool, &path_text)
            .await
            .map_err(|err| index_err("io", err))?;
        if let Some(stored) = stored.as_ref() {
            if stored.model_version == installed.model_version
                && stored.byte_size == byte_size
                && stored.modified_ms == mtime
                && !stored.fingerprint.is_empty()
            {
                skipped += 1;
                scanned += 1;
                continue;
            }
        }
        let fingerprint = hashutil::sha256_file(path, MAX_IMAGE_BYTES).map_err(|err| {
            if err.kind() == std::io::ErrorKind::InvalidData {
                index_err("too_large", "image is too large")
            } else {
                index_err("io", err)
            }
        })?;
        if let Some(stored) = stored.as_ref() {
            if stored.fingerprint.eq_ignore_ascii_case(&fingerprint)
                && stored.model_version == installed.model_version
            {
                face_db::touch_image(&pool, &path_text, byte_size, mtime)
                    .await
                    .map_err(|err| index_err("io", err))?;
                skipped += 1;
                scanned += 1;
                continue;
            }
        }
        let found = match face_infer::recognize_file(
            &installed.runtime,
            &installed.detector,
            &installed.embedder,
            path,
        ) {
            Ok(found) => found,
            Err(err) => {
                log::warn!("skipped an image during face index: {err}");
                scanned += 1;
                continue;
            }
        };
        let old = face_db::embeddings_for_path(&pool, &path_text)
            .await
            .map_err(|err| index_err("io", err))?;
        for (person_id, vector) in &old {
            if let Some(person) = people.iter_mut().find(|person| person.id == *person_id) {
                if let Some(pos) = person.vectors.iter().position(|item| item == vector) {
                    person.vectors.remove(pos);
                }
            }
        }
        people.retain(|person| !person.vectors.is_empty());
        let (width, height) = match image::image_dimensions(path) {
            Ok(size) => (i64::from(size.0), i64::from(size.1)),
            Err(_) => (0, 0),
        };
        let mut rows = Vec::with_capacity(found.len());
        for face in found {
            let person_id = assign_or_create(&pool, &mut people, &face).await?;
            if let Some(id) = person_id {
                if let Some(person) = people.iter_mut().find(|person| person.id == id) {
                    if person.vectors.len() < 64 {
                        person.vectors.push(face.embedding.clone());
                    }
                }
            }
            face_total += 1;
            rows.push(NewFace {
                score: face.score,
                bbox: face.bbox,
                landmarks: face.landmarks,
                embedding: face.embedding,
                person_id,
                model_version: installed.model_version.clone(),
            });
        }
        face_db::replace_image_faces(
            &pool,
            &face_db::ImageWrite {
                path: &path_text,
                fingerprint: &fingerprint,
                byte_size,
                modified_ms: mtime,
                width,
                height,
                model_version: &installed.model_version,
            },
            &rows,
        )
        .await
        .map_err(|err| index_err("io", err))?;
        scanned += 1;
    }
    let phase = if cancelled() { "cancelled" } else { "done" };
    send(phase, scanned, total, face_total, "");
    Ok(FaceIndexSummary {
        cancelled: cancelled(),
        scanned,
        skipped,
        faces: face_total,
        truncated,
    })
}

async fn assign_or_create(
    pool: &sqlx::SqlitePool,
    people: &mut Vec<MemoryPerson>,
    face: &FoundFace,
) -> Result<Option<i64>, String> {
    let centers = centroids_of(people);
    if let Some(id) = assign_person(&face.embedding, &centers, MATCH_COSINE, MATCH_MARGIN) {
        return Ok(Some(id));
    }
    if people.len() >= 2000 {
        return Ok(None);
    }
    let id = face_db::insert_person(pool)
        .await
        .map_err(|err| index_err("io", err))?;
    people.push(MemoryPerson {
        id,
        vectors: Vec::new(),
    });
    Ok(Some(id))
}

#[tauri::command]
pub async fn face_index_start(
    folder: String,
    allowed_root: Option<String>,
    on_progress: Channel<FaceIndexEvent>,
) -> Result<FaceIndexSummary, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err(index_err("busy", "a face index is already running"));
    }
    CANCEL.store(false, Ordering::SeqCst);
    let outcome = async {
        let root = gallery::resolve_index_dir(&folder, allowed_root.as_deref()).map_err(|err| {
            let code = match &err {
                GalleryError::PathEscape => "path_escape",
                GalleryError::NotDirectory(_) => "not_directory",
                _ => "io",
            };
            index_err(code, err)
        })?;
        let progress = on_progress;
        let joined = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|err| index_err("io", err))?;
            runtime.block_on(run_index(root, progress))
        })
        .await
        .map_err(|_| index_err("io", "index task stopped"))?;
        joined
    }
    .await;
    RUNNING.store(false, Ordering::SeqCst);
    outcome
}

#[tauri::command]
pub fn face_index_cancel() -> Result<bool, String> {
    request_cancel();
    Ok(true)
}

#[tauri::command]
pub async fn face_people() -> Result<Vec<FacePersonView>, String> {
    face_ext::require_installed().map_err(|err| match err {
        FaceError::ExperimentalOff => index_err("experimental_off", err),
        FaceError::InferenceNotLinked => index_err("inference_not_linked", err),
        other => index_err(other.code(), other),
    })?;
    let pool = face_db::pool().await.map_err(|err| index_err("io", err))?;
    let groups = face_db::list_people(&pool)
        .await
        .map_err(|err| index_err("io", err))?;
    Ok(groups
        .into_iter()
        .map(|group| FacePersonView {
            id: group.id,
            label: group.label,
            created_ms: group.created_ms,
            updated_ms: group.updated_ms,
            photo_count: group.photo_count,
            preview: group.preview.map(|preview| FacePreviewView {
                path: preview.path,
                bbox: preview.bbox,
                image_width: preview.image_width,
                image_height: preview.image_height,
            }),
        })
        .collect())
}

#[tauri::command]
pub async fn face_person_images(id: i64) -> Result<Vec<String>, String> {
    if id <= 0 {
        return Err(index_err("bad_manifest", "person id is invalid"));
    }
    face_ext::require_installed().map_err(|err| match err {
        FaceError::ExperimentalOff => index_err("experimental_off", err),
        FaceError::InferenceNotLinked => index_err("inference_not_linked", err),
        other => index_err(other.code(), other),
    })?;
    let pool = face_db::pool().await.map_err(|err| index_err("io", err))?;
    face_db::list_person_images(&pool, id)
        .await
        .map_err(|err| index_err("io", err))
}

#[tauri::command]
pub async fn face_person_rename(id: i64, label: String) -> Result<(), String> {
    if id <= 0 {
        return Err(index_err("bad_manifest", "person id is invalid"));
    }
    face_ext::require_installed().map_err(|err| index_err(err.code(), err))?;
    let trimmed = label.trim();
    if trimmed.chars().count() > 80
        || trimmed.contains("://")
        || trimmed.chars().any(|ch| ch.is_control())
    {
        return Err(index_err("bad_manifest", "person name is invalid"));
    }
    let stored = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    };
    let pool = face_db::pool().await.map_err(|err| index_err("io", err))?;
    let updated = face_db::rename_person(&pool, id, stored)
        .await
        .map_err(|err| index_err("io", err))?;
    if updated {
        Ok(())
    } else {
        Err(index_err(
            "not_installed",
            "that person is not in the index",
        ))
    }
}
