//! # ezyx-sim — "what will it look like?"
//!
//! * [`StockSim`] – Z-map (dexel) material removal: every feed move sweeps the exact cutter
//!   profile (flat, ball, V, tapered, drill) through a heightfield of the stock.
//! * [`StockSim::render_mesh`] – the predicted finished piece, coloured with the stock material's
//!   procedural appearance (wood grain/end-grain darkening, brushed vs freshly-cut metal) — this is
//!   what the viewport's **Preview Result** button shows.
//! * [`advise`] – machining-outcome hints (scallop height, burn risk, chip thinning, overload).
//! * Rotary: simulate in unwrapped space, then [`wrap_mesh_cylinder`] for display.

use ezyx_cam::{Move, Toolpath};
use ezyx_core::{DVec2, Heightmap, JobSetup};
use ezyx_materials::Material;
use ezyx_mesh::{hm as hmesh, TriMesh};
use ezyx_tooldb::{feeds::MachineLimits, MaterialGroup, Tool, ToolShape};
use glam::Vec3;
use serde::{Deserialize, Serialize};

pub struct StockSim {
    pub hm: Heightmap,
    /// Cell touched by a cutter (rendered with cut-surface appearance).
    pub cut: Vec<bool>,
    pub bottom: f32,
    pub warnings: Vec<String>,
    initial_volume: f64,
}

struct Kernel {
    taps: Vec<(i32, i32, f32)>,
}

impl Kernel {
    fn new(tool: &Tool, cell: f64) -> Self {
        let r = tool.radius().max(cell * 0.5);
        let n = (r / cell).ceil() as i32;
        let mut taps = Vec::new();
        for dy in -n..=n {
            for dx in -n..=n {
                let d = ((dx * dx + dy * dy) as f64).sqrt() * cell;
                let h = if d <= cell * 0.5 { Some(0.0) } else { tool.profile_height(d) };
                if let Some(h) = h {
                    taps.push((dx, dy, h as f32));
                }
            }
        }
        Self { taps }
    }
}

impl StockSim {
    /// Flat stock from the job (top at Z = 0) at resolution `cell` mm.
    pub fn new(job: &JobSetup, cell: f64) -> Self {
        let hm = Heightmap::covering(DVec2::ZERO, DVec2::new(job.width, job.height), cell, 0.0);
        let n = hm.data.len();
        let bottom = -job.thickness as f32;
        let initial_volume = hm.volume_above(bottom);
        Self { hm, cut: vec![false; n], bottom, warnings: vec![], initial_volume }
    }

    fn stamp(&mut self, k: &Kernel, p: DVec2, z: f32) {
        let g = self.hm.to_grid(p);
        let (cx, cy) = (g.x.round() as i64, g.y.round() as i64);
        let (w, h) = (self.hm.width as i64, self.hm.height as i64);
        for &(dx, dy, ph) in &k.taps {
            let (x, y) = (cx + dx as i64, cy + dy as i64);
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let i = (y * w + x) as usize;
            let nz = (z + ph).max(self.bottom);
            if nz < self.hm.data[i] {
                self.hm.data[i] = nz;
                self.cut[i] = true;
            }
        }
    }

    /// Simulate one toolpath. Rotary toolpaths must be simulated *before* wrapping (flat form).
    pub fn apply(&mut self, tp: &Toolpath) {
        let k = Kernel::new(&tp.tool, self.hm.cell);
        let step = (self.hm.cell * 0.5).max(0.01);
        let mut cur: Option<ezyx_cam::Pt> = None;
        let laser = matches!(tp.tool.shape, ToolShape::Laser { .. });
        for m in &tp.moves {
            match m {
                Move::Rapid(p) => {
                    if let Some(c) = cur {
                        // Rapid below the current surface = crash risk.
                        let mid = DVec2::new((c.x + p.x) * 0.5, (c.y + p.y) * 0.5);
                        let surf = self.hm.sample(mid) as f64;
                        if p.z.min(c.z) < surf - 0.05 && (p.x != c.x || p.y != c.y) && self.warnings.len() < 20 {
                            self.warnings.push(format!("{}: rapid move through material near ({:.1}, {:.1})", tp.name, mid.x, mid.y));
                        }
                    }
                    cur = Some(*p);
                }
                Move::Feed { to, .. } => {
                    if laser {
                        cur = Some(*to);
                        continue;
                    }
                    if let Some(c) = cur {
                        let a = c.xyz();
                        let b = to.xyz();
                        let n = ((a.truncate().distance(b.truncate()) / step).ceil() as usize).max(1);
                        for i in 0..=n {
                            let q = a.lerp(b, i as f64 / n as f64);
                            if q.z < 0.0 {
                                self.stamp(&k, q.truncate(), q.z as f32);
                            }
                        }
                    }
                    cur = Some(*to);
                }
                _ => {}
            }
        }
    }

    pub fn removed_volume(&self) -> f64 {
        self.initial_volume - self.hm.volume_above(self.bottom)
    }

    /// Predicted result mesh with per-vertex material colours (`lod` = cell step for display).
    pub fn render_mesh(&self, material: &Material, lod: usize) -> TriMesh {
        let mut m = hmesh::solid(&self.hm, self.bottom, lod);
        let step = lod.max(1);
        let xs: Vec<usize> =
            (0..self.hm.width).step_by(step).chain(std::iter::once(self.hm.width - 1)).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        let ys: Vec<usize> =
            (0..self.hm.height).step_by(step).chain(std::iter::once(self.hm.height - 1)).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        let mut colors = Vec::with_capacity(m.positions.len());
        for &y in &ys {
            for &x in &xs {
                let p = m.positions[colors.len()];
                let n = self.hm.normal(x, y);
                let cut = self.cut[self.hm.idx(x, y)];
                colors.push(material.albedo([p.x, p.y, p.z], n, cut));
            }
        }
        // Bottom/side vertices: side-wall appearance.
        while colors.len() < m.positions.len() {
            let p = m.positions[colors.len()];
            colors.push(material.albedo([p.x, p.y, p.z], [1.0, 0.0, 0.0], false));
        }
        m.colors = colors;
        m
    }
}

