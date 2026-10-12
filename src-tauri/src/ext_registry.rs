//! Generic extension catalog, install, and per-extension update preference.
//!
//! Downloads use the Tauri HTTP client. Packages are hashed and signature-checked
//! before they replace the installed copy. The default catalog is
//! `https://menzies.mariesta.com/extensions/registry.json`.
//! A developer may set `registryUrl` in extensions.json to loopback HTTP for a local test.

use crate::ext_sign::{self, SignError, KEY_ID, REGISTRY_SCHEMA};
use crate::face_ext;
use crate::gallery;
use crate::hashutil;
use crate::paths;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::ipc::Channel;

const APP_ID: &str = "gallery-desktop-menzies";
const DEFAULT_REGISTRY_URL: &str = "https://menzies.mariesta.com/extensions/registry.json";
const MAX_REGISTRY_BYTES: u64 = 256 * 1024;
const MAX_PACKAGE_BYTES: u64 = 200 * 1024 * 1024;
const MAX_DOWNLOAD_CACHE_BYTES: u64 = 250 * 1024 * 1024;
const MAX_ZIP_FILES: usize = 32;
const CHECK_BACKOFF_MS: u64 = 12 * 60 * 60 * 1000;
const ATTEMPTS: u32 = 3;

static BUSY: Mutex<Option<(String, &'static str)>> = Mutex::new(None);
static ERRORS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
static GENERATION: AtomicU64 = AtomicU64::new(1);
static CHECKING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, thiserror::Error)]
pub enum ExtError {
    #[error("transfer cancelled")]
    Cancelled,
    #[error("extension package is not published")]
    Unavailable,
    #[error("downgrade is not allowed")]
    Downgrade,
    #[error("extension registry is invalid")]
    BadRegistry,
    #[error("signature check failed")]
    BadSignature,
    #[error("file hash does not match")]
    HashMismatch,
    #[error("package is too large")]
    TooLarge,
    #[error("download failed")]
    Http,
    #[error("another extension transfer is running")]
    Busy,
    #[error("experimental face recognition is off")]
    ExperimentalOff,
    #[error("extension is not installed")]
    NotInstalled,
    #[error("this Gallery build is older than the package requires")]
    AppTooOld,
    #[error("package was revoked")]
    Revoked,
    #[error("package path escapes the install folder")]
    PathEscape,
    #[error("no package matches this system")]
    NoArtifact,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl ExtError {
    fn code(&self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::Unavailable => "unavailable",
            Self::Downgrade => "downgrade",
            Self::BadRegistry => "bad_registry",
            Self::BadSignature => "bad_signature",
            Self::HashMismatch => "hash_mismatch",
            Self::TooLarge => "too_large",
            Self::Http => "http",
            Self::Busy => "busy",
            Self::ExperimentalOff => "experimental_off",
            Self::NotInstalled => "not_installed",
            Self::AppTooOld => "app_too_old",
            Self::Revoked => "revoked",
            Self::PathEscape => "path_escape",
            Self::NoArtifact => "no_artifact",
            Self::Io(_) => "io",
        }
    }

