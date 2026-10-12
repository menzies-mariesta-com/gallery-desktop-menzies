//! Offline YuNet + SFace inference.
//! The ONNX Runtime library is loaded from the signed extension. Telemetry is disabled.
//! This is face detection and embeddings, not a language model.

use crate::face_math::{
    decode_stride, detector_dims, l2_normalize, non_max_suppression, scale_detection, warp_align,
    Detection, StrideInputs, DETECT_SCORE, NMS_IOU,
};
use image::imageops::FilterType;
use image::RgbImage;
use std::path::Path;

const MAX_PIXELS: u64 = 16_000_000;
const PREVIEW_EDGE: u32 = 1600;

#[derive(Debug, Clone)]
pub struct FoundFace {
    pub score: f32,
    pub bbox: [f32; 4],
    pub landmarks: [f32; 10],
    pub embedding: Vec<f32>,
}

pub fn runtime_compiled() -> bool {
    cfg!(feature = "face-runtime")
}

pub fn recognize_file(
    runtime: &Path,
    detector: &Path,
    embedder: &Path,
    image_path: &Path,
) -> Result<Vec<FoundFace>, String> {
    #[cfg(feature = "face-runtime")]
    {
        engine::recognize_file(runtime, detector, embedder, image_path)
    }
    #[cfg(not(feature = "face-runtime"))]
    {
        let _ = (runtime, detector, embedder, image_path);
        Err("face recognition runtime is not linked".into())
    }
}

#[cfg(feature = "face-runtime")]
mod engine {
    use super::*;
    use ort::session::Session;
    use ort::value::TensorRef;
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn ensure_runtime(path: &Path) -> Result<(), String> {
        static READY: Mutex<Option<PathBuf>> = Mutex::new(None);
        let mut guard = READY
            .lock()
            .map_err(|_| "ONNX Runtime lock is poisoned".to_string())?;
        if let Some(loaded) = guard.as_ref() {
            if loaded == path {
                return Ok(());
            }
            return Err(
                "ONNX Runtime is already loaded. Restart Gallery after replacing the extension."
                    .into(),
            );
        }
        let committed = ort::init_from(path)
            .map_err(|err| err.to_string())?
            .with_telemetry(false)
            .commit();
        if !committed {
            return Err("ONNX Runtime was already initialized".into());
        }
        *guard = Some(path.to_path_buf());
        Ok(())
    }

    fn open_session(path: &Path) -> Result<Session, String> {
        Session::builder()
            .map_err(|err| err.to_string())?
            .with_intra_threads(2)
            .map_err(|err| err.to_string())?
            .commit_from_file(path)
            .map_err(|err| err.to_string())
    }

