//! # ezyx-tooldb
//!
//! Tool (router bit / end mill) definitions, a bundled library and a feeds & speeds calculator.
//!
//! ## Data sources
//! * **SainSmart Genmitsu bit table** (V6.0, Dec 2022, compiled by Graham Bland for the Genmitsu
//!   user community) — converted from the spreadsheets in `data/` by `tools/extract_bit_table.py`.
//!   The table's numbers were tuned for a 3018-class machine (9 000 RPM spindle, 1 200 mm/min max
//!   feed). [`ToolLibrary::scaled_for_machine`] rescales them for stronger machines such as the
//!   4040 PRO MAX, keeping chip load constant.
//! * **Generic geometry templates** for common bit shapes sold by other retailers. These carry no
//!   vendor-published feeds; their cutting data comes from [`feeds::calculate`] (chip-load model).
//!
//! Everything is stored in millimetres.

pub mod feeds;

use serde::{Deserialize, Serialize};

/// Cutter geometry. Angles are *included* angles in degrees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolShape {
    FlatEnd,
    BallNose,
    /// V-bit; `tip_diameter` > 0 means a flat-tipped engraving bit.
    VBit {
        angle: f64,
        tip_diameter: f64,
    },
    /// Tapered ball nose: ball of radius `tip_radius` on a cone of included `angle`.
    TaperedBall {
        angle: f64,
        tip_radius: f64,
    },
    /// Twist drill with point angle (118° typical).
    Drill {
        point_angle: f64,
    },
    /// Large flat cutter for spoilboard surfacing.
    Surfacing,
    /// Laser module (for diode/IR laser heads on hybrid machines, e.g. xTool / Genmitsu laser kits).
    Laser {
        spot: f64,
    },
}

impl ToolShape {
    pub fn label(&self) -> String {
        match self {
            ToolShape::FlatEnd => "End Mill".into(),
            ToolShape::BallNose => "Ball Nose".into(),
            ToolShape::VBit { angle, .. } => format!("V-Bit {angle:.0}°"),
            ToolShape::TaperedBall { angle, .. } => format!("Tapered Ball {angle:.0}°"),
            ToolShape::Drill { .. } => "Drill".into(),
            ToolShape::Surfacing => "Surfacing".into(),
            ToolShape::Laser { .. } => "Laser".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Operation {
    Roughing,
    Finishing,
}

/// Coarse material groups used by cutting-data tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MaterialGroup {
    Softwood,
    Hardwood,
    Mdf,
    Plywood,
    Acrylic,
    Hdpe,
    Aluminum,
    Brass,
    Copper,
    Steel,
    Foam,
}

impl MaterialGroup {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "softwood" => Self::Softwood,
            "hardwood" => Self::Hardwood,
            "mdf" => Self::Mdf,
            "plywood" => Self::Plywood,
            "acrylic" => Self::Acrylic,
            "hdpe" | "plastic" => Self::Hdpe,
            "aluminum" | "aluminium" => Self::Aluminum,
            "brass" => Self::Brass,
            "copper" => Self::Copper,
            "steel" => Self::Steel,
            "foam" => Self::Foam,
            _ => return None,
        })
    }
    pub fn is_metal(self) -> bool {
        matches!(self, Self::Aluminum | Self::Brass | Self::Copper | Self::Steel)
    }
}

/// One row of cutting data: what to run a tool at for a material group + operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CuttingData {
    pub material: MaterialGroup,
    pub operation: Operation,
    pub rpm: f64,
    /// Feed (mm/min).
    pub feed: f64,
    /// Plunge feed (mm/min).
    pub plunge_feed: f64,
    /// Ramp feed (mm/min).
    pub ramp_feed: f64,
    /// Depth per pass (mm).
    pub stepdown: f64,
    /// Stepover (mm).
    pub stepover: f64,
    /// Where these numbers came from (shown to the user).
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    /// Stable library id, e.g. "genmitsu.70C08.d1.0".
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub product_id: Option<String>,
    pub shape: ToolShape,
    /// Cutting diameter (mm). For V-bits: the maximum (shank-side) cutting diameter.
    pub diameter: f64,
    pub shank_diameter: f64,
    pub flutes: u32,
    pub flute_length: f64,
    pub coating: Option<String>,
    pub url: Option<String>,
    pub notes: Vec<String>,
    pub cutting: Vec<CuttingData>,
    /// Tool number used for ATC machines (Makera Carvera) / M6.
    #[serde(default = "one")]
    pub number: u32,
}

