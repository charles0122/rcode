//! PNG and SVG renderers for the fixed radial symbol.

use std::fmt::Write as _;
use std::io::Cursor;

use base64::Engine as _;
use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

use crate::geometry::{self, HALF_CANVAS, Slot};
use crate::{Error, Result, Symbol};

/// Data rendering style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// Draw each set sample as a round dot.
    Dots,
    /// Draw dots and round-ended capsules between adjacent set samples.
    #[default]
    Lines,
}

/// Options shared by PNG and SVG output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderOptions {
    /// Output width and height in pixels for PNG, or the nominal SVG size.
    pub size: u32,
    /// Solid foreground RGB color.  It must retain at least 7:1 contrast
    /// against the white background.
    pub foreground: [u8; 3],
    /// Data stroke style.
    pub style: Style,
    /// Optional PNG/JPEG logo bytes, clipped to the central radius-20 circle.
    pub logo: Option<Vec<u8>>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            size: 768,
            foreground: [17, 24, 39],
            style: Style::Lines,
            logo: None,
        }
    }
}

impl RenderOptions {
    fn validate(&self) -> Result<()> {
        if !(384..=2048).contains(&self.size) {
            return Err(Error::InvalidOptions(
                "size must be between 384 and 2048 pixels".into(),
            ));
        }
        let contrast = contrast_against_white(self.foreground);
        if contrast < 7.0 {
            return Err(Error::InvalidOptions(format!(
                "foreground contrast is {contrast:.2}:1; minimum is 7:1"
            )));
        }
        if let Some(logo) = &self.logo
            && logo.len() > 8 * 1024 * 1024
        {
            return Err(Error::InvalidOptions(
                "logo is larger than the 8 MiB input limit".into(),
            ));
        }
        Ok(())
    }
}

/// Render one symbol as a PNG.
pub fn render_png(symbol: &Symbol, options: &RenderOptions) -> Result<Vec<u8>> {
    options.validate()?;
    validate_symbol(symbol)?;

    let high_size = options
        .size
        .checked_mul(2)
        .ok_or_else(|| Error::InvalidOptions("size overflow".into()))?;
    let mut image = RgbaImage::from_pixel(high_size, high_size, white());
    let scale = high_size as f64 / 128.0;
    draw_symbol(&mut image, symbol, options, scale)?;

    let image = imageops::resize(&image, options.size, options.size, FilterType::Lanczos3);
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|error| Error::InvalidOptions(format!("PNG encoding failed: {error}")))?;
    Ok(output.into_inner())
}

