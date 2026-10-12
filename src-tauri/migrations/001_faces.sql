CREATE TABLE IF NOT EXISTS images (
    id INTEGER PRIMARY KEY NOT NULL,
    path TEXT NOT NULL UNIQUE,
    fingerprint TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    modified_ms INTEGER,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    model_version TEXT NOT NULL,
    indexed_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS people (
    id INTEGER PRIMARY KEY NOT NULL,
    label TEXT,
    created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS faces (
    id INTEGER PRIMARY KEY NOT NULL,
    image_id INTEGER NOT NULL REFERENCES images (id) ON DELETE CASCADE,
    person_id INTEGER REFERENCES people (id) ON DELETE SET NULL,
    score REAL NOT NULL,
    bbox_x REAL NOT NULL,
    bbox_y REAL NOT NULL,
    bbox_w REAL NOT NULL,
    bbox_h REAL NOT NULL,
    landmarks TEXT NOT NULL,
    embedding BLOB NOT NULL,
    model_version TEXT NOT NULL,
    created_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_faces_image ON faces (image_id);
CREATE INDEX IF NOT EXISTS idx_faces_person ON faces (person_id);
