//! Raster edit pipeline. Enhancement is Lanczos upscale, unsharp mask, and contrast.
//! It is not a generative model. Writes require an explicit replace or save-as disposition.

use crate::gallery::{self, GalleryError};
use crate::paths;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits, RgbImage};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_FILE_BYTES: u64 = 80 * 1024 * 1024;
const MAX_SIDE: u32 = 16_384;
const MAX_PIXELS: u64 = 40_000_000;
const MAX_FILTERS: usize = 8;
const MAX_PREVIEW_FILES: usize = 12;
/// Live preview stays interactive; commit still uses full resolution.
const PREVIEW_MAX_SIDE: u32 = 1280;
const PREVIEW_DIR: &str = "cache/edit-preview";

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error(transparent)]
    Gallery(#[from] GalleryError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("image could not be read")]
    Decode,
    #[error("image could not be encoded")]
    Encode,
    #[error("svg and other non-raster files cannot be edited")]
    NotRaster,
    #[error("unsupported image format")]
    Unsupported,
    #[error("image exceeds the size limit")]
    TooLarge,
    #[error("crop rectangle is outside the image")]
    CropOutOfBounds,
    #[error("edit options are invalid")]
    InvalidOps,
    #[error("a file name is required")]
    NameRequired,
    #[error("file name is not allowed")]
    BadName,
    #[error("file name extension does not match the output format")]
    NameExtensionMismatch,
    #[error("replace keeps the original format; choose save as new to convert")]
    ReplaceFormatMismatch,
    #[error("a file with that name already exists")]
    TargetExists,
    #[error("preview path is outside the edit cache")]
    PreviewEscape,
}