    pub(crate) fn into_command(self) -> String {
        gallery::command_err(self.code(), self)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionState {
    Available,
    Installing,
    Installed,
    UpdateAvailable,
    Updating,
    Error,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionView {
    pub id: String,
    pub app_id: String,
    pub name: String,
    pub summary: String,
    pub state: ExtensionState,
    pub update_mode: String,
    pub installed_version: Option<String>,
    pub registry_version: Option<String>,
    pub experimental_enabled: Option<bool>,
    pub recognition_available: bool,
    pub blocker: Option<String>,
    pub can_install: bool,
    pub can_update: bool,
    pub offline_fallback: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionTransferEvent {
    pub phase: String,
    pub done: u64,
    pub total: u64,
}

fn emit_progress(
    on_progress: Option<&Channel<ExtensionTransferEvent>>,
    phase: &str,
    done: u64,
    total: u64,
) {
    if let Some(channel) = on_progress {
        let _ = channel.send(ExtensionTransferEvent {
            phase: phase.into(),
            done,
            total,
        });
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryDoc {
    schema: String,
    sequence: u64,
    issued_at: String,
    key_id: String,
    revoked: Vec<Revoked>,
    extensions: Vec<RegExtension>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Revoked {
    id: String,
    version: String,
    #[serde(default)]
    sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegExtension {
    id: String,
    app_id: String,
    name: String,
    summary: String,
    availability: String,
    #[serde(default)]
    unavailable_reason: Option<String>,
    default_update_mode: String,
    platforms: Vec<String>,
    #[serde(default)]
    release: Option<RegRelease>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegRelease {
    version: String,
    min_app_version: String,
    released_at: String,
    #[serde(default)]
    notes: Option<String>,
    artifacts: Vec<RegArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegArtifact {
    os: String,
    arch: String,
    url: String,
    sha256: String,
    size: u64,
    signature: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ExtensionsFile {
    allow_downgrade: bool,
    last_sequence: u64,
    last_signature: String,
    last_checked_ms: u64,
    registry_url: String,
    items: BTreeMap<String, ExtensionPref>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ExtensionPref {
    update_mode: String,
    consented: bool,
    installed_version: String,
}

impl Default for ExtensionPref {
    fn default() -> Self {
        Self {
            update_mode: "manual".into(),
            consented: false,
            installed_version: String::new(),
        }
    }
}

struct BusyGuard;

impl Drop for BusyGuard {
    fn drop(&mut self) {
        *lock_mutex(&BUSY) = None;
    }
}

struct CheckGuard;

impl Drop for CheckGuard {
    fn drop(&mut self) {
        CHECKING.store(false, AtomicOrdering::SeqCst);
    }
}

fn lock_mutex<T>(mutex: &'static Mutex<T>) -> MutexGuard<'static, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn config_file() -> Result<PathBuf, ExtError> {
    let dirs = paths::ensure_app_dirs().map_err(|err| ExtError::Io(io_other(err)))?;
    Ok(PathBuf::from(dirs.config_dir).join("extensions.json"))
}

fn data_root() -> Result<PathBuf, ExtError> {
    let dirs = paths::ensure_app_dirs().map_err(|err| ExtError::Io(io_other(err)))?;
    Ok(PathBuf::from(dirs.data_dir))
}

fn extension_dir(id: &str) -> Result<PathBuf, ExtError> {
    if !valid_id(id) {
        return Err(ExtError::BadRegistry);
    }
    Ok(data_root()?.join("extensions").join(id))
}

fn download_dir() -> Result<PathBuf, ExtError> {
    let dir = data_root()?.join("cache").join("extension-downloads");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn registry_cache_path() -> Result<PathBuf, ExtError> {
    let dir = data_root()?.join("cache");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("extension-registry.json"))
}

fn io_other(err: impl std::fmt::Display) -> std::io::Error {
    // Stable Clippy prefers Error::other. The suite MSRV is 1.77, where that constructor is newer.
    #[allow(clippy::io_other_error)]
    std::io::Error::new(std::io::ErrorKind::Other, err.to_string())
}

fn valid_id(value: &str) -> bool {
    let chars: Vec<char> = value.chars().collect();
    (1..=64).contains(&chars.len())
        && chars
            .iter()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || *ch == '-')
        && chars.first().is_some_and(|ch| ch.is_ascii_alphanumeric())
        && chars.last().is_some_and(|ch| ch.is_ascii_alphanumeric())
}

fn load_prefs() -> Result<ExtensionsFile, ExtError> {
    let path = config_file()?;
    if !path.exists() {
        return Ok(ExtensionsFile::default());
    }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

fn save_prefs(prefs: &ExtensionsFile) -> Result<(), ExtError> {
    let path = config_file()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_vec_pretty(prefs).map_err(|_| ExtError::BadRegistry)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, raw)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn set_error(id: &str, code: Option<&str>) {
    let mut errors = lock_mutex(&ERRORS);
    match code {
        Some(code) => {
            errors.insert(id.to_string(), code.to_string());
        }
        None => {
            errors.remove(id);
        }
    }
}

fn error_for(id: &str) -> Option<String> {
    let errors = lock_mutex(&ERRORS);
    errors.get(id).cloned().or_else(|| errors.get("*").cloned())
}

fn current_os() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    }
}

fn current_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    }
}

fn url_parts(url: &str) -> Option<(String, String)> {
    if url.contains('@') || url.contains('?') || url.contains('#') {
        return None;
    }
    let (https, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else {
        (false, url.strip_prefix("http://")?)
    };
    let (hostport, path) = rest.split_once('/')?;
    let (host, port) = match hostport.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (hostport, None),
    };
    if host.is_empty() {
        return None;
    }
    let loopback = host == "127.0.0.1" || host == "localhost";
    if https {
        if host != "menzies.mariesta.com" || port.is_some() {
            return None;
        }
    } else if loopback {
        let port = port?;
        if port.is_empty() || !port.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        let value = port.parse::<u16>().ok()?;
        if value == 0 {
            return None;
        }
    } else {
        return None;
    }
    Some((host.to_string(), format!("/{path}")))
}

fn url_allowed(url: &str) -> bool {
    let Some((_, path)) = url_parts(url) else {
        return false;
    };
    path == "/extensions/registry.json"
        || (path.starts_with("/extensions/packages/") && path.ends_with(".zip"))
}

fn registry_url(prefs: &ExtensionsFile) -> String {
    let custom = prefs.registry_url.trim();
    if !custom.is_empty()
        && url_parts(custom).is_some_and(|(_, path)| path == "/extensions/registry.json")
    {
        return custom.to_string();
    }
    DEFAULT_REGISTRY_URL.to_string()
}

fn validate_doc(doc: &RegistryDoc) -> Result<(), ExtError> {
    if doc.schema != REGISTRY_SCHEMA
        || doc.key_id != KEY_ID
        || doc.sequence < 1
        || !iso_utc(&doc.issued_at)
    {
        return Err(ExtError::BadRegistry);
    }
    if doc.extensions.is_empty() || doc.extensions.len() > 64 || doc.revoked.len() > 64 {
        return Err(ExtError::BadRegistry);
    }
    let mut seen = BTreeMap::<String, ()>::new();
    for entry in &doc.extensions {
        if !valid_id(&entry.id) || !valid_id(&entry.app_id) {
            return Err(ExtError::BadRegistry);
        }
        if entry.name.is_empty()
            || entry.name.chars().count() > 80
            || entry.summary.is_empty()
            || entry.summary.chars().count() > 400
            || entry.name.contains("://")
            || entry.summary.contains("://")
        {
            return Err(ExtError::BadRegistry);
        }
        if entry.default_update_mode != "manual" && entry.default_update_mode != "automatic" {
            return Err(ExtError::BadRegistry);
        }
        if entry.platforms.is_empty() || entry.platforms.len() > 3 {
            return Err(ExtError::BadRegistry);
        }
        for platform in &entry.platforms {
            if !matches!(platform.as_str(), "linux" | "darwin" | "windows") {
                return Err(ExtError::BadRegistry);
            }
        }
        let key = format!("{}:{}", entry.app_id, entry.id);
        if seen.insert(key, ()).is_some() {
            return Err(ExtError::BadRegistry);
        }
        match entry.availability.as_str() {
            "unavailable" => {
                if entry.release.is_some() {
                    return Err(ExtError::BadRegistry);
                }
                let reason = entry.unavailable_reason.as_deref().unwrap_or("");
                if reason.is_empty()
                    || reason.len() > 64
                    || !reason
                        .chars()
                        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
                {
                    return Err(ExtError::BadRegistry);
                }
            }
            "available" => {
                if entry.unavailable_reason.is_some() {
                    return Err(ExtError::BadRegistry);
                }
                let release = entry.release.as_ref().ok_or(ExtError::BadRegistry)?;
                validate_release(entry, release)?;
            }
            _ => return Err(ExtError::BadRegistry),
        }
    }
    for revoked in &doc.revoked {
        if !valid_id(&revoked.id) || ext_sign::parse_semver(&revoked.version).is_none() {
            return Err(ExtError::BadRegistry);
        }
        if let Some(sha) = &revoked.sha256 {
            if sha.len() != 64 || !sha.chars().all(|ch| ch.is_ascii_hexdigit()) {
                return Err(ExtError::BadRegistry);
            }
        }
    }
    Ok(())
}

fn validate_release(entry: &RegExtension, release: &RegRelease) -> Result<(), ExtError> {
    if ext_sign::parse_semver(&release.version).is_none()
        || ext_sign::parse_semver(&release.min_app_version).is_none()
        || !iso_utc(&release.released_at)
    {
        return Err(ExtError::BadRegistry);
    }
    if release.notes.as_ref().is_some_and(|notes| {
        notes.is_empty() || notes.chars().count() > 400 || notes.contains("://")
    }) {
        return Err(ExtError::BadRegistry);
    }
    if release.artifacts.is_empty() || release.artifacts.len() > 8 {
        return Err(ExtError::BadRegistry);
    }
    let mut targets = BTreeMap::<String, ()>::new();
    for artifact in &release.artifacts {
        if !matches!(artifact.os.as_str(), "linux" | "darwin" | "windows" | "any")
            || !matches!(artifact.arch.as_str(), "x86_64" | "aarch64" | "any")
        {
            return Err(ExtError::BadRegistry);
        }
        if artifact.size == 0 || artifact.size > MAX_PACKAGE_BYTES {
            return Err(ExtError::TooLarge);
        }
        if artifact.sha256.len() != 64 || !artifact.sha256.chars().all(|ch| ch.is_ascii_hexdigit())
        {
            return Err(ExtError::BadRegistry);
        }
        let expected = format!(
            "/extensions/packages/{}/{}/{}/{}-{}.zip",
            entry.app_id, entry.id, release.version, artifact.os, artifact.arch
        );
        let Some((_, path)) = url_parts(&artifact.url) else {
            return Err(ExtError::BadRegistry);
        };
        if path != expected {
            return Err(ExtError::BadRegistry);
        }
        if targets
            .insert(format!("{}:{}", artifact.os, artifact.arch), ())
            .is_some()
        {
            return Err(ExtError::BadRegistry);
        }
    }
    Ok(())
}

fn verify_registry_bytes(bytes: &[u8]) -> Result<(RegistryDoc, String), ExtError> {
    if bytes.len() as u64 > MAX_REGISTRY_BYTES {
        return Err(ExtError::TooLarge);
    }
    let mut value: Value = serde_json::from_slice(bytes).map_err(|_| ExtError::BadRegistry)?;
    let signature = value
        .get("signature")
        .and_then(Value::as_str)
        .ok_or(ExtError::BadRegistry)?
        .to_string();
    let Some(object) = value.as_object_mut() else {
        return Err(ExtError::BadRegistry);
    };
    object.remove("signature");
    let canonical = ext_sign::canonical_json(&value).map_err(|_| ExtError::BadRegistry)?;
    ext_sign::verify_registry(&canonical, &signature).map_err(sign_to_ext)?;
    let doc: RegistryDoc = serde_json::from_value(value).map_err(|_| ExtError::BadRegistry)?;
    validate_doc(&doc)?;
    Ok((doc, signature))
}

fn iso_utc(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 20
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':')
        && bytes.get(19) == Some(&b'Z')
        && bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit()
        })
}

fn sign_to_ext(err: SignError) -> ExtError {
    match err {
        SignError::BadSignature => ExtError::BadSignature,
        SignError::BadKey | SignError::Canonical => ExtError::BadRegistry,
    }
}

fn accept_sequence(prefs: &ExtensionsFile, sequence: u64, signature: &str) -> Result<(), ExtError> {
    if prefs.allow_downgrade || prefs.last_sequence == 0 {
        return Ok(());
    }
    if sequence > prefs.last_sequence {
        return Ok(());
    }
    if sequence == prefs.last_sequence && signature == prefs.last_signature {
        return Ok(());
    }
    Err(ExtError::Downgrade)
}

fn load_cached_registry() -> Option<RegistryDoc> {
    let path = registry_cache_path().ok()?;
    let bytes = fs::read(path).ok()?;
    verify_registry_bytes(&bytes).ok().map(|(doc, _)| doc)
}

fn builtin_doc() -> RegistryDoc {
    RegistryDoc {
        schema: REGISTRY_SCHEMA.into(),
        sequence: 0,
        issued_at: "2026-10-12T05:30:00Z".into(),
        key_id: KEY_ID.into(),
        revoked: Vec::new(),
        extensions: vec![RegExtension {
            id: "face-recognition".into(),
            app_id: APP_ID.into(),
            name: "Face recognition".into(),
            summary: "Optional Gallery add-on that groups faces on this computer after you install it. Photos are not uploaded. The signed catalog has not been downloaded yet.".into(),
            availability: "unavailable".into(),
            unavailable_reason: Some("catalog_not_loaded".into()),
            default_update_mode: "manual".into(),
            platforms: vec!["linux".into(), "darwin".into(), "windows".into()],
            release: None,
        }],
    }
}

fn active_doc() -> RegistryDoc {
    load_cached_registry().unwrap_or_else(builtin_doc)
}

fn installed_version_of(id: &str) -> String {
    let Ok(dir) = extension_dir(id) else {
        return String::new();
    };
    if id == "face-recognition" {
        return face_ext::installed_summary(&dir).0.unwrap_or_default();
    }
    let path = dir.join("menzies-extension.json");
    let Ok(raw) = fs::read_to_string(path) else {
        return String::new();
    };
    serde_json::from_str::<Value>(&raw)
        .ok()
        .and_then(|value| value.get("version")?.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn version_revoked(doc: &RegistryDoc, id: &str, version: &str) -> bool {
    doc.revoked
        .iter()
        .any(|item| item.id == id && item.version == version && item.sha256.is_none())
}

fn sha_revoked(doc: &RegistryDoc, id: &str, sha: &str) -> bool {
    doc.revoked
        .iter()
        .any(|item| item.id == id && item.sha256.as_deref() == Some(sha))
}

fn select_artifact(artifacts: &[RegArtifact]) -> Option<&RegArtifact> {
    let os = current_os();
    let arch = current_arch();
    artifacts
        .iter()
        .find(|artifact| artifact.os == os && artifact.arch == arch)
        .or_else(|| {
            artifacts.iter().find(|artifact| {
                (artifact.os == os || artifact.os == "any")
                    && (artifact.arch == arch || artifact.arch == "any")
            })
        })
}

fn app_is_new_enough(min_version: &str) -> bool {
    match (
        ext_sign::parse_semver(env!("CARGO_PKG_VERSION")),
        ext_sign::parse_semver(min_version),
    ) {
        (Some(app), Some(required)) => app >= required,
        _ => false,
    }
}

struct Decision {
    state: ExtensionState,
    can_install: bool,
    can_update: bool,
    blocker: Option<String>,
}

fn decide(
    entry: &RegExtension,
    installed_version: &str,
    busy: Option<&str>,
    allow_downgrade: bool,
    experimental_on: bool,
    runtime_ready: bool,
) -> Decision {
    let installed = !installed_version.is_empty();
    if busy == Some("installing") {
        return Decision {
            state: ExtensionState::Installing,
            can_install: false,
            can_update: false,
            blocker: None,
        };
    }
    if busy == Some("updating") {
        return Decision {
            state: ExtensionState::Updating,
            can_install: false,
            can_update: false,
            blocker: None,
        };
    }
    let release = entry.release.as_ref();
    if entry.availability != "available" {
        if installed {
            return face_blocker(
                entry,
                ExtensionState::Installed,
                false,
                false,
                experimental_on,
                runtime_ready,
            );
        }
        return Decision {
            state: ExtensionState::Unavailable,
            can_install: false,
            can_update: false,
            blocker: Some(
                entry
                    .unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "artifact_not_published".into()),
            ),
        };
    }
    let Some(release) = release else {
        return Decision {
            state: ExtensionState::Unavailable,
            can_install: false,
            can_update: false,
            blocker: Some("artifact_not_published".into()),
        };
    };
    if !app_is_new_enough(&release.min_app_version) {
        return Decision {
            state: ExtensionState::Error,
            can_install: false,
            can_update: false,
            blocker: Some("app_too_old".into()),
        };
    }
    if select_artifact(&release.artifacts).is_none() {
        return Decision {
            state: ExtensionState::Unavailable,
            can_install: false,
            can_update: false,
            blocker: Some("no_artifact".into()),
        };
    }
    if !installed {
        return Decision {
            state: ExtensionState::Available,
            can_install: true,
            can_update: false,
            blocker: None,
        };
    }
    let order = ext_sign::parse_semver(&release.version)
        .zip(ext_sign::parse_semver(installed_version))
        .map(|(next, current)| next.cmp(&current));
    if matches!(order, Some(Ordering::Greater))
        || (allow_downgrade && matches!(order, Some(Ordering::Less)))
    {
        return face_blocker(
            entry,
            ExtensionState::UpdateAvailable,
            false,
            true,
            experimental_on,
            runtime_ready,
        );
    }
    face_blocker(
        entry,
        ExtensionState::Installed,
        false,
        false,
        experimental_on,
        runtime_ready,
    )
}

fn face_blocker(
    entry: &RegExtension,
    state: ExtensionState,
    can_install: bool,
    can_update: bool,
    experimental_on: bool,
    runtime_ready: bool,
) -> Decision {
    let blocker = if entry.id == "face-recognition" {
        if !runtime_ready {
            Some("inference_not_linked".into())
        } else if !experimental_on {
            Some("experimental_off".into())
        } else {
            None
        }
    } else {
        None
    };
    Decision {
        state,
        can_install,
        can_update,
        blocker,
    }
}

fn views_from(doc: &RegistryDoc) -> Vec<ExtensionView> {
    let prefs = load_prefs().unwrap_or_default();
    let experimental = face_ext::read_experimental().unwrap_or(false);
    let busy = lock_mutex(&BUSY).clone();
    doc.extensions
        .iter()
        .filter(|entry| entry.app_id == APP_ID)
        .map(|entry| {
            let installed_version = installed_version_of(&entry.id);
            let busy_kind = busy
                .as_ref()
                .filter(|(id, _)| id == &entry.id)
                .map(|(_, kind)| *kind);
            let mut decision = decide(
                entry,
                &installed_version,
                busy_kind,
                prefs.allow_downgrade,
                experimental,
                entry.id == "face-recognition" && face_ext::recognition_runtime_ready(),
            );
            let registry_version = entry
                .release
                .as_ref()
                .map(|release| release.version.as_str())
                .unwrap_or("");
            if (!installed_version.is_empty()
                && version_revoked(doc, &entry.id, &installed_version)
                && !decision.can_update)
                || (!registry_version.is_empty()
                    && version_revoked(doc, &entry.id, registry_version))
            {
                decision = Decision {
                    state: ExtensionState::Error,
                    can_install: false,
                    can_update: false,
                    blocker: Some("revoked".into()),
                };
            }
            let pref = prefs.items.get(&entry.id);
            let update_mode = pref
                .map(|item| {
                    if item.update_mode == "automatic" {
                        "automatic"
                    } else {
                        "manual"
                    }
                })
                .unwrap_or(if entry.default_update_mode == "automatic" {
                    "automatic"
                } else {
                    "manual"
                });
            ExtensionView {
                id: entry.id.clone(),
                app_id: entry.app_id.clone(),
                name: entry.name.clone(),
                summary: entry.summary.clone(),
                state: decision.state,
                update_mode: update_mode.into(),
                installed_version: if installed_version.is_empty() {
                    None
                } else {
                    Some(installed_version.clone())
                },
                registry_version: entry
                    .release
                    .as_ref()
                    .map(|release| release.version.clone()),
                experimental_enabled: if entry.id == "face-recognition" {
                    Some(experimental)
                } else {
                    None
                },
                recognition_available: entry.id == "face-recognition"
                    && experimental
                    && !installed_version.is_empty()
                    && face_ext::recognition_runtime_ready(),
                blocker: decision.blocker,
                can_install: decision.can_install,
                can_update: decision.can_update,
                offline_fallback: entry.id == "face-recognition",
                error: error_for(&entry.id),
            }
        })
        .collect()
}

fn purge_partials() {
    let Ok(dir) = download_dir() else {
        return;
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".part") || name.ends_with(".staging") {
            let path = entry.path();
            if path.is_dir() {
                let _ = fs::remove_dir_all(path);
            } else {
                let _ = fs::remove_file(path);
            }
        }
    }
}

pub fn reject_local_downgrade(id: &str, version: &str) -> Result<(), ExtError> {
    if ext_sign::parse_semver(version).is_none() {
        return Err(ExtError::BadRegistry);
    }
    let prefs = load_prefs()?;
    if prefs.allow_downgrade {
        return Ok(());
    }
    let disk = installed_version_of(id);
    let from_prefs = prefs
        .items
        .get(id)
        .map(|item| item.installed_version.as_str())
        .unwrap_or("");
    let current = if disk_newer(&disk, from_prefs) {
        disk.as_str()
    } else {
        from_prefs
    };
    if current.is_empty() {
        return Ok(());
    }
    match (
        ext_sign::parse_semver(version),
        ext_sign::parse_semver(current),
    ) {
        (Some(next), Some(prev)) if next < prev => Err(ExtError::Downgrade),
        (Some(_), Some(_)) => Ok(()),
        _ => Err(ExtError::BadRegistry),
    }
}

fn disk_newer(disk: &str, prefs: &str) -> bool {
    match (ext_sign::parse_semver(disk), ext_sign::parse_semver(prefs)) {
        (Some(left), Some(right)) => left > right,
        (Some(_), None) => !disk.is_empty(),
        _ => false,
    }
}

pub fn record_local_install(id: &str, version: &str) -> Result<(), ExtError> {
    let mut prefs = load_prefs()?;
    let item = prefs.items.entry(id.to_string()).or_default();
    if item.update_mode != "automatic" {
        item.update_mode = "manual".into();
    }
    item.consented = true;
    item.installed_version = version.to_string();
    save_prefs(&prefs)
}

pub fn record_local_uninstall(id: &str) -> Result<(), ExtError> {
    let mut prefs = load_prefs()?;
    if let Some(item) = prefs.items.get_mut(id) {
        item.consented = false;
        item.installed_version.clear();
        item.update_mode = "manual".into();
    }
    save_prefs(&prefs)
}

fn http_client() -> Result<tauri_plugin_http::reqwest::Client, ExtError> {
    use std::sync::OnceLock;
    use tauri_plugin_http::reqwest::redirect::Policy;
    static CLIENT: OnceLock<tauri_plugin_http::reqwest::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let client = tauri_plugin_http::reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(30))
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() >= 3 {
                return attempt.error(io_other("too many redirects"));
            }
            if url_allowed(attempt.url().as_str()) {
                attempt.follow()
            } else {
                attempt.error(io_other("redirect host is not allowlisted"))
            }
        }))
        .user_agent(concat!(
            "menzies-gallery-extension/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|_| ExtError::Http)?;
    Ok(CLIENT.get_or_init(|| client).clone())
}

fn generation() -> u64 {
    GENERATION.load(AtomicOrdering::SeqCst)
}

async fn sleep_cancellable(total: Duration, generation_at_start: u64) -> Result<(), ExtError> {
    let slice = Duration::from_millis(100);
    let mut left = total;
    while !left.is_zero() {
        if generation() != generation_at_start {
            return Err(ExtError::Cancelled);
        }
        let step = if left < slice { left } else { slice };
        tokio::time::sleep(step).await;
        left = left.saturating_sub(step);
    }
    if generation() != generation_at_start {
        return Err(ExtError::Cancelled);
    }
    Ok(())
}

fn cache_usage(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_file() {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

fn oldest_cache_file(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut oldest: Option<(SystemTime, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let modified = meta.modified().unwrap_or(UNIX_EPOCH);
        let replace = match &oldest {
            None => true,
            Some((time, _)) => modified < *time,
        };
        if replace {
            oldest = Some((modified, path));
        }
    }
    oldest.map(|(_, path)| path)
}

fn reserve_download(incoming: u64) -> Result<(), ExtError> {
    if incoming == 0 || incoming > MAX_PACKAGE_BYTES || incoming > MAX_DOWNLOAD_CACHE_BYTES {
        return Err(ExtError::TooLarge);
    }
    let dir = download_dir()?;
    loop {
        if cache_usage(&dir).saturating_add(incoming) <= MAX_DOWNLOAD_CACHE_BYTES {
            return Ok(());
        }
        let Some(path) = oldest_cache_file(&dir) else {
            return Err(ExtError::TooLarge);
        };
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
}

async fn download_to(
    url: &str,
    dest: &Path,
    max_bytes: u64,
    generation_at_start: u64,
    on_progress: Option<&Channel<ExtensionTransferEvent>>,
) -> Result<(), ExtError> {
    if !url_allowed(url) {
        return Err(ExtError::BadRegistry);
    }
    reserve_download(max_bytes)?;
    let client = http_client()?;
    let mut delay = Duration::from_secs(1);
    let mut last = ExtError::Http;
    emit_progress(on_progress, "download", 0, max_bytes);
    for attempt in 0..ATTEMPTS {
        if generation() != generation_at_start {
            return Err(ExtError::Cancelled);
        }
        let send = client.get(url).send().await;
        match send {
            Ok(response) if response.status().is_success() => {
                if response.content_length().is_some_and(|len| len > max_bytes) {
                    return Err(ExtError::TooLarge);
                }
                if let Err(err) =
                    write_body(response, dest, max_bytes, generation_at_start, on_progress).await
                {
                    let _ = fs::remove_file(dest);
                    if matches!(err, ExtError::Cancelled | ExtError::TooLarge) {
                        return Err(err);
                    }
                    last = err;
                } else {
                    return Ok(());
                }
            }
            Ok(response) => {
                let status = response.status().as_u16();
                if (400..500).contains(&status) && status != 408 && status != 429 {
                    return Err(ExtError::Http);
                }
                last = ExtError::Http;
            }
            Err(_) => last = ExtError::Http,
        }
        if attempt + 1 < ATTEMPTS {
            sleep_cancellable(delay, generation_at_start).await?;
            delay = delay.saturating_mul(2);
        }
    }
    let _ = fs::remove_file(dest);
    Err(last)
}

async fn write_body(
    mut response: tauri_plugin_http::reqwest::Response,
    dest: &Path,
    max_bytes: u64,
    generation_at_start: u64,
    on_progress: Option<&Channel<ExtensionTransferEvent>>,
) -> Result<(), ExtError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(dest)?;
    let mut total = 0u64;
    let expected = response
        .content_length()
        .unwrap_or(max_bytes)
        .min(max_bytes);
    while let Some(chunk) = response.chunk().await.map_err(|_| ExtError::Http)? {
        if generation() != generation_at_start {
            return Err(ExtError::Cancelled);
        }
        total = total.saturating_add(chunk.len() as u64);
        if total > max_bytes {
            return Err(ExtError::TooLarge);
        }
        file.write_all(&chunk)?;
        emit_progress(
            on_progress,
            "download",
            total.min(expected),
            expected.max(1),
        );
    }
    file.sync_all()?;
    emit_progress(on_progress, "download", expected.max(1), expected.max(1));
    Ok(())
}

fn safe_zip_path(name: &str) -> Result<PathBuf, ExtError> {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') || name.contains(':') {
        return Err(ExtError::PathEscape);
    }
    let mut path = PathBuf::new();
    for part in name.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(ExtError::PathEscape);
        }
        path.push(part);
    }
    if path.as_os_str().is_empty() {
        return Err(ExtError::PathEscape);
    }
    Ok(path)
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), ExtError> {
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::create_dir_all(dest)?;
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| ExtError::BadRegistry)?;
    if archive.len() > MAX_ZIP_FILES {
        return Err(ExtError::TooLarge);
    }
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|_| ExtError::BadRegistry)?;
        let name = entry.name().to_string();
        if name.ends_with('/') {
            continue;
        }
        let rel = safe_zip_path(&name)?;
        let target = dest.join(&rel);
        let root = fs::canonicalize(dest)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&target)?;
        let mut buf = [0u8; 64 * 1024];
        loop {
            let read = entry.read(&mut buf)?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read as u64);
            if total > MAX_PACKAGE_BYTES {
                return Err(ExtError::TooLarge);
            }
            out.write_all(&buf[..read])?;
        }
        let canonical = fs::canonicalize(&target)?;
        if canonical != root && !canonical.starts_with(&root) {
            return Err(ExtError::PathEscape);
        }
    }
    Ok(())
}