/// Render one symbol as a self-contained SVG.
pub fn render_svg(symbol: &Symbol, options: &RenderOptions) -> Result<String> {
    options.validate()?;
    validate_symbol(symbol)?;

    let fg = format!(
        "#{:02x}{:02x}{:02x}",
        options.foreground[0], options.foreground[1], options.foreground[2]
    );
    let slots = geometry::slots_for_spokes(symbol.profile.map_or(96, |p| p.spokes()));
    let bits = slot_bits(symbol, &slots)?;
    let radius = stroke_radius(symbol, options.style);
    let mut svg = String::with_capacity(48_000);
    writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 128 128" role="img" aria-label="RCode">"#,
        options.size, options.size
    )
    .unwrap();
    svg.push_str(r##"<rect width="128" height="128" fill="#fff"/>"##);
    // The clear disc keeps a future renderer from placing content below a logo.
    svg.push_str(r##"<circle cx="64" cy="64" r="22" fill="#fff"/>"##);

    for [start, end] in inner_leads(symbol, options.style, &slots, &bits, radius) {
        let (x1, y1) = svg_point(start.0, start.1);
        let (x2, y2) = svg_point(end.0, end.1);
        let width = radius * 2.0;
        writeln!(svg, r#"<line x1="{x1:.4}" y1="{y1:.4}" x2="{x2:.4}" y2="{y2:.4}" stroke="{fg}" stroke-width="{width}" stroke-linecap="round"/>"#).unwrap();
    }

    for (index, slot) in slots.iter().enumerate() {
        if bits[index] {
            let (x, y) = svg_point(slot.x, slot.y);
            writeln!(
                svg,
                r#"<circle cx="{x:.4}" cy="{y:.4}" r="{radius}" fill="{fg}"/>"#
            )
            .unwrap();
        }
    }
    if options.style == Style::Lines {
        for index in 1..slots.len() {
            let previous = slots[index - 1];
            let current = slots[index];
            if previous.spoke == current.spoke
                && previous.ring + 1 == current.ring
                && bits[index - 1]
                && bits[index]
            {
                let (x1, y1) = svg_point(previous.x, previous.y);
                let (x2, y2) = svg_point(current.x, current.y);
                let width = radius * 2.0;
                writeln!(
                    svg,
                    r#"<line x1="{x1:.4}" y1="{y1:.4}" x2="{x2:.4}" y2="{y2:.4}" stroke="{fg}" stroke-width="{width}" stroke-linecap="round"/>"#
                )
                .unwrap();
            }
        }
    }

    append_svg_locators(&mut svg, &fg, locator_radii(symbol));
    if let Some(bytes) = &options.logo {
        let logo = load_logo(bytes)?;
        let png = encode_logo_png(&logo)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        svg.push_str(
            r#"<defs><clipPath id="rcode-logo-clip"><circle cx="64" cy="64" r="20"/></clipPath></defs>"#,
        );
        writeln!(
            svg,
            r##"<image x="44" y="44" width="40" height="40" preserveAspectRatio="none" href="data:image/png;base64,{encoded}" clip-path="url(#rcode-logo-clip)"/>"##
        )
        .unwrap();
    }
    svg.push_str("</svg>");
    Ok(svg)
}

fn validate_symbol(symbol: &Symbol) -> Result<()> {
    let expected = symbol.profile.map_or(202, |p| p.codeword_len());
    if symbol.codeword.len() != expected {
        return Err(Error::InvalidOptions(format!(
            "symbol codeword has {} bytes; expected {expected}",
            symbol.codeword.len()
        )));
    }
    Ok(())
}

fn contrast_against_white(color: [u8; 3]) -> f64 {
    let linear = |component: u8| {
        let value = component as f64 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance =
        0.2126 * linear(color[0]) + 0.7152 * linear(color[1]) + 0.0722 * linear(color[2]);
    1.05 / (luminance + 0.05)
}

fn white() -> Rgba<u8> {
    Rgba([255, 255, 255, 255])
}

fn foreground(options: &RenderOptions) -> Rgba<u8> {
    Rgba([
        options.foreground[0],
        options.foreground[1],
        options.foreground[2],
        255,
    ])
}

fn point(x: f64, y: f64, scale: f64) -> (f64, f64) {
    ((x + HALF_CANVAS) * scale, (y + HALF_CANVAS) * scale)
}

fn svg_point(x: f64, y: f64) -> (f64, f64) {
    (x + HALF_CANVAS, y + HALF_CANVAS)
}

fn slot_bits(symbol: &Symbol, slots: &[Slot]) -> Result<Vec<bool>> {
    if slots.len() < symbol.codeword.len() * 8 {
        return Err(Error::InvalidOptions("invalid geometry slot count".into()));
    }
    Ok(slots
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let raw = if index < symbol.codeword.len() * 8 {
                (symbol.codeword[index / 8] >> (7 - index % 8)) & 1 != 0
            } else {
                false
            };
            raw ^ geometry::mask(index)
        })
        .collect())
}

fn draw_symbol(
    image: &mut RgbaImage,
    symbol: &Symbol,
    options: &RenderOptions,
    scale: f64,
) -> Result<()> {
    let fg = foreground(options);
    let slots = geometry::slots_for_spokes(symbol.profile.map_or(96, |p| p.spokes()));
    let bits = slot_bits(symbol, &slots)?;
    let radius = stroke_radius(symbol, options.style);

    draw_circle(image, point(0.0, 0.0, scale), 22.0 * scale, white());
    for [start, end] in inner_leads(symbol, options.style, &slots, &bits, radius) {
        draw_capsule(
            image,
            point(start.0, start.1, scale),
            point(end.0, end.1, scale),
            radius * scale,
            fg,
        );
    }
    for (index, slot) in slots.iter().enumerate() {
        if bits[index] {
            draw_circle(image, point(slot.x, slot.y, scale), radius * scale, fg);
        }
    }
    if options.style == Style::Lines {
        for index in 1..slots.len() {
            let previous = slots[index - 1];
            let current = slots[index];
            if previous.spoke == current.spoke
                && previous.ring + 1 == current.ring
                && bits[index - 1]
                && bits[index]
            {
                draw_capsule(
                    image,
                    point(previous.x, previous.y, scale),
                    point(current.x, current.y, scale),
                    radius * scale,
                    fg,
                );
            }
        }
    }

    draw_locators(image, scale, fg, locator_radii(symbol));
    if let Some(bytes) = &options.logo {
        draw_logo(image, bytes, scale)?;
    }
    Ok(())
}

// A fixed non-data contour gives every spoke the same visible inner edge.
// It ends before the first data sample at radius 25 unless that sample is black.
fn inner_leads(
    symbol: &Symbol,
    style: Style,
    slots: &[Slot],
    bits: &[bool],
    radius: f64,
) -> Vec<[(f64, f64); 2]> {
    if symbol.profile.is_none() {
        return Vec::new();
    }
    slots
        .iter()
        .zip(bits)
        .filter(|(slot, _)| slot.ring == 0)
        .map(|(slot, on)| {
            let start = 22.0 + radius;
            let end = if style == Style::Lines {
                if *on { 25.0 } else { 23.2 }
            } else {
                start
            };
            [
                (slot.x * start / 25.0, slot.y * start / 25.0),
                (slot.x * end / 25.0, slot.y * end / 25.0),
            ]
        })
        .collect()
}

fn stroke_radius(symbol: &Symbol, style: Style) -> f64 {
    let Some(profile) = symbol.profile else {
        return 0.5;
    };
    // Samples are 1.5 units apart; leave 0.4 units between round dots.
    if style == Style::Dots {
        return 0.55;
    }
    match profile.spokes() {
        36 => 1.0,
        54 => 0.9,
        _ => 0.7,
    }
}

fn locator_radii(symbol: &Symbol) -> [f64; 3] {
    if symbol.profile.is_some() {
        geometry::LOCATOR_RADII
    } else {
        geometry::LEGACY_LOCATOR_RADII
    }
}

fn draw_locators(image: &mut RgbaImage, scale: f64, fg: Rgba<u8>, radii: [f64; 3]) {
    for (index, (x, y)) in geometry::LOCATORS.iter().enumerate() {
        let [outer, opening, core] = if index == 3 {
            geometry::LEGACY_LOCATOR_RADII
        } else {
            radii
        };
        let center = point(*x, *y, scale);
        draw_circle(image, center, outer * scale, fg);
        draw_circle(image, center, opening * scale, white());
        if index < 3 {
            draw_circle(image, center, core * scale, fg);
        }
    }
}

fn append_svg_locators(svg: &mut String, fg: &str, radii: [f64; 3]) {
    for (index, (x, y)) in geometry::LOCATORS.iter().enumerate() {
        let [outer, opening, core] = if index == 3 {
            geometry::LEGACY_LOCATOR_RADII
        } else {
            radii
        };
        let (cx, cy) = svg_point(*x, *y);
        writeln!(
            svg,
            r#"<circle cx="{cx:.4}" cy="{cy:.4}" r="{outer}" fill="{fg}"/>"#
        )
        .unwrap();
        writeln!(
            svg,
            r##"<circle cx="{cx:.4}" cy="{cy:.4}" r="{opening}" fill="#fff"/>"##
        )
        .unwrap();
        if index < 3 {
            writeln!(
                svg,
                r#"<circle cx="{cx:.4}" cy="{cy:.4}" r="{core}" fill="{fg}"/>"#
            )
            .unwrap();
        }
    }
}

fn draw_circle(image: &mut RgbaImage, center: (f64, f64), radius: f64, color: Rgba<u8>) {
    let min_x = (center.0 - radius).floor().max(0.0) as u32;
    let max_x = (center.0 + radius).ceil().min(image.width() as f64 - 1.0) as u32;
    let min_y = (center.1 - radius).floor().max(0.0) as u32;
    let max_y = (center.1 + radius).ceil().min(image.height() as f64 - 1.0) as u32;
    let radius_squared = radius * radius;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f64 + 0.5 - center.0;
            let dy = y as f64 + 0.5 - center.1;
            if dx * dx + dy * dy <= radius_squared {
                blend_pixel(image, x, y, color);
            }
        }
    }
}

