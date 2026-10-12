//! Face geometry: YuNet decode, five-point alignment, cosine similarity, grouping.
//! This is a detector plus an embedding model. It is not a language model.

pub const DETECT_SCORE: f32 = 0.9;
pub const NMS_IOU: f32 = 0.3;
pub const MATCH_COSINE: f32 = 0.55;
pub const MATCH_MARGIN: f32 = 0.05;
pub const ALIGN_SIZE: u32 = 112;
pub const MAX_DETECTOR_EDGE: u32 = 640;

/// SFace / ArcFace five-point template on a 112x112 crop. OpenCV FaceRecognizerSF.
pub const SFACE_TEMPLATE: [[f64; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

#[derive(Debug, Clone)]
pub struct Detection {
    pub score: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub landmarks: [[f32; 2]; 5],
}

pub fn cosine(left: &[f32], right: &[f32]) -> Option<f32> {
    if left.len() != right.len() || left.is_empty() {
        return None;
    }
    let mut dot = 0.0f32;
    let mut left_sq = 0.0f32;
    let mut right_sq = 0.0f32;
    for (a, b) in left.iter().zip(right.iter()) {
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        dot += a * b;
        left_sq += a * a;
        right_sq += b * b;
    }
    let denom = left_sq.sqrt() * right_sq.sqrt();
    if denom <= f32::EPSILON {
        None
    } else {
        Some((dot / denom).clamp(-1.0, 1.0))
    }
}

pub fn l2_normalize(values: &mut [f32]) -> bool {
    let mut sum = 0.0f32;
    for value in values.iter() {
        if !value.is_finite() {
            return false;
        }
        sum += value * value;
    }
    let norm = sum.sqrt();
    if norm <= f32::EPSILON {
        return false;
    }
    for value in values.iter_mut() {
        *value /= norm;
    }
    true
}

/// Mean of equal-length vectors, then L2 normalized. Empty or mixed lengths yield None.
pub fn centroid(vectors: &[Vec<f32>]) -> Option<Vec<f32>> {
    let width = vectors.first()?.len();
    if width == 0 || vectors.iter().any(|row| row.len() != width) {
        return None;
    }
    let mut mean = vec![0.0f32; width];
    for row in vectors {
        for (slot, value) in mean.iter_mut().zip(row.iter()) {
            *slot += value;
        }
    }
    let scale = vectors.len() as f32;
    for value in &mut mean {
        *value /= scale;
    }
    if l2_normalize(&mut mean) {
        Some(mean)
    } else {
        None
    }
}

/// Assign to a person only when the best centroid is clearly above the threshold.
/// Two strong matches stay unlabeled so clusters do not collapse together.
pub fn assign_person(
    embedding: &[f32],
    people: &[(i64, Vec<f32>)],
    threshold: f32,
    margin: f32,
) -> Option<i64> {
    let mut best_id = None;
    let mut best_score = f32::NEG_INFINITY;
    let mut second = f32::NEG_INFINITY;
    for (id, center) in people {
        let Some(score) = cosine(embedding, center) else {
            continue;
        };
        if score > best_score {
            second = best_score;
            best_score = score;
            best_id = Some(*id);
        } else if score >= second {
            second = score;
        }
    }
    let id = best_id?;
    if !best_score.is_finite() || best_score < threshold || second >= threshold {
        return None;
    }
    if best_score < second + margin {
        return None;
    }
    Some(id)
}

fn activate(value: f32) -> f32 {
    if (0.0..=1.0).contains(&value) {
        value
    } else {
        1.0 / (1.0 + (-value).exp())
    }
}

struct Map<'a> {
    data: &'a [f32],
    height: usize,
    width: usize,
    channels: usize,
    nchw: bool,
}

impl<'a> Map<'a> {
    fn parse(
        data: &'a [f32],
        shape: &[usize],
        channels: usize,
        height: usize,
        width: usize,
    ) -> Option<Self> {
        let cells = height.saturating_mul(width);
        let need = cells.saturating_mul(channels);
        if data.len() < need || height == 0 || width == 0 || channels == 0 {
            return None;
        }
        let nchw = match shape {
            [1, c, h, w] if *c == channels && *h == height && *w == width => true,
            [c, h, w] if *c == channels && *h == height && *w == width => true,
            [1, h, w, c] if *c == channels && *h == height && *w == width => false,
            [h, w, c] if *c == channels && *h == height && *w == width => false,
            [1, n, c] if *c == channels && *n == cells => false,
            _ => data.len() == need,
        };
        Some(Self {
            data,
            height,
            width,
            channels,
            nchw,
        })
    }

    fn at(&self, y: usize, x: usize, channel: usize) -> f32 {
        let index = if self.nchw {
            channel * self.height * self.width + y * self.width + x
        } else {
            (y * self.width + x) * self.channels + channel
        };
        self.data.get(index).copied().unwrap_or(0.0)
    }
}

pub struct StrideInputs<'a> {
    pub cls: &'a [f32],
    pub cls_shape: &'a [usize],
    pub obj: &'a [f32],
    pub obj_shape: &'a [usize],
    pub bbox: &'a [f32],
    pub bbox_shape: &'a [usize],
    pub kps: &'a [f32],
    pub kps_shape: &'a [usize],
}

