//! Move builder: handles retracts, plunges, ramps and nearest-neighbour ordering so the individual
//! operations only describe *where to cut*.

use crate::{Move, Pt, Toolpath};
use ezyx_core::{DVec2, DVec3, Polyline};
use ezyx_tooldb::{CuttingData, Tool};

pub struct PathBuilder {
    pub moves: Vec<Move>,
    pub safe_z: f64,
    pub cutting: CuttingData,
    cur: Option<Pt>,
}

impl PathBuilder {
    pub fn new(tool: &Tool, cutting: &CuttingData, safe_z: f64) -> Self {
        let moves = vec![
            Move::Comment(format!("Tool: {} ({})", tool.name, tool.shape.label())),
            Move::ToolChange { number: tool.number, name: tool.name.clone() },
            Move::Spindle { rpm: cutting.rpm, on: true },
        ];
        Self { moves, safe_z, cutting: cutting.clone(), cur: None }
    }

    pub fn current(&self) -> Option<Pt> {
        self.cur
    }

    pub fn comment(&mut self, s: impl Into<String>) {
        self.moves.push(Move::Comment(s.into()));
    }

    pub fn rapid(&mut self, p: Pt) {
        self.moves.push(Move::Rapid(p));
        self.cur = Some(p);
    }

    pub fn feed(&mut self, p: Pt, feed: f64) {
        if let Some(c) = self.cur {
            if c.dist(&p) < 1e-6 && (c.a - p.a).abs() < 1e-9 {
                return;
            }
        }
        self.moves.push(Move::Feed { to: p, feed });
        self.cur = Some(p);
    }

    pub fn retract(&mut self) {
        if let Some(c) = self.cur {
            if c.z < self.safe_z - 1e-9 {
                self.rapid(Pt { z: self.safe_z, ..c });
            }
        }
    }

    /// Move to above `p` at safe Z, rapid down to just above the stock, then plunge to `p.z`.
    pub fn plunge_to(&mut self, p: Pt, surface_z: f64) {
        self.retract();
        self.rapid(Pt { z: self.safe_z, ..p });
        let approach = (surface_z + 0.5).min(self.safe_z);
        if approach > p.z {
            self.rapid(Pt { z: approach, ..p });
        }
        self.feed(p, self.cutting.plunge_feed);
    }

    /// Cut a 3D polyline: plunge at its start (unless already there), feed along.
    pub fn cut_polyline3(&mut self, pts: &[DVec3], surface_z: f64) {
        if pts.is_empty() {
            return;
        }
        let start = Pt::from_v(pts[0]);
        let near = self.cur.map(|c| c.dist(&start) < 1e-6).unwrap_or(false);
        if !near {
            self.plunge_to(start, surface_z);
        }
        let f = self.cutting.feed;
        for p in &pts[1..] {
            self.feed(Pt::from_v(*p), f);
        }
    }

    /// Link to `next` by feeding at depth (stay-down) instead of retracting.
    pub fn link_direct(&mut self, next: Pt) {
        self.feed(next, self.cutting.feed);
    }

    /// Cut a closed 2D loop at depth `z`, entering with a linear ramp of `ramp_len` from `from_z`.
    /// After the ramp the full loop is cut at depth and the ramp section is re-cut, so the loop ends
    /// exactly where the ramp reached full depth.
    pub fn cut_loop_ramped(&mut self, path: &Polyline, z: f64, from_z: f64, ramp_len: f64, surface_z: f64) {
        if path.len() < 2 {
            return;
        }
        let f = self.cutting.feed;
        let total = path.length();
        let ramp_len = ramp_len.min(total * 0.9);
        let start = path.pts[0];
        let n = path.pts.len();
        if ramp_len <= 1e-6 || from_z <= z + 1e-9 {
            let p0 = Pt::new(start.x, start.y, z);
            let at = self.cur.map(|c| c.dist(&p0) < 1e-6).unwrap_or(false);
            if !at {
                self.plunge_to(p0, surface_z);
            }
            for i in 1..=n {
                let b = path.pts[i % n];
                self.feed(Pt::new(b.x, b.y, z), f);
            }
            return;
        }
        let begin = Pt::new(start.x, start.y, from_z);
        let at = self.cur.map(|c| c.dist(&begin) < 1e-6).unwrap_or(false);
        if !at {
            self.plunge_to(begin, surface_z);
        }
        // Ramp: walk the path until ramp_len, descending linearly.
        let mut s = 0.0;
        let mut seg = 0usize;
        let mut ramp_end = start;
        for i in 0..n {
            let (a, b) = (path.pts[i], path.pts[(i + 1) % n]);
            let l = a.distance(b);
            if s + l >= ramp_len {
                ramp_end = a.lerp(b, (ramp_len - s) / l.max(1e-12));
                seg = i;
                self.feed(Pt::new(ramp_end.x, ramp_end.y, z), self.cutting.ramp_feed);
                break;
            }
            s += l;
            self.feed(Pt::new(b.x, b.y, from_z + (z - from_z) * (s / ramp_len)), self.cutting.ramp_feed);
        }
        // Full loop at depth from the ramp end back to the ramp end.
        for k in 1..=n {
            let b = path.pts[(seg + k) % n];
            self.feed(Pt::new(b.x, b.y, z), f);
        }
        self.feed(Pt::new(ramp_end.x, ramp_end.y, z), f);
    }

    pub fn finish(mut self, name: impl Into<String>, tool: &Tool, rotary: bool) -> Toolpath {
        self.retract();
        Toolpath { name: name.into(), tool: tool.clone(), cutting: self.cutting.clone(), moves: self.moves, rotary }
    }
}

/// Order closed loops greedily by nearest start point, rotating each loop so it starts at the
/// vertex nearest to the previous end. Keeps relative order between `groups` (e.g. depth levels).
pub fn order_loops(mut loops: Vec<Polyline>, start: DVec2) -> Vec<Polyline> {
    let mut out = Vec::with_capacity(loops.len());
    let mut cur = start;
    while !loops.is_empty() {
        let (mut bi, mut bj, mut bd) = (0, 0, f64::INFINITY);
        for (i, l) in loops.iter().enumerate() {
            for (j, p) in l.pts.iter().enumerate() {
                let d = p.distance_squared(cur);
                if d < bd {
                    bd = d;
                    bi = i;
                    bj = j;
                }
                if !l.closed {
                    break;
                }
            }
            if !l.closed {
                if let Some(p) = l.pts.last() {
                    let d = p.distance_squared(cur);
                    if d < bd {
                        bd = d;
                        bi = i;
                        bj = usize::MAX;
                    }
                }
            }
        }
        let mut l = loops.swap_remove(bi);
        if l.closed {
            l.pts.rotate_left(bj);
            cur = l.pts[0];
        } else {
            if bj == usize::MAX {
                l.pts.reverse();
            }
            cur = *l.pts.last().unwrap();
        }
        out.push(l);
    }
    out
}