fn draw_capsule(
    image: &mut RgbaImage,
    start: (f64, f64),
    end: (f64, f64),
    radius: f64,
    color: Rgba<u8>,
) {
    let min_x = (start.0.min(end.0) - radius).floor().max(0.0) as u32;
    let max_x = (start.0.max(end.0) + radius)
        .ceil()
        .min(image.width() as f64 - 1.0) as u32;
    let min_y = (start.1.min(end.1) - radius).floor().max(0.0) as u32;
    let max_y = (start.1.max(end.1) + radius)
        .ceil()
        .min(image.height() as f64 - 1.0) as u32;
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length_squared = dx * dx + dy * dy;
    let radius_squared = radius * radius;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f64 + 0.5;
            let py = y as f64 + 0.5;
            let t = if length_squared == 0.0 {
                0.0
            } else {
                ((px - start.0) * dx + (py - start.1) * dy) / length_squared
            }
            .clamp(0.0, 1.0);
            let nearest_x = start.0 + t * dx;
            let nearest_y = start.1 + t * dy;
            let distance_x = px - nearest_x;
            let distance_y = py - nearest_y;
            if distance_x * distance_x + distance_y * distance_y <= radius_squared {
                blend_pixel(image, x, y, color);
            }
        }
    }
}

fn blend_pixel(image: &mut RgbaImage, x: u32, y: u32, source: Rgba<u8>) {
    if source[3] == 255 {
        image.put_pixel(x, y, source);
        return;
    }
    let destination = *image.get_pixel(x, y);
    let source_alpha = source[3] as f32 / 255.0;
    let destination_alpha = destination[3] as f32 / 255.0;
    let alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if alpha == 0.0 {
        return;
    }
    let mut result = [0u8; 4];
    for channel in 0..3 {
        let value = (source[channel] as f32 * source_alpha
            + destination[channel] as f32 * destination_alpha * (1.0 - source_alpha))
            / alpha;
        result[channel] = value.round().clamp(0.0, 255.0) as u8;
    }
    result[3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
    image.put_pixel(x, y, Rgba(result));
}

fn load_logo(bytes: &[u8]) -> Result<RgbaImage> {
    crate::decode::read_image(bytes)
        .map_err(|error| Error::InvalidOptions(format!("logo is not a PNG or JPEG: {error}")))
        .map(|image| {
            let image = image.to_rgba8();
            let side = image.width().min(image.height());
            let left = (image.width() - side) / 2;
            let top = (image.height() - side) / 2;
            imageops::crop_imm(&image, left, top, side, side).to_image()
        })
}

fn encode_logo_png(logo: &RgbaImage) -> Result<Vec<u8>> {
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(logo.clone())
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|error| Error::InvalidOptions(format!("logo normalization failed: {error}")))?;
    Ok(output.into_inner())
}