pub fn decode_stride(
    stride: f32,
    height: usize,
    width: usize,
    inputs: &StrideInputs<'_>,
    score_threshold: f32,
    out: &mut Vec<Detection>,
) {
    let Some(cls_map) = Map::parse(inputs.cls, inputs.cls_shape, 1, height, width) else {
        return;
    };
    let Some(obj_map) = Map::parse(inputs.obj, inputs.obj_shape, 1, height, width) else {
        return;
    };
    let Some(box_map) = Map::parse(inputs.bbox, inputs.bbox_shape, 4, height, width) else {
        return;
    };
    let Some(kps_map) = Map::parse(inputs.kps, inputs.kps_shape, 10, height, width) else {
        return;
    };
    for y in 0..height {
        for x in 0..width {
            let score = (activate(cls_map.at(y, x, 0)) * activate(obj_map.at(y, x, 0))).sqrt();
            if !score.is_finite() || score < score_threshold {
                continue;
            }
            let dx = box_map.at(y, x, 0);
            let dy = box_map.at(y, x, 1);
            let dw = box_map.at(y, x, 2);
            let dh = box_map.at(y, x, 3);
            let cx = (x as f32 + dx) * stride;
            let cy = (y as f32 + dy) * stride;
            let bw = dw.exp() * stride;
            let bh = dh.exp() * stride;
            if !bw.is_finite() || !bh.is_finite() || bw < 2.0 || bh < 2.0 {
                continue;
            }
            let mut landmarks = [[0.0f32; 2]; 5];
            for (point, landmark) in landmarks.iter_mut().enumerate() {
                *landmark = [
                    (kps_map.at(y, x, point * 2) + x as f32) * stride,
                    (kps_map.at(y, x, point * 2 + 1) + y as f32) * stride,
                ];
            }
            out.push(Detection {
                score,
                x: cx - bw / 2.0,
                y: cy - bh / 2.0,
                w: bw,
                h: bh,
                landmarks,
            });
        }
    }
}

pub fn non_max_suppression(
    mut faces: Vec<Detection>,
    iou_threshold: f32,
    limit: usize,
) -> Vec<Detection> {
    faces.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if faces.len() > 200 {
        faces.truncate(200);
    }
    let mut kept: Vec<Detection> = Vec::new();
    for face in faces {
        let overlaps = kept.iter().any(|prior| iou(&face, prior) >= iou_threshold);
        if !overlaps {
            kept.push(face);
            if kept.len() == limit {
                break;
            }
        }
    }
    kept
}

fn iou(left: &Detection, right: &Detection) -> f32 {
    let ix1 = left.x.max(right.x);
    let iy1 = left.y.max(right.y);
    let ix2 = (left.x + left.w).min(right.x + right.w);
    let iy2 = (left.y + left.h).min(right.y + right.h);
    let iw = (ix2 - ix1).max(0.0);
    let ih = (iy2 - iy1).max(0.0);
    let inter = iw * ih;
    let union = left.w * left.h + right.w * right.h - inter;
    if union <= f32::EPSILON {
        0.0
    } else {
        inter / union
    }
}

pub fn scale_detection(
    face: &Detection,
    scale_x: f32,
    scale_y: f32,
    image_w: f32,
    image_h: f32,
) -> Detection {
    let x = (face.x * scale_x).clamp(0.0, image_w);
    let y = (face.y * scale_y).clamp(0.0, image_h);
    let x2 = ((face.x + face.w) * scale_x).clamp(0.0, image_w);
    let y2 = ((face.y + face.h) * scale_y).clamp(0.0, image_h);
    let mut landmarks = face.landmarks;
    for point in &mut landmarks {
        point[0] = (point[0] * scale_x).clamp(0.0, image_w);
        point[1] = (point[1] * scale_y).clamp(0.0, image_h);
    }
    Detection {
        score: face.score,
        x,
        y,
        w: (x2 - x).max(0.0),
        h: (y2 - y).max(0.0),
        landmarks,
    }
}

