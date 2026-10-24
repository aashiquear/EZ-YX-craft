//! Heightmap → triangle mesh.

use crate::TriMesh;
use ezyx_core::Heightmap;
use glam::Vec3;

/// Top surface only, optionally decimated by `step` cells (viewport LOD).
pub fn surface(hm: &Heightmap, step: usize) -> TriMesh {
    let step = step.max(1);
    let xs: Vec<usize> = (0..hm.width).step_by(step).chain(std::iter::once(hm.width - 1)).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let ys: Vec<usize> = (0..hm.height).step_by(step).chain(std::iter::once(hm.height - 1)).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let mut m = TriMesh::default();
    for &y in &ys {
        for &x in &xs {
            let c = hm.cell_center(x, y);
            m.positions.push(Vec3::new(c.x as f32, c.y as f32, hm.get(x, y)));
        }
    }
    let w = xs.len() as u32;
    for j in 0..ys.len().saturating_sub(1) as u32 {
        for i in 0..w.saturating_sub(1) {
            let a = j * w + i;
            let (b, c, d) = (a + 1, a + w, a + w + 1);
            m.indices.push([a, b, d]);
            m.indices.push([a, d, c]);
        }
    }
    m
}

/// Watertight solid: relief top, flat bottom at `base_z`, vertical side walls.
pub fn solid(hm: &Heightmap, base_z: f32, step: usize) -> TriMesh {
    let mut m = surface(hm, step);
    let n_top = m.positions.len() as u32;
    let step = step.max(1);
    let xs = ((hm.width - 1) / step + 1 + usize::from((hm.width - 1) % step != 0)) as u32;
    let ys = n_top / xs;
    // Bottom copies.
    for i in 0..n_top as usize {
        let p = m.positions[i];
        m.positions.push(Vec3::new(p.x, p.y, base_z));
    }
    let top = |i: u32, j: u32| j * xs + i;
    let bot = |i: u32, j: u32| n_top + j * xs + i;
    for j in 0..ys - 1 {
        for i in 0..xs - 1 {
            let (a, b, c, d) = (bot(i, j), bot(i + 1, j), bot(i, j + 1), bot(i + 1, j + 1));
            m.indices.push([a, d, b]);
            m.indices.push([a, c, d]);
        }
    }
    // Side walls (outward facing).
    let mut wall = |t0: u32, t1: u32, b0: u32, b1: u32| {
        m.indices.push([t0, b1, b0]);
        m.indices.push([t0, t1, b1]);
    };
    for i in 0..xs - 1 {
        wall(top(i + 1, 0), top(i, 0), bot(i + 1, 0), bot(i, 0)); // front (y min)
        wall(top(i, ys - 1), top(i + 1, ys - 1), bot(i, ys - 1), bot(i + 1, ys - 1));
        // back
    }
    for j in 0..ys - 1 {
        wall(top(0, j), top(0, j + 1), bot(0, j), bot(0, j + 1)); // left
        wall(top(xs - 1, j + 1), top(xs - 1, j), bot(xs - 1, j + 1), bot(xs - 1, j));
        // right
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use ezyx_core::DVec2;
    #[test]
    fn solid_is_watertight_with_volume() {
        let hm = Heightmap::new(11, 6, 1.0, DVec2::ZERO, 2.0);
        let m = solid(&hm, 0.0, 1);
        assert_eq!(m.boundary_edge_count(), 0);
        assert!((m.volume() - 10.0 * 5.0 * 2.0).abs() < 1e-3, "{}", m.volume());
        let m2 = solid(&hm, 0.0, 3);
        assert_eq!(m2.boundary_edge_count(), 0);
    }
}
