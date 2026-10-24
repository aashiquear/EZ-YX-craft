//! Closed regions (polygons with holes): boolean operations and offsetting.
//!
//! Backed by `geo` (`BooleanOps` and `Buffer`, both built on `i_overlay`), which is pure Rust and
//! robust enough for CAM offsets. If profiling shows offsets dominating, a Clipper2 backend can be
//! slotted in behind the same API.

use crate::{Bounds2, DVec2, Polyline};
use geo::{BooleanOps, Buffer, Coord, LineString, MultiPolygon, Polygon};
use serde::{Deserialize, Serialize};

/// A planar region with even-odd semantics. Exterior rings CCW, holes CW.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Region {
    pub polygons: Vec<RegionPolygon>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegionPolygon {
    pub exterior: Polyline,
    pub holes: Vec<Polyline>,
}

fn ls(p: &Polyline) -> LineString<f64> {
    let mut c: Vec<Coord<f64>> = p.pts.iter().map(|v| Coord { x: v.x, y: v.y }).collect();
    if let Some(f) = c.first().copied() {
        c.push(f);
    }
    LineString::new(c)
}

fn pl(l: &LineString<f64>) -> Polyline {
    Polyline::closed(l.coords().map(|c| DVec2::new(c.x, c.y)).collect())
}

impl Region {
    /// Build a region from arbitrary closed polylines using even-odd fill
    /// (a letter "O" made of two rings becomes a ring with a hole).
    pub fn from_polylines<'a>(paths: impl IntoIterator<Item = &'a Polyline>) -> Self {
        let mut acc: Option<MultiPolygon<f64>> = None;
        for p in paths {
            if !p.closed || p.len() < 3 || p.signed_area().abs() < 1e-12 {
                continue;
            }
            let poly = MultiPolygon::new(vec![Polygon::new(ls(p), vec![])]);
            acc = Some(match acc {
                None => poly,
                Some(a) => a.xor(&poly),
            });
        }
        acc.map(|m| Self::from_geo(&m)).unwrap_or_default()
    }

    pub fn from_geo(m: &MultiPolygon<f64>) -> Self {
        Self {
            polygons: m
                .0
                .iter()
                .map(|p| RegionPolygon { exterior: pl(p.exterior()).oriented(true), holes: p.interiors().iter().map(|h| pl(h).oriented(false)).collect() })
                .filter(|p| p.exterior.len() >= 3)
                .collect(),
        }
    }

    pub fn to_geo(&self) -> MultiPolygon<f64> {
        MultiPolygon::new(self.polygons.iter().map(|p| Polygon::new(ls(&p.exterior), p.holes.iter().map(ls).collect())).collect())
    }

    pub fn is_empty(&self) -> bool {
        self.polygons.is_empty()
    }

    /// Offset outward (`d > 0`) or inward (`d < 0`) with round joins.
    pub fn offset(&self, d: f64) -> Region {
        if self.is_empty() {
            return Region::default();
        }
        if d.abs() < 1e-9 {
            return self.clone();
        }
        Self::from_geo(&self.to_geo().buffer(d))
    }

    pub fn union(&self, o: &Region) -> Region {
        Self::from_geo(&self.to_geo().union(&o.to_geo()))
    }
    pub fn difference(&self, o: &Region) -> Region {
        Self::from_geo(&self.to_geo().difference(&o.to_geo()))
    }
    pub fn intersection(&self, o: &Region) -> Region {
        Self::from_geo(&self.to_geo().intersection(&o.to_geo()))
    }

    /// All rings (exteriors then holes) as closed polylines.
    pub fn rings(&self) -> Vec<Polyline> {
        let mut v = Vec::new();
        for p in &self.polygons {
            v.push(p.exterior.clone());
            v.extend(p.holes.iter().cloned());
        }
        v
    }

    pub fn area(&self) -> f64 {
        self.polygons.iter().map(|p| p.exterior.signed_area().abs() - p.holes.iter().map(|h| h.signed_area().abs()).sum::<f64>()).sum()
    }

    pub fn contains(&self, pt: DVec2) -> bool {
        self.polygons.iter().any(|p| p.exterior.contains(pt) && !p.holes.iter().any(|h| h.contains(pt)))
    }

    pub fn bounds(&self) -> Bounds2 {
        self.polygons.iter().fold(Bounds2::EMPTY, |b, p| b.union(&p.exterior.bounds()))
    }

    /// Distance from `pt` to the nearest region boundary (unsigned).
    pub fn boundary_distance(&self, pt: DVec2) -> f64 {
        let mut best = f64::INFINITY;
        for r in self.rings() {
            for (a, b) in r.segments() {
                best = best.min(crate::path::dist_point_segment(pt, a, b));
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::shapes::*;

    #[test]
    fn even_odd_ring() {
        let outer = circle(DVec2::ZERO, 10.0);
        let inner = circle(DVec2::ZERO, 5.0);
        let r = Region::from_polylines([&outer, &inner]);
        assert_eq!(r.polygons.len(), 1);
        assert_eq!(r.polygons[0].holes.len(), 1);
        assert!(r.contains(DVec2::new(7.0, 0.0)));
        assert!(!r.contains(DVec2::new(1.0, 0.0)));
    }

    #[test]
    fn offset_square() {
        let sq = rectangle(DVec2::ZERO, 10.0, 10.0, 0.0);
        let r = Region::from_polylines([&sq]);
        let inner = r.offset(-1.0);
        assert!((inner.area() - 64.0).abs() < 0.1, "{}", inner.area());
        let outer = r.offset(1.0);
        // 10x10 + 4*10*1 + pi*1^2
        assert!((outer.area() - (140.0 + std::f64::consts::PI)).abs() < 0.2, "{}", outer.area());
        assert!(r.offset(-6.0).is_empty());
    }
}
