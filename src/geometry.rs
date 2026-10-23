//! Fixed radial layout.
//!
//! Coordinates are logical units on a 128 by 128 canvas centred at `(0, 0)`.
//! Spoke zero starts at `-PI/2` and spokes advance counter-clockwise in the
//! mathematical coordinate system; renderers map positive `y` downwards as a
//! normal image canvas.

use std::f64::consts::PI;

/// Logical canvas edge in each direction.
pub const HALF_CANVAS: f64 = 64.0;
/// Number of samples on each spoke.
pub const RINGS: usize = 22;

/// Locator centres in stable logical order: TL, TR, BL, BR.
pub const LOCATORS: [(f64, f64); 4] = [(-34.0, -34.0), (34.0, -34.0), (-34.0, 34.0), (34.0, 34.0)];

/// Reference proportions: main locator diameter is about 9% of the code disc.
pub(crate) const LOCATOR_RADII: [f64; 3] = [5.2, 3.5, 1.1];

/// A data sample in the radial layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    /// Logical x coordinate.
    pub x: f64,
    /// Logical y coordinate.
    pub y: f64,
    /// Spoke index.
    pub spoke: usize,
    /// Ring index, from 0 to 21.
    pub ring: usize,
}

/// Return whether a point belongs to a locating-mark reservation.
pub(crate) fn reserved_point(x: f64, y: f64) -> bool {
    LOCATORS
        .iter()
        .any(|(lx, ly)| (x - lx).mul_add(x - lx, (y - ly) * (y - ly)) < 100.0)
}

/// Generate the deterministic list of data samples.
pub fn slots() -> Vec<Slot> {
    slots_for_spokes(72)
}

/// Shared radial geometry; callers select a validated protocol profile.
pub(crate) fn slots_for_spokes(spokes: usize) -> Vec<Slot> {
    let mut slots = Vec::with_capacity(spokes * RINGS);
    for spoke in 0..spokes {
        let angle = -PI / 2.0 + 2.0 * PI * spoke as f64 / spokes as f64;
        let (sin, cos) = angle.sin_cos();
        for ring in 0..RINGS {
            let radius = 25.0 + 1.5 * ring as f64;
            let x = radius * cos;
            let y = radius * sin;
            if !reserved_point(x, y) {
                slots.push(Slot { x, y, spoke, ring });
            }
        }
    }
    slots
}

/// Return the fixed mask bit for a slot index.
///
/// Starting from `(index as u32) + 0x9e3779b9` (wrapping), apply xorshift/multiply
/// steps `>>16, *0x7feb352d, >>15, *0x846ca68b, >>16`; the low bit is the mask.
/// It is deliberately stateless so every implementation can reproduce it.
pub fn mask(index: usize) -> bool {
    let mut x = (index as u32).wrapping_add(0x9e37_79b9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x & 1 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_slot_count_and_order() {
        let slots = slots();
        assert_eq!(slots.len(), 1_380);
        assert_eq!(slots[0].spoke, 0);
        assert_eq!(slots.last().unwrap().spoke, 71);
        assert!(slots.iter().all(|slot| !reserved_point(slot.x, slot.y)));
        assert!(slots.iter().all(|slot| slot.ring < RINGS));
    }

    #[test]
    fn mask_is_deterministic_and_not_constant() {
        let a: Vec<bool> = (0..32).map(mask).collect();
        let b: Vec<bool> = (0..32).map(mask).collect();
        assert_eq!(a, b);
        assert!(a.iter().any(|bit| *bit));
        assert!(a.iter().any(|bit| !*bit));
    }
}
