//! Bitmap → vector tracing (outline and centreline).

use crate::{Bitmap, GrayImage};
use ezyx_core::{DVec2, Polyline};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThresholdMode {
    /// Otsu automatic.
    #[default]
    Auto,
    Fixed(u8),
    /// Local mean — for photographed pencil sketches.
    Adaptive {
        radius: u16,
        offset: u8,
    },
    /// Edges of a photo (Sobel) instead of dark regions.
    Edges(u8),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceOptions {
    pub threshold: ThresholdMode,
    pub invert: bool,
    /// Pre-blur sigma (px) to suppress paper texture.
    pub blur: f32,
    /// Gap closing radius (px).
    pub close_gaps: usize,
    /// Drop blobs smaller than this many pixels.
    pub min_area_px: usize,
    /// Corner/curve fidelity: RDP tolerance in px.
    pub simplify_px: f64,
    /// Chaikin smoothing iterations.
    pub smooth: usize,
    /// Physical width of the traced image (mm); height follows aspect ratio.
    pub width_mm: f64,
    pub origin: DVec2,
}

impl Default for TraceOptions {
    fn default() -> Self {
        Self {
            threshold: ThresholdMode::Auto,
            invert: false,
            blur: 0.8,
            close_gaps: 1,
            min_area_px: 12,
            simplify_px: 0.8,
            smooth: 2,
            width_mm: 100.0,
            origin: DVec2::ZERO,
        }
    }
}

/// Produce the cleaned binary mask used by both tracers.
pub fn mask(img: &GrayImage, opt: &TraceOptions) -> Bitmap {
    let mut g = img.clone();
    if opt.invert {
        g.invert();
    }
    g.blur(opt.blur);
    let mut b = match opt.threshold {
        ThresholdMode::Auto => g.threshold(g.otsu()),
        ThresholdMode::Fixed(t) => g.threshold(t as f32 / 255.0),
        ThresholdMode::Adaptive { radius, offset } => g.adaptive_threshold(radius as usize, offset as f32 / 255.0),
        ThresholdMode::Edges(t) => {
            let mut e = g.sobel();
            e.invert(); // strong edges → dark → foreground
            e.threshold(1.0 - t as f32 / 255.0)
        }
    };
    if opt.close_gaps > 0 {
        b = b.close(opt.close_gaps);
    }
    b.remove_specks(opt.min_area_px);
    b
}

fn px_to_mm(img_w: usize, img_h: usize, opt: &TraceOptions) -> impl Fn(DVec2) -> DVec2 {
    let s = opt.width_mm / img_w.max(1) as f64;
    let o = opt.origin;
    move |p: DVec2| o + DVec2::new(p.x * s, (img_h as f64 - p.y) * s)
}

/// Outline tracing: boundaries between foreground and background pixels as closed loops.
pub fn trace_bitmap_outlines(b: &Bitmap) -> Vec<Vec<DVec2>> {
    // Directed boundary edges with foreground on the left (image coords, y down).
    // Vertex (x, y) is the pixel corner.
    let mut next: HashMap<(i32, i32), Vec<(i32, i32)>> = HashMap::new();
    let (w, h) = (b.width as isize, b.height as isize);
    for y in 0..h {
        for x in 0..w {
            if !b.get(x, y) {
                continue;
            }
            let (xi, yi) = (x as i32, y as i32);
            if !b.get(x, y - 1) {
                next.entry((xi + 1, yi)).or_default().push((xi, yi)); // top edge, right→left
            }
            if !b.get(x, y + 1) {
                next.entry((xi, yi + 1)).or_default().push((xi + 1, yi + 1)); // bottom, left→right
            }
            if !b.get(x - 1, y) {
                next.entry((xi, yi)).or_default().push((xi, yi + 1)); // left, top→bottom
            }
            if !b.get(x + 1, y) {
                next.entry((xi + 1, yi + 1)).or_default().push((xi + 1, yi)); // right, bottom→top
            }
        }
    }
    let mut loops = Vec::new();
    let mut starts: Vec<(i32, i32)> = next.keys().copied().collect();
    starts.sort();
    for s in starts {
        while next.get(&s).map(|v| !v.is_empty()).unwrap_or(false) {
            let mut lp = vec![s];
            let mut prev = s;
            let mut cur = next.get_mut(&s).unwrap().pop().unwrap();
            let mut guard = 0;
            while cur != s && guard < 10_000_000 {
                guard += 1;
                lp.push(cur);
                let outs = next.get_mut(&cur);
                let Some(outs) = outs else { break };
                if outs.is_empty() {
                    break;
                }
                // At saddle vertices pick the right-most turn to keep diagonal blobs separate.
                let din = (cur.0 - prev.0, cur.1 - prev.1);
                let idx = if outs.len() > 1 {
                    outs.iter()
                        .enumerate()
                        .max_by_key(|(_, o)| {
                            let d = (o.0 - cur.0, o.1 - cur.1);
                            din.0 * d.1 - din.1 * d.0 // cross product: prefer right turn in y-down
                        })
                        .map(|(i, _)| i)
                        .unwrap()
                } else {
                    0
                };
                prev = cur;
                cur = outs.swap_remove(idx);
            }
            if lp.len() >= 4 {
                loops.push(lp.into_iter().map(|(x, y)| DVec2::new(x as f64, y as f64)).collect());
            }
        }
    }
    loops
}

/// Image → closed outline vectors in mm.
pub fn trace_outlines(img: &GrayImage, opt: &TraceOptions) -> Vec<Polyline> {
    let b = mask(img, opt);
    let to_mm = px_to_mm(img.width, img.height, opt);
    trace_bitmap_outlines(&b)
        .into_iter()
        .map(|pts| {
            let pl = Polyline::closed(pts).simplify(opt.simplify_px).chaikin(opt.smooth);
            Polyline::closed(pl.pts.into_iter().map(&to_mm).collect())
        })
        .filter(|p| p.len() >= 3 && p.signed_area().abs() > 1e-6)
        .collect()
}

/// Image → single-line centreline vectors (for engraving pen sketches) in mm.
pub fn trace_centerlines(img: &GrayImage, opt: &TraceOptions) -> Vec<Polyline> {
    let skel = mask(img, opt).skeleton();
    let to_mm = px_to_mm(img.width, img.height, opt);
    skeleton_paths(&skel)
        .into_iter()
        .filter(|p| p.len() >= 2)
        .map(|pts| {
            let pl = Polyline::open(pts.into_iter().map(|(x, y)| DVec2::new(x as f64 + 0.5, y as f64 + 0.5)).collect());
            let pl = pl.simplify(opt.simplify_px).chaikin(opt.smooth.min(1));
            Polyline::open(pl.pts.into_iter().map(&to_mm).collect())
        })
        .collect()
}

/// Walk a 1-px skeleton into pixel chains, splitting at true junctions (crossing number ≥ 3),
/// then join chains whose ends touch.
pub fn skeleton_paths(s: &Bitmap) -> Vec<Vec<(isize, isize)>> {
    type P = (isize, isize);
    const RING: [(isize, isize); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];
    let nbrs = |p: P| -> Vec<P> { RING.iter().map(|(dx, dy)| (p.0 + dx, p.1 + dy)).filter(|q| s.get(q.0, q.1)).collect() };
    let crossings = |p: P| -> usize {
        let v: Vec<bool> = RING.iter().map(|(dx, dy)| s.get(p.0 + dx, p.1 + dy)).collect();
        (0..8).filter(|&i| !v[i] && v[(i + 1) % 8]).count()
    };
    let is_node = |p: P| {
        let n = nbrs(p).len();
        n == 1 || n == 0 || crossings(p) >= 3
    };
    let key = |a: P, b: P| if a < b { (a, b) } else { (b, a) };
    let adjacent = |a: P, b: P| (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1;
    let mut used = std::collections::HashSet::new();
    let mut visited_px = std::collections::HashSet::new();
    let mut paths: Vec<Vec<P>> = Vec::new();

    let walk = |start: P, first: P, used: &mut std::collections::HashSet<(P, P)>, visited: &mut std::collections::HashSet<P>| {
        let mut path = vec![start, first];
        used.insert(key(start, first));
        visited.insert(start);
        visited.insert(first);
        let mut prev = start;
        let mut cur = first;
        let mut guard = 0;
        while !is_node(cur) && guard < 1_000_000 {
            guard += 1;
            let mut cand: Vec<P> = nbrs(cur)
                .into_iter()
                .filter(|n| *n != prev && !used.contains(&key(cur, *n)) && !adjacent(*n, prev) && !(visited.contains(n) && *n != start))
                .collect();
            if cand.is_empty() {
                // allow closing a loop back to start
                if adjacent(cur, start) && path.len() > 3 {
                    path.push(start);
                }
                break;
            }
            // Prefer 4-connected steps.
            cand.sort_by_key(|n| (n.0 - cur.0).abs() + (n.1 - cur.1).abs());
            let nx = cand[0];
            used.insert(key(cur, nx));
            visited.insert(nx);
            prev = cur;
            cur = nx;
            path.push(cur);
        }
        path
    };
    let (w, h) = (s.width as isize, s.height as isize);
    for y in 0..h {
        for x in 0..w {
            let p = (x, y);
            if s.get(x, y) && is_node(p) {
                for nb in nbrs(p) {
                    if !used.contains(&key(p, nb)) && !visited_px.contains(&nb) {
                        paths.push(walk(p, nb, &mut used, &mut visited_px));
                    }
                }
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let p = (x, y);
            if s.get(x, y) && !visited_px.contains(&p) {
                if let Some(nb) = nbrs(p).into_iter().find(|n| !visited_px.contains(n)) {
                    paths.push(walk(p, nb, &mut used, &mut visited_px));
                }
            }
        }
    }
    join_chains(paths)
}

/// Greedily join chains whose endpoints are 8-adjacent (or identical).
fn join_chains(mut paths: Vec<Vec<(isize, isize)>>) -> Vec<Vec<(isize, isize)>> {
    let near = |a: (isize, isize), b: (isize, isize)| (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1;
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for i in 0..paths.len() {
            for j in 0..paths.len() {
                if i == j || paths[i].is_empty() || paths[j].is_empty() {
                    continue;
                }
                let (ie, js, je) = (*paths[i].last().unwrap(), paths[j][0], *paths[j].last().unwrap());
                if near(ie, js) {
                    let tail = std::mem::take(&mut paths[j]);
                    paths[i].extend(tail.into_iter().skip_while(|p| *p == ie));
                    changed = true;
                    break 'outer;
                }
                if near(ie, je) {
                    let mut tail = std::mem::take(&mut paths[j]);
                    tail.reverse();
                    paths[i].extend(tail.into_iter().skip_while(|p| *p == ie));
                    changed = true;
                    break 'outer;
                }
            }
        }
        paths.retain(|p| !p.is_empty());
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use ezyx_core::region::Region;

    fn ring_image() -> GrayImage {
        let mut g = GrayImage::new(100, 100, 1.0);
        for y in 0..100 {
            for x in 0..100 {
                let d = (((x as f32 - 50.0).powi(2) + (y as f32 - 50.0).powi(2)) as f32).sqrt();
                if (20.0..35.0).contains(&d) {
                    g.data[y * 100 + x] = 0.0;
                }
            }
        }
        g
    }

    #[test]
    fn ring_traces_to_region_with_hole() {
        let polys = trace_outlines(&ring_image(), &TraceOptions { width_mm: 100.0, ..Default::default() });
        assert_eq!(polys.len(), 2, "outer + inner");
        let r = Region::from_polylines(polys.iter());
        let expected = std::f64::consts::PI * (35.0f64.powi(2) - 20.0f64.powi(2));
        assert!((r.area() - expected).abs() / expected < 0.06, "{} vs {}", r.area(), expected);
    }

    #[test]
    fn centerline_of_ring_is_closed_loop() {
        let mut opt = TraceOptions::default();
        opt.smooth = 0;
        let lines = trace_centerlines(&ring_image(), &opt);
        assert!(!lines.is_empty());
        let total: f64 = lines.iter().map(|l| l.length()).sum();
        let expected = std::f64::consts::TAU * 27.5;
        assert!((total - expected).abs() / expected < 0.25, "{total} vs {expected}");
    }
}