fn one() -> u32 {
    1
}

impl Tool {
    pub fn radius(&self) -> f64 {
        self.diameter * 0.5
    }

    /// Half included angle in radians for V-shaped tools.
    pub fn half_angle(&self) -> Option<f64> {
        match self.shape {
            ToolShape::VBit { angle, .. } | ToolShape::TaperedBall { angle, .. } => Some((angle * 0.5).to_radians()),
            ToolShape::Drill { point_angle } => Some((point_angle * 0.5).to_radians()),
            _ => None,
        }
    }

    /// Height of the cutter's profile above its lowest point at radial distance `r` (mm).
    /// `None` when `r` lies outside the cutter. This single function drives drop-cutter
    /// toolpaths, V-carving and the material-removal simulation.
    pub fn profile_height(&self, r: f64) -> Option<f64> {
        let rad = self.radius();
        if r > rad + 1e-9 {
            return None;
        }
        Some(match self.shape {
            ToolShape::FlatEnd | ToolShape::Surfacing | ToolShape::Laser { .. } => 0.0,
            ToolShape::BallNose => rad - (rad * rad - r * r).max(0.0).sqrt(),
            ToolShape::VBit { angle, tip_diameter } => {
                let t = tip_diameter * 0.5;
                if r <= t {
                    0.0
                } else {
                    (r - t) / (angle * 0.5).to_radians().tan()
                }
            }
            ToolShape::TaperedBall { angle, tip_radius } => {
                let ha = (angle * 0.5).to_radians();
                // Ball/cone tangent point.
                let rt = tip_radius * ha.cos();
                if r <= rt {
                    tip_radius - (tip_radius * tip_radius - r * r).max(0.0).sqrt()
                } else {
                    let zt = tip_radius - (tip_radius * tip_radius - rt * rt).sqrt();
                    zt + (r - rt) / ha.tan()
                }
            }
            ToolShape::Drill { point_angle } => r / (point_angle * 0.5).to_radians().tan(),
        })
    }

    /// Cutting data for a material group/operation; falls back to the other operation,
    /// then to the chip-load calculator.
    pub fn cutting_for(&self, material: MaterialGroup, op: Operation, machine: &feeds::MachineLimits) -> CuttingData {
        let pick = |o: Operation| self.cutting.iter().find(|c| c.material == material && c.operation == o).cloned();
        let other = match op {
            Operation::Roughing => Operation::Finishing,
            Operation::Finishing => Operation::Roughing,
        };
        pick(op)
            .or_else(|| {
                pick(other).map(|mut c| {
                    c.operation = op;
                    c.source = format!("{} (adapted from {:?})", c.source, other);
                    c
                })
            })
            .unwrap_or_else(|| feeds::calculate(self, material, op, machine))
    }
}

// ---------------------------------------------------------------------------------------------
// Library
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolLibrary {
    pub tools: Vec<Tool>,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolDbError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Deserialize)]
struct RawTable {
    source: String,
    machine: RawMachine,
    tools: Vec<RawTool>,
}
#[derive(Deserialize)]
struct RawMachine {
    max_rpm: f64,
    max_feed: f64,
}
#[derive(Deserialize)]
struct RawTool {
    product_id: String,
    description: String,
    shank_dia: Option<f64>,
    cut_dia: f64,
    tip_dia: Option<f64>,
    flutes: u32,
    flute_len: Option<f64>,
    coating: Option<String>,
    link: Option<String>,
    cutting: Vec<RawCut>,
}
#[derive(Deserialize)]
struct RawCut {
    material: String,
    operation: String,
    rpm: f64,
    feed: f64,
    stepdown: Option<f64>,
    stepover: Option<f64>,
    plunge_feed: Option<f64>,
    ramp_feed: Option<f64>,
}

