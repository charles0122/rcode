use crate::{Error, Result, codec, geometry};
use image::{DynamicImage, GrayImage, ImageFormat, ImageReader};
use std::io::Cursor;

/// Coordinates are in the original input image, before any sampling.
#[derive(Clone, Debug)]
pub struct Landmark {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug)]
pub struct DecodeResult {
    pub payload: Vec<u8>,
    pub corrected_bytes: usize,
    pub landmarks: Vec<Landmark>,
    pub width: u32,
    pub height: u32,
    pub profile: crate::Profile,
}

pub(crate) fn read_image(bytes: &[u8]) -> Result<DynamicImage> {
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(Error::InvalidImage("图片不能超过 8 MiB".into()));
    }
    let format = image::guess_format(bytes)
        .map_err(|_| Error::InvalidImage("请提供 PNG 或 JPEG 图片".into()))?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg) {
        return Err(Error::InvalidImage("仅支持 PNG 和 JPEG 图片".into()));
    }
    let (w, h) = ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|e| Error::InvalidImage(e.to_string()))?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 16_000_000 {
        return Err(Error::InvalidImage("图片尺寸不能超过 1600 万像素".into()));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(w);
    limits.max_image_height = Some(h);
    limits.max_alloc = Some(160 * 1024 * 1024);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| Error::InvalidImage(e.to_string()))
}

/// Locate and decode one complete profiled RCode using only image pixels.
pub fn decode_image(bytes: &[u8]) -> Result<DecodeResult> {
    let image = read_image(bytes)?;
    let (width, height) = (image.width(), image.height());
    // Composite transparent pixels over the same white surface as the renderer.
    let rgba = image.to_rgba8();
    let gray = GrayImage::from_fn(width, height, |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let luminance =
            (299 * u32::from(p[0]) + 587 * u32::from(p[1]) + 114 * u32::from(p[2])) / 1000;
        image::Luma([((luminance * u32::from(p[3]) + 255 * (255 - u32::from(p[3]))) / 255) as u8])
    });
    decode_gray(&gray)
}

/// Decode a camera frame supplied as 8-bit luma pixels.
///
/// `row_stride` is the number of bytes between row starts; padding is ignored.
pub fn decode_luma8(
    pixels: &[u8],
    width: u32,
    height: u32,
    row_stride: usize,
) -> Result<DecodeResult> {
    let invalid = || Error::InvalidImage("无效的 8 位亮度图像尺寸或缓冲区".into());
    let (w, h) = (
        usize::try_from(width).map_err(|_| invalid())?,
        usize::try_from(height).map_err(|_| invalid())?,
    );
    if w < 2 || h < 2 || u64::from(width) * u64::from(height) > 16_000_000 || row_stride < w {
        return Err(invalid());
    }
    let packed_len = w.checked_mul(h).ok_or_else(invalid)?;
    let last_row = (h - 1).checked_mul(row_stride).ok_or_else(invalid)?;
    let needed = last_row.checked_add(w).ok_or_else(invalid)?;
    if pixels.len() < needed {
        return Err(invalid());
    }
    let mut packed = Vec::with_capacity(packed_len);
    for y in 0..h {
        let start = y.checked_mul(row_stride).ok_or_else(invalid)?;
        packed.extend_from_slice(&pixels[start..start + w]);
    }
    let gray = GrayImage::from_raw(width, height, packed).ok_or_else(invalid)?;
    decode_gray(&gray)
}

