use std::f64::consts::PI;
use std::io::Cursor;

use crc32fast::Hasher;
use image::codecs::jpeg::JpegEncoder;
use image::imageops;
use image::{ColorType, DynamicImage, ImageEncoder, ImageFormat, Rgba, RgbaImage};
use rcode::{DecodeResult, Error, RenderOptions, Style, decode_image, encode, render_png};

const TEST_SIZE: u32 = 768;

#[test]
fn png_roundtrip_covers_utf8_binary_boundary_and_styles() {
    let mut random = Vec::with_capacity(128);
    let mut rng = Rng::new(0x5eed_1234);
    for _ in 0..128 {
        random.push(rng.next_u8());
    }
    let payloads = vec![Vec::new(), "你好，RCode 🌀".as_bytes().to_vec(), random];

    for style in [Style::Dots, Style::Lines] {
        for payload in &payloads {
            let symbol = encode(payload).expect("payload should fit v1");
            let png = render_png(
                &symbol,
                &RenderOptions {
                    style,
                    size: TEST_SIZE,
                    ..RenderOptions::default()
                },
            )
            .expect("PNG rendering should succeed");
            let decoded = decode_image(&png).expect("rendered PNG should decode");
            assert_eq!(&decoded.payload, payload);
            assert_eq!((decoded.width, decoded.height), (TEST_SIZE, TEST_SIZE));
            assert_eq!(decoded.landmarks.len(), 4);
        }
    }

    let too_long = vec![0u8; 129];
    assert!(matches!(encode(&too_long), Err(Error::PayloadTooLong(129))));
}

#[test]
fn logo_is_decodable_without_changing_the_payload() {
    let logo = logo_png();
    for style in [Style::Dots, Style::Lines] {
        let symbol = encode("logo stays in the centre".as_bytes()).unwrap();
        let png = render_png(
            &symbol,
            &RenderOptions {
                style,
                logo: Some(logo.clone()),
                size: TEST_SIZE,
                ..RenderOptions::default()
            },
        )
        .unwrap();
        assert_eq!(decode_image(&png).unwrap().payload, symbol.payload);
    }
}

#[test]
fn transformed_png_roundtrip_accepts_rotation_scale_and_translation() {
    let payload = "旋转、缩放、平移仍然可读".as_bytes();
    let symbol = encode(payload).unwrap();
    let source = render_png(
        &symbol,
        &RenderOptions {
            size: TEST_SIZE,
            ..RenderOptions::default()
        },
    )
    .unwrap();

    // A half-size case is intentionally included to cover a 384 px canvas.
    for &(angle, size, tx, ty) in &[
        (13.0, 384, 6.0, -5.0),
        (45.0, 512, -8.0, 7.0),
        (90.0, 768, 11.0, -12.0),
        (179.0, 1024, -15.0, 12.0),
        (271.0, 1536, 18.0, -14.0),
    ] {
        let transformed = warp_png(
            &source,
            size,
            angle,
            f64::from(size) / f64::from(TEST_SIZE),
            tx,
            ty,
        );
        let decoded = decode_image(&transformed)
            .unwrap_or_else(|error| panic!("{angle}°/{size}px transform should decode: {error}"));
        assert_eq!(decoded.payload, payload);
        assert_eq!((decoded.width, decoded.height), (size, size));
    }
}

#[test]
fn light_image_damage_remains_decodable() {
    let symbol = encode("noise tolerance".as_bytes()).unwrap();
    let source = render_png(
        &symbol,
        &RenderOptions {
            size: TEST_SIZE,
            ..RenderOptions::default()
        },
    )
    .unwrap();
    let base = image::load_from_memory(&source).unwrap().to_rgba8();

    let mut gaussian = base.clone();
    add_gaussian_noise(&mut gaussian, 8.0, &mut Rng::new(0x11aa_22bb));
    assert_payload(&png_bytes(gaussian), b"noise tolerance");

    let mut salt_pepper = base.clone();
    add_salt_pepper(&mut salt_pepper, 0.005, &mut Rng::new(0x3344_5566));
    assert_payload(&png_bytes(salt_pepper), b"noise tolerance");

    let blurred = imageops::blur(&base, 0.6);
    assert_payload(&png_bytes(blurred), b"noise tolerance");

    let jpeg = jpeg_bytes(base, 85);
    assert_payload(&jpeg, b"noise tolerance");
}

