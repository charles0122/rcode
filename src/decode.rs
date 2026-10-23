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
    pub profile: Option<crate::Profile>,
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

/// Locate and decode one complete RadialCode v1 using only image pixels.
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
    let binary = Binary::new(&gray);
    let mut candidates = locate(&binary);
    candidates.sort_by(|a, b| b.hits.cmp(&a.hits));
    candidates.truncate(24);
    let mut saw_geometry = false;
    for (i, corner) in candidates.iter().enumerate() {
        for (j, first) in candidates.iter().enumerate() {
            if i == j {
                continue;
            }
            for (k, second) in candidates.iter().enumerate() {
                if i == k || j == k {
                    continue;
                }
                let Some(transform) = Transform::from_triangle(corner, first, second) else {
                    continue;
                };
                let landmarks: Vec<_> = [(-34., -34.), (34., -34.), (-34., 34.), (34., 34.)]
                    .into_iter()
                    .map(|(x, y)| transform.at(x, y))
                    .collect();
                let black = landmarks[..3]
                    .iter()
                    .map(|p| sample(&gray, p.x, p.y))
                    .sum::<f64>()
                    / 3.;
                let measured_radius = (corner.radius + first.radius + second.radius)
                    / (3. * transform.a.hypot(transform.b));
                let radii = geometry::SUPPORTED_LOCATOR_RADII
                    .into_iter()
                    .min_by(|a, b| {
                        (a[0] - measured_radius)
                            .abs()
                            .total_cmp(&(b[0] - measured_radius).abs())
                    })
                    .unwrap();
                let white_radius = (radii[1] + radii[2]) / 2.;
                let white = geometry::LOCATORS[..3]
                    .iter()
                    .map(|(x, y)| {
                        let p = transform.at(x + white_radius, *y);
                        sample(&gray, p.x, p.y)
                    })
                    .sum::<f64>()
                    / 3.;
                if white - black < 70. {
                    continue;
                }
                let threshold = (black + white) / 2.;
                if !check_ring(&gray, &transform, threshold) {
                    continue;
                }
                saw_geometry = true;
                let profiles =
                    std::iter::once(None).chain([36, 54, 72].into_iter().flat_map(|spokes| {
                        crate::EccLevel::ALL
                            .into_iter()
                            .map(move |level| Some(crate::Profile::new(spokes, level).unwrap()))
                    }));
                for profile in profiles {
                    let slots = geometry::slots_for_spokes(profile.map_or(96, |p| p.spokes()));
                    let word_len = profile.map_or(202, |p| p.codeword_len());
                    let mut codeword = vec![0u8; word_len];
                    let mut outside = false;
                    for (index, slot) in slots.iter().take(word_len * 8).enumerate() {
                        let p = transform.at(slot.x, slot.y);
                        if p.x < 1.
                            || p.y < 1.
                            || p.x >= f64::from(width - 1)
                            || p.y >= f64::from(height - 1)
                        {
                            outside = true;
                            break;
                        }
                        let mut value = 0.;
                        for (dx, dy) in [(0., 0.), (0.18, 0.), (-0.18, 0.), (0., 0.18), (0., -0.18)]
                        {
                            let q = transform.at(slot.x + dx, slot.y + dy);
                            value += sample(&gray, q.x, q.y);
                        }
                        let bit = (value / 5. < threshold) ^ geometry::mask(index);
                        if bit {
                            codeword[index / 8] |= 1 << (7 - index % 8);
                        }
                    }
                    if outside {
                        continue;
                    }
                    let decoded = match profile {
                        Some(p) => codec::decode_profile(&codeword, p),
                        None => codec::decode_codeword(&codeword),
                    };
                    if let Ok((payload, corrected_bytes)) = decoded {
                        return Ok(DecodeResult {
                            payload,
                            corrected_bytes,
                            landmarks,
                            width,
                            height,
                            profile,
                        });
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
                        if let Some((cx, hr)) = cross(binary, x, iy, false) {
                            if (vr - hr).abs() < (vr + hr) * 0.12 {
                                let radius = (vr + hr) / 2.;
                                if let Some(existing) = found.iter_mut().find(|p| {
                                    (p.x - cx).hypot(p.y - cy) < radius * 0.4
                                        && (p.radius - radius).abs() < radius * 0.3
                                }) {
                                    let hits = existing.hits as f64;
                                    existing.x = (existing.x * hits + cx) / (hits + 1.);
                                    existing.y = (existing.y * hits + cy) / (hits + 1.);
                                    existing.radius =
                                        (existing.radius * hits + radius) / (hits + 1.);
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
    tx: f64,
    ty: f64,
}

impl Transform {
    fn at(&self, x: f64, y: f64) -> Landmark {
        Landmark {
            x: self.a * x - self.b * y + self.tx,
            y: self.b * x + self.a * y + self.ty,
        }
    }
    fn from_triangle(p: &Candidate, q: &Candidate, r: &Candidate) -> Option<Self> {
        let (ux, uy, vx, vy) = (q.x - p.x, q.y - p.y, r.x - p.x, r.y - p.y);
        let (ul, vl) = (ux.hypot(uy), vx.hypot(vy));
        if ul < 80.
            || vl < 80.
            || (ul - vl).abs() > (ul + vl) * 0.06
            || (ux * vx + uy * vy).abs() > ul * vl * 0.1
            || ux * vy - uy * vx <= 0.
        {
            return None;
        }
        let scale = (ul + vl) / 136.;
        if [p, q, r].iter().any(|point| {
            geometry::SUPPORTED_LOCATOR_RADII
                .iter()
                .all(|radius| (point.radius / (radius[0] * scale) - 1.).abs() > 0.15)
        }) {
            return None;
        }
        let source = [(-34., -34.), (34., -34.), (-34., 34.)];
        let target = [p, q, r];
        let (mx, my) = ((p.x + q.x + r.x) / 3., (p.y + q.y + r.y) / 3.);
        let mean = -34. / 3.;
        let (mut dot, mut determinant, mut denominator) = (0., 0., 0.);
        for ((x, y), point) in source.into_iter().zip(target) {
            let (x, y, xx, yy) = (x - mean, y - mean, point.x - mx, point.y - my);
            dot += x * xx + y * yy;
            determinant += x * yy - y * xx;
            denominator += x * x + y * y;
        }
        let (a, b) = (dot / denominator, determinant / denominator);
        Some(Self {
            a,
            b,
            tx: mx - a * mean + b * mean,
            ty: my - b * mean - a * mean,
        })
    }
}

fn check_ring(gray: &GrayImage, transform: &Transform, threshold: f64) -> bool {
    let p = transform.at(34., 34.);
    if sample(gray, p.x, p.y) <= threshold {
        return false;
    }
    geometry::SUPPORTED_LOCATOR_RADII
        .into_iter()
        .any(|[outer_radius, opening, _]| {
            let mut valid = 0;
            let black_radius = (outer_radius + opening) / 2.;
            for i in 0..12 {
                let angle = std::f64::consts::TAU * f64::from(i) / 12.;
                let outer = transform.at(
                    34. + black_radius * angle.cos(),
                    34. + black_radius * angle.sin(),
                );
                let inner = transform.at(
                    34. + opening * 0.5 * angle.cos(),
                    34. + opening * 0.5 * angle.sin(),
                );
                if sample(gray, outer.x, outer.y) < threshold
                    && sample(gray, inner.x, inner.y) > threshold
                {
                    valid += 1;
                }
            }
            valid >= 10
        })
}