fn remove_tree(path: &Path) -> Result<(), ExtError> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    if let Some(name) = path.file_name() {
        let previous = path.with_file_name(format!("{}.previous", name.to_string_lossy()));
        if previous.exists() {
            fs::remove_dir_all(previous)?;
        }
    }
    Ok(())
}

async fn install_release(
    doc: &RegistryDoc,
    entry: &RegExtension,
    kind: &'static str,
    preserve_mode: bool,
    on_progress: Option<&Channel<ExtensionTransferEvent>>,
) -> Result<(), ExtError> {
    if entry.app_id != APP_ID {
        return Err(ExtError::Unavailable);
    }
    if entry.availability != "available" {
        return Err(ExtError::Unavailable);
    }
    let release = entry.release.as_ref().ok_or(ExtError::Unavailable)?;
    if version_revoked(doc, &entry.id, &release.version) {
        return Err(ExtError::Revoked);
    }
    if !app_is_new_enough(&release.min_app_version) {
        return Err(ExtError::AppTooOld);
    }
    reject_local_downgrade(&entry.id, &release.version)?;
    let artifact = select_artifact(&release.artifacts).ok_or(ExtError::NoArtifact)?;
    if sha_revoked(doc, &entry.id, &artifact.sha256) {
        return Err(ExtError::Revoked);
    }
    {
        let mut busy = lock_mutex(&BUSY);
        if busy.is_some() {
            return Err(ExtError::Busy);
        }
        *busy = Some((entry.id.clone(), kind));
    }
    let _guard = BusyGuard;
    let generation_at_start = generation();
    let part = download_dir()?.join(format!("{}-{}.part", entry.id, release.version));
    download_to(
        &artifact.url,
        &part,
        artifact.size,
        generation_at_start,
        on_progress,
    )
    .await?;
    emit_progress(on_progress, "verify", 0, 1);
    let hashed = hashutil::sha256_file(&part, artifact.size).map_err(|err| {
        if err.kind() == std::io::ErrorKind::InvalidData {
            ExtError::TooLarge
        } else {
            ExtError::Io(err)
        }
    })?;
    if !hashed.eq_ignore_ascii_case(&artifact.sha256) {
        let _ = fs::remove_file(&part);
        return Err(ExtError::HashMismatch);
    }
    let bytes = fs::read(&part)?;
    if bytes.len() as u64 != artifact.size {
        let _ = fs::remove_file(&part);
        return Err(ExtError::HashMismatch);
    }
    ext_sign::verify_artifact(&bytes, &artifact.signature).map_err(sign_to_ext)?;
    emit_progress(on_progress, "verify", 1, 1);
    emit_progress(on_progress, "extract", 0, 1);
    let staging = download_dir()?.join(format!("{}-{}.staging", entry.id, release.version));
    extract_zip(&part, &staging)?;
    let _ = fs::remove_file(&part);
    emit_progress(on_progress, "extract", 1, 1);
    emit_progress(on_progress, "install", 0, 1);
    let dest = extension_dir(&entry.id)?;
    if entry.id == "face-recognition" {
        face_ext::install_verified_package(&staging, &dest).map_err(face_to_ext)?;
    } else {
        install_generic_package(&staging, &dest, &entry.id, &release.version)?;
    }
    let _ = fs::remove_dir_all(&staging);
    let mut prefs = load_prefs()?;
    let item = prefs.items.entry(entry.id.clone()).or_default();
    if !preserve_mode || (item.update_mode != "manual" && item.update_mode != "automatic") {
        item.update_mode = "manual".into();
    }
    item.consented = true;
    item.installed_version = release.version.clone();
    save_prefs(&prefs)?;
    set_error(&entry.id, None);
    emit_progress(on_progress, "install", 1, 1);
    emit_progress(on_progress, "complete", 1, 1);
    Ok(())
}

