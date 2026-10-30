use rcode::{
    DrawOperation, EccLevel, Paint, Profile, Style, build_render_plan, decode_luma8, encode,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct Plan {
    version: u32,
    viewport: f64,
    logo_region: [f64; 3],
    operations: Vec<Operation>,
}

#[derive(Serialize)]
struct Operation {
    kind: &'static str,
    paint: &'static str,
    values: [f64; 6],
}

#[derive(Serialize)]
struct Decoded {
    payload: Vec<u8>,
    corrected_bytes: usize,
    landmarks: Vec<[f64; 2]>,
    width: u32,
    height: u32,
    spokes: usize,
    ecc: u32,
}

/// Browser profile choices use the same capacity calculations as native clients.
#[wasm_bindgen]
pub fn profiles_json() -> String {
    let profiles: Vec<_> = [36, 54, 72]
        .into_iter()
        .flat_map(|spokes| {
            EccLevel::ALL.into_iter().map(move |level| {
                let profile = Profile::new(spokes, level).unwrap();
                serde_json::json!({
                    "spokes": spokes,
                    "ecc": level.to_string(),
                    "capacity": profile.capacity(),
                    "correction_percent": level.percent(),
                    "correctable_bytes": profile.correctable_bytes(),
                    "parity_bytes": profile.parity_bytes(),
                })
            })
        })
        .collect();
    serde_json::to_string(&profiles).unwrap()
}

/// Encode payload bytes into JSON drawing commands. No image format is produced.
#[wasm_bindgen]
pub fn encode_plan_json(
    payload: &[u8],
    spokes: u32,
    ecc_id: u32,
    style_id: u32,
) -> Result<String, JsValue> {
    let profile = Profile::new(spokes as usize, ecc(ecc_id)?)
        .map_err(|error| js_error(&error.to_string()))?;
    let symbol = encode(payload, profile).map_err(|error| js_error(&error.to_string()))?;
    let plan = build_render_plan(&symbol, style(style_id)?)
        .map_err(|error| js_error(&error.to_string()))?;
    let response = Plan {
        version: plan.version,
        viewport: plan.viewport,
        logo_region: [plan.logo_region.0, plan.logo_region.1, plan.logo_region.2],
        operations: plan.operations.into_iter().map(Operation::from).collect(),
    };
    serde_json::to_string(&response).map_err(|_| js_error("unable to serialize render plan"))
}

/// Decode an 8-bit grayscale browser frame and return JSON metadata.
#[wasm_bindgen]
pub fn decode_luma8_json(
    pixels: &[u8],
    width: u32,
    height: u32,
    row_stride: usize,
) -> Result<String, JsValue> {
    let decoded = decode_luma8(pixels, width, height, row_stride)
        .map_err(|error| js_error(&error.to_string()))?;
    decoded_json(decoded)
}

/// Decode Canvas ImageData without a JavaScript per-pixel conversion loop.
#[wasm_bindgen]
pub fn decode_rgba8_json(
    pixels: &[u8],
    width: u32,
    height: u32,
    row_stride: usize,
) -> Result<String, JsValue> {
    let width = usize::try_from(width).map_err(|_| js_error("invalid width"))?;
    let height = usize::try_from(height).map_err(|_| js_error("invalid height"))?;
    let row_bytes = width
        .checked_mul(4)
        .ok_or_else(|| js_error("image dimensions overflow"))?;
    if width < 2 || height < 2 || row_stride < row_bytes {
        return Err(js_error("invalid RGBA frame layout"));
    }
    let required = row_stride
        .checked_mul(height - 1)
        .and_then(|value| value.checked_add(row_bytes))
        .ok_or_else(|| js_error("image dimensions overflow"))?;
    if pixels.len() < required {
        return Err(js_error("RGBA frame is truncated"));
    }
    let pixel_count = width
        .checked_mul(height)
        .ok_or_else(|| js_error("image dimensions overflow"))?;
    if pixel_count > 16_000_000 {
        return Err(js_error("image exceeds 16 megapixels"));
    }
    let mut luma = Vec::with_capacity(pixel_count);
    for row in 0..height {
        let source = &pixels[row * row_stride..row * row_stride + row_bytes];
        for rgba in source.chunks_exact(4) {
            luma.push(
                ((299 * u32::from(rgba[0]) + 587 * u32::from(rgba[1]) + 114 * u32::from(rgba[2]))
                    / 1000) as u8,
            );
        }
    }
    decode_luma8_json(&luma, width as u32, height as u32, width)
}

fn decoded_json(decoded: rcode::DecodeResult) -> Result<String, JsValue> {
    let ecc = match decoded.profile.level {
        EccLevel::L => 0,
        EccLevel::M => 1,
        EccLevel::Q => 2,
        EccLevel::H => 3,
    };
    let result = Decoded {
        payload: decoded.payload,
        corrected_bytes: decoded.corrected_bytes,
        landmarks: decoded
            .landmarks
            .into_iter()
            .map(|point| [point.x, point.y])
            .collect(),
        width: decoded.width,
        height: decoded.height,
        spokes: decoded.profile.spokes(),
        ecc,
    };
    serde_json::to_string(&result).map_err(|_| js_error("unable to serialize decode result"))
}

fn ecc(id: u32) -> Result<EccLevel, JsValue> {
    [EccLevel::L, EccLevel::M, EccLevel::Q, EccLevel::H]
        .get(id as usize)
        .copied()
        .ok_or_else(|| js_error("unknown ECC id"))
}

fn style(id: u32) -> Result<Style, JsValue> {
    [Style::Lines, Style::Dots, Style::Rings]
        .get(id as usize)
        .copied()
        .ok_or_else(|| js_error("unknown style id"))
}

fn js_error(message: &str) -> JsValue {
    JsValue::from_str(message)
}

impl From<DrawOperation> for Operation {
    fn from(operation: DrawOperation) -> Self {
        let paint = match operation {
            DrawOperation::Circle { paint, .. }
            | DrawOperation::Line { paint, .. }
            | DrawOperation::Arc { paint, .. } => match paint {
                Paint::Background => "background",
                Paint::Foreground => "foreground",
            },
        };
        match operation {
            DrawOperation::Circle { cx, cy, radius, .. } => Self {
                kind: "circle",
                paint,
                values: [cx, cy, radius, 0.0, 0.0, 0.0],
            },
            DrawOperation::Line {
                x1,
                y1,
                x2,
                y2,
                width,
                ..
            } => Self {
                kind: "line",
                paint,
                values: [x1, y1, x2, y2, width, 0.0],
            },
            DrawOperation::Arc {
                cx,
                cy,
                radius,
                start_angle,
                sweep_angle,
                width,
                ..
            } => Self {
                kind: "arc",
                paint,
                values: [cx, cy, radius, start_angle, sweep_angle, width],
            },
        }
    }
}