/// Wrap an unwrapped rotary-sim mesh (x, y = θ·R, z = r − R) back onto the cylinder for display.
pub fn wrap_mesh_cylinder(m: &mut TriMesh, radius: f32) {
    for p in &mut m.positions {
        let theta = p.y / radius;
        let r = (radius + p.z).max(0.0);
        *p = Vec3::new(p.x, r * theta.cos(), r * theta.sin());
    }
}

// ---------------------------------------------------------------------------------------------
// Outcome advice
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Danger,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Advice {
    pub severity: Severity,
    pub message: String,
}

/// Scallop (cusp) height left by a ball-nose at `stepover` on a flat area.
pub fn scallop_height(ball_radius: f64, stepover: f64) -> f64 {
    let h = stepover * 0.5;
    if h >= ball_radius {
        return ball_radius;
    }
    ball_radius - (ball_radius * ball_radius - h * h).sqrt()
}

/// Heuristic predictions about surface finish and risk, shown next to the preview.
pub fn advise(tp: &Toolpath, group: MaterialGroup, machine: &MachineLimits) -> Vec<Advice> {
    let mut out = Vec::new();
    let c = &tp.cutting;
    let t = &tp.tool;
    let chip = c.feed / (c.rpm.max(1.0) * t.flutes.max(1) as f64);
    if matches!(t.shape, ToolShape::BallNose | ToolShape::TaperedBall { .. }) {
        let r = match t.shape {
            ToolShape::TaperedBall { tip_radius, .. } => tip_radius,
            _ => t.radius(),
        };
        let s = scallop_height(r, c.stepover);
        let sev = if s > 0.1 { Severity::Warning } else { Severity::Info };
        out.push(Advice {
            severity: sev,
            message: format!("Predicted scallop height {s:.3} mm at {:.3} mm stepover (finer stepover = smoother, slower).", c.stepover),
        });
    }
    if !group.is_metal() && chip < 0.01 && !matches!(t.shape, ToolShape::Laser { .. }) {
        out.push(Advice {
            severity: if matches!(group, MaterialGroup::Hardwood) { Severity::Warning } else { Severity::Info },
            message: format!("Chip load {chip:.4} mm/tooth is very low — rubbing may burn {group:?}. Raise feed or lower RPM."),
        });
    }
    if group.is_metal() && t.flutes > 2 {
        out.push(Advice {
            severity: Severity::Warning,
            message: "Use 1–2 flute cutters in non-ferrous metals to clear chips; 3+ flutes tend to pack and break.".into(),
        });
    }
    if group.is_metal() && c.stepdown > t.diameter * 0.25 {
        out.push(Advice {
            severity: Severity::Danger,
            message: format!("Depth per pass {:.2} mm is aggressive for metal on a hobby router (≤ {:.2} mm suggested).", c.stepdown, t.diameter * 0.2),
        });
    }
    if c.feed > machine.max_feed {
        out.push(Advice {
            severity: Severity::Warning,
            message: format!("Feed {:.0} exceeds the machine max {:.0} mm/min; the controller will clamp it.", c.feed, machine.max_feed),
        });
    }
    if c.stepdown > t.flute_length && t.flute_length > 0.0 {
        out.push(Advice { severity: Severity::Danger, message: "Depth per pass exceeds flute length — the shank will rub.".into() });
    }
    let st = tp.stats(machine.max_feed);
    if -st.min[2] > t.flute_length + 0.5 && t.flute_length > 0.0 && !matches!(t.shape, ToolShape::VBit { .. }) {
        out.push(Advice {
            severity: Severity::Warning,
            message: format!("Total depth {:.1} mm is deeper than the flute length {:.1} mm.", -st.min[2], t.flute_length),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ezyx_cam::{ops::pocket, CutDirection, CutParams};
    use ezyx_core::path::shapes::rectangle;
    use ezyx_tooldb::{Operation, ToolLibrary};

    #[test]
    fn pocket_removes_expected_volume() {
        let job = JobSetup { width: 60.0, height: 40.0, thickness: 10.0, ..Default::default() };
        let lib = ToolLibrary::generic_templates();
        let tool = lib.get("generic.flat_1_8_2f").unwrap().clone();
        let cutting = tool.cutting_for(MaterialGroup::Hardwood, Operation::Roughing, &MachineLimits::default());
        let p = CutParams { tool, cutting, start_depth: 0.0, cut_depth: 3.0, safe_z: 5.0, direction: CutDirection::Climb };
        let tp = pocket::generate("p", &[rectangle(DVec2::new(10.0, 10.0), 30.0, 20.0, 0.0)], &p, &Default::default());
        let mut sim = StockSim::new(&job, 0.2);
        sim.apply(&tp);
        let v = sim.removed_volume();
        let expected = 30.0 * 20.0 * 3.0;
        assert!((v - expected).abs() / expected < 0.06, "{v} vs {expected}");
        assert!(sim.warnings.is_empty(), "{:?}", sim.warnings);
        let mat = ezyx_materials::find("hardwood.walnut").unwrap();
        let m = sim.render_mesh(&mat, 2);
        assert_eq!(m.colors.len(), m.positions.len());
    }

    #[test]
    fn scallop() {
        assert!((scallop_height(1.5875, 0.3) - 0.00711).abs() < 1e-4);
    }
}