fn decode_gray(gray: &GrayImage) -> Result<DecodeResult> {
    let (width, height) = (gray.width(), gray.height());
    let binary = Binary::new(gray);
    let mut candidates = locate(&binary);
    candidates.sort_by(|a, b| {
        (b.hits as f64 / b.radius.powf(1.5)).total_cmp(&(a.hits as f64 / a.radius.powf(1.5)))
    });
    candidates.truncate(12);
    let mut saw_geometry = false;
    let profiles = candidate_profiles();
    for a in 0..candidates.len() {
        for b in a + 1..candidates.len() {
            for c in b + 1..candidates.len() {
                for d in c + 1..candidates.len() {
                    let quad = order_quad([
                        candidates[a].clone(),
                        candidates[b].clone(),
                        candidates[c].clone(),
                        candidates[d].clone(),
                    ]);
                    if !valid_quad(&quad) {
                        continue;
                    }
                    for reverse in [false, true] {
                        for start in 0..4 {
                            let ordered = std::array::from_fn(|index| {
                                let offset = if reverse {
                                    (start + 4 - index) % 4
                                } else {
                                    (start + index) % 4
                                };
                                quad[offset].clone()
                            });
                            let Some(transform) = Transform::from_quad(&ordered) else {
                                continue;
                            };
                            let Some(landmarks) = geometry::LOCATORS
                                .into_iter()
                                .map(|(x, y)| transform.at(x, y))
                                .collect::<Option<Vec<_>>>()
                            else {
                                continue;
                            };
                            let Some(threshold) = check_locators(gray, &transform) else {
                                continue;
                            };
                            saw_geometry = true;
                            for profile in &profiles {
                                let slots = geometry::slots_for_spokes(profile.spokes());
                                let word_len = profile.codeword_len();
                                let mut codeword = vec![0u8; word_len];
                                let mut outside = false;
                                for (index, slot) in slots.iter().take(word_len * 8).enumerate() {
                                    let Some(point) = transform.at(slot.x, slot.y) else {
                                        outside = true;
                                        break;
                                    };
                                    if point.x < 1.
                                        || point.y < 1.
                                        || point.x >= f64::from(width - 1)
                                        || point.y >= f64::from(height - 1)
                                    {
                                        outside = true;
                                        break;
                                    }
                                    let mut value = 0.;
                                    for (dx, dy) in
                                        [(0., 0.), (0.18, 0.), (-0.18, 0.), (0., 0.18), (0., -0.18)]
                                    {
                                        let Some(sample_point) =
                                            transform.at(slot.x + dx, slot.y + dy)
                                        else {
                                            outside = true;
                                            break;
                                        };
                                        value += sample(gray, sample_point.x, sample_point.y);
                                    }
                                    if outside {
                                        break;
                                    }
                                    if (value / 5. < threshold) ^ geometry::mask(index) {
                                        codeword[index / 8] |= 1 << (7 - index % 8);
                                    }
                                }
                                if outside {
                                    continue;
                                }
                                if let Ok((payload, corrected_bytes)) =
                                    codec::decode(&codeword, *profile)
                                {
                                    return Ok(DecodeResult {
                                        payload,
                                        corrected_bytes,
                                        landmarks,
                                        width,
                                        height,
                                        profile: *profile,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if saw_geometry {
        Err(Error::CorruptCode("定位成功，但数据损坏或校验失败".into()))
    } else {
        Err(Error::NoCode)
    }
}

fn candidate_profiles() -> Vec<crate::Profile> {
    let mut profiles = vec![crate::Profile::default()];
    for spokes in [36, 54, 72] {
        for level in crate::EccLevel::ALL {
            let profile = crate::Profile::new(spokes, level).unwrap();
            if profile != crate::Profile::default() {
                profiles.push(profile);
            }
        }
    }
    profiles
}

fn sample(image: &GrayImage, x: f64, y: f64) -> f64 {
    if x < 0. || y < 0. || x >= f64::from(image.width() - 1) || y >= f64::from(image.height() - 1) {
        return 255.;
    }
    let (ix, iy) = (x.floor() as u32, y.floor() as u32);
    let (fx, fy) = (x - f64::from(ix), y - f64::from(iy));
    let pixel = |dx, dy| f64::from(image.get_pixel(ix + dx, iy + dy).0[0]);
    (pixel(0, 0) * (1. - fx) + pixel(1, 0) * fx) * (1. - fy)
        + (pixel(0, 1) * (1. - fx) + pixel(1, 1) * fx) * fy
}

struct Binary {
    pixels: Vec<bool>,
    width: usize,
    height: usize,
}

impl Binary {
    fn new(gray: &GrayImage) -> Self {
        let mut hist = [0u64; 256];
        for p in gray.as_raw() {
            hist[usize::from(*p)] += 1;
        }
        let total = u64::from(gray.width()) * u64::from(gray.height());
        let sum: u64 = hist.iter().enumerate().map(|(i, n)| i as u64 * n).sum();
        let (mut weight, mut partial, mut best, mut threshold) = (0u64, 0u64, 0., 127u8);
        for (i, n) in hist.iter().enumerate() {
            weight += n;
            partial += i as u64 * n;
            if weight == 0 || weight == total {
                continue;
            }
            let delta =
                partial as f64 / weight as f64 - (sum - partial) as f64 / (total - weight) as f64;
            let score = weight as f64 * (total - weight) as f64 * delta * delta;
            if score > best {
                best = score;
                threshold = i as u8;
            }
        }
        let (width, height) = (gray.width() as usize, gray.height() as usize);
        let raw: Vec<bool> = gray.as_raw().iter().map(|p| *p <= threshold).collect();
        let mut pixels = raw.clone();
        // A 3x3 majority removes isolated salt/pepper pixels only for locator search.
        // Data is sampled from the original grayscale image.
        if width > 2 && height > 2 {
            for y in 1..height - 1 {
                for x in 1..width - 1 {
                    let mut count = 0;
                    for yy in y - 1..=y + 1 {
                        for xx in x - 1..=x + 1 {
                            count += usize::from(raw[yy * width + xx]);
                        }
                    }
                    pixels[y * width + x] = count >= 5;
                }
            }
        }
        Self {
            pixels,
            width,
            height,
        }
    }
    fn at(&self, x: usize, y: usize) -> bool {
        self.pixels[y * self.width + x]
    }
}

#[derive(Clone, Debug)]
struct Candidate {
    x: f64,
    y: f64,
    radius: f64,
    hits: usize,
}

fn matches_runs(runs: &[usize; 5]) -> bool {
    let total: usize = runs.iter().sum();
    if total < 20 || runs.contains(&0) {
        return false;
    }
    let unit = total as f64 / 5.;
    runs.iter().all(|n| (*n as f64 - unit).abs() < unit * 0.65)
}

fn cross(binary: &Binary, x: usize, y: usize, vertical: bool) -> Option<(f64, f64)> {
    let (center, limit) = if vertical {
        (y, binary.height)
    } else {
        (x, binary.width)
    };
    let at = |p| {
        if vertical {
            binary.at(x, p)
        } else {
            binary.at(p, y)
        }
    };
    if !at(center) {
        return None;
    }
    let mut runs = [0usize; 5];
    let mut lo = center as isize;
    for index in [2, 1, 0] {
        let black = index % 2 == 0;
        while lo >= 0 && at(lo as usize) == black {
            runs[index] += 1;
            lo -= 1;
        }
    }
    let mut hi = center + 1;
    for index in [2, 3, 4] {
        let black = index % 2 == 0;
        while hi < limit && at(hi) == black {
            runs[index] += 1;
            hi += 1;
        }
    }
    if lo < 0 || hi >= limit || !matches_runs(&runs) {
        return None;
    }
    let total: usize = runs.iter().sum();
    Some((
        lo as f64 + 1. + runs[0] as f64 + runs[1] as f64 + (runs[2] as f64 - 1.) / 2.,
        total as f64 / 2.,
    ))
}

fn locate(binary: &Binary) -> Vec<Candidate> {
    let mut found: Vec<Candidate> = Vec::new();
    let stride = (binary.height / 1000).max(1);
    for y in (0..binary.height).step_by(stride) {
        let mut runs: Vec<(bool, usize, usize)> = Vec::new();
        let mut start = 0;
        while start < binary.width {
            let black = binary.at(start, y);
            let mut end = start + 1;
            while end < binary.width && binary.at(end, y) == black {
                end += 1;
            }
            runs.push((black, start, end - start));
            if runs.len() >= 5 && black {
                let n = runs.len();
                let window = &runs[n - 5..];
                let lengths = std::array::from_fn(|i| window[i].2);
                if window[0].0 && matches_runs(&lengths) {
                    let x = window[2].1 + window[2].2 / 2;
                    if let Some((cy, vr)) = cross(binary, x, y, true) {
                        let iy = cy.round().clamp(0., (binary.height - 1) as f64) as usize;
                        if let Some((cx, hr)) = cross(binary, x, iy, false)
                            && (vr - hr).abs() < (vr + hr) * 0.3
                        {
                            let radius = (vr + hr) / 2.;
                            if let Some(existing) = found.iter_mut().find(|p| {
                                (p.x - cx).hypot(p.y - cy) < radius * 0.4
                                    && (p.radius - radius).abs() < radius * 0.3
                            }) {
                                let hits = existing.hits as f64;
                                existing.x = (existing.x * hits + cx) / (hits + 1.);
                                existing.y = (existing.y * hits + cy) / (hits + 1.);
                                existing.radius = (existing.radius * hits + radius) / (hits + 1.);
                                existing.hits += 1;
                            } else if found.len() < 256 {
                                found.push(Candidate {
                                    x: cx,
                                    y: cy,
                                    radius,
                                    hits: 1,
                                });
                            }
                        }
                    }
                }
            }
            start = end;
        }
    }
    found.retain(|p| p.hits >= 2);
    found
}

struct Transform {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
    g: f64,
    h: f64,
}

impl Transform {
    fn at(&self, x: f64, y: f64) -> Option<Landmark> {
        let (u, v) = ((x + 34.) / 68., (y + 34.) / 68.);
        let denominator = self.g * u + self.h * v + 1.;
        if !denominator.is_finite() || denominator.abs() < 1e-8 {
            return None;
        }
        let point = Landmark {
            x: (self.a * u + self.b * v + self.c) / denominator,
            y: (self.d * u + self.e * v + self.f) / denominator,
        };
        (point.x.is_finite() && point.y.is_finite()).then_some(point)
    }
    fn from_quad(points: &[Candidate; 4]) -> Option<Self> {
        let (x0, y0) = (points[0].x, points[0].y);
        let (x1, y1) = (points[1].x, points[1].y);
        let (x2, y2) = (points[2].x, points[2].y);
        let (x3, y3) = (points[3].x, points[3].y);
        let (dx1, dx2, dx3) = (x1 - x2, x3 - x2, x0 - x1 + x2 - x3);
        let (dy1, dy2, dy3) = (y1 - y2, y3 - y2, y0 - y1 + y2 - y3);
        let (g, h) = if dx3.abs() < 1e-8 && dy3.abs() < 1e-8 {
            (0., 0.)
        } else {
            let denominator = dx1 * dy2 - dx2 * dy1;
            if denominator.abs() < 1e-8 {
                return None;
            }
            (
                (dx3 * dy2 - dx2 * dy3) / denominator,
                (dx1 * dy3 - dx3 * dy1) / denominator,
            )
        };
        let transform = Self {
            a: x1 - x0 + g * x1,
            b: x3 - x0 + h * x3,
            c: x0,
            d: y1 - y0 + g * y1,
            e: y3 - y0 + h * y3,
            f: y0,
            g,
            h,
        };
        let denominators = [1., 1. + g, 1. + g + h, 1. + h];
        if denominators
            .iter()
            .any(|value| !value.is_finite() || value.abs() < 1e-5)
            || denominators.iter().any(|value| value.is_sign_positive())
                != denominators.iter().all(|value| value.is_sign_positive())
        {
            return None;
        }
        Some(transform)
    }
}

fn order_quad(mut points: [Candidate; 4]) -> [Candidate; 4] {
    let center = (
        points.iter().map(|p| p.x).sum::<f64>() / 4.,
        points.iter().map(|p| p.y).sum::<f64>() / 4.,
    );
    points.sort_by(|a, b| {
        (a.y - center.1)
            .atan2(a.x - center.0)
            .total_cmp(&(b.y - center.1).atan2(b.x - center.0))
    });
    points
}

fn valid_quad(points: &[Candidate; 4]) -> bool {
    let mut min_side = f64::INFINITY;
    let mut max_side: f64 = 0.;
    let mut crosses = [0.; 4];
    for index in 0..4 {
        let p = &points[index];
        let q = &points[(index + 1) % 4];
        let r = &points[(index + 2) % 4];
        let side = (q.x - p.x).hypot(q.y - p.y);
        min_side = min_side.min(side);
        max_side = max_side.max(side);
        crosses[index] = (q.x - p.x) * (r.y - q.y) - (q.y - p.y) * (r.x - q.x);
    }
    if min_side < 80. || max_side / min_side > 3.5 {
        return false;
    }
    let positive = crosses[0] > 0.;
    if crosses
        .iter()
        .any(|cross| cross.abs() < min_side * min_side * 0.03 || (*cross > 0.) != positive)
    {
        return false;
    }
    for index in 0..4 {
        let previous = &points[(index + 3) % 4];
        let current = &points[index];
        let next = &points[(index + 1) % 4];
        let local_scale = ((current.x - previous.x).hypot(current.y - previous.y)
            + (next.x - current.x).hypot(next.y - current.y))
            / 136.;
        let expected_radius = geometry::LOCATOR_RADII[0] * local_scale;
        if current.radius < expected_radius * 0.35 || current.radius > expected_radius * 2.5 {
            return false;
        }
    }
    true
}

fn check_locators(gray: &GrayImage, transform: &Transform) -> Option<f64> {
    let [outer_radius, opening, core_radius] = geometry::LOCATOR_RADII;
    let white_radius = (opening + core_radius) / 2.;
    let black_radius = (outer_radius + opening) / 2.;
    let mut dark_values = Vec::with_capacity(4 * 9);
    let mut white_values = Vec::with_capacity(4 * 8);
    let mut locator_samples = Vec::with_capacity(4);
    for (x, y) in geometry::LOCATORS {
        let center = transform.at(x, y)?;
        let center_value = sample(gray, center.x, center.y);
        let mut white = Vec::with_capacity(8);
        let mut outer = Vec::with_capacity(8);
        for i in 0..8 {
            let angle = std::f64::consts::TAU * f64::from(i) / 8.;
            let white_point = transform.at(
                x + white_radius * angle.cos(),
                y + white_radius * angle.sin(),
            )?;
            let outer_point = transform.at(
                x + black_radius * angle.cos(),
                y + black_radius * angle.sin(),
            )?;
            white.push(sample(gray, white_point.x, white_point.y));
            outer.push(sample(gray, outer_point.x, outer_point.y));
        }
        dark_values.push(center_value);
        dark_values.extend(outer.iter().copied());
        white_values.extend(white.iter().copied());
        locator_samples.push((center_value, mean(&white), mean(&outer)));
    }
    let dark = mean(&dark_values);
    let white = mean(&white_values);
    if white - dark < 60. {
        return None;
    }
    let threshold = (dark + white) / 2.;
    if locator_samples.iter().any(|(center, middle, outer)| {
        *center >= threshold || *middle <= threshold || *outer >= threshold
    }) {
        return None;
    }
    Some(threshold)
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}
