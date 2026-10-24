//! Units. Internally everything is millimetres; conversion happens only at the UI and post-processor edges.

use serde::{Deserialize, Serialize};

pub const MM_PER_INCH: f64 = 25.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Units {
    #[default]
    Millimeters,
    Inches,
}

impl Units {
    /// Convert a value expressed in `self` into millimetres.
    pub fn to_mm(self, v: f64) -> f64 {
        match self {
            Units::Millimeters => v,
            Units::Inches => v * MM_PER_INCH,
        }
    }
    /// Convert a millimetre value into `self`.
    pub fn from_mm(self, mm: f64) -> f64 {
        match self {
            Units::Millimeters => mm,
            Units::Inches => mm / MM_PER_INCH,
        }
    }
    pub fn suffix(self) -> &'static str {
        match self {
            Units::Millimeters => "mm",
            Units::Inches => "in",
        }
    }
    /// Feed-rate label.
    pub fn feed_suffix(self) -> &'static str {
        match self {
            Units::Millimeters => "mm/min",
            Units::Inches => "in/min",
        }
    }
    /// Format with a sensible number of decimals for the unit.
    pub fn fmt(self, mm: f64) -> String {
        match self {
            Units::Millimeters => format!("{:.2} mm", mm),
            Units::Inches => format!("{:.4} in", mm / MM_PER_INCH),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let v = 12.7;
        assert!((Units::Inches.to_mm(Units::Inches.from_mm(v)) - v).abs() < 1e-12);
        assert_eq!(Units::Inches.to_mm(1.0), 25.4);
    }
}
