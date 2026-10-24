//! Tolerant G-code parser → linear segments in millimetres (absolute).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedMove {
    pub from: [f64; 4],
    pub to: [f64; 4],
    pub rapid: bool,
    /// mm/min (0 for rapids).
    pub feed: f64,
    /// 1-based source line.
    pub line: usize,
}

fn strip_comments(l: &str) -> String {
    let mut out = String::with_capacity(l.len());
    let mut depth = 0;
    for c in l.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            ';' if depth == 0 => break,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Split "G1X10.5Y-2" into (letter, value) words.
pub fn words(line: &str) -> Vec<(char, f64)> {
    let s = strip_comments(line).to_ascii_uppercase();
    let b = s.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphabetic() {
            let mut j = i + 1;
            while j < b.len() && (b[j] == b' ') {
                j += 1;
            }
            let start = j;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.' || b[j] == b'-' || b[j] == b'+') {
                j += 1;
            }
            if let Ok(v) = s[start..j].parse::<f64>() {
                out.push((c, v));
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

pub fn parse(src: &str) -> Vec<ParsedMove> {
    let mut pos = [0.0f64; 4];
    let mut motion = 0u8; // 0 rapid, 1 linear, 2 cw, 3 ccw
    let mut absolute = true;
    let mut scale = 1.0;
    let mut feed = 0.0;
    let mut out = Vec::new();
    for (ln, line) in src.lines().enumerate() {
        let ws = words(line);
        if ws.is_empty() {
            continue;
        }
        let mut target = pos;
        let mut has_axis = false;
        let (mut i_off, mut j_off) = (0.0, 0.0);
        for &(c, v) in &ws {
            match c {
                'G' => match v as i32 {
                    0 => motion = 0,
                    1 => motion = 1,
                    2 => motion = 2,
                    3 => motion = 3,
                    20 => scale = 25.4,
                    21 => scale = 1.0,
                    90 => absolute = true,
                    91 => absolute = false,
                    _ => {}
                },
                'X' | 'Y' | 'Z' => {
                    let k = (c as u8 - b'X') as usize;
                    target[k] = if absolute { v * scale } else { pos[k] + v * scale };
                    has_axis = true;
                }
                'A' | 'B' => {
                    target[3] = if absolute { v } else { pos[3] + v };
                    has_axis = true;
                }
                'I' => i_off = v * scale,
                'J' => j_off = v * scale,
                'F' => feed = v * scale,
                _ => {}
            }
        }
        if !has_axis {
            continue;
        }
        match motion {
            0 | 1 => out.push(ParsedMove { from: pos, to: target, rapid: motion == 0, feed: if motion == 0 { 0.0 } else { feed }, line: ln + 1 }),
            _ => {
                // Arc in XY with centre offset I/J; linear Z/A interpolation (helix).
                let c = [pos[0] + i_off, pos[1] + j_off];
                let r = ((pos[0] - c[0]).powi(2) + (pos[1] - c[1]).powi(2)).sqrt();
                let a0 = (pos[1] - c[1]).atan2(pos[0] - c[0]);
                let mut a1 = (target[1] - c[1]).atan2(target[0] - c[0]);
                let cw = motion == 2;
                if cw && a1 >= a0 {
                    a1 -= std::f64::consts::TAU;
                }
                if !cw && a1 <= a0 {
                    a1 += std::f64::consts::TAU;
                }
                let sweep = a1 - a0;
                let n = ((sweep.abs() * r / 0.5).ceil() as usize).clamp(4, 720);
                let mut prev = pos;
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    let a = a0 + sweep * t;
                    let p = [c[0] + r * a.cos(), c[1] + r * a.sin(), pos[2] + (target[2] - pos[2]) * t, pos[3] + (target[3] - pos[3]) * t];
                    out.push(ParsedMove { from: prev, to: p, rapid: false, feed, line: ln + 1 });
                    prev = p;
                }
                target = prev;
            }
        }
        pos = target;
    }
    out
}

/// Total estimated run time in seconds (no acceleration model).
pub fn estimate_seconds(moves: &[ParsedMove], rapid_rate: f64) -> f64 {
    moves
        .iter()
        .map(|m| {
            let d = ((m.to[0] - m.from[0]).powi(2) + (m.to[1] - m.from[1]).powi(2) + (m.to[2] - m.from[2]).powi(2)).sqrt();
            d / if m.rapid { rapid_rate } else { m.feed.max(1.0) } * 60.0
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic() {
        let m = parse("G21 G90\nG0 X10 Y10 (go)\nG1 Z-1 F100 ; plunge\nG91 G1 X5\n");
        assert_eq!(m.len(), 3);
        assert_eq!(m[2].to[0], 15.0);
        assert_eq!(m[1].feed, 100.0);
    }
    #[test]
    fn inches() {
        let m = parse("G20\nG1 X1 F10");
        assert!((m[0].to[0] - 25.4).abs() < 1e-12);
    }
    #[test]
    fn arc_quarter() {
        let m = parse("G0 X10 Y0\nG3 X0 Y10 I-10 J0 F100");
        let last = m.last().unwrap();
        assert!((last.to[0]).abs() < 1e-9 && (last.to[1] - 10.0).abs() < 1e-9);
        for s in &m[1..] {
            let r = (s.to[0].powi(2) + s.to[1].powi(2)).sqrt();
            assert!((r - 10.0).abs() < 1e-9);
        }
    }
}