/// Forward 2x3 map: source landmark pixels to the 112x112 template.
pub fn similarity_src_to_dst(src: [[f64; 2]; 5], dst: [[f64; 2]; 5]) -> Option<[[f64; 3]; 2]> {
    let mut src_mean = [0.0, 0.0];
    let mut dst_mean = [0.0, 0.0];
    for index in 0..5 {
        src_mean[0] += src[index][0];
        src_mean[1] += src[index][1];
        dst_mean[0] += dst[index][0];
        dst_mean[1] += dst[index][1];
    }
    src_mean[0] /= 5.0;
    src_mean[1] /= 5.0;
    dst_mean[0] /= 5.0;
    dst_mean[1] /= 5.0;
    let mut src_d = [[0.0; 2]; 5];
    let mut dst_d = [[0.0; 2]; 5];
    for index in 0..5 {
        src_d[index] = [src[index][0] - src_mean[0], src[index][1] - src_mean[1]];
        dst_d[index] = [dst[index][0] - dst_mean[0], dst[index][1] - dst_mean[1]];
    }
    let mut a00 = 0.0;
    let mut a01 = 0.0;
    let mut a10 = 0.0;
    let mut a11 = 0.0;
    for index in 0..5 {
        a00 += dst_d[index][0] * src_d[index][0];
        a01 += dst_d[index][0] * src_d[index][1];
        a10 += dst_d[index][1] * src_d[index][0];
        a11 += dst_d[index][1] * src_d[index][1];
    }
    a00 /= 5.0;
    a01 /= 5.0;
    a10 /= 5.0;
    a11 /= 5.0;
    let (u, singular, vt) = svd_2x2([[a00, a01], [a10, a11]]);
    let mut d = [1.0, 1.0];
    if a00 * a11 - a01 * a10 < 0.0 {
        d[1] = -1.0;
    }
    let det_u = u[0][0] * u[1][1] - u[0][1] * u[1][0];
    let det_vt = vt[0][0] * vt[1][1] - vt[0][1] * vt[1][0];
    let smax = singular[0].max(singular[1]);
    let tol = smax * 2.0 * f64::EPSILON;
    let rank = usize::from(singular[0] > tol) + usize::from(singular[1] > tol);
    let t = if rank == 1 && det_u * det_vt > 0.0 {
        matmul_2x2(u, vt)
    } else {
        if rank == 1 && det_u * det_vt <= 0.0 {
            d[1] = -1.0;
        }
        let dvt = [
            [d[0] * vt[0][0], d[0] * vt[0][1]],
            [d[1] * vt[1][0], d[1] * vt[1][1]],
        ];
        matmul_2x2(u, dvt)
    };
    let mut var = 0.0;
    for point in &src_d {
        var += point[0] * point[0] + point[1] * point[1];
    }
    var /= 5.0;
    if var <= f64::EPSILON {
        return None;
    }
    let scale = (singular[0] * d[0] + singular[1] * d[1]) / var;
    let ts0 = t[0][0] * src_mean[0] + t[0][1] * src_mean[1];
    let ts1 = t[1][0] * src_mean[0] + t[1][1] * src_mean[1];
    Some([
        [t[0][0] * scale, t[0][1] * scale, dst_mean[0] - scale * ts0],
        [t[1][0] * scale, t[1][1] * scale, dst_mean[1] - scale * ts1],
    ])
}

fn matmul_2x2(left: [[f64; 2]; 2], right: [[f64; 2]; 2]) -> [[f64; 2]; 2] {
    [
        [
            left[0][0] * right[0][0] + left[0][1] * right[1][0],
            left[0][0] * right[0][1] + left[0][1] * right[1][1],
        ],
        [
            left[1][0] * right[0][0] + left[1][1] * right[1][0],
            left[1][0] * right[0][1] + left[1][1] * right[1][1],
        ],
    ]
}