#[test]
fn seeded_rotation_scale_cases_roundtrip() {
    let mut rng = Rng::new(0xdec0_de01);
    for case in 0..200 {
        let length = usize::from(rng.next_u8()) % 129;
        let mut payload = vec![0u8; length];
        for byte in &mut payload {
            *byte = rng.next_u8();
        }
        let symbol = encode(&payload).unwrap();
        let source = render_png(
            &symbol,
            &RenderOptions {
                size: 384,
                style: if case % 2 == 0 {
                    Style::Lines
                } else {
                    Style::Dots
                },
                ..RenderOptions::default()
            },
        )
        .unwrap();
        let size = 384 + u32::from(rng.next_u8()) % 257;
        let angle = f64::from(rng.next_u16()) / f64::from(u16::MAX) * 360.0;
        let scale = f64::from(size) / 384.0;
        let tx = f64::from(rng.next_u8()) / 255.0 * 12.0 - 6.0;
        let ty = f64::from(rng.next_u8()) / 255.0 * 12.0 - 6.0;
        let transformed = warp_png(&source, size, angle, scale, tx, ty);
        let decoded = decode_image(&transformed).unwrap_or_else(|error| {
            panic!("seeded case {case} ({size}px, {angle:.2}°) failed: {error}")
        });
        assert_eq!(decoded.payload, payload, "seeded case {case}");
    }
}

#[test]
fn blank_noise_qr_and_malformed_inputs_are_rejected() {
    let blank = png_bytes(RgbaImage::from_pixel(384, 384, Rgba([255, 255, 255, 255])));
    assert!(decode_image(&blank).is_err());

    let mut noise = RgbaImage::from_pixel(384, 384, Rgba([0, 0, 0, 255]));
    let mut rng = Rng::new(0xa11c_e55e);
    for pixel in noise.pixels_mut() {
        let value = rng.next_u8();
        *pixel = Rgba([value, value, value, 255]);
    }
    assert!(decode_image(&png_bytes(noise)).is_err());
    assert!(decode_image(&qr_like_png()).is_err());

    assert!(matches!(
        decode_image(b"not an image"),
        Err(Error::InvalidImage(_))
    ));
    assert!(matches!(
        decode_image(b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;"),
        Err(Error::InvalidImage(_))
    ));
    assert!(matches!(
        decode_image(&oversized_png()),
        Err(Error::InvalidImage(_))
    ));
}

fn assert_payload(bytes: &[u8], expected: &[u8]) {
    let decoded = decode_image(bytes).expect("damaged image should still decode");
    assert_eq!(decoded.payload, expected);
}

fn logo_png() -> Vec<u8> {
    let image = RgbaImage::from_fn(32, 32, |x, y| {
        let stripe = (x / 4 + y / 4) % 2 == 0;
        if stripe {
            Rgba([0x07, 0x82, 0x3d, 255])
        } else {
            Rgba([0xff, 0xd1, 0x66, 255])
        }
    });
    png_bytes(image)
}

fn png_bytes(image: RgbaImage) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut output, ImageFormat::Png)
        .expect("PNG encoding should succeed");
    output.into_inner()
}

fn jpeg_bytes(image: RgbaImage, quality: u8) -> Vec<u8> {
    let rgb = DynamicImage::ImageRgba8(image).to_rgb8();
    let mut output = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut output, quality);
    encoder
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ColorType::Rgb8.into(),
        )
        .expect("JPEG encoding should succeed");
    output.into_inner()
}

