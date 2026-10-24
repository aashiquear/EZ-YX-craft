//! Vector import: SVG (via usvg — handles transforms, shapes, arcs, CSS units) and DXF.

use ezyx_core::{path::shapes, DVec2, Polyline};

/// Import an SVG file. Output in mm with Y up (SVG user units are 96 DPI px).
pub fn import_svg(data: &[u8]) -> Result<Vec<Polyline>, String> {
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let px_to_mm = 25.4 / 96.0;
    let h = tree.size().height() as f64;
    let mut out = Vec::new();
    collect(tree.root(), &mut out, px_to_mm, h);
    Ok(out)
}

fn collect(g: &usvg::Group, out: &mut Vec<Polyline>, s: f64, h: f64) {
    for node in g.children() {
        match node {
            usvg::Node::Group(gr) => collect(gr, out, s, h),
            usvg::Node::Path(p) => {
                let t = p.abs_transform();
                let map = |pt: usvg::tiny_skia_path::Point| {
                    let mut q = pt;
                    t.map_point(&mut q);
                    DVec2::new(q.x as f64 * s, (h - q.y as f64) * s)
                };
                let mut cur: Vec<DVec2> = Vec::new();
                let mut pen = DVec2::ZERO;
                for seg in p.data().segments() {
                    use usvg::tiny_skia_path::PathSegment::*;
                    match seg {
                        MoveTo(a) => {
                            if cur.len() > 1 {
                                out.push(Polyline::open(std::mem::take(&mut cur)));
                            }
                            cur.clear();
                            pen = map(a);
                            cur.push(pen);
                        }
                        LineTo(a) => {
                            pen = map(a);
                            cur.push(pen);
                        }
                        QuadTo(c, a) => {
                            let (c, a) = (map(c), map(a));
                            let c1 = pen + (c - pen) * (2.0 / 3.0);
                            let c2 = a + (c - a) * (2.0 / 3.0);
                            cur.extend(shapes::cubic_bezier(pen, c1, c2, a, 0.01));
                            pen = a;
                        }
                        CubicTo(c1, c2, a) => {
                            let (c1, c2, a) = (map(c1), map(c2), map(a));
                            cur.extend(shapes::cubic_bezier(pen, c1, c2, a, 0.01));
                            pen = a;
                        }
                        Close => {
                            if cur.len() > 2 {
                                out.push(Polyline::closed(std::mem::take(&mut cur)));
                            }
                            cur.clear();
                        }
                    }
                }
                if cur.len() > 1 {
                    let closed = cur.first().unwrap().distance(*cur.last().unwrap()) < 1e-6;
                    out.push(Polyline::new(cur, closed));
                }
            }
            _ => {}
        }
    }
}

/// Import a DXF file (LINE, LWPOLYLINE with bulges, POLYLINE, CIRCLE, ARC, ELLIPSE, SPLINE via
/// control polygon). Units are taken as mm unless `$INSUNITS` says inches.
pub fn import_dxf(path: &std::path::Path) -> Result<Vec<Polyline>, String> {
    use dxf::entities::EntityType;
    let d = dxf::Drawing::load_file(path).map_err(|e| e.to_string())?;
    let scale = if d.header.default_drawing_units == dxf::enums::Units::Inches { 25.4 } else { 1.0 };
    let p2 = |x: f64, y: f64| DVec2::new(x * scale, y * scale);
    let mut out = Vec::new();
    let mut lines: Vec<(DVec2, DVec2)> = Vec::new();
    for e in d.entities() {
        match &e.specific {
            EntityType::Line(l) => lines.push((p2(l.p1.x, l.p1.y), p2(l.p2.x, l.p2.y))),
            EntityType::Circle(c) => out.push(shapes::circle(p2(c.center.x, c.center.y), c.radius * scale)),
            EntityType::Arc(a) => {
                let mut sweep = (a.end_angle - a.start_angle).to_radians();
                if sweep <= 0.0 {
                    sweep += std::f64::consts::TAU;
                }
                out.push(shapes::arc(p2(a.center.x, a.center.y), a.radius * scale, a.start_angle.to_radians(), sweep));
            }
            EntityType::Ellipse(el) => {
                let major = DVec2::new(el.major_axis.x, el.major_axis.y) * scale;
                let rx = major.length();
                out.push(shapes::ellipse(p2(el.center.x, el.center.y), rx, rx * el.minor_axis_ratio, major.y.atan2(major.x)));
            }
            EntityType::LwPolyline(pl) => {
                let mut pts = Vec::new();
                let n = pl.vertices.len();
                let closed = pl.is_closed();
                for i in 0..n {
                    let v = &pl.vertices[i];
                    let a = p2(v.x, v.y);
                    pts.push(a);
                    if v.bulge.abs() > 1e-9 && (closed || i + 1 < n) {
                        let w = &pl.vertices[(i + 1) % n];
                        let b = p2(w.x, w.y);
                        pts.extend(bulge_arc(a, b, v.bulge));
                    }
                }
                out.push(Polyline::new(pts, closed));
            }
            EntityType::Polyline(pl) => {
                let pts: Vec<DVec2> = pl.vertices().map(|v| p2(v.location.x, v.location.y)).collect();
                out.push(Polyline::new(pts, pl.is_closed()));
            }
            EntityType::Spline(sp) => {
                let src = if sp.fit_points.len() >= 2 { &sp.fit_points } else { &sp.control_points };
                let pts: Vec<DVec2> = src.iter().map(|p| p2(p.x, p.y)).collect();
                if pts.len() >= 2 {
                    out.push(Polyline::open(pts).chaikin(3));
                }
            }
            _ => {}
        }
    }
    out.extend(join_segments(lines));
    Ok(out.into_iter().filter(|p| p.len() >= 2).collect())
}