/// A = U * diag(S) * Vt, singular values descending.
fn svd_2x2(a: [[f64; 2]; 2]) -> ([[f64; 2]; 2], [f64; 2], [[f64; 2]; 2]) {
    let ata00 = a[0][0] * a[0][0] + a[1][0] * a[1][0];
    let ata01 = a[0][0] * a[0][1] + a[1][0] * a[1][1];
    let ata11 = a[0][1] * a[0][1] + a[1][1] * a[1][1];
    let trace = ata00 + ata11;
    let det = ata00 * ata11 - ata01 * ata01;
    let disc = (trace * trace - 4.0 * det).max(0.0).sqrt();
    let l0 = ((trace + disc) / 2.0).max(0.0);
    let l1 = ((trace - disc) / 2.0).max(0.0);
    let s0 = l0.sqrt();
    let s1 = l1.sqrt();
    let v0 = sym_unit(ata00, ata01, ata11, l0);
    let mut v1 = [-v0[1], v0[0]];
    if (ata00 * v1[0] + ata01 * v1[1] - l1 * v1[0]).abs()
        + (ata01 * v1[0] + ata11 * v1[1] - l1 * v1[1]).abs()
        > 1e-6
    {
        v1 = sym_unit(ata00, ata01, ata11, l1);
    }
    let vt = [[v0[0], v0[1]], [v1[0], v1[1]]];
    let mut u = [[1.0, 0.0], [0.0, 1.0]];
    if s0 > 1e-12 {
        let col = [
            (a[0][0] * v0[0] + a[0][1] * v0[1]) / s0,
            (a[1][0] * v0[0] + a[1][1] * v0[1]) / s0,
        ];
        u[0][0] = col[0];
        u[1][0] = col[1];
    }
    if s1 > 1e-12 {
        let col = [
            (a[0][0] * v1[0] + a[0][1] * v1[1]) / s1,
            (a[1][0] * v1[0] + a[1][1] * v1[1]) / s1,
        ];
        u[0][1] = col[0];
        u[1][1] = col[1];
    } else {
        u[0][1] = -u[1][0];
        u[1][1] = u[0][0];
    }
    (u, [s0, s1], vt)
}

fn sym_unit(a00: f64, a01: f64, a11: f64, lambda: f64) -> [f64; 2] {
    if a01.abs() > 1e-12 {
        let y = -(a00 - lambda) / a01;
        let norm = (1.0 + y * y).sqrt();
        [1.0 / norm, y / norm]
    } else if (a00 - lambda).abs() <= (a11 - lambda).abs() {
        [1.0, 0.0]
    } else {
        [0.0, 1.0]
    }
}

pub fn apply_affine(matrix: [[f64; 3]; 2], x: f64, y: f64) -> (f64, f64) {
    (
        matrix[0][0] * x + matrix[0][1] * y + matrix[0][2],
        matrix[1][0] * x + matrix[1][1] * y + matrix[1][2],
    )
}

fn invert_affine(matrix: [[f64; 3]; 2]) -> Option<[[f64; 3]; 2]> {
    let det = matrix[0][0] * matrix[1][1] - matrix[0][1] * matrix[1][0];
    if det.abs() <= 1e-12 {
        return None;
    }
    let inv00 = matrix[1][1] / det;
    let inv01 = -matrix[0][1] / det;
    let inv10 = -matrix[1][0] / det;
    let inv11 = matrix[0][0] / det;
    let tx = matrix[0][2];
    let ty = matrix[1][2];
    Some([
        [inv00, inv01, -(inv00 * tx + inv01 * ty)],
        [inv10, inv11, -(inv10 * tx + inv11 * ty)],
    ])
}

fn sample_bilinear(image: &image::RgbImage, x: f64, y: f64) -> [u8; 3] {
    if x < 0.0 || y < 0.0 {
        return [0, 0, 0];
    }
    let x1 = x.floor() as i32;
    let y1 = y.floor() as i32;
    let x2 = x1 + 1;
    let y2 = y1 + 1;
    let width = image.width() as i32;
    let height = image.height() as i32;
    if x1 < 0 || y1 < 0 || x2 >= width || y2 >= height {
        return [0, 0, 0];
    }
    let fx = (x - f64::from(x1)) as f32;
    let fy = (y - f64::from(y1)) as f32;
    let p11 = image.get_pixel(x1 as u32, y1 as u32).0;
    let p21 = image.get_pixel(x2 as u32, y1 as u32).0;
    let p12 = image.get_pixel(x1 as u32, y2 as u32).0;
    let p22 = image.get_pixel(x2 as u32, y2 as u32).0;
    let mut out = [0u8; 3];
    for channel in 0..3 {
        let top = f32::from(p11[channel]) * (1.0 - fx) + f32::from(p21[channel]) * fx;
        let bottom = f32::from(p12[channel]) * (1.0 - fx) + f32::from(p22[channel]) * fx;
        out[channel] = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
    }
    out
}