/// Parse "V 60° groove" style descriptions into a shape.
pub fn shape_from_description(desc: &str, tip_dia: Option<f64>) -> (ToolShape, Vec<String>) {
    let d = desc.to_ascii_lowercase();
    let mut notes = Vec::new();
    let angle = d
        .split_whitespace()
        .find_map(|w| w.trim_end_matches('°').trim_end_matches("\u{b0}").parse::<f64>().ok())
        .or_else(|| d.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).find_map(|s| s.parse::<f64>().ok()));
    let shape = if d.starts_with('v') {
        ToolShape::VBit { angle: angle.unwrap_or(90.0), tip_diameter: tip_dia.unwrap_or(0.0) }
    } else if d.contains("tapered") {
        notes.push("Taper angle not in source table; 10° assumed — verify against the bit.".into());
        ToolShape::TaperedBall { angle: 10.0, tip_radius: 0.0 }
    } else if d.contains("ball") {
        ToolShape::BallNose
    } else if d.contains("drill") {
        ToolShape::Drill { point_angle: 118.0 }
    } else if d.contains("surfacing") {
        ToolShape::Surfacing
    } else {
        if d.contains("corncob") || d.contains("roughing") {
            notes.push("Corncob/roughing geometry: use for roughing, not finishing walls.".into());
        }
        if d.contains("downcut") {
            notes.push("Downcut: clean top edge, packs chips — reduce depth per pass in slots.".into());
        }
        if d.contains("compression") {
            notes.push("Compression: first pass must be deeper than the upcut length.".into());
        }
        if d.contains("o flute") {
            notes.push("O-flute: good for plastics and aluminium.".into());
        }
        ToolShape::FlatEnd
    };
    (shape, notes)
}

impl ToolLibrary {
    /// The SainSmart Genmitsu community table (metric source), as bundled.
    pub fn genmitsu() -> Self {
        Self::from_genmitsu_json(include_str!("../data/genmitsu_sainsmart_metric.json"), 1.0).expect("bundled genmitsu table is valid")
    }

    /// Parse a table produced by `tools/extract_bit_table.py`. `to_mm` converts length units.
    pub fn from_genmitsu_json(json: &str, to_mm: f64) -> Result<Self, ToolDbError> {
        let raw: RawTable = serde_json::from_str(json)?;
        let src = format!("{} — tuned for {:.0} RPM / {:.0} mm/min machine", raw.source, raw.machine.max_rpm, raw.machine.max_feed * to_mm);
        let mut tools = Vec::new();
        for (i, t) in raw.tools.into_iter().enumerate() {
            let (mut shape, notes) = shape_from_description(&t.description, t.tip_dia.map(|v| v * to_mm));
            let dia = t.cut_dia * to_mm;
            if let ToolShape::TaperedBall { tip_radius, .. } = &mut shape {
                *tip_radius = dia * 0.5;
            }
            // For tapered bits the table's diameter is the tip; use shank as max cutting dia.
            let diameter = match shape {
                ToolShape::TaperedBall { .. } => t.shank_dia.unwrap_or(dia) * to_mm,
                _ => dia,
            };
            let cutting = t
                .cutting
                .iter()
                .filter_map(|c| {
                    Some(CuttingData {
                        material: MaterialGroup::parse(&c.material)?,
                        operation: if c.operation.eq_ignore_ascii_case("roughing") { Operation::Roughing } else { Operation::Finishing },
                        rpm: c.rpm,
                        feed: c.feed * to_mm,
                        plunge_feed: c.plunge_feed.unwrap_or(c.feed * 0.4) * to_mm,
                        ramp_feed: c.ramp_feed.unwrap_or(c.feed * 0.75) * to_mm,
                        stepdown: c.stepdown.unwrap_or(dia * 0.25) * to_mm,
                        stepover: c.stepover.unwrap_or(dia * 0.4) * to_mm,
                        source: src.clone(),
                    })
                })
                .collect();
            let name = format!("{} {} Ø{:.3} mm", t.product_id, t.description, dia);
            tools.push(Tool {
                id: format!("genmitsu.{}.{}.{}", t.product_id.replace(' ', "_"), i, t.description.replace(' ', "_")),
                name,
                vendor: "SainSmart Genmitsu".into(),
                product_id: Some(t.product_id),
                shape,
                diameter,
                shank_diameter: t.shank_dia.unwrap_or(3.175) * to_mm,
                flutes: t.flutes.max(1),
                flute_length: t.flute_len.unwrap_or(10.0) * to_mm,
                coating: t.coating.filter(|s| !s.is_empty()),
                url: t.link,
                notes,
                cutting,
                number: 1,
            });
        }
        Ok(Self { tools })
    }

