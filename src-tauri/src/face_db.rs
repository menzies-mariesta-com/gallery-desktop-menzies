//! SQLite face index under the Menzies data directory.

use crate::paths;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LIST_ROWS: i64 = 4000;
const MAX_VECTORS_PER_PERSON: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("face database is unavailable")]
    Unavailable,
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

pub struct StoredImage {
    pub fingerprint: String,
    pub byte_size: i64,
    pub modified_ms: Option<i64>,
    pub model_version: String,
}

pub struct NewFace {
    pub score: f32,
    pub bbox: [f32; 4],
    pub landmarks: [f32; 10],
    pub embedding: Vec<f32>,
    pub person_id: Option<i64>,
    pub model_version: String,
}

pub struct PersonMemory {
    pub id: i64,
    pub vectors: Vec<Vec<f32>>,
}

#[derive(Debug, Clone)]
pub struct FacePreview {
    pub path: String,
    pub bbox: [f32; 4],
    pub image_width: i64,
    pub image_height: i64,
}

#[derive(Debug, Clone)]
pub struct PersonGroup {
    pub id: i64,
    pub label: Option<String>,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub photo_count: i64,
    pub preview: Option<FacePreview>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn db_path() -> Result<PathBuf, DbError> {
    let dirs = paths::ensure_app_dirs().map_err(|_| DbError::Unavailable)?;
    Ok(PathBuf::from(dirs.data_dir).join("faces.sqlite"))
}

pub async fn pool() -> Result<SqlitePool, DbError> {
    static POOL: OnceLock<SqlitePool> = OnceLock::new();
    if let Some(existing) = POOL.get() {
        return Ok(existing.clone());
    }
    let path = db_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| DbError::Unavailable)?;
    }
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
        .map_err(|_| DbError::Unavailable)?
        .create_if_missing(true)
        .foreign_keys(true);
    let opened = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&opened).await?;
    let _ = POOL.set(opened.clone());
    Ok(POOL.get().cloned().unwrap_or(opened))
}

pub async fn image_by_path(pool: &SqlitePool, path: &str) -> Result<Option<StoredImage>, DbError> {
    let row = sqlx::query(
        "SELECT fingerprint, byte_size, modified_ms, model_version FROM images WHERE path = ?",
    )
    .bind(path)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| StoredImage {
        fingerprint: row.get("fingerprint"),
        byte_size: row.get("byte_size"),
        modified_ms: row.get("modified_ms"),
        model_version: row.get("model_version"),
    }))
}

pub struct ImageWrite<'a> {
    pub path: &'a str,
    pub fingerprint: &'a str,
    pub byte_size: i64,
    pub modified_ms: Option<i64>,
    pub width: i64,
    pub height: i64,
    pub model_version: &'a str,
}

