//! A tiny GRBL simulator: answers `ok`, reports status, tracks position from G0/G1/$J moves.
//! Lets users explore the Machine tab (and lets tests exercise the streamer) without hardware.

use super::Transport;
use crate::MachineError;
use std::collections::VecDeque;

pub struct SimulatedGrbl {
    out: VecDeque<u8>,
    line: Vec<u8>,
    pos: [f64; 4],
    wco: [f64; 4],
    absolute: bool,
    hold: bool,
    pub lines_received: usize,
}

impl Default for SimulatedGrbl {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulatedGrbl {
    pub fn new() -> Self {
        let mut s = Self { out: VecDeque::new(), line: Vec::new(), pos: [0.0; 4], wco: [0.0; 4], absolute: true, hold: false, lines_received: 0 };
        s.say("\r\nGrbl 1.1h ['$' for help] (EZ-YX-craft simulator)");
        s
    }
    fn say(&mut self, s: &str) {
        self.out.extend(s.as_bytes());
        self.out.extend(b"\r\n");
    }
    fn handle_line(&mut self, l: &str) {
        self.lines_received += 1;
        let l = l.trim();
        if l.is_empty() {
            return;
        }
        if l == "$$" {
            for (k, v) in [(110, "8000"), (111, "8000"), (112, "1000"), (130, "400"), (131, "400"), (132, "110")] {
                self.say(&format!("${k}={v}"));
            }
        } else if l == "$I" {
            self.say("[VER:1.1h.EZYX-SIM:]");
            self.say("[OPT:V,15,128]");
        } else if l.starts_with("G10") && l.contains("L20") {
            for (i, a) in ['X', 'Y', 'Z', 'A'].iter().enumerate() {
                if l.contains(*a) {
                    self.wco[i] = self.pos[i];
                }
            }
        } else {
            let body = l.strip_prefix("$J=").unwrap_or(l);
            let ws = ezyx_words(body);
            let mut rel = !self.absolute;
            for (c, v) in &ws {
                if *c == 'G' && *v == 91.0 {
                    rel = true;
                }
                if *c == 'G' && *v == 90.0 {
                    rel = false;
                }
            }
            if !l.starts_with("$J=") {
                self.absolute = !rel;
            }
            for (c, v) in ws {
                let i = match c {
                    'X' => 0,
                    'Y' => 1,
                    'Z' => 2,
                    'A' => 3,
                    _ => continue,
                };
                if rel {
                    self.pos[i] += v;
                } else {
                    self.pos[i] = v + self.wco[i];
                }
            }
        }
        self.say("ok");
    }
}

fn ezyx_words(s: &str) -> Vec<(char, f64)> {
    let mut out = Vec::new();
    let up = s.to_ascii_uppercase();
    let mut chars = up.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c.is_ascii_alphabetic() {
            let mut j = i + 1;
            while let Some(&(k, d)) = chars.peek() {
                if d.is_ascii_digit() || d == '.' || d == '-' {
                    j = k + 1;
                    chars.next();
                } else {
                    break;
                }
            }
            if let Ok(v) = up[i + 1..j].parse() {
                out.push((c, v));
            }
        }
    }
    out
}

impl Transport for SimulatedGrbl {
    fn write_all(&mut self, data: &[u8]) -> Result<(), MachineError> {
        for &b in data {
            match b {
                b'?' => {
                    let st = if self.hold { "Hold:0" } else { "Idle" };
                    let p = self.pos;
                    let w = self.wco;
                    self.say(&format!("<{st}|MPos:{:.3},{:.3},{:.3}|FS:0,0|WCO:{:.3},{:.3},{:.3}>", p[0], p[1], p[2], w[0], w[1], w[2]));
                }
                b'!' => self.hold = true,
                b'~' => self.hold = false,
                0x18 => {
                    self.line.clear();
                    self.say("Grbl 1.1h ['$' for help]");
                }
                0x80..=0xFF => {}
                b'\n' => {
                    let l = String::from_utf8_lossy(&self.line).to_string();
                    self.line.clear();
                    self.handle_line(&l);
                }
                b'\r' => {}
                _ => self.line.push(b),
            }
        }
        Ok(())
    }
    fn read_some(&mut self, buf: &mut [u8]) -> Result<usize, MachineError> {
        let n = buf.len().min(self.out.len());
        for b in buf.iter_mut().take(n) {
            *b = self.out.pop_front().unwrap();
        }
        if n == 0 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        Ok(n)
    }
    fn describe(&self) -> String {
        "simulator".into()
    }
}
