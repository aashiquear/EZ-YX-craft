//! Chip-load based feeds & speeds calculator.
//!
//! `feed = rpm × flutes × chipload`, with chip load proportional to tool diameter and scaled by a
//! per-material factor. The factors are deliberately conservative "hobby machine" starting points
//! (rigidity of a 3018/4040-class router is far below an industrial spindle). Users should always
//! verify with a test cut; the UI shows the `source` string next to every number.

use crate::{CuttingData, MaterialGroup, Operation, Tool, ToolShape};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MachineLimits {
    pub name: String,
    pub min_rpm: f64,
    pub max_rpm: f64,
    /// Max XY feed (mm/min).
    pub max_feed: f64,
    /// Max Z feed (mm/min).
    pub max_z_feed: f64,
    /// Spindle power (W) – used to cap material removal rate.
    pub spindle_watts: f64,
}

impl MachineLimits {
    /// The machine the bundled Genmitsu table was tuned for.
    pub fn genmitsu_3018() -> Self {
        Self { name: "Genmitsu 3018 (775 motor)".into(), min_rpm: 1000.0, max_rpm: 9000.0, max_feed: 1200.0, max_z_feed: 600.0, spindle_watts: 65.0 }
    }
}

impl Default for MachineLimits {
    fn default() -> Self {
        Self { name: "Generic hobby router".into(), min_rpm: 5000.0, max_rpm: 18000.0, max_feed: 3000.0, max_z_feed: 1000.0, spindle_watts: 300.0 }
    }
}

/// Chip load per tooth per mm of diameter (mm/tooth/mm), roughing.
fn chip_factor(m: MaterialGroup) -> f64 {
    use MaterialGroup::*;
    match m {
        Foam => 0.020,
        Softwood => 0.012,
        Mdf | Plywood => 0.011,
        Hardwood => 0.009,
        Hdpe => 0.012,
        Acrylic => 0.008,
        Aluminum => 0.004,
        Brass => 0.0035,
        Copper => 0.003,
        Steel => 0.0015,
    }
}

/// Depth-of-cut factor (× diameter) for roughing.
fn doc_factor(m: MaterialGroup) -> f64 {
    use MaterialGroup::*;
    match m {
        Foam => 1.0,
        Softwood | Mdf | Plywood | Hdpe => 0.5,
        Hardwood | Acrylic => 0.35,
        Aluminum => 0.15,
        Brass | Copper => 0.1,
        Steel => 0.05,
    }
}

/// Preferred spindle speed fraction (metals like lower surface speed on small routers).
fn rpm_fraction(m: MaterialGroup) -> f64 {
    use MaterialGroup::*;
    match m {
        Aluminum => 0.9,
        Brass | Copper => 0.7,
        Steel => 0.5,
        Acrylic => 0.8,
        _ => 1.0,
    }
}

pub fn calculate(tool: &Tool, material: MaterialGroup, op: Operation, m: &MachineLimits) -> CuttingData {
    let d = match tool.shape {
        // V-bits cut with a small effective diameter in normal use.
        ToolShape::VBit { .. } | ToolShape::TaperedBall { .. } => (tool.diameter * 0.35).max(0.5),
        _ => tool.diameter,
    };
    let rpm = (m.max_rpm * rpm_fraction(material)).clamp(m.min_rpm, m.max_rpm);
    let mut chip = chip_factor(material) * d;
    if op == Operation::Finishing {
        chip *= 0.75;
    }
    let flutes = tool.flutes.max(1) as f64;
    let mut feed = rpm * flutes * chip;
    // Cap by spindle power: very rough MRR limit (mm³/min per W).
    let k_mrr = if material.is_metal() { 15.0 } else { 120.0 };
    let mut stepdown = d * doc_factor(material);
    let stepover = match (op, &tool.shape) {
        (_, ToolShape::BallNose | ToolShape::TaperedBall { .. }) => d * if op == Operation::Finishing { 0.1 } else { 0.4 },
        (_, ToolShape::VBit { .. }) => d * 0.1,
        (Operation::Roughing, _) => d * 0.4,
        (Operation::Finishing, _) => d * 0.15,
    };
    if op == Operation::Finishing {
        stepdown *= 0.5;
    }
    let mrr = feed * stepdown * stepover.max(d * 0.4);
    let mrr_cap = m.spindle_watts * k_mrr;
    if mrr > mrr_cap {
        feed *= mrr_cap / mrr;
    }
    feed = feed.clamp(30.0, m.max_feed);
    CuttingData {
        material,
        operation: op,
        rpm,
        feed,
        plunge_feed: (feed * 0.3).min(m.max_z_feed),
        ramp_feed: feed * 0.6,
        stepdown: stepdown.max(0.05),
        stepover: stepover.max(0.01),
        source: format!("Chip-load estimate ({:.4} mm/tooth) — verify with a test cut", chip),
    }
}

/// Estimated surface speed (m/min) – shown in the UI as a sanity value.
pub fn surface_speed(diameter_mm: f64, rpm: f64) -> f64 {
    std::f64::consts::PI * diameter_mm * rpm / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolLibrary;

    #[test]
    fn metals_slower_than_wood() {
        let lib = ToolLibrary::generic_templates();
        let t = lib.get("generic.flat_1_8_2f").unwrap();
        let m = MachineLimits::default();
        let wood = calculate(t, MaterialGroup::Hardwood, Operation::Roughing, &m);
        let alu = calculate(t, MaterialGroup::Aluminum, Operation::Roughing, &m);
        assert!(alu.feed < wood.feed);
        assert!(alu.stepdown < wood.stepdown);
        assert!(wood.feed <= m.max_feed);
    }
}
