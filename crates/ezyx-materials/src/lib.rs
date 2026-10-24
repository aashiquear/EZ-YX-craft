//! # ezyx-materials
//!
//! Stock materials: machining group (drives feeds & speeds) + appearance (drives the
//! "predicted final result" renderer in the viewport).
//!
//! The appearance model is procedural so it costs no texture assets:
//! * **Wood** – growth rings around a virtual log axis, perturbed by value noise, plus a darker
//!   "end grain" tint on steep cut walls. Cut faces are slightly lighter/rougher than the planed top.
//! * **Metal** – base albedo + anisotropic brushed streaks on the stock top; freshly machined faces
//!   render brighter and more specular (that is what you see after a finishing pass).
//! * **Plastic** – flat colour, optional translucency hint (acrylic).

use ezyx_tooldb::MaterialGroup;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaterialFamily {
    Wood,
    EngineeredWood,
    Metal,
    Plastic,
    Foam,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Appearance {
    /// Linear-ish sRGB 0..1.
    pub base: [f32; 3],
    /// Secondary colour (latewood rings / brushed streaks).
    pub secondary: [f32; 3],
    /// Ring frequency (rings per mm) for wood; streak frequency for brushed metal.
    pub frequency: f32,
    /// Noise amplitude for ring distortion / streaks.
    pub turbulence: f32,
    pub metallic: f32,
    pub roughness: f32,
    /// Tint multiplier applied to freshly cut surfaces.
    pub cut_tint: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub id: String,
    pub name: String,
    pub family: MaterialFamily,
    pub group: MaterialGroup,
    /// Janka hardness (lbf) for woods; Brinell for metals; 0 when n/a.
    pub hardness: f64,
    /// kg/m³, for weight estimates.
    pub density: f64,
    pub appearance: Appearance,
    pub notes: String,
}

#[allow(clippy::too_many_arguments)]
fn wood(id: &str, name: &str, group: MaterialGroup, janka: f64, density: f64, base: [f32; 3], ring: [f32; 3], freq: f32, notes: &str) -> Material {
    Material {
        id: format!("{}.{}", if group == MaterialGroup::Softwood { "softwood" } else { "hardwood" }, id),
        name: name.into(),
        family: MaterialFamily::Wood,
        group,
        hardness: janka,
        density,
        appearance: Appearance { base, secondary: ring, frequency: freq, turbulence: 0.35, metallic: 0.0, roughness: 0.75, cut_tint: [1.06, 1.04, 1.0] },
        notes: notes.into(),
    }
}

fn metal(id: &str, name: &str, group: MaterialGroup, brinell: f64, density: f64, base: [f32; 3], notes: &str) -> Material {
    let s = [base[0] * 0.85, base[1] * 0.85, base[2] * 0.85];
    Material {
        id: format!("metal.{id}"),
        name: name.into(),
        family: MaterialFamily::Metal,
        group,
        hardness: brinell,
        density,
        appearance: Appearance { base, secondary: s, frequency: 6.0, turbulence: 0.15, metallic: 1.0, roughness: 0.35, cut_tint: [1.12, 1.12, 1.12] },
        notes: notes.into(),
    }
}

fn plain(id: &str, name: &str, family: MaterialFamily, group: MaterialGroup, density: f64, base: [f32; 3], notes: &str) -> Material {
    Material {
        id: id.into(),
        name: name.into(),
        family,
        group,
        hardness: 0.0,
        density,
        appearance: Appearance { base, secondary: base, frequency: 0.0, turbulence: 0.05, metallic: 0.0, roughness: 0.6, cut_tint: [0.97, 0.97, 0.97] },
        notes: notes.into(),
    }
}

/// The bundled material library.
pub fn library() -> Vec<Material> {
    use MaterialGroup as G;
    vec![
        wood("pine", "Pine", G::Softwood, 380.0, 500.0, [0.87, 0.74, 0.52], [0.72, 0.52, 0.30], 0.35, "Soft, resinous; knots behave like hardwood."),
        wood("cedar", "Western Red Cedar", G::Softwood, 350.0, 370.0, [0.78, 0.52, 0.36], [0.60, 0.36, 0.24], 0.4, "Very soft; fuzzy with dull bits."),
        wood(
            "basswood",
            "Basswood",
            G::Softwood,
            410.0,
            420.0,
            [0.92, 0.85, 0.70],
            [0.85, 0.76, 0.60],
            0.25,
            "Classic carving wood (technically a hardwood, machines like softwood).",
        ),
        wood("poplar", "Poplar", G::Hardwood, 540.0, 450.0, [0.86, 0.82, 0.62], [0.66, 0.68, 0.48], 0.3, "Easy hardwood, good for practice."),
        wood("cherry", "Cherry", G::Hardwood, 950.0, 560.0, [0.72, 0.42, 0.28], [0.58, 0.30, 0.20], 0.45, "Burns easily at low feed."),
        wood("walnut", "Black Walnut", G::Hardwood, 1010.0, 610.0, [0.42, 0.28, 0.19], [0.28, 0.18, 0.12], 0.4, "Excellent for V-carved signs and reliefs."),
        wood("maple", "Hard Maple", G::Hardwood, 1450.0, 705.0, [0.93, 0.84, 0.68], [0.82, 0.70, 0.52], 0.5, "Dense, crisp detail; prone to burning."),
        wood("oak", "Red Oak", G::Hardwood, 1290.0, 700.0, [0.80, 0.60, 0.42], [0.62, 0.42, 0.27], 0.3, "Open grain tears on fine detail."),
        wood("white_oak", "White Oak", G::Hardwood, 1360.0, 750.0, [0.76, 0.63, 0.45], [0.58, 0.46, 0.30], 0.3, ""),
        wood("bamboo", "Bamboo (strand)", G::Hardwood, 1400.0, 700.0, [0.84, 0.70, 0.46], [0.70, 0.55, 0.33], 1.2, "Abrasive; use carbide."),
        wood("padauk", "Padauk", G::Hardwood, 1970.0, 750.0, [0.75, 0.30, 0.18], [0.55, 0.20, 0.12], 0.5, ""),
        Material {
            id: "engineered.mdf".into(),
            name: "MDF".into(),
            family: MaterialFamily::EngineeredWood,
            group: G::Mdf,
            hardness: 0.0,
            density: 750.0,
            appearance: Appearance {
                base: [0.62, 0.50, 0.38],
                secondary: [0.58, 0.46, 0.35],
                frequency: 0.0,
                turbulence: 0.08,
                metallic: 0.0,
                roughness: 0.9,
                cut_tint: [0.92, 0.9, 0.88],
            },
            notes: "No grain; fuzzy cut edges. Wear a mask.".into(),
        },
        Material {
            id: "engineered.baltic_birch".into(),
            name: "Baltic Birch Plywood".into(),
            family: MaterialFamily::EngineeredWood,
            group: G::Plywood,
            hardness: 1260.0,
            density: 680.0,
            appearance: Appearance {
                base: [0.90, 0.80, 0.62],
                secondary: [0.70, 0.58, 0.40],
                frequency: 0.67,
                turbulence: 0.1,
                metallic: 0.0,
                roughness: 0.7,
                cut_tint: [1.0, 1.0, 1.0],
            },
            notes: "Layered: cut walls show plies (rendered as bands every 1.5 mm).".into(),
        },
        metal("al6061", "Aluminium 6061", G::Aluminum, 95.0, 2700.0, [0.80, 0.81, 0.83], "Single-flute bit, mist/air blast, shallow passes."),
        metal("al5052", "Aluminium 5052 (sheet)", G::Aluminum, 60.0, 2680.0, [0.78, 0.79, 0.81], "Gummy; prefer 6061 for machining."),
        metal("brass360", "Brass C360", G::Brass, 80.0, 8500.0, [0.85, 0.70, 0.36], "Free-machining; zero-rake engraving bits work well."),
        metal("copper110", "Copper C110", G::Copper, 50.0, 8900.0, [0.85, 0.52, 0.38], "Gummy; sharp single-flute, lubrication."),
        metal("mild_steel", "Mild Steel (engraving only)", G::Steel, 120.0, 7850.0, [0.55, 0.56, 0.58], "Hobby routers: light engraving only."),
        metal("stainless", "Stainless (engraving only)", G::Steel, 200.0, 8000.0, [0.68, 0.69, 0.70], "Diamond drag / laser marking recommended."),
        plain(
            "plastic.acrylic",
            "Acrylic (cast)",
            MaterialFamily::Plastic,
            G::Acrylic,
            1180.0,
            [0.85, 0.92, 0.95],
            "Cast acrylic machines cleanly; extruded melts.",
        ),
        plain("plastic.hdpe", "HDPE", MaterialFamily::Plastic, G::Hdpe, 950.0, [0.95, 0.95, 0.93], "Cutting boards; high feeds, single flute."),
        plain("plastic.delrin", "Acetal (Delrin)", MaterialFamily::Plastic, G::Hdpe, 1410.0, [0.96, 0.96, 0.94], ""),
        plain("foam.hdu", "HDU Sign Foam", MaterialFamily::Foam, G::Foam, 240.0, [0.88, 0.86, 0.80], "Great for 3D relief signs."),
    ]
}

pub fn find(id: &str) -> Option<Material> {
    library().into_iter().find(|m| m.id == id)
}

/// Cheap deterministic 2D value noise in [0,1].
pub fn value_noise(x: f32, y: f32) -> f32 {
    fn h(ix: i32, iy: i32) -> f32 {
        let mut n = (ix as u32).wrapping_mul(374761393) ^ (iy as u32).wrapping_mul(668265263);
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
    }
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - ix as f32, y - iy as f32);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = h(ix, iy) + (h(ix + 1, iy) - h(ix, iy)) * sx;
    let b = h(ix, iy + 1) + (h(ix + 1, iy + 1) - h(ix, iy + 1)) * sx;
    a + (b - a) * sy
}