impl EditError {
    fn code(&self) -> &'static str {
        match self {
            Self::Gallery(GalleryError::PathEscape) => "path_escape",
            Self::Gallery(GalleryError::NotFile(_)) => "not_file",
            Self::Gallery(GalleryError::NotDirectory(_)) => "not_directory",
            Self::Gallery(GalleryError::NoPicturesDir) => "no_pictures_dir",
            Self::Gallery(_) => "gallery",
            Self::Io(_) => "io",
            Self::Decode => "decode",
            Self::Encode => "encode",
            Self::NotRaster => "not_raster",
            Self::Unsupported => "unsupported",
            Self::TooLarge => "too_large",
            Self::CropOutOfBounds => "crop_out_of_bounds",
            Self::InvalidOps => "invalid_ops",
            Self::NameRequired => "name_required",
            Self::BadName => "bad_name",
            Self::NameExtensionMismatch => "name_extension_mismatch",
            Self::ReplaceFormatMismatch => "replace_format_mismatch",
            Self::TargetExists => "target_exists",
            Self::PreviewEscape => "preview_escape",
        }
    }

    fn into_command(self) -> String {
        gallery::command_err(self.code(), self)
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Rotate {
    #[default]
    None,
    Cw90,
    Ccw90,
    Deg180,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResizeMode {
    Exact,
    FitInside,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeSpec {
    pub width: u32,
    pub height: u32,
    pub mode: ResizeMode,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FilterId {
    Grayscale,
    Invert,
    Sepia,
    Blur,
    Sharpen,
    Brighten,
    Darken,
    Warm,
    Cool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReduceMode {
    Auto,
    // Variant field rename_all is required: enum rename_all only renames variant names.
    #[serde(rename_all = "camelCase")]
    Manual {
        quality: u8,
        #[serde(default)]
        max_bytes: Option<u64>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EnhanceMode {
    Auto,
    // Variant field rename_all is required: enum rename_all only renames variant names.
    #[serde(rename_all = "camelCase")]
    Manual {
        quality: u8,
        scale_percent: u32,
        sharpen: i32,
        contrast: i32,
    },
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum QualityIntent {
    #[default]
    Keep,
    Reduce {
        mode: ReduceMode,
    },
    Enhance {
        mode: EnhanceMode,
    },
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OutputFormat {
    #[default]
    Keep,
    Jpeg,
    Png,
    Webp,
    Gif,
    Bmp,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Disposition {
    Replace,
    SaveAsNew,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageEditOps {
    pub rotate: Rotate,
    pub crop: Option<CropRect>,
    pub resize: Option<ResizeSpec>,
    pub filters: Vec<FilterId>,
    pub quality: QualityIntent,
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageFileInfo {
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub format: String,
    pub raster: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageEditResult {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub format: String,
    pub quality_applied: bool,
    pub quality: Option<u8>,
    pub notes: Vec<String>,
    pub disposition: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EncodeKind {
    Jpeg,
    Png,
    Webp,
    Gif,
    Bmp,
}

#[derive(Debug)]
struct Rendered {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    quality_applied: bool,
    quality: Option<u8>,
    notes: Vec<String>,
    format: String,
}

fn raster_kind(ext: &str) -> Result<EncodeKind, EditError> {
    match ext {
        "jpg" | "jpeg" => Ok(EncodeKind::Jpeg),
        "png" => Ok(EncodeKind::Png),
        "webp" => Ok(EncodeKind::Webp),
        "gif" => Ok(EncodeKind::Gif),
        "bmp" => Ok(EncodeKind::Bmp),
        "svg" => Err(EditError::NotRaster),
        _ => Err(EditError::Unsupported),
    }
}

fn extension_of(path: &Path) -> Result<String, EditError> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .ok_or(EditError::Unsupported)
}

fn kind_label(kind: EncodeKind) -> &'static str {
    match kind {
        EncodeKind::Jpeg => "jpeg",
        EncodeKind::Png => "png",
        EncodeKind::Webp => "webp",
        EncodeKind::Gif => "gif",
        EncodeKind::Bmp => "bmp",
    }
}

fn output_kind(source_ext: &str, format: OutputFormat) -> Result<EncodeKind, EditError> {
    match format {
        OutputFormat::Keep => raster_kind(source_ext),
        OutputFormat::Jpeg => Ok(EncodeKind::Jpeg),
        OutputFormat::Png => Ok(EncodeKind::Png),
        OutputFormat::Webp => Ok(EncodeKind::Webp),
        OutputFormat::Gif => Ok(EncodeKind::Gif),
        OutputFormat::Bmp => Ok(EncodeKind::Bmp),
    }
}

pub(crate) fn output_extension(
    source_name: &str,
    format: OutputFormat,
) -> Result<String, EditError> {
    let source_ext = extension_of(Path::new(source_name))?;
    match format {
        OutputFormat::Keep => {
            raster_kind(&source_ext)?;
            Ok(source_ext)
        }
        OutputFormat::Jpeg => Ok("jpg".into()),
        OutputFormat::Png => Ok("png".into()),
        OutputFormat::Webp => Ok("webp".into()),
        OutputFormat::Gif => Ok("gif".into()),
        OutputFormat::Bmp => Ok("bmp".into()),
    }
}

pub(crate) fn suggest_save_as_name(
    source_name: &str,
    format: OutputFormat,
) -> Result<String, EditError> {
    let ext = output_extension(source_name, format)?;
    let stem = Path::new(source_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("image");
    Ok(format!("{stem}-edited.{ext}"))
}

fn lossy(kind: EncodeKind) -> bool {
    matches!(kind, EncodeKind::Jpeg)
}

fn clamp_quality(quality: u8) -> Result<u8, EditError> {
    if (1..=100).contains(&quality) {
        Ok(quality)
    } else {
        Err(EditError::InvalidOps)
    }
}

fn check_pixels(width: u32, height: u32) -> Result<(), EditError> {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width > MAX_SIDE || height > MAX_SIDE || pixels > MAX_PIXELS {
        Err(EditError::TooLarge)
    } else {
        Ok(())
    }
}

fn image_reader(path: &Path) -> Result<ImageReader<std::io::BufReader<std::fs::File>>, EditError> {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(256 * 1024 * 1024);
    let mut reader = ImageReader::open(path)
        .map_err(EditError::Io)?
        .with_guessed_format()
        .map_err(|_| EditError::Decode)?;
    reader.limits(limits);
    Ok(reader)
}

fn decode_file(path: &Path) -> Result<(DynamicImage, bool), EditError> {
    let ext = extension_of(path)?;
    raster_kind(&ext)?;
    let meta = fs::metadata(path)?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(EditError::TooLarge);
    }
    let reader = image_reader(path)?;
    // Apply Exif orientation so rotate left/right match the pixels the webview shows.
    let mut decoder = reader.into_decoder().map_err(|_| EditError::Decode)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(|_| EditError::Decode)?;
    img.apply_orientation(orientation);
    check_pixels(img.width(), img.height())?;
    Ok((img, ext == "gif"))
}

fn shrink_for_preview(img: DynamicImage) -> DynamicImage {
    let max = img.width().max(img.height());
    if max <= PREVIEW_MAX_SIDE {
        return img;
    }
    img.resize(PREVIEW_MAX_SIDE, PREVIEW_MAX_SIDE, FilterType::Triangle)
}

fn apply_geometry(mut img: DynamicImage, ops: &ImageEditOps) -> Result<DynamicImage, EditError> {
    img = match ops.rotate {
        Rotate::None => img,
        Rotate::Cw90 => img.rotate90(),
        Rotate::Ccw90 => img.rotate270(),
        Rotate::Deg180 => img.rotate180(),
    };
    if let Some(crop) = ops.crop {
        if crop.width == 0
            || crop.height == 0
            || crop.x.saturating_add(crop.width) > img.width()
            || crop.y.saturating_add(crop.height) > img.height()
        {
            return Err(EditError::CropOutOfBounds);
        }
        img = img.crop_imm(crop.x, crop.y, crop.width, crop.height);
    }
    if let Some(resize) = ops.resize {
        if resize.width == 0
            || resize.height == 0
            || resize.width > MAX_SIDE
            || resize.height > MAX_SIDE
        {
            return Err(EditError::InvalidOps);
        }
        img = match resize.mode {
            ResizeMode::Exact => {
                img.resize_exact(resize.width, resize.height, FilterType::Lanczos3)
            }
            ResizeMode::FitInside => {
                if img.width() <= resize.width && img.height() <= resize.height {
                    img
                } else {
                    img.resize(resize.width, resize.height, FilterType::Lanczos3)
                }
            }
        };
        check_pixels(img.width(), img.height())?;
    }
    Ok(img)
}

fn apply_filter(img: DynamicImage, filter: FilterId) -> DynamicImage {
    match filter {
        FilterId::Grayscale => DynamicImage::ImageLuma8(imageops::grayscale(&img)),
        FilterId::Invert => {
            let mut img = img;
            imageops::invert(&mut img);
            img
        }
        FilterId::Sepia => DynamicImage::ImageRgb8(sepia(&img.to_rgb8())),
        FilterId::Blur => DynamicImage::ImageRgba8(imageops::blur(&img.to_rgba8(), 1.4)),
        FilterId::Sharpen => {
            DynamicImage::ImageRgba8(imageops::unsharpen(&img.to_rgba8(), 1.0, 10))
        }
        FilterId::Brighten => DynamicImage::ImageRgba8(imageops::brighten(&img.to_rgba8(), 24)),
        FilterId::Darken => DynamicImage::ImageRgba8(imageops::brighten(&img.to_rgba8(), -24)),
        FilterId::Warm => DynamicImage::ImageRgba8(imageops::huerotate(&img.to_rgba8(), 15)),
        FilterId::Cool => DynamicImage::ImageRgba8(imageops::huerotate(&img.to_rgba8(), -15)),
    }
}

fn sepia(src: &RgbImage) -> RgbImage {
    let mut out = src.clone();
    for pixel in out.pixels_mut() {
        let r = f32::from(pixel[0]);
        let g = f32::from(pixel[1]);
        let b = f32::from(pixel[2]);
        pixel[0] = (0.393 * r + 0.769 * g + 0.189 * b).min(255.0) as u8;
        pixel[1] = (0.349 * r + 0.686 * g + 0.168 * b).min(255.0) as u8;
        pixel[2] = (0.272 * r + 0.534 * g + 0.131 * b).min(255.0) as u8;
    }
    out
}

fn auto_enhance_size(width: u32, height: u32) -> Result<(u32, u32), EditError> {
    if width.max(height) >= 2000 {
        return Ok((width, height));
    }
    let mut next_w = ((f64::from(width) * 1.25).round() as u32).max(1);
    let mut next_h = ((f64::from(height) * 1.25).round() as u32).max(1);
    let cap = 4096u32;
    if next_w > cap || next_h > cap {
        let scale = (f64::from(cap) / f64::from(next_w)).min(f64::from(cap) / f64::from(next_h));
        next_w = ((f64::from(next_w) * scale).floor() as u32).max(1);
        next_h = ((f64::from(next_h) * scale).floor() as u32).max(1);
    }
    check_pixels(next_w, next_h)?;
    Ok((next_w, next_h))
}

fn apply_enhance(
    mut img: DynamicImage,
    mode: &EnhanceMode,
) -> Result<(DynamicImage, u8), EditError> {
    match mode {
        EnhanceMode::Auto => {
            let (width, height) = auto_enhance_size(img.width(), img.height())?;
            if (width, height) != (img.width(), img.height()) {
                img = img.resize(width, height, FilterType::Lanczos3);
            }
            let sharp = imageops::unsharpen(&img.to_rgba8(), 1.2, 8);
            let contrasted = imageops::contrast(&sharp, 12.0);
            Ok((DynamicImage::ImageRgba8(contrasted), 92))
        }
        EnhanceMode::Manual {
            quality,
            scale_percent,
            sharpen,
            contrast,
        } => {
            let quality = clamp_quality(*quality)?;
            if !(100..=200).contains(scale_percent)
                || !(-50..=50).contains(contrast)
                || !(0..=100).contains(sharpen)
            {
                return Err(EditError::InvalidOps);
            }
            if *scale_percent != 100 {
                let width = (u64::from(img.width()) * u64::from(*scale_percent) / 100)
                    .clamp(1, u64::from(MAX_SIDE)) as u32;
                let height = (u64::from(img.height()) * u64::from(*scale_percent) / 100)
                    .clamp(1, u64::from(MAX_SIDE)) as u32;
                check_pixels(width, height)?;
                img = img.resize_exact(width, height, FilterType::Lanczos3);
            }
            if *sharpen > 0 {
                let sigma = (*sharpen as f32 / 40.0).max(0.4);
                img = DynamicImage::ImageRgba8(imageops::unsharpen(&img.to_rgba8(), sigma, 8));
            }
            if *contrast != 0 {
                img =
                    DynamicImage::ImageRgba8(imageops::contrast(&img.to_rgba8(), *contrast as f32));
            }
            Ok((img, quality))
        }
    }
}

fn encode_image(img: &DynamicImage, kind: EncodeKind, quality: u8) -> Result<Vec<u8>, EditError> {
    let mut buf = Cursor::new(Vec::new());
    match kind {
        EncodeKind::Jpeg => {
            let mut encoder = JpegEncoder::new_with_quality(&mut buf, quality);
            encoder.encode_image(img).map_err(|_| EditError::Encode)?;
        }
        EncodeKind::Png => {
            img.write_to(&mut buf, ImageFormat::Png)
                .map_err(|_| EditError::Encode)?;
        }
        EncodeKind::Webp => {
            let _ = quality;
            img.write_to(&mut buf, ImageFormat::WebP)
                .map_err(|_| EditError::Encode)?;
        }
        EncodeKind::Gif => {
            img.write_to(&mut buf, ImageFormat::Gif)
                .map_err(|_| EditError::Encode)?;
        }
        EncodeKind::Bmp => {
            img.write_to(&mut buf, ImageFormat::Bmp)
                .map_err(|_| EditError::Encode)?;
        }
    }
    Ok(buf.into_inner())
}

fn search_quality(
    kind: EncodeKind,
    img: &DynamicImage,
    target: u64,
    low: u8,
    high: u8,
) -> Result<(Vec<u8>, u8), EditError> {
    let mut lo = low;
    let mut hi = high;
    let mut best: Option<(Vec<u8>, u8)> = None;
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let bytes = encode_image(img, kind, mid)?;
        if bytes.len() as u64 <= target {
            best = Some((bytes, mid));
            lo = mid.saturating_add(1);
        } else if mid == low {
            break;
        } else {
            hi = mid - 1;
        }
    }
    if let Some(found) = best {
        Ok(found)
    } else {
        let bytes = encode_image(img, kind, low)?;
        Ok((bytes, low))
    }
}

fn render_edit(
    img: DynamicImage,
    ops: &ImageEditOps,
    source_len: u64,
    source_ext: &str,
    gif: bool,
    preview: bool,
) -> Result<Rendered, EditError> {
    if ops.filters.len() > MAX_FILTERS {
        return Err(EditError::InvalidOps);
    }
    let kind = output_kind(source_ext, ops.format)?;
    let mut img = apply_geometry(img, ops)?;
    let mut notes = Vec::new();
    if gif {
        notes.push("gif_first_frame".into());
    }
    let mut requested_quality = 90u8;
    match &ops.quality {
        QualityIntent::Keep => {}
        QualityIntent::Reduce { mode } => match mode {
            ReduceMode::Auto => {}
            ReduceMode::Manual { quality, max_bytes } => {
                requested_quality = clamp_quality(*quality)?;
                if let Some(limit) = max_bytes {
                    if *limit < 1024 {
                        return Err(EditError::InvalidOps);
                    }
                }
            }
        },
        QualityIntent::Enhance { mode } => {
            let (enhanced, quality) = apply_enhance(img, mode)?;
            img = enhanced;
            requested_quality = quality;
            notes.push("enhance_classical".into());
        }
    }
    for filter in &ops.filters {
        img = apply_filter(img, *filter);
    }
    if preview {
        // Downscale before encode so live preview stays snappy on large photos.
        img = shrink_for_preview(img);
    }
    check_pixels(img.width(), img.height())?;

    let (bytes, quality_applied, quality) = if kind == EncodeKind::Webp {
        if !matches!(ops.quality, QualityIntent::Keep) {
            notes.push("webp_lossless_only".into());
        }
        (encode_image(&img, kind, 90)?, false, None)
    } else if !lossy(kind) {
        if !matches!(ops.quality, QualityIntent::Keep) {
            notes.push("lossless_quality_ignored".into());
        }
        (encode_image(&img, kind, 90)?, false, None)
    } else {
        match &ops.quality {
            QualityIntent::Keep => (encode_image(&img, kind, 90)?, true, Some(90)),
            QualityIntent::Reduce {
                mode: ReduceMode::Auto,
            } => {
                if preview {
                    // Skip binary quality search during live preview.
                    notes.push("reduce_auto".into());
                    (encode_image(&img, kind, 70)?, true, Some(70))
                } else {
                    let target = (source_len.saturating_mul(7) / 10).max(4096);
                    let (bytes, quality) = search_quality(kind, &img, target, 40, 90)?;
                    notes.push("reduce_auto".into());
                    (bytes, true, Some(quality))
                }
            }
            QualityIntent::Reduce {
                mode: ReduceMode::Manual { max_bytes, .. },
            } => {
                if let Some(limit) = max_bytes {
                    if preview {
                        let quality = requested_quality.min(75);
                        let bytes = encode_image(&img, kind, quality)?;
                        if bytes.len() as u64 > *limit {
                            notes.push("target_not_met".into());
                        }
                        (bytes, true, Some(quality))
                    } else {
                        let (bytes, quality) =
                            search_quality(kind, &img, *limit, 30, requested_quality)?;
                        if bytes.len() as u64 > *limit {
                            notes.push("target_not_met".into());
                        }
                        (bytes, true, Some(quality))
                    }
                } else {
                    (
                        encode_image(&img, kind, requested_quality)?,
                        true,
                        Some(requested_quality),
                    )
                }
            }
            QualityIntent::Enhance { .. } => (
                encode_image(&img, kind, requested_quality)?,
                true,
                Some(requested_quality),
            ),
        }
    };

    Ok(Rendered {
        width: img.width(),
        height: img.height(),
        bytes,
        quality_applied,
        quality,
        notes,
        format: kind_label(kind).into(),
    })
}

fn preview_dir() -> Result<PathBuf, EditError> {
    let dirs = paths::ensure_app_dirs().map_err(GalleryError::from)?;
    let dir = PathBuf::from(dirs.data_dir).join(PREVIEW_DIR);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub(crate) fn evict_cache_dir(dir: &Path, max_files: usize) -> std::io::Result<()> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(UNIX_EPOCH);
        files.push((modified, path));
    }
    if files.len() <= max_files {
        return Ok(());
    }
    files.sort_by_key(|(modified, _)| *modified);
    let extra = files.len() - max_files;
    for (_, path) in files.into_iter().take(extra) {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

fn write_preview(rendered: &Rendered) -> Result<PathBuf, EditError> {
    let dir = preview_dir()?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let ext = match rendered.format.as_str() {
        "jpeg" => "jpg",
        other => other,
    };
    // Nanos + pid keeps concurrent auto-previews from colliding on the same path.
    let path = dir.join(format!("preview-{stamp}-{}.{ext}", std::process::id()));
    fs::write(&path, &rendered.bytes)?;
    evict_cache_dir(&dir, MAX_PREVIEW_FILES)?;
    Ok(path)
}

fn result_from(path: PathBuf, rendered: Rendered, disposition: &str) -> ImageEditResult {
    ImageEditResult {
        path: path.to_string_lossy().into_owned(),
        width: rendered.width,
        height: rendered.height,
        bytes: rendered.bytes.len() as u64,
        format: rendered.format,
        quality_applied: rendered.quality_applied,
        quality: rendered.quality,
        notes: rendered.notes,
        disposition: disposition.into(),
    }
}

fn load_for_edit(
    path: &Path,
    allowed_root: Option<&str>,
) -> Result<(PathBuf, DynamicImage, String, u64, bool), EditError> {
    let canonical = gallery::gate_file(path, allowed_root)?;
    let ext = extension_of(&canonical)?;
    let (img, gif) = decode_file(&canonical)?;
    let len = fs::metadata(&canonical)?.len();
    Ok((canonical, img, ext, len, gif))
}

fn valid_file_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 || name == "." || name == ".." {
        return false;
    }
    !name.contains('/') && !name.contains('\\') && !name.chars().any(|ch| ch.is_control())
}

fn extension_matches(
    name: &str,
    expected_kind: EncodeKind,
    source_ext: &str,
    format: OutputFormat,
) -> bool {
    let Some(ext) = Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
    else {
        return false;
    };
    match format {
        OutputFormat::Keep => ext == source_ext,
        _ => raster_kind(&ext).ok() == Some(expected_kind),
    }
}

fn atomic_publish(dest: &Path, bytes: &[u8], replace: bool) -> Result<(), EditError> {
    if !replace && dest.exists() {
        return Err(EditError::TargetExists);
    }
    let parent = dest.parent().ok_or(EditError::BadName)?;
    let tmp_name = format!(
        ".menzies-{}-{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let tmp = parent.join(tmp_name);
    let write_result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok::<(), std::io::Error>(())
    })();
    if let Err(err) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(EditError::Io(err));
    }
    match fs::rename(&tmp, dest) {
        Ok(()) => Ok(()),
        Err(err) if replace => {
            let removed = fs::remove_file(dest);
            if removed.is_ok() {
                if let Err(rename_err) = fs::rename(&tmp, dest) {
                    let _ = fs::remove_file(&tmp);
                    return Err(EditError::Io(rename_err));
                }
                Ok(())
            } else {
                let _ = fs::remove_file(&tmp);
                Err(EditError::Io(err))
            }
        }
        Err(err) => {
            let _ = fs::remove_file(&tmp);
            if dest.exists() {
                Err(EditError::TargetExists)
            } else {
                Err(EditError::Io(err))
            }
        }
    }
}

#[tauri::command]
pub fn suggest_edited_name(source_name: String, format: OutputFormat) -> Result<String, String> {
    suggest_save_as_name(&source_name, format).map_err(EditError::into_command)
}

#[tauri::command]
pub fn image_file_info(
    path: String,
    allowed_root: Option<String>,
) -> Result<ImageFileInfo, String> {
    let canonical = gallery::gate_file(Path::new(&path), allowed_root.as_deref())
        .map_err(gallery::gallery_command_err)?;
    let ext = extension_of(&canonical).map_err(EditError::into_command)?;
    let bytes = fs::metadata(&canonical)
        .map_err(EditError::Io)
        .map_err(EditError::into_command)?
        .len();
    if raster_kind(&ext).is_err() {
        return Ok(ImageFileInfo {
            width: 0,
            height: 0,
            bytes,
            format: ext,
            raster: false,
        });
    }
    if bytes > MAX_FILE_BYTES {
        return Err(EditError::TooLarge.into_command());
    }
    // Oriented decode so width/height match the pixels rotate/crop operate on.
    let (img, _) = decode_file(&canonical).map_err(EditError::into_command)?;
    Ok(ImageFileInfo {
        width: img.width(),
        height: img.height(),
        bytes,
        format: ext,
        raster: true,
    })
}

#[tauri::command]
pub fn preview_image_edit(
    source_path: String,
    allowed_root: Option<String>,
    ops: ImageEditOps,
) -> Result<ImageEditResult, String> {
    let (_path, img, ext, len, gif) =
        load_for_edit(Path::new(&source_path), allowed_root.as_deref())
            .map_err(EditError::into_command)?;
    let rendered = render_edit(img, &ops, len, &ext, gif, true).map_err(EditError::into_command)?;
    let preview = write_preview(&rendered).map_err(EditError::into_command)?;
    Ok(result_from(preview, rendered, "preview"))
}

#[tauri::command]
pub fn commit_image_edit(
    source_path: String,
    allowed_root: Option<String>,
    ops: ImageEditOps,
    disposition: Disposition,
    save_as_name: Option<String>,
) -> Result<ImageEditResult, String> {
    let (source, img, ext, len, gif) =
        load_for_edit(Path::new(&source_path), allowed_root.as_deref())
            .map_err(EditError::into_command)?;
    let rendered =
        render_edit(img, &ops, len, &ext, gif, false).map_err(EditError::into_command)?;
    let kind = output_kind(&ext, ops.format).map_err(EditError::into_command)?;
    let dest = match disposition {
        Disposition::Replace => {
            if output_kind(&ext, OutputFormat::Keep).ok() != Some(kind) {
                return Err(EditError::ReplaceFormatMismatch.into_command());
            }
            source.clone()
        }
        Disposition::SaveAsNew => {
            let name = save_as_name.ok_or_else(|| EditError::NameRequired.into_command())?;
            if !valid_file_name(&name) {
                return Err(EditError::BadName.into_command());
            }
            if !extension_matches(&name, kind, &ext, ops.format) {
                return Err(EditError::NameExtensionMismatch.into_command());
            }
            let parent = source
                .parent()
                .ok_or_else(|| EditError::BadName.into_command())?;
            parent.join(name)
        }
    };
    let replace = disposition == Disposition::Replace;
    atomic_publish(&dest, &rendered.bytes, replace).map_err(EditError::into_command)?;
    let label = match disposition {
        Disposition::Replace => "replace",
        Disposition::SaveAsNew => "saveAsNew",
    };
    Ok(result_from(dest, rendered, label))
}

#[tauri::command]
pub fn discard_edit_preview(path: String) -> Result<(), String> {
    let dir = preview_dir().map_err(EditError::into_command)?;
    let candidate = PathBuf::from(&path);
    let canonical = match fs::canonicalize(&candidate) {
        Ok(path) => path,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(EditError::Io(err).into_command()),
    };
    if canonical != dir && !canonical.starts_with(&dir) {
        return Err(EditError::PreviewEscape.into_command());
    }
    if canonical.is_file() {
        fs::remove_file(&canonical).map_err(|err| EditError::Io(err).into_command())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use std::fs::OpenOptions;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("gallery-edit-{label}-{nanos}"));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn noisy(width: u32, height: u32) -> DynamicImage {
        let mut img = RgbImage::new(width, height);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            pixel[0] = (x.wrapping_mul(3) % 256) as u8;
            pixel[1] = (y.wrapping_mul(5) % 256) as u8;
            pixel[2] = (x.wrapping_mul(y) % 256) as u8;
        }
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn rotate_crop_and_resize_change_dimensions() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 4, Rgb([10, 20, 30])));
        let ops = ImageEditOps {
            rotate: Rotate::Cw90,
            crop: Some(CropRect {
                x: 0,
                y: 0,
                width: 4,
                height: 2,
            }),
            resize: Some(ResizeSpec {
                width: 3,
                height: 5,
                mode: ResizeMode::Exact,
            }),
            ..ImageEditOps::default()
        };
        let rendered = render_edit(img, &ops, 1000, "png", false, false).expect("render");
        assert_eq!((rendered.width, rendered.height), (3, 5));
        assert!(!rendered.bytes.is_empty());
    }

    #[test]
    fn rotate_cw_and_ccw_match_image_crate_direction() {
        use image::GenericImageView;
        // 2x2 marker: TL red, TR green, BL blue, BR white
        let mut rgb = RgbImage::new(2, 2);
        rgb.put_pixel(0, 0, Rgb([255, 0, 0]));
        rgb.put_pixel(1, 0, Rgb([0, 255, 0]));
        rgb.put_pixel(0, 1, Rgb([0, 0, 255]));
        rgb.put_pixel(1, 1, Rgb([255, 255, 255]));
        let img = DynamicImage::ImageRgb8(rgb);

        let cw = apply_geometry(
            img.clone(),
            &ImageEditOps {
                rotate: Rotate::Cw90,
                ..ImageEditOps::default()
            },
        )
        .expect("cw");
        // image::rotate90 is CW: TL (red) moves to TR
        assert_eq!(cw.get_pixel(1, 0), image::Rgba([255, 0, 0, 255]));

        let ccw = apply_geometry(
            img,
            &ImageEditOps {
                rotate: Rotate::Ccw90,
                ..ImageEditOps::default()
            },
        )
        .expect("ccw");
        // image::rotate270 is 90 CCW: TL (red) moves to BL
        assert_eq!(ccw.get_pixel(0, 1), image::Rgba([255, 0, 0, 255]));

        let parsed_cw: Rotate = serde_json::from_str("\"cw90\"").expect("cw json");
        let parsed_ccw: Rotate = serde_json::from_str("\"ccw90\"").expect("ccw json");
        assert_eq!(parsed_cw, Rotate::Cw90);
        assert_eq!(parsed_ccw, Rotate::Ccw90);
    }

    #[test]
    fn crop_outside_is_rejected() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(4, 4, Rgb([1, 2, 3])));
        let ops = ImageEditOps {
            crop: Some(CropRect {
                x: 2,
                y: 2,
                width: 4,
                height: 4,
            }),
            ..ImageEditOps::default()
        };
        let err = render_edit(img, &ops, 100, "png", false, false).expect_err("crop");
        assert!(matches!(err, EditError::CropOutOfBounds));
    }

    #[test]
    fn lower_jpeg_quality_is_smaller() {
        let img = noisy(64, 64);
        let high = encode_image(&img, EncodeKind::Jpeg, 95).expect("high");
        let low = encode_image(&img, EncodeKind::Jpeg, 40).expect("low");
        assert!(low.len() < high.len());
    }

    #[test]
    fn reduce_auto_records_classical_note_only_for_enhance() {
        let img = noisy(48, 48);
        let ops = ImageEditOps {
            quality: QualityIntent::Enhance {
                mode: EnhanceMode::Auto,
            },
            format: OutputFormat::Jpeg,
            ..ImageEditOps::default()
        };
        let rendered = render_edit(img, &ops, 50_000, "png", false, false).expect("enhance");
        assert!(rendered
            .notes
            .iter()
            .any(|note| note == "enhance_classical"));
        assert!(rendered.quality_applied);
        assert!(rendered.width >= 48);
    }

    #[test]
    fn lossless_reduce_does_not_switch_format() {
        let img = noisy(16, 16);
        let ops = ImageEditOps {
            quality: QualityIntent::Reduce {
                mode: ReduceMode::Auto,
            },
            ..ImageEditOps::default()
        };
        let rendered = render_edit(img, &ops, 10_000, "png", false, false).expect("png");
        assert_eq!(rendered.format, "png");
        assert!(rendered
            .notes
            .iter()
            .any(|note| note == "lossless_quality_ignored"));
        assert!(!rendered.quality_applied);
    }

    #[test]
    fn preview_render_downscales_large_images() {
        let img = noisy(2400, 1600);
        let ops = ImageEditOps {
            filters: vec![FilterId::Grayscale],
            format: OutputFormat::Jpeg,
            ..ImageEditOps::default()
        };
        let rendered = render_edit(img, &ops, 100_000, "jpg", false, true).expect("preview");
        assert!(rendered.width.max(rendered.height) <= PREVIEW_MAX_SIDE);
        assert!(!rendered.bytes.is_empty());
    }

    #[test]
    fn save_as_new_does_not_change_source_and_replace_blocks_conversion() {
        let dir = temp_dir("commit");
        let source = dir.join("photo.png");
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(6, 6, Rgb([8, 9, 10])));
        img.save_with_format(&source, ImageFormat::Png)
            .expect("save");
        let before = fs::read(&source).expect("read");
        let ops = ImageEditOps {
            rotate: Rotate::Deg180,
            format: OutputFormat::Jpeg,
            ..ImageEditOps::default()
        };
        let err = commit_image_edit(
            source.to_string_lossy().into_owned(),
            Some(dir.to_string_lossy().into_owned()),
            ops.clone(),
            Disposition::Replace,
            None,
        )
        .expect_err("replace convert");
        assert!(err.contains("gallery:replace_format_mismatch:"));
        assert_eq!(fs::read(&source).expect("still"), before);

        let saved = commit_image_edit(
            source.to_string_lossy().into_owned(),
            Some(dir.to_string_lossy().into_owned()),
            ops,
            Disposition::SaveAsNew,
            Some("photo-edited.jpg".into()),
        )
        .expect("save as");
        assert_eq!(fs::read(&source).expect("original"), before);
        assert!(Path::new(&saved.path).is_file());
        assert_ne!(saved.path, source.to_string_lossy());

        let clash = commit_image_edit(
            source.to_string_lossy().into_owned(),
            Some(dir.to_string_lossy().into_owned()),
            ImageEditOps {
                format: OutputFormat::Jpeg,
                ..ImageEditOps::default()
            },
            Disposition::SaveAsNew,
            Some("photo-edited.jpg".into()),
        )
        .expect_err("exists");
        assert!(clash.contains("gallery:target_exists:"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_outside_allowed_root_is_rejected() {
        let dir = temp_dir("inside");
        let outside = temp_dir("outside");
        let source = outside.join("x.png");
        DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 2, Rgb([1, 1, 1])))
            .save_with_format(&source, ImageFormat::Png)
            .expect("png");
        let err = image_file_info(
            source.to_string_lossy().into_owned(),
            Some(dir.to_string_lossy().into_owned()),
        )
        .expect_err("escape");
        assert!(err.contains("gallery:path_escape:"));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn suggest_name_uses_output_extension() {
        let name = suggest_save_as_name("Vacation.JPEG", OutputFormat::Png).expect("name");
        assert_eq!(name, "Vacation-edited.png");
    }

    #[test]
    fn frontend_camel_case_ops_deserialize() {
        let reduce: ImageEditOps = serde_json::from_str(
            r#"{
                "rotate":"cw90",
                "filters":["grayscale","sepia"],
                "quality":{"kind":"reduce","mode":{"kind":"manual","quality":70,"maxBytes":50000}},
                "format":"jpeg"
            }"#,
        )
        .expect("reduce ops");
        assert_eq!(reduce.rotate, Rotate::Cw90);
        assert_eq!(reduce.filters.len(), 2);
        match reduce.quality {
            QualityIntent::Reduce {
                mode: ReduceMode::Manual { quality, max_bytes },
            } => {
                assert_eq!(quality, 70);
                assert_eq!(max_bytes, Some(50_000));
            }
            other => panic!("unexpected quality {other:?}"),
        }

        let enhance: ImageEditOps = serde_json::from_str(
            r#"{
                "rotate":"none",
                "resize":{"width":80,"height":40,"mode":"fitInside"},
                "filters":[],
                "quality":{
                    "kind":"enhance",
                    "mode":{"kind":"manual","quality":92,"scalePercent":125,"sharpen":20,"contrast":8}
                },
                "format":"png"
            }"#,
        )
        .expect("enhance ops");
        match enhance.quality {
            QualityIntent::Enhance {
                mode:
                    EnhanceMode::Manual {
                        quality,
                        scale_percent,
                        sharpen,
                        contrast,
                    },
            } => {
                assert_eq!(quality, 92);
                assert_eq!(scale_percent, 125);
                assert_eq!(sharpen, 20);
                assert_eq!(contrast, 8);
            }
            other => panic!("unexpected quality {other:?}"),
        }
        let resize = enhance.resize.expect("resize");
        assert_eq!(resize.mode, ResizeMode::FitInside);
    }

    #[test]
    fn preview_cache_keeps_newest_files() {
        let dir = temp_dir("cache");
        let start = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for index in 0..10 {
            let path = dir.join(format!("f{index}.txt"));
            fs::write(&path, b"x").expect("file");
            let file = OpenOptions::new().write(true).open(&path).expect("open");
            file.set_modified(start + std::time::Duration::from_secs(index))
                .expect("mtime");
        }
        evict_cache_dir(&dir, 8).expect("evict");
        let left = fs::read_dir(&dir).expect("read").count();
        assert_eq!(left, 8);
        assert!(!dir.join("f0.txt").exists());
        assert!(dir.join("f9.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