fn warp_png(source: &[u8], size: u32, angle_degrees: f64, scale: f64, tx: f64, ty: f64) -> Vec<u8> {
    let source = image::load_from_memory(source).unwrap().to_rgba8();
    let mut output = RgbaImage::from_pixel(size, size, Rgba([255, 255, 255, 255]));
    let src_center = (
        (f64::from(source.width()) - 1.0) / 2.0,
        (f64::from(source.height()) - 1.0) / 2.0,
    );
    let dst_center = (
        (f64::from(size) - 1.0) / 2.0 + tx,
        (f64::from(size) - 1.0) / 2.0 + ty,
    );
    let angle = angle_degrees * PI / 180.0;
    let (sin, cos) = angle.sin_cos();
    for y in 0..size {
        for x in 0..size {
            let dx = (f64::from(x) - dst_center.0) / scale;
            let dy = (f64::from(y) - dst_center.1) / scale;
            let sx = src_center.0 + cos * dx + sin * dy;
            let sy = src_center.1 - sin * dx + cos * dy;
            if let Some(pixel) = sample_rgba(&source, sx, sy) {
                output.put_pixel(x, y, pixel);
            }
        }
    }
    png_bytes(output)
}

fn sample_rgba(image: &RgbaImage, x: f64, y: f64) -> Option<Rgba<u8>> {
    if x < 0.0 || y < 0.0 || x > f64::from(image.width() - 1) || y > f64::from(image.height() - 1) {
        return None;
    }
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(image.width() - 1);
    let y1 = (y0 + 1).min(image.height() - 1);
    let fx = x - f64::from(x0);
    let fy = y - f64::from(y0);
    let a = image.get_pixel(x0, y0).0;
    let b = image.get_pixel(x1, y0).0;
    let c = image.get_pixel(x0, y1).0;
    let d = image.get_pixel(x1, y1).0;
    let mut value = [0u8; 4];
    for channel in 0..4 {
        let top = f64::from(a[channel]) * (1.0 - fx) + f64::from(b[channel]) * fx;
        let bottom = f64::from(c[channel]) * (1.0 - fx) + f64::from(d[channel]) * fx;
        value[channel] = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
    }
    Some(Rgba(value))
}

fn add_gaussian_noise(image: &mut RgbaImage, sigma: f64, rng: &mut Rng) {
    for pixel in image.pixels_mut() {
        // Twelve uniforms approximate a unit normal without pulling in rand.
        let normal = (0..12).map(|_| rng.next_unit()).sum::<f64>() - 6.0;
        let delta = normal * sigma;
        for channel in &mut pixel.0[..3] {
            *channel = (f64::from(*channel) + delta).round().clamp(0.0, 255.0) as u8;
        }
    }
}

fn add_salt_pepper(image: &mut RgbaImage, fraction: f64, rng: &mut Rng) {
    for pixel in image.pixels_mut() {
        if rng.next_unit() < fraction {
            let value = if rng.next_u8() & 1 == 0 { 0 } else { 255 };
            *pixel = Rgba([value, value, value, 255]);
        }
    }
}

fn qr_like_png() -> Vec<u8> {
    const MODULES: usize = 21;
    const SCALE: u32 = 16;
    const QUIET: u32 = 4;
    let side = (MODULES as u32 + QUIET * 2) * SCALE;
    let mut modules = [[false; MODULES]; MODULES];
    for &(left, top) in &[(0usize, 0usize), (14, 0), (0, 14)] {
        for y in 0..7 {
            for x in 0..7 {
                modules[top + y][left + x] = x == 0
                    || x == 6
                    || y == 0
                    || y == 6
                    || (2..=4).contains(&x) && (2..=4).contains(&y);
            }
        }
    }
    for index in 8..13 {
        modules[6][index] = index % 2 == 0;
        modules[index][6] = index % 2 == 0;
    }
    for y in 8..MODULES {
        for x in 8..MODULES {
            if !modules[y][x] {
                modules[y][x] = (x * 17 + y * 31) % 7 < 3;
            }
        }
    }
    let mut image = RgbaImage::from_pixel(side, side, Rgba([255, 255, 255, 255]));
    for y in 0..MODULES {
        for x in 0..MODULES {
            if modules[y][x] {
                for py in 0..SCALE {
                    for px in 0..SCALE {
                        image.put_pixel(
                            (x as u32 + QUIET) * SCALE + px,
                            (y as u32 + QUIET) * SCALE + py,
                            Rgba([0, 0, 0, 255]),
                        );
                    }
                }
            }
        }
    }
    png_bytes(image)
}