impl Material {
    /// Shade a surface point. `p` is the world position (mm), `normal` the unit normal,
    /// `cut` whether the point was machined. Returns sRGB 0..1 *albedo* (lighting is applied by the
    /// renderer so the same function works for CPU previews and GPU shaders ported 1:1).
    pub fn albedo(&self, p: [f32; 3], normal: [f32; 3], cut: bool) -> [f32; 3] {
        let a = &self.appearance;
        let mut c = match self.family {
            MaterialFamily::Wood | MaterialFamily::EngineeredWood if a.frequency > 0.0 => {
                if self.group == MaterialGroup::Plywood {
                    // Plies: bands in Z, faint grain on faces.
                    let band = ((p[2] * a.frequency).fract() < 0.12) as i32 as f32;
                    mix(a.base, a.secondary, band * 0.8)
                } else {
                    // Virtual log axis along X, 120 mm below the stock, slightly tilted.
                    let dy = p[1] + 40.0 + p[0] * 0.03;
                    let dz = p[2] + 120.0;
                    let r = (dy * dy + dz * dz).sqrt();
                    let n = value_noise(p[0] * 0.02, p[1] * 0.15) * a.turbulence * 8.0;
                    let ring = ((r + n) * a.frequency).fract();
                    let late = smoothstep(0.65, 0.95, ring);
                    let fine = value_noise(p[0] * 0.8, p[1] * 6.0) * 0.08;
                    let mut c = mix(a.base, a.secondary, late);
                    for v in &mut c {
                        *v *= 0.96 + fine;
                    }
                    c
                }
            }
            MaterialFamily::Metal => {
                let streak = value_noise(p[0] * 0.05, p[1] * a.frequency) * a.turbulence;
                mix(a.base, a.secondary, streak)
            }
            _ => {
                let n = value_noise(p[0] * 0.3, p[1] * 0.3) * a.turbulence;
                mix(a.base, a.secondary, n)
            }
        };
        if cut {
            for (v, t) in c.iter_mut().zip(a.cut_tint) {
                *v = (*v * t).min(1.0);
            }
            // Steep walls in wood expose end grain: darker.
            if matches!(self.family, MaterialFamily::Wood) && normal[2].abs() < 0.5 {
                for v in &mut c {
                    *v *= 0.82;
                }
            }
        }
        c
    }
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn library_unique_ids() {
        let lib = library();
        let mut ids: Vec<_> = lib.iter().map(|m| m.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), lib.len());
        assert!(find("hardwood.walnut").is_some());
        assert!(find("metal.al6061").is_some());
    }
    #[test]
    fn albedo_in_range() {
        for m in library() {
            for i in 0..20 {
                let c = m.albedo([i as f32 * 3.1, i as f32 * 1.7, -(i as f32) * 0.2], [0.0, 0.0, 1.0], i % 2 == 0);
                assert!(c.iter().all(|v| (0.0..=1.0).contains(v)), "{} {:?}", m.id, c);
            }
        }
    }
}