fn draw_logo(image: &mut RgbaImage, bytes: &[u8], scale: f64) -> Result<()> {
    let logo = load_logo(bytes)?;
    let side = (40.0 * scale).round() as u32;
    let resized = imageops::resize(&logo, side, side, FilterType::Lanczos3);
    let left = (64.0 - 20.0) * scale;
    let top = (64.0 - 20.0) * scale;
    let radius = 20.0 * scale;
    for y in 0..side {
        for x in 0..side {
            let dx = x as f64 + 0.5 - side as f64 / 2.0;
            let dy = y as f64 + 0.5 - side as f64 / 2.0;
            if dx * dx + dy * dy <= radius * radius {
                blend_pixel(
                    image,
                    left as u32 + x,
                    top as u32 + y,
                    *resized.get_pixel(x, y),
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode;

    #[test]
    fn png_dimensions_and_svg_styles() {
        let symbol = encode(b"render").unwrap();
        let options = RenderOptions {
            size: 384,
            ..RenderOptions::default()
        };
        let png = render_png(&symbol, &options).unwrap();
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!(decoded.width(), 384);
        assert_eq!(decoded.height(), 384);

        let lines = render_svg(&symbol, &options).unwrap();
        assert!(lines.contains("stroke-linecap=\"round\""));
        let dots = render_svg(
            &symbol,
            &RenderOptions {
                style: Style::Dots,
                ..options
            },
        )
        .unwrap();
        assert!(!dots.contains("stroke-linecap=\"round\""));
    }

    #[test]
    fn options_and_logo_are_bounded() {
        let symbol = encode(b"logo").unwrap();
        assert!(matches!(
            render_png(
                &symbol,
                &RenderOptions {
                    size: 100,
                    ..RenderOptions::default()
                }
            ),
            Err(Error::InvalidOptions(_))
        ));
        assert!(matches!(
            render_svg(
                &symbol,
                &RenderOptions {
                    foreground: [255, 255, 255],
                    ..RenderOptions::default()
                }
            ),
            Err(Error::InvalidOptions(_))
        ));

        let mut logo = RgbaImage::from_pixel(3, 2, Rgba([255, 0, 0, 255]));
        logo.put_pixel(0, 0, Rgba([0, 255, 0, 255]));
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(logo)
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        let options = RenderOptions {
            logo: Some(bytes.into_inner()),
            ..RenderOptions::default()
        };
        let svg = render_svg(&symbol, &options).unwrap();
        assert!(svg.contains("data:image/png;base64,"));
        assert!(svg.contains("rcode-logo-clip"));
    }
}