fn face_to_ext(err: face_ext::FaceError) -> ExtError {
    match err {
        face_ext::FaceError::ExperimentalOff => ExtError::ExperimentalOff,
        face_ext::FaceError::HashMismatch => ExtError::HashMismatch,
        face_ext::FaceError::PathEscape => ExtError::PathEscape,
        face_ext::FaceError::TooLarge => ExtError::TooLarge,
        face_ext::FaceError::BadSignature => ExtError::BadSignature,
        face_ext::FaceError::Io(io) => ExtError::Io(io),
        face_ext::FaceError::BadManifest | face_ext::FaceError::InferenceNotLinked => {
            ExtError::BadRegistry
        }
    }
}

fn install_generic_package(
    staging: &Path,
    dest: &Path,
    id: &str,
    version: &str,
) -> Result<(), ExtError> {
    let manifest_path = staging.join("menzies-extension.json");
    let raw = fs::read_to_string(&manifest_path)?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| ExtError::BadRegistry)?;
    if value.get("format").and_then(Value::as_str) != Some("menzies-extension-package/1") {
        return Err(ExtError::BadRegistry);
    }
    if value.get("id").and_then(Value::as_str) != Some(id) {
        return Err(ExtError::BadRegistry);
    }
    if value.get("version").and_then(Value::as_str) != Some(version) {
        return Err(ExtError::BadRegistry);
    }
    let files = value
        .get("files")
        .and_then(Value::as_array)
        .ok_or(ExtError::BadRegistry)?;
    if files.is_empty() || files.len() > MAX_ZIP_FILES {
        return Err(ExtError::BadRegistry);
    }
    for file in files {
        let rel = file.get("path").and_then(Value::as_str).unwrap_or("");
        let sha = file.get("sha256").and_then(Value::as_str).unwrap_or("");
        let rel_path = safe_zip_path(rel)?;
        let source = staging.join(&rel_path);
        let hashed = hashutil::sha256_file(&source, MAX_PACKAGE_BYTES)
            .map_err(|_| ExtError::HashMismatch)?;
        if !hashed.eq_ignore_ascii_case(sha) {
            return Err(ExtError::HashMismatch);
        }
    }
    face_ext::swap_staged(staging, dest).map_err(face_to_ext)?;
    Ok(())
}