    fn tensor_copy(
        outputs: &ort::session::SessionOutputs<'_>,
        name: &str,
    ) -> Result<(Vec<usize>, Vec<f32>), String> {
        let value = outputs
            .get(name)
            .ok_or_else(|| format!("missing output {name}"))?;
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|err| err.to_string())?;
        let dims = shape.iter().map(|dim| (*dim).max(0) as usize).collect();
        Ok((dims, data.to_vec()))
    }

    fn pack_nchw(image: &RgbImage, bgr: bool) -> Vec<f32> {
        let width = image.width() as usize;
        let height = image.height() as usize;
        let plane = height * width;
        let mut data = vec![0.0f32; 3 * plane];
        for y in 0..height {
            for x in 0..width {
                let pixel = image.get_pixel(x as u32, y as u32).0;
                let offset = y * width + x;
                let channels = if bgr {
                    [pixel[2], pixel[1], pixel[0]]
                } else {
                    [pixel[0], pixel[1], pixel[2]]
                };
                data[offset] = f32::from(channels[0]);
                data[plane + offset] = f32::from(channels[1]);
                data[2 * plane + offset] = f32::from(channels[2]);
            }
        }
        data
    }

    fn limit_image(image: RgbImage) -> (RgbImage, f32, f32) {
        let (width, height) = image.dimensions();
        if u64::from(width) * u64::from(height) <= MAX_PIXELS {
            return (image, 1.0, 1.0);
        }
        let long = width.max(height).max(1);
        let factor = f64::from(PREVIEW_EDGE) / f64::from(long);
        let next_w = ((f64::from(width) * factor).round() as u32).max(1);
        let next_h = ((f64::from(height) * factor).round() as u32).max(1);
        let scaled = image::imageops::resize(&image, next_w, next_h, FilterType::Triangle);
        (
            scaled,
            width as f32 / next_w as f32,
            height as f32 / next_h as f32,
        )
    }

    pub fn recognize_file(
        runtime: &Path,
        detector_path: &Path,
        embedder_path: &Path,
        image_path: &Path,
    ) -> Result<Vec<FoundFace>, String> {
        ensure_runtime(runtime)?;
        let decoded = image::ImageReader::open(image_path)
            .map_err(|err| err.to_string())?
            .with_guessed_format()
            .map_err(|err| err.to_string())?
            .decode()
            .map_err(|err| err.to_string())?
            .into_rgb8();
        let (image, back_x, back_y) = limit_image(decoded);
        let (work_w, work_h) = image.dimensions();
        let (net_w, net_h, scale_x, scale_y) = detector_dims(work_w, work_h);
        let resized = image::imageops::resize(&image, net_w, net_h, FilterType::Triangle);
        let mut detector = open_session(detector_path)?;
        let mut embedder = open_session(embedder_path)?;
        let input = pack_nchw(&resized, true);
        let tensor = TensorRef::from_array_view((
            [1usize, 3, net_h as usize, net_w as usize],
            input.as_slice(),
        ))
        .map_err(|err| err.to_string())?;
        let outputs = detector
            .run(ort::inputs!["input" => tensor])
            .map_err(|err| err.to_string())?;
        let mut raw: Vec<Detection> = Vec::new();
        for stride in [8u32, 16, 32] {
            let height = (net_h / stride) as usize;
            let width = (net_w / stride) as usize;
            let tag = stride.to_string();
            let (cls_shape, cls) = tensor_copy(&outputs, &format!("cls_{tag}"))?;
            let (obj_shape, obj) = tensor_copy(&outputs, &format!("obj_{tag}"))?;
            let (box_shape, bbox) = tensor_copy(&outputs, &format!("bbox_{tag}"))?;
            let (kps_shape, kps) = tensor_copy(&outputs, &format!("kps_{tag}"))?;
            decode_stride(
                stride as f32,
                height,
                width,
                &StrideInputs {
                    cls: &cls,
                    cls_shape: &cls_shape,
                    obj: &obj,
                    obj_shape: &obj_shape,
                    bbox: &bbox,
                    bbox_shape: &box_shape,
                    kps: &kps,
                    kps_shape: &kps_shape,
                },
                DETECT_SCORE,
                &mut raw,
            );
        }
        drop(outputs);
        let faces = non_max_suppression(raw, NMS_IOU, 20);
        let mut found = Vec::with_capacity(faces.len());
        let original_w = work_w as f32 * back_x;
        let original_h = work_h as f32 * back_y;
        for face in faces {
            let scaled = scale_detection(
                &face,
                scale_x * back_x,
                scale_y * back_y,
                original_w,
                original_h,
            );
            if scaled.w < 8.0 || scaled.h < 8.0 {
                continue;
            }
            let mut working_marks = scaled.landmarks;
            for point in &mut working_marks {
                point[0] /= back_x;
                point[1] /= back_y;
            }
            let Some(aligned) = warp_align(&image, working_marks) else {
                continue;
            };
            let packed = pack_nchw(&aligned, false);
            let face_tensor =
                TensorRef::from_array_view(([1usize, 3, 112usize, 112], packed.as_slice()))
                    .map_err(|err| err.to_string())?;
            let embedded = embedder
                .run(ort::inputs!["data" => face_tensor])
                .map_err(|err| err.to_string())?;
            let output_name = if embedded.contains_key("fc1") {
                "fc1".to_string()
            } else {
                embedded
                    .iter()
                    .next()
                    .map(|(name, _)| name.to_string())
                    .ok_or_else(|| "embedder returned no output".to_string())?
            };
            let (_shape, mut vector) = tensor_copy(&embedded, &output_name)?;
            if vector.len() < 32 || vector.len() > 512 || !l2_normalize(&mut vector) {
                continue;
            }
            let mut landmarks = [0.0f32; 10];
            for (index, point) in scaled.landmarks.iter().enumerate() {
                landmarks[index * 2] = point[0];
                landmarks[index * 2 + 1] = point[1];
            }
            found.push(FoundFace {
                score: scaled.score,
                bbox: [scaled.x, scaled.y, scaled.w, scaled.h],
                landmarks,
                embedding: vector,
            });
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};
    use std::path::PathBuf;

    #[test]
    fn blank_fixture_reports_no_face_when_package_is_present() {
        let Some(dir) = std::env::var_os("MENZIES_FACE_EXT_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let image_path = std::env::temp_dir().join(format!("gallery-blank-face-{nanos}.png"));
        let blank: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(320, 240, Rgb([114, 114, 114]));
        blank.save(&image_path).expect("save blank");
        let faces = recognize_file(
            &dir.join("runtime/libonnxruntime.so"),
            &dir.join("detector.onnx"),
            &dir.join("embedder.onnx"),
            &image_path,
        )
        .expect("infer");
        let _ = std::fs::remove_file(image_path);
        assert!(
            faces.is_empty(),
            "a flat gray image should not produce a face, found {}",
            faces.len()
        );
    }
}