fn oversized_png() -> Vec<u8> {
    // A valid 1×1 PNG whose IHDR dimensions are replaced with 4097×4097.
    // The decoder rejects it from the header before allocating pixels.
    let mut png = vec![
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0,
        0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    png[16..20].copy_from_slice(&4097u32.to_be_bytes());
    png[20..24].copy_from_slice(&4097u32.to_be_bytes());
    let mut hasher = Hasher::new();
    hasher.update(&png[12..29]);
    png[29..33].copy_from_slice(&hasher.finalize().to_be_bytes());
    png
}

#[derive(Clone, Copy)]
struct Rng(u64);

impl Rng {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.0 >> 32) as u32
    }

    fn next_u16(&mut self) -> u16 {
        self.next_u32() as u16
    }

    fn next_u8(&mut self) -> u8 {
        self.next_u32() as u8
    }

    fn next_unit(&mut self) -> f64 {
        f64::from(self.next_u32()) / f64::from(u32::MAX)
    }
}

#[allow(dead_code)]
fn _assert_result_shape(result: &DecodeResult) {
    assert_eq!(result.landmarks.len(), 4);
}

#[test]
fn all_twelve_profiles_decode_from_pixels_with_capacity_boundary_and_rotation() {
    use rcode::{EccLevel, Profile, encode_with_profile, render_svg};
    for spokes in [36, 54, 72] {
        for level in EccLevel::ALL {
            let profile = Profile::new(spokes, level).unwrap();
            let payload: Vec<_> = (0..profile.capacity()).map(|i| (i * 31) as u8).collect();
            let symbol = encode_with_profile(&payload, profile).unwrap();
            for style in [Style::Dots, Style::Lines] {
                let options = RenderOptions {
                    size: 384,
                    style,
                    ..RenderOptions::default()
                };
                let png = render_png(&symbol, &options).unwrap();
                assert!(render_svg(&symbol, &options).unwrap().contains("<svg"));
                let mut noisy = image::load_from_memory(&png).unwrap().to_rgba8();
                add_gaussian_noise(&mut noisy, 8.0, &mut Rng::new(0x76543210));
                for image in [
                    png.clone(),
                    warp_png(&png, 384, 37.5, 1.0, 3., -4.),
                    png_bytes(noisy),
                ] {
                    let decoded = decode_image(&image)
                        .unwrap_or_else(|e| panic!("{spokes}/{level}/{style:?}: {e}"));
                    assert_eq!(decoded.payload, payload);
                    assert_eq!(decoded.profile, Some(profile));
                }
            }
            let empty = render_png(
                &encode_with_profile(b"", profile).unwrap(),
                &RenderOptions {
                    size: 384,
                    ..RenderOptions::default()
                },
            )
            .unwrap();
            assert!(decode_image(&empty).unwrap().payload.is_empty());
        }
    }
}

#[test]
fn original_v2_locator_proportions_remain_decodable() {
    use rcode::{EccLevel, Profile};
    let decoded = decode_image(include_bytes!("fixtures/v2-original54m.png")).unwrap();
    assert_eq!(decoded.payload, b"RadialCode");
    assert_eq!(
        decoded.profile,
        Some(Profile::new(54, EccLevel::M).unwrap())
    );
}

#[test]
fn previous_refined_v2_proportions_remain_decodable() {
    use rcode::{EccLevel, Profile};
    let decoded = decode_image(include_bytes!("fixtures/v2-refined54m.png")).unwrap();
    assert_eq!(decoded.payload, b"RadialCode");
    assert_eq!(
        decoded.profile,
        Some(Profile::new(54, EccLevel::M).unwrap())
    );
}