async fn maybe_auto_update(doc: &RegistryDoc) -> Result<(), ExtError> {
    let prefs = load_prefs()?;
    for entry in &doc.extensions {
        if entry.app_id != APP_ID || entry.availability != "available" {
            continue;
        }
        let Some(pref) = prefs.items.get(&entry.id) else {
            continue;
        };
        if !pref.consented || pref.update_mode != "automatic" || pref.installed_version.is_empty() {
            continue;
        }
        let Some(release) = &entry.release else {
            continue;
        };
        let order = ext_sign::parse_semver(&release.version)
            .zip(ext_sign::parse_semver(&pref.installed_version))
            .map(|(next, current)| next.cmp(&current));
        let should = matches!(order, Some(Ordering::Greater))
            || (prefs.allow_downgrade && matches!(order, Some(Ordering::Less)));
        if !should {
            continue;
        }
        install_release(doc, entry, "updating", true, None).await?;
    }
    Ok(())
}

async fn fetch_registry(force: bool) -> Result<(), ExtError> {
    let mut prefs = load_prefs()?;
    if !force && prefs.last_checked_ms > 0 {
        let age = now_ms().saturating_sub(prefs.last_checked_ms);
        if age < CHECK_BACKOFF_MS {
            return Ok(());
        }
    }
    let url = registry_url(&prefs);
    let part = download_dir()?.join("registry.json.part");
    download_to(&url, &part, MAX_REGISTRY_BYTES, generation(), None).await?;
    let bytes = fs::read(&part)?;
    let _ = fs::remove_file(&part);
    let (doc, signature) = verify_registry_bytes(&bytes)?;
    accept_sequence(&prefs, doc.sequence, &signature)?;
    fs::write(registry_cache_path()?, &bytes)?;
    prefs.last_sequence = doc.sequence;
    prefs.last_signature = signature;
    prefs.last_checked_ms = now_ms();
    save_prefs(&prefs)?;
    set_error("*", None);
    maybe_auto_update(&doc).await?;
    Ok(())
}