    /// Geometry templates for bit shapes commonly sold by other retailers (Amana, Whiteside,
    /// Makera, Bits&Bits, Freud, Spetool…). No vendor feeds are claimed: cutting data comes from
    /// the chip-load calculator until the user enters their own.
    pub fn generic_templates() -> Self {
        let mk = |id: &str, name: &str, shape: ToolShape, d: f64, flutes: u32, fl: f64| Tool {
            id: format!("generic.{id}"),
            name: name.into(),
            vendor: "Generic (geometry template)".into(),
            product_id: None,
            shape,
            diameter: d,
            shank_diameter: if d > 3.2 { 6.35 } else { 3.175 },
            flutes,
            flute_length: fl,
            coating: None,
            url: None,
            notes: vec!["No vendor feed data; speeds/feeds computed from chip load — verify with a test cut.".into()],
            cutting: vec![],
            number: 1,
        };
        Self {
            tools: vec![
                mk("flat_1_8_2f", "1/8\" 2-flute flat upcut", ToolShape::FlatEnd, 3.175, 2, 12.0),
                mk("flat_1_4_2f", "1/4\" 2-flute flat upcut", ToolShape::FlatEnd, 6.35, 2, 25.0),
                mk("flat_1_4_down", "1/4\" 2-flute downcut", ToolShape::FlatEnd, 6.35, 2, 25.0),
                mk("flat_1_16_2f", "1/16\" 2-flute flat", ToolShape::FlatEnd, 1.5875, 2, 6.0),
                mk("flat_3mm_1f_alu", "3.175 mm single-flute (aluminium/acrylic)", ToolShape::FlatEnd, 3.175, 1, 12.0),
                mk("flat_6mm_1f_alu", "6 mm single-flute (aluminium)", ToolShape::FlatEnd, 6.0, 1, 20.0),
                mk("ball_1_8", "1/8\" ball nose", ToolShape::BallNose, 3.175, 2, 12.0),
                mk("ball_1_16", "1/16\" ball nose", ToolShape::BallNose, 1.5875, 2, 6.0),
                mk("ball_1_4", "1/4\" ball nose", ToolShape::BallNose, 6.35, 2, 20.0),
                mk("taper_0_25", "0.25 mm tip tapered ball, 10° (detail relief)", ToolShape::TaperedBall { angle: 10.0, tip_radius: 0.25 }, 3.175, 2, 15.0),
                mk("taper_0_5", "0.5 mm tip tapered ball, 15°", ToolShape::TaperedBall { angle: 15.0, tip_radius: 0.5 }, 3.175, 2, 15.0),
                mk("v30", "30° V-bit (fine detail)", ToolShape::VBit { angle: 30.0, tip_diameter: 0.0 }, 6.35, 2, 10.0),
                mk("v60", "60° V-bit", ToolShape::VBit { angle: 60.0, tip_diameter: 0.0 }, 12.7, 2, 11.0),
                mk("v90", "90° V-bit (signs / chamfer)", ToolShape::VBit { angle: 90.0, tip_diameter: 0.0 }, 12.7, 2, 6.35),
                mk("v120", "120° V-bit (large lettering)", ToolShape::VBit { angle: 120.0, tip_diameter: 0.0 }, 25.4, 2, 7.3),
                mk(
                    "engrave_20_0_1",
                    "20° engraving bit, 0.1 mm tip (PCB / metal engraving)",
                    ToolShape::VBit { angle: 20.0, tip_diameter: 0.1 },
                    3.175,
                    1,
                    8.0,
                ),
                mk("drill_3", "3 mm twist drill", ToolShape::Drill { point_angle: 118.0 }, 3.0, 2, 20.0),
                mk("surf_1", "1\" spoilboard surfacing", ToolShape::Surfacing, 25.4, 2, 6.0),
                mk("laser_5w", "Diode laser (≈0.08 mm spot)", ToolShape::Laser { spot: 0.08 }, 0.08, 1, 0.0),
            ],
        }
    }

