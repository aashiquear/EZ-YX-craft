//! Job / stock setup — the first thing a user defines (mirrors the "Job Setup" step every carving
//! package has, but with sensible defaults so beginners can just click "Next").

use crate::{Bounds2, DVec2, Units};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobKind {
    /// Single-sided flat stock.
    Flat,
    /// Two-sided machining; the part is flipped about `flip_axis` between sides.
    DoubleSided { flip_axis: FlipAxis },
    /// Rotary (4th axis). The cylinder axis lies along X (most hobby rotaries) or Y.
    Rotary { diameter: f64, length: f64, axis: RotaryAxis },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlipAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotaryAxis {
    /// Cylinder axis parallel to machine X; A rotates around X.
    AlongX,
    /// Cylinder axis parallel to machine Y; B rotates around Y (rare on hobby machines).
    AlongY,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ZZero {
    #[default]
    MaterialSurface,
    MachineBed,
}

/// Where machine XY zero sits relative to the stock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum XyDatum {
    #[default]
    BottomLeft,
    BottomCenter,
    BottomRight,
    CenterLeft,
    Center,
    CenterRight,
    TopLeft,
    TopCenter,
    TopRight,
}

impl XyDatum {
    pub const ALL: [XyDatum; 9] = [
        XyDatum::TopLeft,
        XyDatum::TopCenter,
        XyDatum::TopRight,
        XyDatum::CenterLeft,
        XyDatum::Center,
        XyDatum::CenterRight,
        XyDatum::BottomLeft,
        XyDatum::BottomCenter,
        XyDatum::BottomRight,
    ];
    /// Fractional position (0..1, 0..1) on the stock.
    pub fn fraction(self) -> DVec2 {
        use XyDatum::*;
        let (fx, fy) = match self {
            BottomLeft => (0.0, 0.0),
            BottomCenter => (0.5, 0.0),
            BottomRight => (1.0, 0.0),
            CenterLeft => (0.0, 0.5),
            Center => (0.5, 0.5),
            CenterRight => (1.0, 0.5),
            TopLeft => (0.0, 1.0),
            TopCenter => (0.5, 1.0),
            TopRight => (1.0, 1.0),
        };
        DVec2::new(fx, fy)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobSetup {
    pub kind: JobKind,
    /// Stock size X (mm). For rotary jobs this is the unwrapped length.
    pub width: f64,
    /// Stock size Y (mm). For rotary jobs this is the circumference (π·D).
    pub height: f64,
    /// Stock thickness Z (mm). For rotary jobs this is the radius.
    pub thickness: f64,
    pub z_zero: ZZero,
    pub xy_datum: XyDatum,
    /// Units shown in the UI and used by the post-processor.
    pub display_units: Units,
    /// Material id from `ezyx-materials` (e.g. "hardwood.walnut").
    pub material: String,
    /// Safe Z above the stock for rapids (mm).
    pub safe_z: f64,
    /// Home / park position after the job (mm, machine coords relative to datum).
    pub home: [f64; 3],
}

impl Default for JobSetup {
    fn default() -> Self {
        Self {
            kind: JobKind::Flat,
            width: 300.0,
            height: 200.0,
            thickness: 18.0,
            z_zero: ZZero::MaterialSurface,
            xy_datum: XyDatum::BottomLeft,
            display_units: Units::Millimeters,
            material: "hardwood.maple".into(),
            safe_z: 5.0,
            home: [0.0, 0.0, 20.0],
        }
    }
}

impl JobSetup {
    pub fn rotary(diameter: f64, length: f64) -> Self {
        Self {
            kind: JobKind::Rotary { diameter, length, axis: RotaryAxis::AlongX },
            width: length,
            height: std::f64::consts::PI * diameter,
            thickness: diameter * 0.5,
            ..Default::default()
        }
    }

    /// Design-space bounds of the stock (design origin is the stock's lower-left corner).
    pub fn stock_bounds(&self) -> Bounds2 {
        Bounds2 { min: DVec2::ZERO, max: DVec2::new(self.width, self.height) }
    }

    /// Offset added to design coordinates to obtain machine (work) coordinates.
    pub fn datum_offset(&self) -> DVec2 {
        -self.xy_datum.fraction() * DVec2::new(self.width, self.height)
    }

    /// Z of the stock top in work coordinates.
    pub fn top_z(&self) -> f64 {
        match self.z_zero {
            ZZero::MaterialSurface => 0.0,
            ZZero::MachineBed => self.thickness,
        }
    }

    pub fn is_rotary(&self) -> bool {
        matches!(self.kind, JobKind::Rotary { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn datum_center() {
        let j = JobSetup { xy_datum: XyDatum::Center, width: 100.0, height: 50.0, ..Default::default() };
        assert_eq!(j.datum_offset(), DVec2::new(-50.0, -25.0));
    }
    #[test]
    fn rotary_circumference() {
        let j = JobSetup::rotary(50.0, 200.0);
        assert!((j.height - 157.0796).abs() < 1e-3);
        assert!(j.is_rotary());
    }
}