#[test]
fn every_spoke_has_the_same_visible_inner_boundary() {
    use rcode::{EccLevel, Profile, encode_with_profile};
    for spokes in [36, 54, 72] {
        let profile = Profile::new(spokes, EccLevel::M).unwrap();
        for payload in [Vec::new(), vec![0xa5; profile.capacity()]] {
            for style in [Style::Lines, Style::Dots] {
                let png = render_png(
                    &encode_with_profile(&payload, profile).unwrap(),
                    &RenderOptions {
                        style,
                        ..RenderOptions::default()
                    },
                )
                .unwrap();
                let image = image::load_from_memory(&png).unwrap().to_rgba8();
                for spoke in 0..spokes {
                    let angle = -PI / 2. + 2. * PI * spoke as f64 / spokes as f64;
                    let first_dark = (0..=44)
                        .map(|step| 21.0 + step as f64 * 0.05)
                        .find(|radius| {
                            let pixel = sample_rgba(
                                &image,
                                (64. + radius * angle.cos()) * 6. - 0.5,
                                (64. + radius * angle.sin()) * 6. - 0.5,
                            )
                            .unwrap();
                            pixel.0[..3].iter().map(|v| u32::from(*v)).sum::<u32>() < 3 * 128
                        })
                        .expect("each spoke needs an inner contour mark");
                    assert!(
                        (first_dark - 22.0).abs() < 0.2,
                        "{spokes}/{style:?}, spoke {spoke}: {first_dark}"
                    );
                }
                assert_eq!(decode_image(&png).unwrap().payload, payload);
            }
        }
    }
}

#[test]
fn dots_keep_visible_radial_gaps() {
    use rcode::{EccLevel, Profile, encode_with_profile};
    for spokes in [36, 54, 72] {
        let profile = Profile::new(spokes, EccLevel::M).unwrap();
        for payload in [Vec::new(), vec![0xa5; profile.capacity()]] {
            for size in [384, 768] {
                let png = render_png(
                    &encode_with_profile(&payload, profile).unwrap(),
                    &RenderOptions {
                        size,
                        style: Style::Dots,
                        ..RenderOptions::default()
                    },
                )
                .unwrap();
                let image = image::load_from_memory(&png).unwrap().to_rgba8();
                let scale = f64::from(size) / 128.;
                for spoke in 0..spokes {
                    let angle = -PI / 2. + 2. * PI * spoke as f64 / spokes as f64;
                    for ring in 0..21 {
                        let radius = 25.75 + 1.5 * ring as f64;
                        let (x, y) = (radius * angle.cos(), radius * angle.sin());
                        if rcode::geometry::LOCATORS
                            .iter()
                            .any(|(lx, ly)| (x - lx).hypot(y - ly) < 11.)
                        {
                            continue;
                        }
                        let pixel =
                            sample_rgba(&image, (64. + x) * scale - 0.5, (64. + y) * scale - 0.5)
                                .unwrap();
                        let brightness =
                            pixel.0[..3].iter().map(|v| u32::from(*v)).sum::<u32>() / 3;
                        assert!(
                            brightness > 150,
                            "{spokes} dots, {size}px, spoke {spoke}, ring {ring}: {brightness}"
                        );
                    }
                }
                assert_eq!(decode_image(&png).unwrap().payload, payload);
            }
        }
    }
}

#[test]
fn dense_inner_spokes_keep_visible_gaps_at_minimum_size() {
    use rcode::{EccLevel, Profile, encode_with_profile};
    for spokes in [36, 54, 72] {
        let profile = Profile::new(spokes, EccLevel::M).unwrap();
        for payload in [Vec::new(), vec![0xa5; profile.capacity()]] {
            let png = render_png(
                &encode_with_profile(&payload, profile).unwrap(),
                &RenderOptions {
                    size: 384,
                    style: Style::Lines,
                    ..RenderOptions::default()
                },
            )
            .unwrap();
            let image = image::load_from_memory(&png).unwrap().to_rgba8();
            for spoke in 0..spokes {
                let angle = -PI / 2. + 2. * PI * (spoke as f64 + 0.5) / spokes as f64;
                for radius in [22.8, 23.2, 23.6] {
                    let p = sample_rgba(
                        &image,
                        (64. + radius * angle.cos()) * 3. - 0.5,
                        (64. + radius * angle.sin()) * 3. - 0.5,
                    )
                    .unwrap();
                    let brightness = p.0[..3].iter().map(|v| u32::from(*v)).sum::<u32>() / 3;
                    assert!(
                        brightness > 200,
                        "{spokes} lines, gap {spoke}, radius {radius}: {brightness}"
                    );
                }
            }
        }
    }
}
