//! Platform-neutral drawing plan for one encoded RCode symbol.

use crate::geometry::{self, HALF_CANVAS, Slot};
use crate::{Error, Result, Symbol};

/// Logical width and height used by every drawing plan.
pub const VIEWPORT: f64 = HALF_CANVAS * 2.0;
pub const RENDER_PLAN_VERSION: u32 = 1;

/// Data rendering style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// Draw each set sample as a round dot.
    Dots,
    /// Draw dots and round-ended capsules between adjacent set samples.
    #[default]
    Lines,
    /// Draw set samples as broken arcs on incrementally larger concentric rings.
    Rings,
}

/// Semantic paint selected by the business-side renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    Background,
    Foreground,
}

/// One ordered, platform-neutral drawing operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DrawOperation {
    Circle {
        cx: f64,
        cy: f64,
        radius: f64,
        paint: Paint,
    },
    /// A round-ended line. A zero-length line is a round dot.
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        width: f64,
        paint: Paint,
    },
    /// A clockwise arc in a screen coordinate system whose positive y-axis points down.
    Arc {
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
        width: f64,
        paint: Paint,
    },
}

/// Immutable geometry that any native or web canvas can draw.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderPlan {
    pub version: u32,
    pub viewport: f64,
    pub operations: Vec<DrawOperation>,
    /// Circular region reserved for an optional business-provided logo.
    pub logo_region: (f64, f64, f64),
}

/// Build the authoritative drawing geometry without producing an image format.
pub fn build_render_plan(symbol: &Symbol, style: Style) -> Result<RenderPlan> {
    validate_symbol(symbol)?;
    let slots = geometry::slots_for_spokes(symbol.profile.spokes());
    let bits = slot_bits(symbol, &slots)?;
    let stroke_radius = stroke_radius(symbol, style);
    let width = stroke_radius * 2.0;
    let mut operations = Vec::with_capacity(slots.len() * 2);

    operations.push(DrawOperation::Circle {
        cx: HALF_CANVAS,
        cy: HALF_CANVAS,
        radius: 22.0,
        paint: Paint::Background,
    });

    if style == Style::Rings {
        operations.push(DrawOperation::Arc {
            cx: HALF_CANVAS,
            cy: HALF_CANVAS,
            radius: 22.0 + stroke_radius,
            start_angle: 0.0,
            sweep_angle: std::f64::consts::TAU,
            width,
            paint: Paint::Foreground,
        });
        operations.extend(
            ring_arcs(&slots, &bits, symbol.profile.spokes()).map(|arc| DrawOperation::Arc {
                cx: HALF_CANVAS,
                cy: HALF_CANVAS,
                radius: arc.radius,
                start_angle: arc.start,
                sweep_angle: arc.end - arc.start,
                width,
                paint: Paint::Foreground,
            }),
        );
    } else {
        operations.extend(
            inner_leads(style, &slots, &bits, stroke_radius)
                .into_iter()
                .map(|[start, end]| line(start, end, width)),
        );
        operations.extend(slots.iter().zip(&bits).filter_map(|(slot, bit)| {
            bit.then_some(DrawOperation::Circle {
                cx: slot.x + HALF_CANVAS,
                cy: slot.y + HALF_CANVAS,
                radius: stroke_radius,
                paint: Paint::Foreground,
            })
        }));
        if style == Style::Lines {
            for index in 1..slots.len() {
                let previous = slots[index - 1];
                let current = slots[index];
                if previous.spoke == current.spoke
                    && previous.ring + 1 == current.ring
                    && bits[index - 1]
                    && bits[index]
                {
                    operations.push(line(
                        (previous.x, previous.y),
                        (current.x, current.y),
                        width,
                    ));
                }
            }
        }
    }

    for (x, y) in geometry::LOCATORS {
        let [outer, opening, core] = geometry::LOCATOR_RADII;
        let (cx, cy) = (x + HALF_CANVAS, y + HALF_CANVAS);
        operations.push(circle(cx, cy, outer, Paint::Foreground));
        operations.push(circle(cx, cy, opening, Paint::Background));
        operations.push(circle(cx, cy, core, Paint::Foreground));
    }

    Ok(RenderPlan {
        version: RENDER_PLAN_VERSION,
        viewport: VIEWPORT,
        operations,
        logo_region: (HALF_CANVAS, HALF_CANVAS, 20.0),
    })
}