pub fn warp_align(image: &image::RgbImage, landmarks: [[f32; 2]; 5]) -> Option<image::RgbImage> {
    let mut src = [[0.0; 2]; 5];
    for index in 0..5 {
        src[index] = [
            f64::from(landmarks[index][0]),
            f64::from(landmarks[index][1]),
        ];
    }
    let forward = similarity_src_to_dst(src, SFACE_TEMPLATE)?;
    let inverse = invert_affine(forward)?;
    let mut out = image::RgbImage::new(ALIGN_SIZE, ALIGN_SIZE);
    for y in 0..ALIGN_SIZE {
        for x in 0..ALIGN_SIZE {
            let (sx, sy) = apply_affine(inverse, f64::from(x), f64::from(y));
            let pixel = sample_bilinear(image, sx, sy);
            out.put_pixel(x, y, image::Rgb(pixel));
        }
    }
    Some(out)
}

pub fn detector_dims(width: u32, height: u32) -> (u32, u32, f32, f32) {
    // The published YuNet graph fixes the input at 640 by 640.
    let net = MAX_DETECTOR_EDGE;
    let scale_x = width.max(1) as f32 / net as f32;
    let scale_y = height.max(1) as f32 / net as f32;
    (net, net, scale_x, scale_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_is_one_for_the_same_vector_and_zero_when_orthogonal() {
        let score = cosine(&[1.0, 0.0, 0.0], &[1.0, 0.0, 0.0]).expect("same");
        assert!((score - 1.0).abs() < 1e-5);
        let score = cosine(&[1.0, 0.0], &[0.0, 1.0]).expect("orthogonal");
        assert!(score.abs() < 1e-5);
        assert!(cosine(&[1.0], &[1.0, 0.0]).is_none());
    }

    #[test]
    fn assignment_refuses_ambiguous_and_weak_matches() {
        let people = vec![(1, vec![1.0, 0.0]), (2, vec![0.0, 1.0])];
        assert_eq!(
            assign_person(&[1.0, 0.0], &people, MATCH_COSINE, MATCH_MARGIN),
            Some(1)
        );
        assert_eq!(
            assign_person(&[0.8, 0.8], &people, MATCH_COSINE, MATCH_MARGIN),
            None
        );
        assert_eq!(
            assign_person(&[0.2, 0.1], &people, MATCH_COSINE, MATCH_MARGIN),
            Some(1)
        );
    }

    #[test]
    fn alignment_maps_shifted_points_back_to_the_template() {
        let mut src = SFACE_TEMPLATE;
        for point in &mut src {
            point[0] += 12.0;
            point[1] += 7.0;
        }
        let matrix = similarity_src_to_dst(src, SFACE_TEMPLATE).expect("matrix");
        for index in 0..5 {
            let (x, y) = apply_affine(matrix, src[index][0], src[index][1]);
            assert!((x - SFACE_TEMPLATE[index][0]).abs() < 0.05);
            assert!((y - SFACE_TEMPLATE[index][1]).abs() < 0.05);
        }
    }

    #[test]
    fn yunet_cell_decodes_to_a_stride_sized_box() {
        let stride = 8.0f32;
        let mut cls = vec![0.0f32; 4];
        let mut obj = vec![0.0f32; 4];
        let bbox = vec![0.0f32; 16];
        let kps = vec![0.0f32; 40];
        cls[3] = 1.0;
        obj[3] = 1.0;
        let mut faces = Vec::new();
        decode_stride(
            stride,
            2,
            2,
            &StrideInputs {
                cls: &cls,
                cls_shape: &[1, 1, 2, 2],
                obj: &obj,
                obj_shape: &[1, 1, 2, 2],
                bbox: &bbox,
                bbox_shape: &[1, 4, 2, 2],
                kps: &kps,
                kps_shape: &[1, 10, 2, 2],
            },
            0.9,
            &mut faces,
        );
        assert_eq!(faces.len(), 1);
        let face = &faces[0];
        assert!((face.w - stride).abs() < 1e-3);
        assert!((face.h - stride).abs() < 1e-3);
        assert!((face.x - 4.0).abs() < 1e-3);
        assert!((face.y - 4.0).abs() < 1e-3);
        let kept = non_max_suppression(faces.clone(), 0.3, 10);
        assert_eq!(kept.len(), 1);
        let overlap = faces[0].clone();
        let suppressed = non_max_suppression(vec![faces[0].clone(), overlap], 0.3, 10);
        assert_eq!(suppressed.len(), 1);
    }

    #[test]
    fn detector_dims_match_the_fixed_yunet_input() {
        let (w, h, scale_x, scale_y) = detector_dims(320, 240);
        assert_eq!((w, h), (640, 640));
        assert!((scale_x - 0.5).abs() < 1e-6);
        assert!((scale_y - 0.375).abs() < 1e-6);
    }
}