    /// Everything shipped with the app.
    pub fn bundled() -> Self {
        let mut l = Self::genmitsu();
        l.tools.extend(Self::generic_templates().tools);
        l
    }

    pub fn get(&self, id: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.id == id)
    }

    /// Fuzzy search on name / product id / shape (case-insensitive, all words must match).
    pub fn search(&self, q: &str) -> Vec<&Tool> {
        let words: Vec<String> = q.to_ascii_lowercase().split_whitespace().map(String::from).collect();
        self.tools
            .iter()
            .filter(|t| {
                let hay = format!("{} {} {}", t.name, t.shape.label(), t.vendor).to_ascii_lowercase();
                words.iter().all(|w| hay.contains(w))
            })
            .collect()
    }

    /// Rescale bundled table speeds for a different machine, keeping chip load constant:
    /// feed scales with RPM, then everything is clamped to the machine limits.
    pub fn scaled_for_machine(&self, from: &feeds::MachineLimits, to: &feeds::MachineLimits) -> Self {
        let mut out = self.clone();
        let rpm_scale = to.max_rpm / from.max_rpm;
        for t in &mut out.tools {
            for c in &mut t.cutting {
                c.rpm = (c.rpm * rpm_scale).clamp(to.min_rpm, to.max_rpm);
                let s = rpm_scale.min(to.max_feed / from.max_feed.max(1.0)).max(0.1);
                c.feed = (c.feed * s).min(to.max_feed);
                c.plunge_feed = (c.plunge_feed * s).min(to.max_feed * 0.5);
                c.ramp_feed = (c.ramp_feed * s).min(to.max_feed);
                c.source = format!("{} — rescaled for {}", c.source, to.name);
            }
        }
        out
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
    pub fn from_json(s: &str) -> Result<Self, ToolDbError> {
        Ok(serde_json::from_str(s)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genmitsu_loads() {
        let lib = ToolLibrary::genmitsu();
        assert!(lib.tools.len() > 200, "{}", lib.tools.len());
        let v = lib.tools.iter().find(|t| matches!(t.shape, ToolShape::VBit { angle, .. } if (angle - 60.0).abs() < 1e-9));
        assert!(v.is_some());
        assert!(lib.tools.iter().any(|t| !t.cutting.is_empty()));
    }

    #[test]
    fn inch_and_metric_agree() {
        let m = ToolLibrary::genmitsu();
        let i = ToolLibrary::from_genmitsu_json(include_str!("../data/genmitsu_sainsmart_inch.json"), 25.4).unwrap();
        assert_eq!(m.tools.len(), i.tools.len());
        // Same bit, diameters agree to within table rounding.
        assert!((m.tools[0].diameter - i.tools[0].diameter).abs() < 0.05);
    }

    #[test]
    fn profiles() {
        let lib = ToolLibrary::generic_templates();
        let v90 = lib.get("generic.v90").unwrap();
        assert!((v90.profile_height(1.0).unwrap() - 1.0).abs() < 1e-9);
        let ball = lib.get("generic.ball_1_8").unwrap();
        assert!((ball.profile_height(ball.radius()).unwrap() - ball.radius()).abs() < 1e-9);
        assert!(ball.profile_height(10.0).is_none());
        let tb = lib.get("generic.taper_0_5").unwrap();
        let mut last = -1.0;
        for i in 0..100 {
            let h = tb.profile_height(i as f64 * 0.015).unwrap();
            assert!(h >= last - 1e-12, "tapered profile must be monotonic");
            last = h;
        }
    }

    #[test]
    fn description_parsing() {
        assert!(
            matches!(shape_from_description("V 30° engraving", Some(0.1)).0, ToolShape::VBit { angle, tip_diameter } if angle == 30.0 && tip_diameter == 0.1)
        );
        assert!(matches!(shape_from_description("ball nose", None).0, ToolShape::BallNose));
        assert!(matches!(shape_from_description("drill", None).0, ToolShape::Drill { .. }));
    }
}