pub async fn replace_image_faces(
    pool: &SqlitePool,
    image: &ImageWrite<'_>,
    faces: &[NewFace],
) -> Result<(), DbError> {
    let mut tx = pool.begin().await?;
    let indexed_ms = now_ms();
    let image_id: i64 = sqlx::query(
        "INSERT INTO images (path, fingerprint, byte_size, modified_ms, width, height, model_version, indexed_ms)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(path) DO UPDATE SET
            fingerprint = excluded.fingerprint,
            byte_size = excluded.byte_size,
            modified_ms = excluded.modified_ms,
            width = excluded.width,
            height = excluded.height,
            model_version = excluded.model_version,
            indexed_ms = excluded.indexed_ms
         RETURNING id",
    )
    .bind(image.path)
    .bind(image.fingerprint)
    .bind(image.byte_size)
    .bind(image.modified_ms)
    .bind(image.width)
    .bind(image.height)
    .bind(image.model_version)
    .bind(indexed_ms)
    .fetch_one(&mut *tx)
    .await?
    .get("id");
    sqlx::query("DELETE FROM faces WHERE image_id = ?")
        .bind(image_id)
        .execute(&mut *tx)
        .await?;
    for face in faces {
        let landmarks = serde_json::to_string(&face.landmarks).unwrap_or_else(|_| "[]".into());
        let mut blob = Vec::with_capacity(face.embedding.len() * 4);
        for value in &face.embedding {
            blob.extend_from_slice(&value.to_le_bytes());
        }
        sqlx::query(
            "INSERT INTO faces (
                image_id, person_id, score, bbox_x, bbox_y, bbox_w, bbox_h,
                landmarks, embedding, model_version, created_ms
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(image_id)
        .bind(face.person_id)
        .bind(face.score)
        .bind(face.bbox[0])
        .bind(face.bbox[1])
        .bind(face.bbox[2])
        .bind(face.bbox[3])
        .bind(landmarks)
        .bind(blob)
        .bind(&face.model_version)
        .bind(indexed_ms)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "DELETE FROM people WHERE id NOT IN (
            SELECT person_id FROM faces WHERE person_id IS NOT NULL
         )",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn touch_image(
    pool: &SqlitePool,
    path: &str,
    byte_size: i64,
    modified_ms: Option<i64>,
) -> Result<(), DbError> {
    sqlx::query("UPDATE images SET byte_size = ?, modified_ms = ?, indexed_ms = ? WHERE path = ?")
        .bind(byte_size)
        .bind(modified_ms)
        .bind(now_ms())
        .bind(path)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn embeddings_for_path(
    pool: &SqlitePool,
    path: &str,
) -> Result<Vec<(i64, Vec<f32>)>, DbError> {
    let rows = sqlx::query(
        "SELECT f.person_id, f.embedding
         FROM faces f
         JOIN images i ON i.id = f.image_id
         WHERE i.path = ? AND f.person_id IS NOT NULL",
    )
    .bind(path)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for row in rows {
        let Some(person_id) = row.get::<Option<i64>, _>("person_id") else {
            continue;
        };
        let blob: Vec<u8> = row.get("embedding");
        let Some(vector) = decode_embedding(&blob) else {
            continue;
        };
        out.push((person_id, vector));
    }
    Ok(out)
}

pub async fn insert_person(pool: &SqlitePool) -> Result<i64, DbError> {
    let now = now_ms();
    let id: i64 = sqlx::query(
        "INSERT INTO people (label, created_ms, updated_ms) VALUES (NULL, ?, ?) RETURNING id",
    )
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?
    .get("id");
    Ok(id)
}

pub async fn load_people_memory(
    pool: &SqlitePool,
    model_version: &str,
) -> Result<Vec<PersonMemory>, DbError> {
    let rows = sqlx::query(
        "SELECT person_id, embedding FROM faces
         WHERE person_id IS NOT NULL AND model_version = ?
         ORDER BY id",
    )
    .bind(model_version)
    .fetch_all(pool)
    .await?;
    let mut people: Vec<PersonMemory> = Vec::new();
    for row in rows {
        let person_id: i64 = row.get("person_id");
        let blob: Vec<u8> = row.get("embedding");
        let Some(vector) = decode_embedding(&blob) else {
            continue;
        };
        if let Some(person) = people.iter_mut().find(|person| person.id == person_id) {
            if person.vectors.len() < MAX_VECTORS_PER_PERSON {
                person.vectors.push(vector);
            }
        } else if people.len() < 2000 {
            people.push(PersonMemory {
                id: person_id,
                vectors: vec![vector],
            });
        }
    }
    Ok(people)
}

fn decode_embedding(blob: &[u8]) -> Option<Vec<f32>> {
    if blob.is_empty() || blob.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(blob.len() / 4);
    for chunk in blob.chunks_exact(4) {
        let value = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if !value.is_finite() {
            return None;
        }
        out.push(value);
    }
    Some(out)
}

pub async fn list_people(pool: &SqlitePool) -> Result<Vec<PersonGroup>, DbError> {
    let rows = sqlx::query(
        "SELECT p.id, p.label, p.created_ms, p.updated_ms,
                COUNT(DISTINCT i.id) AS photo_count
         FROM people p
         JOIN faces f ON f.person_id = p.id
         JOIN images i ON i.id = f.image_id
         GROUP BY p.id
         ORDER BY p.label IS NULL, p.label, p.id
         LIMIT ?",
    )
    .bind(MAX_LIST_ROWS)
    .fetch_all(pool)
    .await?;
    let mut groups: Vec<PersonGroup> = rows
        .into_iter()
        .map(|row| PersonGroup {
            id: row.get("id"),
            label: row.get("label"),
            created_ms: row.get("created_ms"),
            updated_ms: row.get("updated_ms"),
            photo_count: row.get("photo_count"),
            preview: None,
        })
        .collect();
    if groups.is_empty() {
        return Ok(groups);
    }
    let preview_rows = sqlx::query(
        "SELECT f.person_id, i.path, f.score, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h,
                i.width, i.height
         FROM faces f
         JOIN images i ON i.id = f.image_id
         WHERE f.person_id IS NOT NULL
         ORDER BY f.person_id, f.score DESC, f.id",
    )
    .fetch_all(pool)
    .await?;
    let mut best: std::collections::HashMap<i64, FacePreview> = std::collections::HashMap::new();
    for row in preview_rows {
        let person_id: i64 = row.get("person_id");
        if best.contains_key(&person_id) {
            continue;
        }
        best.insert(
            person_id,
            FacePreview {
                path: row.get("path"),
                bbox: [
                    row.get("bbox_x"),
                    row.get("bbox_y"),
                    row.get("bbox_w"),
                    row.get("bbox_h"),
                ],
                image_width: row.get("width"),
                image_height: row.get("height"),
            },
        );
    }
    for group in &mut groups {
        group.preview = best.remove(&group.id);
    }
    Ok(groups)
}

pub async fn list_person_images(pool: &SqlitePool, person_id: i64) -> Result<Vec<String>, DbError> {
    let rows = sqlx::query(
        "SELECT DISTINCT i.path
         FROM faces f
         JOIN images i ON i.id = f.image_id
         WHERE f.person_id = ?
         ORDER BY i.path
         LIMIT ?",
    )
    .bind(person_id)
    .bind(MAX_LIST_ROWS)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|row| row.get("path")).collect())
}