fn circle(cx: f64, cy: f64, radius: f64, paint: Paint) -> DrawOperation {
    DrawOperation::Circle {
        cx,
        cy,
        radius,
        paint,
    }
}

fn line(start: (f64, f64), end: (f64, f64), width: f64) -> DrawOperation {
    DrawOperation::Line {
        x1: start.0 + HALF_CANVAS,
        y1: start.1 + HALF_CANVAS,
        x2: end.0 + HALF_CANVAS,
        y2: end.1 + HALF_CANVAS,
        width,
        paint: Paint::Foreground,
    }
}

fn validate_symbol(symbol: &Symbol) -> Result<()> {
    let expected = symbol.profile.codeword_len();
    if symbol.codeword.len() != expected {
        return Err(Error::InvalidOptions(format!(
            "symbol codeword has {} bytes; expected {expected}",
            symbol.codeword.len()
        )));
    }
    Ok(())
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

// A fixed non-data contour gives every spoke the same visible inner edge.
fn inner_leads(style: Style, slots: &[Slot], bits: &[bool], radius: f64) -> Vec<[(f64, f64); 2]> {
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
    if style == Style::Dots {
        return 0.55;
    }
    if style == Style::Rings {
        return 0.42;
    }
    match symbol.profile.spokes() {
        36 => 1.0,
        54 => 0.9,
        _ => 0.7,
    }
}

#[derive(Clone, Copy)]
struct RingArc {
    radius: f64,
    start: f64,
    end: f64,
}

fn ring_arcs<'a>(
    slots: &'a [Slot],
    bits: &'a [bool],
    spokes: usize,
) -> impl Iterator<Item = RingArc> + 'a {
    let mut states = vec![vec![None; spokes]; geometry::RINGS];
    for (index, slot) in slots.iter().enumerate() {
        states[slot.ring][slot.spoke] = Some(bits[index]);
    }

    let step = std::f64::consts::TAU / spokes as f64;
    let half = step * 0.29;
    let mut arcs = Vec::new();
    for slot in slots {
        if states[slot.ring][slot.spoke] != Some(true) {
            continue;
        }
        let center = -std::f64::consts::FRAC_PI_2 + step * slot.spoke as f64;
        let radius = 25.0 + 1.5 * slot.ring as f64;
        arcs.push(RingArc {
            radius,
            start: center - half,
            end: center + half,
        });

        let next = (slot.spoke + 1) % spokes;
        if states[slot.ring][next] == Some(true) {
            let next_center = if next == 0 {
                center + step
            } else {
                -std::f64::consts::FRAC_PI_2 + step * next as f64
            };
            arcs.push(RingArc {
                radius,
                start: center + half,
                end: next_center - half,
            });
        }
    }
    arcs.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Profile, encode};

    #[test]
    fn every_style_is_expressed_with_shared_primitives() {
        let symbol = encode(b"render plan", Profile::default()).unwrap();
        let lines = build_render_plan(&symbol, Style::Lines).unwrap();
        let dots = build_render_plan(&symbol, Style::Dots).unwrap();
        let rings = build_render_plan(&symbol, Style::Rings).unwrap();

        assert_eq!(lines.viewport, 128.0);
        assert_eq!(lines.version, RENDER_PLAN_VERSION);
        assert!(
            lines
                .operations
                .iter()
                .any(|op| matches!(op, DrawOperation::Line { .. }))
        );
        assert!(
            !dots
                .operations
                .iter()
                .any(|op| matches!(op, DrawOperation::Arc { .. }))
        );
        assert!(
            rings
                .operations
                .iter()
                .any(|op| matches!(op, DrawOperation::Arc { .. }))
        );
        assert_eq!(rings.logo_region, (64.0, 64.0, 20.0));
    }
}
