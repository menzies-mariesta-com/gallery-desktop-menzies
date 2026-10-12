//! Local face-recognition extension install.
//!
//! This is a face detector plus an embedding model, not a general language model.
//! Package hashes are checked before copy. Inference runs only after install
//! and after the experimental switch is on.

use crate::gallery;
use crate::hashutil;
use crate::paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const FORMAT_ID: &str = "menzies-gallery-face-extension/1";
const EXTENSION_ID: &str = "face-recognition";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_MODEL_BYTES: u64 = 180 * 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 200 * 1024 * 1024;
const MAX_LICENSE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FaceError {
    #[error("experimental face recognition is off")]
    ExperimentalOff,
    #[error("extension package is invalid")]
    BadManifest,
    #[error("a model file hash does not match the manifest")]
    HashMismatch,
    #[error("package path escapes the selected folder")]
    PathEscape,
    #[error("package is too large")]
    TooLarge,
    #[error("package signature is invalid")]
    BadSignature,
    #[error("face recognition runtime is not linked")]
    InferenceNotLinked,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl FaceError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::ExperimentalOff => "experimental_off",
            Self::BadManifest => "bad_manifest",
            Self::HashMismatch => "hash_mismatch",
            Self::PathEscape => "path_escape",
            Self::TooLarge => "too_large",
            Self::BadSignature => "bad_signature",
            Self::InferenceNotLinked => "inference_not_linked",
            Self::Io(_) => "io",
        }
    }

    fn into_command(self) -> String {
        gallery::command_err(self.code(), self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FaceManifest {
    format: String,
    id: String,
    version: String,
    models: Vec<FaceModel>,
    #[serde(default)]
    runtime: Vec<FaceRuntime>,
    #[serde(default)]
    signature: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FaceModel {
    role: String,
    file: String,
    sha256: String,
    #[serde(default)]
    license: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FaceRuntime {
    os: String,
    arch: String,
    file: String,
    sha256: String,
    #[serde(default)]
    license: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExperimentalFile {
    #[serde(default)]
    face_recognition: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceExtensionStatus {
    pub experimental_enabled: bool,
    pub installed: bool,
    pub recognition_available: bool,
    pub inference: String,
    pub hosted_artifact: String,
    pub install_dir: String,
    pub manifest_version: Option<String>,
    pub detector_sha256: Option<String>,
    pub embedder_sha256: Option<String>,
    pub blocker: String,
}

fn io_other(err: impl std::fmt::Display) -> std::io::Error {
    // Stable Clippy prefers Error::other. The suite MSRV is 1.77, where that constructor is newer.
    #[allow(clippy::io_other_error)]
    std::io::Error::new(std::io::ErrorKind::Other, err.to_string())
}

fn experimental_path() -> Result<PathBuf, FaceError> {
    let dirs = paths::ensure_app_dirs().map_err(io_other)?;
    Ok(PathBuf::from(dirs.config_dir).join("experimental.json"))
}

fn install_dir() -> Result<PathBuf, FaceError> {
    let dirs = paths::ensure_app_dirs().map_err(io_other)?;
    Ok(PathBuf::from(dirs.data_dir)
        .join("extensions")
        .join(EXTENSION_ID))
}

pub(crate) fn read_experimental() -> Result<bool, FaceError> {
    let path = experimental_path()?;
    if !path.exists() {
        return Ok(false);
    }
    let raw = fs::read_to_string(&path)?;
    let parsed: ExperimentalFile = serde_json::from_str(&raw).unwrap_or_default();
    Ok(parsed.face_recognition)
}

fn write_experimental(enabled: bool) -> Result<(), FaceError> {
    let path = experimental_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(&ExperimentalFile {
        face_recognition: enabled,
    })
    .map_err(|_| FaceError::BadManifest)?;
    fs::write(path, raw)?;
    Ok(())
}

fn valid_version(version: &str) -> bool {
    crate::ext_sign::parse_semver(version).is_some()
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn relative_inside(root: &Path, rel: &str) -> Result<PathBuf, FaceError> {
    if rel.is_empty()
        || rel.starts_with('/')
        || rel.contains('\\')
        || rel
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(FaceError::PathEscape);
    }
    let root = fs::canonicalize(root)?;
    let full = fs::canonicalize(root.join(rel))?;
    if full != root && !full.starts_with(&root) {
        return Err(FaceError::PathEscape);
    }
    if !full.is_file() {
        return Err(FaceError::BadManifest);
    }
    Ok(full)
}

fn validate_manifest(manifest: &FaceManifest) -> Result<(), FaceError> {
    if manifest.format != FORMAT_ID
        || manifest.id != EXTENSION_ID
        || !valid_version(&manifest.version)
    {
        return Err(FaceError::BadManifest);
    }
    if manifest.signature.as_ref().is_some_and(String::is_empty) {
        return Err(FaceError::BadSignature);
    }
    if manifest.models.len() != 2 {
        return Err(FaceError::BadManifest);
    }
    let mut detector = false;
    let mut embedder = false;
    for model in &manifest.models {
        match model.role.as_str() {
            "detector" => detector = true,
            "embedder" => embedder = true,
            _ => return Err(FaceError::BadManifest),
        }
        if !valid_sha256(&model.sha256) {
            return Err(FaceError::BadManifest);
        }
    }
    if !(detector && embedder) {
        return Err(FaceError::BadManifest);
    }
    if manifest.runtime.len() > 8 {
        return Err(FaceError::BadManifest);
    }
    for runtime in &manifest.runtime {
        if !matches!(runtime.os.as_str(), "linux" | "darwin" | "windows")
            || !matches!(runtime.arch.as_str(), "x86_64" | "aarch64")
            || !valid_sha256(&runtime.sha256)
        {
            return Err(FaceError::BadManifest);
        }
    }
    Ok(())
}

pub(crate) fn install_verified_package(
    package_dir: &Path,
    dest_dir: &Path,
) -> Result<(), FaceError> {
    let package = fs::canonicalize(package_dir)?;
    if !package.is_dir() {
        return Err(FaceError::BadManifest);
    }
    let manifest_path = package.join("manifest.json");
    let meta = fs::metadata(&manifest_path)?;
    if meta.len() > MAX_MANIFEST_BYTES {
        return Err(FaceError::TooLarge);
    }
    let raw = fs::read_to_string(&manifest_path)?;
    verify_optional_signature(&raw)?;
    let manifest: FaceManifest = serde_json::from_str(&raw).map_err(|_| FaceError::BadManifest)?;
    validate_manifest(&manifest)?;

    let mut planned: Vec<(String, PathBuf)> = vec![("manifest.json".into(), manifest_path)];
    let mut total = 0u64;
    for model in &manifest.models {
        let source = relative_inside(&package, &model.file)?;
        let hashed = hashutil::sha256_file(&source, MAX_MODEL_BYTES).map_err(|err| {
            if err.kind() == std::io::ErrorKind::InvalidData {
                FaceError::TooLarge
            } else {
                FaceError::Io(err)
            }
        })?;
        if !hashed.eq_ignore_ascii_case(&model.sha256) {
            return Err(FaceError::HashMismatch);
        }
        let len = fs::metadata(&source)?.len();
        total = total.saturating_add(len);
        if total > MAX_PACKAGE_BYTES {
            return Err(FaceError::TooLarge);
        }
        planned.push((model.file.clone(), source));
        if let Some(license) = &model.license {
            let license_path = relative_inside(&package, license)?;
            let license_len = fs::metadata(&license_path)?.len();
            if license_len > MAX_LICENSE_BYTES {
                return Err(FaceError::TooLarge);
            }
            total = total.saturating_add(license_len);
            if total > MAX_PACKAGE_BYTES {
                return Err(FaceError::TooLarge);
            }
            planned.push((license.clone(), license_path));
        }
    }
    for runtime in &manifest.runtime {
        let source = relative_inside(&package, &runtime.file)?;
        let hashed = hashutil::sha256_file(&source, MAX_MODEL_BYTES).map_err(|err| {
            if err.kind() == std::io::ErrorKind::InvalidData {
                FaceError::TooLarge
            } else {
                FaceError::Io(err)
            }
        })?;
        if !hashed.eq_ignore_ascii_case(&runtime.sha256) {
            return Err(FaceError::HashMismatch);
        }
        let len = fs::metadata(&source)?.len();
        total = total.saturating_add(len);
        if total > MAX_PACKAGE_BYTES {
            return Err(FaceError::TooLarge);
        }
        planned.push((runtime.file.clone(), source));
        if let Some(license) = &runtime.license {
            let license_path = relative_inside(&package, license)?;
            let license_len = fs::metadata(&license_path)?.len();
            if license_len > MAX_LICENSE_BYTES {
                return Err(FaceError::TooLarge);
            }
            total = total.saturating_add(license_len);
            if total > MAX_PACKAGE_BYTES {
                return Err(FaceError::TooLarge);
            }
            planned.push((license.clone(), license_path));
        }
    }

    let staging = dest_dir.with_file_name(format!("{EXTENSION_ID}.staging"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    if let Some(parent) = staging.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir_all(&staging)?;
    for (rel, source) in &planned {
        let target = staging.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, &target)?;
    }
    swap_staged(&staging, dest_dir)?;
    Ok(())
}

fn verify_optional_signature(raw: &str) -> Result<(), FaceError> {
    let mut value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| FaceError::BadManifest)?;
    let signature = match value.get("signature") {
        None | Some(serde_json::Value::Null) => return Ok(()),
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(_) => return Err(FaceError::BadSignature),
    };
    let Some(object) = value.as_object_mut() else {
        return Err(FaceError::BadManifest);
    };
    object.remove("signature");
    let canonical = crate::ext_sign::canonical_json(&value).map_err(|_| FaceError::BadSignature)?;
    crate::ext_sign::verify_domain(crate::ext_sign::FACE_DOMAIN, &canonical, &signature)
        .map_err(|_| FaceError::BadSignature)
}

pub(crate) fn swap_staged(staging: &Path, dest: &Path) -> Result<(), FaceError> {
    let Some(name) = dest.file_name() else {
        return Err(FaceError::BadManifest);
    };
    let previous = dest.with_file_name(format!("{}.previous", name.to_string_lossy()));
    if previous.exists() {
        fs::remove_dir_all(&previous)?;
    }
    if dest.exists() {
        fs::rename(dest, &previous)?;
    }
    if let Err(err) = fs::rename(staging, dest) {
        if previous.exists() && !dest.exists() {
            let _ = fs::rename(&previous, dest);
        }
        return Err(FaceError::Io(err));
    }
    Ok(())
}

pub(crate) fn installed_summary(dir: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let manifest_path = dir.join("manifest.json");
    let Ok(raw) = fs::read_to_string(&manifest_path) else {
        return (None, None, None);
    };
    let Ok(manifest) = serde_json::from_str::<FaceManifest>(&raw) else {
        return (None, None, None);
    };
    let mut detector = None;
    let mut embedder = None;
    for model in &manifest.models {
        let hash = Some(model.sha256.to_ascii_lowercase());
        if model.role == "detector" {
            detector = hash;
        } else if model.role == "embedder" {
            embedder = hash;
        }
    }
    (Some(manifest.version), detector, embedder)
}

fn status_from(experimental_enabled: bool, dir: PathBuf) -> FaceExtensionStatus {
    let installed = dir.join("manifest.json").is_file();
    let (manifest_version, detector_sha256, embedder_sha256) = if installed {
        installed_summary(&dir)
    } else {
        (None, None, None)
    };
    let runtime_ready = crate::face_infer::runtime_compiled() && load_installed(&dir).is_some();
    let blocker = if !installed {
        "not_installed"
    } else if !experimental_enabled {
        "experimental_off"
    } else if !runtime_ready {
        FaceError::InferenceNotLinked.code()
    } else {
        ""
    };
    FaceExtensionStatus {
        experimental_enabled,
        installed,
        recognition_available: experimental_enabled && runtime_ready,
        inference: if runtime_ready {
            "onnxruntime".into()
        } else {
            "not_linked".into()
        },
        hosted_artifact: "https://menzies.mariesta.com/extensions/registry.json".into(),
        install_dir: dir.to_string_lossy().into_owned(),
        manifest_version,
        detector_sha256,
        embedder_sha256,
        blocker: blocker.into(),
    }
}

fn target_os() -> &'static str {
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

fn target_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    }
}

pub struct InstalledFace {
    pub model_version: String,
    pub detector: PathBuf,
    pub embedder: PathBuf,
    pub runtime: PathBuf,
}

fn load_installed(dir: &Path) -> Option<InstalledFace> {
    let raw = fs::read_to_string(dir.join("manifest.json")).ok()?;
    let manifest: FaceManifest = serde_json::from_str(&raw).ok()?;
    if validate_manifest(&manifest).is_err() {
        return None;
    }
    let mut detector = None;
    let mut embedder = None;
    let mut detector_sha = None;
    for model in &manifest.models {
        let path = dir.join(&model.file);
        if !path.is_file() {
            return None;
        }
        if model.role == "detector" {
            detector = Some(path);
            detector_sha = Some(model.sha256.to_ascii_lowercase());
        } else if model.role == "embedder" {
            embedder = Some(path);
        }
    }
    let runtime = manifest.runtime.iter().find(|runtime| {
        runtime.os == target_os()
            && runtime.arch == target_arch()
            && dir.join(&runtime.file).is_file()
    })?;
    Some(InstalledFace {
        model_version: format!("{}:{}", manifest.version, detector_sha.unwrap_or_default()),
        detector: detector?,
        embedder: embedder?,
        runtime: dir.join(&runtime.file),
    })
}

pub(crate) fn recognition_runtime_ready() -> bool {
    install_dir()
        .ok()
        .is_some_and(|dir| crate::face_infer::runtime_compiled() && load_installed(&dir).is_some())
}

pub(crate) fn require_installed() -> Result<InstalledFace, FaceError> {
    if !read_experimental()? {
        return Err(FaceError::ExperimentalOff);
    }
    let dir = install_dir()?;
    load_installed(&dir).ok_or(FaceError::InferenceNotLinked)
}

#[tauri::command]
pub fn face_extension_status() -> Result<FaceExtensionStatus, String> {
    let enabled = read_experimental().map_err(FaceError::into_command)?;
    let dir = install_dir().map_err(FaceError::into_command)?;
    Ok(status_from(enabled, dir))
}

#[tauri::command]
pub fn set_face_recognition_experimental(enabled: bool) -> Result<FaceExtensionStatus, String> {
    write_experimental(enabled).map_err(FaceError::into_command)?;
    face_extension_status()
}

fn package_version(package_dir: &Path) -> Result<String, FaceError> {
    let raw = fs::read_to_string(package_dir.join("manifest.json"))?;
    let manifest: FaceManifest = serde_json::from_str(&raw).map_err(|_| FaceError::BadManifest)?;
    if !valid_version(&manifest.version) {
        return Err(FaceError::BadManifest);
    }
    Ok(manifest.version)
}

#[tauri::command]
pub fn install_face_extension(package_dir: String) -> Result<FaceExtensionStatus, String> {
    let package = Path::new(&package_dir);
    let version = package_version(package).map_err(FaceError::into_command)?;
    crate::ext_registry::reject_local_downgrade(EXTENSION_ID, &version)
        .map_err(|err| err.into_command())?;
    let dest = install_dir().map_err(FaceError::into_command)?;
    install_verified_package(package, &dest).map_err(FaceError::into_command)?;
    crate::ext_registry::record_local_install(EXTENSION_ID, &version)
        .map_err(|err| err.into_command())?;
    face_extension_status()
}

#[tauri::command]
pub fn uninstall_face_extension() -> Result<FaceExtensionStatus, String> {
    let dest = install_dir().map_err(FaceError::into_command)?;
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|err| FaceError::Io(err).into_command())?;
    }
    if let Some(name) = dest.file_name() {
        let previous = dest.with_file_name(format!("{}.previous", name.to_string_lossy()));
        if previous.exists() {
            fs::remove_dir_all(previous).map_err(|err| FaceError::Io(err).into_command())?;
        }
    }
    crate::ext_registry::record_local_uninstall(EXTENSION_ID).map_err(|err| err.into_command())?;
    face_extension_status()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("gallery-face-{label}-{nanos}"));
        fs::create_dir_all(&dir).expect("dir");
        dir
    }

    fn write_package(dir: &Path, detector_hash: &str, signature: bool) {
        fs::write(dir.join("detector.onnx"), b"detector-bytes").expect("detector");
        fs::write(dir.join("embedder.onnx"), b"embedder-bytes").expect("embedder");
        fs::write(dir.join("license.txt"), b"Example license").expect("license");
        let embedder_hash = hashutil::sha256_hex(b"embedder-bytes");
        let signature_line = if signature {
            "\n  \"signature\": \"not-a-real-signature\","
        } else {
            ""
        };
        let manifest = format!(
            r#"{{
  "format": "menzies-gallery-face-extension/1",
  "id": "face-recognition",
  "version": "0.1.0",{signature_line}
  "models": [
    {{
      "role": "detector",
      "file": "detector.onnx",
      "sha256": "{detector_hash}",
      "license": "license.txt"
    }},
    {{
      "role": "embedder",
      "file": "embedder.onnx",
      "sha256": "{embedder_hash}"
    }}
  ]
}}"#
        );
        fs::write(dir.join("manifest.json"), manifest).expect("manifest");
    }

    #[test]
    fn install_checks_hash_and_rejects_unverified_signatures() {
        let package = temp_dir("pkg");
        let dest = temp_dir("dest");
        let dest_dir = dest.join("face-recognition");
        write_package(&package, &hashutil::sha256_hex(b"detector-bytes"), true);
        let err = install_verified_package(&package, &dest_dir).expect_err("signature");
        assert!(matches!(err, FaceError::BadSignature));

        write_package(
            &package,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            false,
        );
        let err = install_verified_package(&package, &dest_dir).expect_err("hash");
        assert!(matches!(err, FaceError::HashMismatch));

        write_package(&package, &hashutil::sha256_hex(b"detector-bytes"), false);
        install_verified_package(&package, &dest_dir).expect("install");

        if let Some(real) = std::env::var_os("MENZIES_FACE_EXT_DIR") {
            let real_dest = temp_dir("real-dest").join("face-recognition");
            install_verified_package(Path::new(&real), &real_dest).expect("real package");
            assert!(real_dest.join("runtime/libonnxruntime.so").is_file());
            assert!(real_dest.join("detector.onnx").is_file());
            assert!(real_dest.join("embedder.onnx").is_file());
        }
        assert!(dest_dir.join("detector.onnx").is_file());
        assert!(dest_dir.join("embedder.onnx").is_file());
        assert!(dest_dir.join("license.txt").is_file());
        let status = status_from(true, dest_dir.clone());
        assert!(status.installed);
        assert!(!status.recognition_available);
        assert_eq!(status.inference, "not_linked");
        assert_eq!(
            status.hosted_artifact,
            "https://menzies.mariesta.com/extensions/registry.json"
        );
        assert_eq!(status.blocker, "inference_not_linked");
        assert_eq!(status.manifest_version.as_deref(), Some("0.1.0"));

        let manifest = fs::read_to_string(package.join("manifest.json")).expect("read");
        let escaped = manifest.replace("detector.onnx", "../detector.onnx");
        fs::write(package.join("manifest.json"), escaped).expect("rewrite");
        let err = install_verified_package(&package, &dest_dir).expect_err("escape");
        assert!(matches!(err, FaceError::PathEscape));

        let _ = fs::remove_dir_all(&package);
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn experimental_off_blocks_status_before_runtime() {
        let dir = temp_dir("empty");
        let status = status_from(false, dir.join("missing"));
        assert!(!status.experimental_enabled);
        assert!(!status.installed);
        assert_eq!(status.blocker, "not_installed");
        assert!(!status.recognition_available);
        let _ = fs::remove_dir_all(&dir);
    }
}