pub async fn rename_person(
    pool: &SqlitePool,
    id: i64,
    label: Option<&str>,
) -> Result<bool, DbError> {
    let result = sqlx::query("UPDATE people SET label = ?, updated_ms = ? WHERE id = ?")
        .bind(label)
        .bind(now_ms())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migration_round_trip_keeps_embeddings() {
        let nanos = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("gallery-faces-{nanos}.sqlite"));
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
            .expect("options")
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrate");
        let embedding = vec![0.25f32, -0.5, 0.75, 1.0];
        replace_image_faces(
            &pool,
            &ImageWrite {
                path: "/tmp/gallery-face-fixture.png",
                fingerprint: "abc",
                byte_size: 12,
                modified_ms: Some(10),
                width: 8,
                height: 8,
                model_version: "1.0.0",
            },
            &[NewFace {
                score: 0.99,
                bbox: [1.0, 2.0, 3.0, 4.0],
                landmarks: [0.0; 10],
                embedding: embedding.clone(),
                person_id: None,
                model_version: "1.0.0".into(),
            }],
        )
        .await
        .expect("write");
        let person = insert_person(&pool).await.expect("person");
        sqlx::query("UPDATE faces SET person_id = ?")
            .bind(person)
            .execute(&pool)
            .await
            .expect("assign");
        let memory = load_people_memory(&pool, "1.0.0").await.expect("memory");
        assert_eq!(memory.len(), 1);
        assert_eq!(memory[0].vectors[0], embedding);
        let listed = list_people(&pool).await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].photo_count, 1);
        assert!(listed[0].preview.is_some());
        let images = list_person_images(&pool, person).await.expect("images");
        assert_eq!(images.len(), 1);
        assert!(rename_person(&pool, person, Some("Ada"))
            .await
            .expect("rename"));
        let _ = std::fs::remove_file(path);
    }
}
