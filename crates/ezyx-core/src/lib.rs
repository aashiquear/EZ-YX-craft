//! # ezyx-core
//!
//! Foundation types shared by every EZ-YX-craft crate:
//!
//! * [`units`] – millimetre/inch handling. **All internal geometry is millimetres.**
//! * [`path`] – 2D polylines (open/closed), shape builders, simplification.
//! * [`region`] – boolean ops and offsetting of closed regions (backed by `geo`).
//! * [`heightmap`] – regular-grid height fields used by reliefs, 3D carving and simulation.
//! * [`job`] – stock / job setup (flat, two-sided, rotary), datum and Z-zero.
//!
//! Coordinate convention: X right, Y away from the operator (back of machine), Z up.
//! Z = 0 is the stock top unless [`job::ZZero::MachineBed`] is selected.

pub mod heightmap;
pub mod job;
pub mod path;
pub mod region;
pub mod units;

pub use glam::{DAffine2, DVec2, DVec3};
pub use heightmap::Heightmap;
pub use job::{JobKind, JobSetup, XyDatum, ZZero};
pub use path::Polyline;
pub use units::Units;

/// Common error type for geometry-level failures.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("invalid geometry: {0}")]
    InvalidGeometry(String),
    #[error("empty input")]
    Empty,
}

/// Axis aligned 2D bounding box.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Bounds2 {
    pub min: DVec2,
    pub max: DVec2,
}

impl Bounds2 {
    pub const EMPTY: Bounds2 = Bounds2 { min: DVec2::splat(f64::INFINITY), max: DVec2::splat(f64::NEG_INFINITY) };
    pub fn from_points<'a>(pts: impl IntoIterator<Item = &'a DVec2>) -> Self {
        let mut b = Self::EMPTY;
        for p in pts {
            b.include(*p);
        }
        b
    }
    pub fn include(&mut self, p: DVec2) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }
    pub fn union(&self, o: &Bounds2) -> Bounds2 {
        Bounds2 { min: self.min.min(o.min), max: self.max.max(o.max) }
    }
    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x || self.min.y > self.max.y
    }
    pub fn size(&self) -> DVec2 {
        if self.is_empty() {
            DVec2::ZERO
        } else {
            self.max - self.min
        }
    }
    pub fn center(&self) -> DVec2 {
        (self.min + self.max) * 0.5
    }
}