#[tauri::command]
pub fn extension_catalog() -> Result<Vec<ExtensionView>, String> {
    purge_partials();
    Ok(views_from(&active_doc()))
}

#[tauri::command]
pub async fn extension_check(force: bool) -> Result<Vec<ExtensionView>, String> {
    if CHECKING.swap(true, AtomicOrdering::SeqCst) {
        return Ok(views_from(&active_doc()));
    }
    let _guard = CheckGuard;
    if let Err(err) = fetch_registry(force).await {
        set_error("*", Some(err.code()));
        log::warn!("extension registry check failed: {err}");
    }
    extension_catalog()
}

#[tauri::command]
pub async fn extension_install(
    id: String,
    on_progress: Channel<ExtensionTransferEvent>,
) -> Result<Vec<ExtensionView>, String> {
    let doc = load_cached_registry().ok_or_else(|| ExtError::Unavailable.into_command())?;
    let entry = doc
        .extensions
        .iter()
        .find(|entry| entry.id == id && entry.app_id == APP_ID)
        .ok_or_else(|| ExtError::Unavailable.into_command())?;
    let preserve = load_prefs()
        .ok()
        .and_then(|prefs| prefs.items.get(&id).map(|item| item.consented))
        .unwrap_or(false);
    install_release(&doc, entry, "installing", preserve, Some(&on_progress))
        .await
        .map_err(|err| {
            set_error(&id, Some(err.code()));
            err.into_command()
        })?;
    extension_catalog()
}

