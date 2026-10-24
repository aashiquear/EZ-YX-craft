//! # ezyx-cam
//!
//! Toolpath generation. Every operation produces a [`Toolpath`]: a flat list of [`Move`]s in
//! *design coordinates* (mm, Z = 0 at stock top, Z negative into material). Datum shifts, unit
//! conversion and dialect quirks are applied later by `ezyx-gcode`.
//!
//! Operations (see [`ops`]):
//! | op | module | notes |
//! |---|---|---|
//! | Profile (outside / inside / on-line) | [`ops::profile`] | multi-pass, tabs, ramps, climb/conventional |
//! | Pocket (offset / raster) | [`ops::pocket`] | stay-down linking inside cleared area |
//! | V-carve (incl. flat-bottom) | [`ops::vcarve`] | offset-contour method |
//! | Engrave / quick-engrave on vectors | [`ops::engrave`] | |
//! | Drill (peck) | [`ops::drill`] | expanded to G0/G1 for GRBL |
//! | 3D raster rough / finish | [`ops::raster3d`] | drop-cutter on a heightmap, any tool shape |
//! | Rotary wrap & rotary 3D | [`ops::rotary`] | Y→A mapping with feed compensation |
//! | Photo V-carve / 3D polylines | [`ops::polyline3d`] | |

pub mod link;
pub mod ops;

use ezyx_core::DVec3;
use ezyx_tooldb::{CuttingData, Tool};
use serde::{Deserialize, Serialize};

/// A 4-axis point. `a` is degrees and only meaningful for rotary toolpaths.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub a: f64,
}

impl Pt {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z, a: 0.0 }
    }
    pub fn xyz(&self) -> DVec3 {
        DVec3::new(self.x, self.y, self.z)
    }
    pub fn from_v(v: DVec3) -> Self {
        Self::new(v.x, v.y, v.z)
    }
    pub fn dist(&self, o: &Pt) -> f64 {
        self.xyz().distance(o.xyz())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Move {
    Rapid(Pt),
    /// Linear feed move; `feed` in mm/min (for rotary moves: already compensated machine units).
    Feed {
        to: Pt,
        feed: f64,
    },
    Spindle {
        rpm: f64,
        on: bool,
    },
    /// Laser power 0..1 (laser-mode posts only).
    Laser {
        power: f64,
    },
    ToolChange {
        number: u32,
        name: String,
    },
    Dwell {
        seconds: f64,
    },
    Comment(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CutDirection {
    #[default]
    Climb,
    Conventional,
}

/// Common parameters shared by most operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutParams {
    pub tool: Tool,
    pub cutting: CuttingData,
    /// Depth at which the operation starts (≥ 0, mm below stock top).
    pub start_depth: f64,
    /// Final cut depth (> 0, mm below stock top).
    pub cut_depth: f64,
    /// Rapid clearance above stock top.
    pub safe_z: f64,
    pub direction: CutDirection,
}

impl CutParams {
    /// Z levels for multi-pass cutting (negative numbers, last one exactly `-cut_depth`).
    pub fn pass_levels(&self) -> Vec<f64> {
        pass_levels(self.start_depth, self.cut_depth, self.cutting.stepdown)
    }
}

pub fn pass_levels(start: f64, end: f64, step: f64) -> Vec<f64> {
    let step = step.max(0.01);
    let total = (end - start).max(0.0);
    if total <= 1e-9 {
        return vec![-end];
    }
    let n = (total / step).ceil().max(1.0) as usize;
    (1..=n).map(|i| -(start + total * i as f64 / n as f64)).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Toolpath {
    pub name: String,
    pub tool: Tool,
    pub cutting: CuttingData,
    pub moves: Vec<Move>,
    /// Uses the A axis.
    pub rotary: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolpathStats {
    pub cut_length: f64,
    pub rapid_length: f64,
    /// Estimated seconds (ignores acceleration; the planner in `ezyx-sim` refines this).
    pub est_seconds: f64,
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Toolpath {
    pub fn stats(&self, rapid_rate: f64) -> ToolpathStats {
        let mut s = ToolpathStats { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3], ..Default::default() };
        let mut cur: Option<Pt> = None;
        for m in &self.moves {
            let (to, feed) = match m {
                Move::Rapid(p) => (*p, None),
                Move::Feed { to, feed } => (*to, Some(*feed)),
                Move::Dwell { seconds } => {
                    s.est_seconds += seconds;
                    continue;
                }
                _ => continue,
            };
            for (i, v) in [to.x, to.y, to.z].into_iter().enumerate() {
                s.min[i] = s.min[i].min(v);
                s.max[i] = s.max[i].max(v);
            }
            if let Some(c) = cur {
                let d = c.dist(&to) + (to.a - c.a).abs() * 0.0;
                match feed {
                    Some(f) => {
                        s.cut_length += d;
                        s.est_seconds += d / f.max(1.0) * 60.0;
                    }
                    None => {
                        s.rapid_length += d;
                        s.est_seconds += d / rapid_rate.max(1.0) * 60.0;
                    }
                }
            }
            cur = Some(to);
        }
        s
    }

    /// Iterate motion endpoints (for previews).
    pub fn points(&self) -> impl Iterator<Item = (Pt, bool)> + '_ {
        self.moves.iter().filter_map(|m| match m {
            Move::Rapid(p) => Some((*p, true)),
            Move::Feed { to, .. } => Some((*to, false)),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn levels() {
        let l = pass_levels(0.0, 3.0, 1.0);
        assert_eq!(l, vec![-1.0, -2.0, -3.0]);
        let l = pass_levels(0.0, 2.5, 1.0);
        assert_eq!(l.len(), 3);
        assert!((l[2] + 2.5).abs() < 1e-12);
    }
}