/// Points strictly between a and b along a DXF bulge arc (bulge = tan(θ/4), positive = CCW).
fn bulge_arc(a: DVec2, b: DVec2, bulge: f64) -> Vec<DVec2> {
    let chord = b - a;
    let l = chord.length();
    if l < 1e-12 || bulge.abs() < 1e-12 {
        return vec![];
    }
    let theta = 4.0 * bulge.atan();
    let left = DVec2::new(-chord.y, chord.x) / l;
    let c = (a + b) * 0.5 + left * (l * (1.0 - bulge * bulge) / (4.0 * bulge));
    let r = c.distance(a);
    let a0 = (a - c).y.atan2((a - c).x);
    let segs = ezyx_core::path::arc_segments(r, theta, 0.01);
    (1..segs).map(|i| c + DVec2::from_angle(a0 + theta * i as f64 / segs as f64) * r).collect()
}

/// Chain loose LINE entities into polylines (DXF exports often explode shapes into lines).
pub fn join_segments(mut segs: Vec<(DVec2, DVec2)>) -> Vec<Polyline> {
    let eps = 1e-4;
    let mut out = Vec::new();
    while let Some((a, b)) = segs.pop() {
        let mut pts = vec![a, b];
        loop {
            let end = *pts.last().unwrap();
            let start = pts[0];
            if let Some(i) = segs.iter().position(|(p, q)| p.distance(end) < eps || q.distance(end) < eps) {
                let (p, q) = segs.swap_remove(i);
                pts.push(if p.distance(end) < eps { q } else { p });
            } else if let Some(i) = segs.iter().position(|(p, q)| p.distance(start) < eps || q.distance(start) < eps) {
                let (p, q) = segs.swap_remove(i);
                pts.insert(0, if p.distance(start) < eps { q } else { p });
            } else {
                break;
            }
        }
        let closed = pts.len() > 3 && pts[0].distance(*pts.last().unwrap()) < eps;
        out.push(Polyline::new(pts, closed));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn svg_rect_and_circle() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm" viewBox="0 0 100 50">
            <rect x="10" y="10" width="30" height="20"/><circle cx="70" cy="25" r="10"/></svg>"#;
        let v = import_svg(svg).unwrap();
        assert_eq!(v.len(), 2);
        let r = v.iter().find(|p| p.len() <= 5).unwrap();
        // usvg resolves the viewBox to px; 1 user unit = 1 mm here.
        assert!((r.signed_area().abs() - 600.0).abs() < 5.0, "{}", r.signed_area());
        assert!(v.iter().all(|p| p.closed));
    }
    #[test]
    fn bulge_semicircle() {
        let pts = bulge_arc(DVec2::ZERO, DVec2::new(2.0, 0.0), 1.0);
        let mid = pts[pts.len() / 2];
        assert!((mid - DVec2::new(1.0, -1.0)).length() < 0.05, "{mid:?}");
        assert!(pts.iter().all(|p| (p.distance(DVec2::new(1.0, 0.0)) - 1.0).abs() < 1e-9));
    }
    #[test]
    fn joins_lines() {
        let s = vec![(DVec2::ZERO, DVec2::X), (DVec2::X, DVec2::ONE), (DVec2::ONE, DVec2::Y), (DVec2::Y, DVec2::ZERO)];
        let p = join_segments(s);
        assert_eq!(p.len(), 1);
        assert!(p[0].closed);
        assert!((p[0].signed_area().abs() - 1.0).abs() < 1e-9);
    }
}