#[tauri::command]
pub async fn extension_update(
    id: String,
    on_progress: Channel<ExtensionTransferEvent>,
) -> Result<Vec<ExtensionView>, String> {
    let doc = load_cached_registry().ok_or_else(|| ExtError::Unavailable.into_command())?;
    let entry = doc
        .extensions
        .iter()
        .find(|entry| entry.id == id && entry.app_id == APP_ID)
        .ok_or_else(|| ExtError::Unavailable.into_command())?;
    let installed = installed_version_of(&id);
    if installed.is_empty() {
        return Err(ExtError::NotInstalled.into_command());
    }
    install_release(&doc, entry, "updating", true, Some(&on_progress))
        .await
        .map_err(|err| {
            set_error(&id, Some(err.code()));
            err.into_command()
        })?;
    extension_catalog()
}

#[tauri::command]
pub fn extension_set_update_mode(id: String, mode: String) -> Result<Vec<ExtensionView>, String> {
    if mode != "manual" && mode != "automatic" {
        return Err(ExtError::BadRegistry.into_command());
    }
    if installed_version_of(&id).is_empty() {
        return Err(ExtError::NotInstalled.into_command());
    }
    let mut prefs = load_prefs().map_err(ExtError::into_command)?;
    let item = prefs.items.entry(id).or_default();
    if !item.consented {
        return Err(ExtError::NotInstalled.into_command());
    }
    item.update_mode = mode;
    save_prefs(&prefs).map_err(ExtError::into_command)?;
    extension_catalog()
}

#[tauri::command]
pub fn extension_uninstall(
    id: String,
    on_progress: Channel<ExtensionTransferEvent>,
) -> Result<Vec<ExtensionView>, String> {
    if !valid_id(&id) {
        return Err(ExtError::BadRegistry.into_command());
    }
    emit_progress(Some(&on_progress), "remove", 0, 1);
    let dest = extension_dir(&id).map_err(ExtError::into_command)?;
    remove_tree(&dest).map_err(ExtError::into_command)?;
    record_local_uninstall(&id).map_err(ExtError::into_command)?;
    set_error(&id, None);
    emit_progress(Some(&on_progress), "remove", 1, 1);
    emit_progress(Some(&on_progress), "complete", 1, 1);
    extension_catalog()
}

#[tauri::command]
pub fn extension_cancel_transfer() -> Result<Vec<ExtensionView>, String> {
    GENERATION.fetch_add(1, AtomicOrdering::SeqCst);
    extension_catalog()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn unavailable_face_entry_cannot_install() {
        let doc = builtin_doc();
        let entry = &doc.extensions[0];
        let decision = decide(entry, "", None, false, true, false);
        assert!(matches!(decision.state, ExtensionState::Unavailable));
        assert!(!decision.can_install);
        assert_eq!(decision.blocker.as_deref(), Some("catalog_not_loaded"));
        let installed = decide(entry, "0.1.0", None, false, true, false);
        assert!(matches!(installed.state, ExtensionState::Installed));
        assert!(!installed.can_update);
        assert_eq!(installed.blocker.as_deref(), Some("inference_not_linked"));
    }

    #[test]
    fn downgrade_and_hosts_are_rejected() {
        let err = reject_sequence_case(2, "sig-a", 2, "sig-b");
        assert!(matches!(err, Err(ExtError::Downgrade)));
        let ok = reject_sequence_case(2, "sig-a", 3, "sig-c");
        assert!(ok.is_ok());
        assert!(!url_allowed(
            "http://menzies.mariesta.com/extensions/registry.json"
        ));
        assert!(!url_allowed(
            "https://store.menzies.design/extensions/registry.json"
        ));
        assert!(!url_allowed(
            "https://evil.example/extensions/registry.json"
        ));
        assert!(url_allowed(
            "https://menzies.mariesta.com/extensions/registry.json"
        ));
        assert!(url_allowed(
            "http://127.0.0.1:4010/extensions/registry.json"
        ));
        assert!(url_allowed(
            "https://menzies.mariesta.com/extensions/packages/gallery-desktop-menzies/face-recognition/1.0.0/linux-x86_64.zip"
        ));
    }

    fn reject_sequence_case(
        last: u64,
        last_sig: &str,
        next: u64,
        next_sig: &str,
    ) -> Result<(), ExtError> {
        let prefs = ExtensionsFile {
            last_sequence: last,
            last_signature: last_sig.into(),
            ..ExtensionsFile::default()
        };
        accept_sequence(&prefs, next, next_sig)
    }

    #[test]
    fn zip_slip_is_rejected() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("gallery-ext-zip-{nanos}"));
        fs::create_dir_all(&root).expect("root");
        let zip_path = root.join("bad.zip");
        let file = File::create(&zip_path).expect("zip");
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("../evil.txt", zip::write::SimpleFileOptions::default())
            .expect("start");
        writer.write_all(b"nope").expect("write");
        writer.finish().expect("finish");
        let dest = root.join("out");
        let err = extract_zip(&zip_path, &dest);
        assert!(err.is_err());
        assert!(!root.join("evil.txt").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn manual_is_the_default_preference() {
        let pref = ExtensionPref::default();
        assert_eq!(pref.update_mode, "manual");
        assert!(!pref.consented);
    }
}
