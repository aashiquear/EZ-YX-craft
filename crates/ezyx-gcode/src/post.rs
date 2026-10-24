//! Post-processors.
//!
//! Template placeholders: `{rpm}`, `{tool}`, `{tool_name}`, `{safe_z}`, `{program}`, `{power}`,
//! `{seconds}`, `{ms}`, `{units}` (G20/G21).

use ezyx_cam::{Move, Pt, Toolpath};
use ezyx_core::{JobSetup, Units, ZZero};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommentStyle {
    Paren,
    Semicolon,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputUnits {
    /// Use the job's display units.
    Job,
    Millimeters,
    Inches,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub extension: String,
    pub units: OutputUnits,
    pub decimals: usize,
    pub comments: CommentStyle,
    pub line_numbers: bool,
    pub header: Vec<String>,
    pub footer: Vec<String>,
    /// `None` = no tool changer: emit a pause (`M0`) with a comment instead when the tool changes.
    pub tool_change: Option<Vec<String>>,
    pub manual_tool_change: Vec<String>,
    pub spindle_on: String,
    pub spindle_off: String,
    pub supports_rotary: bool,
    pub rotary_letter: char,
    /// Laser mode: spindle commands become `M4 S{power}`; S scaled to `laser_s_max`.
    pub laser_mode: bool,
    pub laser_s_max: f64,
    pub max_rpm: f64,
    pub dwell: String,
    /// Suppress repeated G-words and F-words (smaller files, better streaming).
    pub modal: bool,
}

impl PostConfig {
    fn base(id: &str, name: &str, desc: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: desc.into(),
            extension: "nc".into(),
            units: OutputUnits::Job,
            decimals: 3,
            comments: CommentStyle::Paren,
            line_numbers: false,
            header: vec!["{units}".into(), "G90 G94 G17".into(), "G0 Z{safe_z}".into()],
            footer: vec!["G0 Z{safe_z}".into(), "{spindle_off}".into(), "G0 X0 Y0".into(), "M30".into()],
            tool_change: None,
            manual_tool_change: vec!["G0 Z{safe_z}".into(), "{spindle_off}".into(), "(Change to tool {tool}: {tool_name} — then resume)".into(), "M0".into()],
            spindle_on: "M3 S{rpm}".into(),
            spindle_off: "M5".into(),
            supports_rotary: false,
            rotary_letter: 'A',
            laser_mode: false,
            laser_s_max: 1000.0,
            max_rpm: 30000.0,
            dwell: "G4 P{seconds}".into(),
            modal: true,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// Built-in posts. Users can add more as JSON in the app's `posts/` config folder.
pub fn builtin_posts() -> Vec<PostConfig> {
    let mut v = Vec::new();

    let grbl = PostConfig::base("grbl", "GRBL 1.1 (generic)", "Any GRBL 1.1 / grblHAL router. Manual tool changes via M0 pause.");
    v.push(grbl.clone());

    let mut g = grbl.clone();
    g.id = "genmitsu_grbl".into();
    g.name = "Genmitsu (GRBL 1.1) — 3018 / 3030 / 4030 / 4040 PRO & PRO MAX".into();
    g.description = "SainSmart Genmitsu routers on GRBL 1.1 firmware, incl. the optional 4th-axis kit (A axis).".into();
    g.supports_rotary = true;
    g.max_rpm = 24000.0;
    v.push(g);

    let mut gl = grbl.clone();
    gl.id = "genmitsu_laser".into();
    gl.name = "Genmitsu laser module (GRBL laser mode)".into();
    gl.description = "Requires $32=1. Uses M4 dynamic power.".into();
    gl.laser_mode = true;
    gl.header = vec!["{units}".into(), "G90 G94 G17".into(), "M4 S0".into()];
    gl.footer = vec!["M5".into(), "G0 X0 Y0".into(), "M2".into()];
    v.push(gl.clone());

    let mut x = gl.clone();
    x.id = "xtool_laser".into();
    x.name = "xTool laser (D1 / D1 Pro / S1 — GRBL-compatible mode)".into();
    x.description = "For xTool machines driven as GRBL devices (e.g. via LightBurn-compatible G-code). Verify on your model; xTool firmwares differ.".into();
    x.extension = "gcode".into();
    x.comments = CommentStyle::Semicolon;
    v.push(x);

    let mut fl = grbl.clone();
    fl.id = "fluidnc".into();
    fl.name = "FluidNC / grblHAL (ATC capable)".into();
    fl.tool_change = Some(vec!["G0 Z{safe_z}".into(), "M6 T{tool}".into()]);
    fl.supports_rotary = true;
    v.push(fl);

    let mut mk = PostConfig::base(
        "makera_carvera",
        "Makera Carvera / Carvera Air",
        "Smoothieware-based with auto tool changer (M6 T#) and 4th axis. Ensure tool numbers match the ATC slots.",
    );
    mk.tool_change = Some(vec!["G0 Z{safe_z}".into(), "M6 T{tool}".into()]);
    mk.supports_rotary = true;
    mk.max_rpm = 15000.0;
    mk.footer = vec!["G0 Z{safe_z}".into(), "{spindle_off}".into(), "M30".into()];
    v.push(mk);

    let mut lc = PostConfig::base("linuxcnc", "LinuxCNC", "LinuxCNC with tool changer (M6 T#, G43 H#).");
    lc.line_numbers = true;
    lc.header = vec!["%".into(), "{units}".into(), "G90 G94 G17 G40 G49 G80".into(), "G0 Z{safe_z}".into()];
    lc.footer = vec!["G0 Z{safe_z}".into(), "M5".into(), "M2".into(), "%".into()];
    lc.tool_change = Some(vec!["G0 Z{safe_z}".into(), "M6 T{tool}".into(), "G43 H{tool}".into()]);
    lc.supports_rotary = true;
    lc.extension = "ngc".into();
    v.push(lc);

    let mut m3 = PostConfig::base("mach3", "Mach3 / Mach4", "Mach3/4 routers.");
    m3.extension = "tap".into();
    m3.line_numbers = true;
    m3.tool_change = Some(vec!["G0 Z{safe_z}".into(), "M6 T{tool}".into()]);
    m3.supports_rotary = true;
    v.push(m3);

    let mut ml =
        PostConfig::base("marlin", "Marlin (Snapmaker / hybrid printers)", "Marlin CNC mode. Rapids are emitted as G0; set your max travel feed in firmware.");
    ml.comments = CommentStyle::Semicolon;
    ml.extension = "gcode".into();
    ml.footer = vec!["G0 Z{safe_z}".into(), "M5".into(), "M84".into()];
    v.push(ml);

    v
}

pub fn find_post(id: &str) -> Option<PostConfig> {
    builtin_posts().into_iter().find(|p| p.id == id)
}

#[derive(Debug, thiserror::Error)]
pub enum PostError {
    #[error("toolpath '{0}' uses the rotary axis but post '{1}' does not support it")]
    RotaryUnsupported(String, String),
}

struct Emitter<'a> {
    out: String,
    post: &'a PostConfig,
    scale: f64,
    line: u32,
    last_g: Option<&'static str>,
    last_f: Option<i64>,
    last: [Option<f64>; 4],
}

impl Emitter<'_> {
    fn raw(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        if self.post.line_numbers && !s.starts_with('%') {
            self.line += 10;
            let _ = writeln!(self.out, "N{} {}", self.line, s);
        } else {
            let _ = writeln!(self.out, "{s}");
        }
    }
    fn comment(&mut self, s: &str) {
        let clean = s.replace(['(', ')'], "");
        match self.post.comments {
            CommentStyle::Paren => self.raw(&format!("({clean})")),
            CommentStyle::Semicolon => self.raw(&format!("; {clean}")),
            CommentStyle::None => {}
        }
    }
    fn num(&self, v: f64) -> String {
        let s = format!("{:.*}", self.post.decimals, v);
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
        if s == "-0" {
            "0".into()
        } else {
            s
        }
    }
    fn motion(&mut self, g: &'static str, p: &Pt, feed: Option<f64>, rotary: bool) {
        let mut words = String::new();
        if !self.post.modal || self.last_g != Some(g) {
            words.push_str(g);
        }
        let axes = [('X', p.x * self.scale), ('Y', p.y * self.scale), ('Z', p.z * self.scale), (self.post.rotary_letter, p.a)];
        let mut any = false;
        for (i, (letter, v)) in axes.iter().enumerate() {
            if i == 3 && !rotary {
                continue;
            }
            let txt = self.num(*v);
            let prev = self.last[i].map(|pv| self.num(pv));
            if !self.post.modal || prev.as_deref() != Some(txt.as_str()) {
                if !words.is_empty() {
                    words.push(' ');
                }
                let _ = write!(words, "{letter}{txt}");
                self.last[i] = Some(*v);
                any = true;
            }
        }
        if let Some(f) = feed {
            let fi = (f * self.scale).round() as i64;
            if !self.post.modal || self.last_f != Some(fi) {
                let _ = write!(words, " F{fi}");
                self.last_f = Some(fi);
            }
        }
        if !any {
            return;
        }
        self.last_g = Some(g);
        let w = words.trim().to_string();
        self.raw(&w);
    }
}

fn fill(t: &str, vars: &[(&str, String)]) -> String {
    let mut s = t.to_string();
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// Emit a complete program for `toolpaths` on `job` using `post`.
pub fn emit(job: &JobSetup, toolpaths: &[Toolpath], post: &PostConfig, program: &str) -> Result<String, PostError> {
    let units = match post.units {
        OutputUnits::Job => job.display_units,
        OutputUnits::Millimeters => Units::Millimeters,
        OutputUnits::Inches => Units::Inches,
    };
    let scale = units.from_mm(1.0);
    let off = job.datum_offset();
    let z_off = if job.z_zero == ZZero::MachineBed { job.thickness } else { 0.0 };
    let mut e = Emitter { out: String::new(), post, scale, line: 0, last_g: None, last_f: None, last: [None; 4] };
    let safe = e.num(job.safe_z + z_off);
    let base_vars = vec![
        ("units", if units == Units::Inches { "G20".to_string() } else { "G21".to_string() }),
        ("safe_z", e.num((job.safe_z + z_off) * scale)),
        ("program", program.to_string()),
        ("spindle_off", post.spindle_off.clone()),
    ];
    let _ = safe;
    e.comment(&format!("EZ-YX-craft — {program}"));
    e.comment(&format!("Post: {}", post.name));
    e.comment(&format!("Stock {:.2} x {:.2} x {:.2} mm, datum {:?}, Z zero {:?}", job.width, job.height, job.thickness, job.xy_datum, job.z_zero));
    for h in &post.header {
        let l = fill(h, &base_vars);
        e.raw(&l);
    }
    let mut current_tool: Option<u32> = None;
    let mut first_tool = true;
    for tp in toolpaths {
        if tp.rotary && !post.supports_rotary {
            return Err(PostError::RotaryUnsupported(tp.name.clone(), post.name.clone()));
        }
        e.comment(&format!("Toolpath: {}", tp.name));
        for m in &tp.moves {
            match m {
                Move::Comment(c) => e.comment(c),
                Move::ToolChange { number, name } => {
                    if current_tool == Some(*number) && !first_tool {
                        continue;
                    }
                    let mut vars = base_vars.clone();
                    vars.push(("tool", number.to_string()));
                    vars.push(("tool_name", name.replace(['(', ')'], "")));
                    let tmpl = match &post.tool_change {
                        Some(t) => t.clone(),
                        None if first_tool => vec![],
                        None => post.manual_tool_change.clone(),
                    };
                    if first_tool && post.tool_change.is_none() {
                        e.comment(&format!("Load tool {number}: {name}"));
                    }
                    for l in tmpl {
                        let line = fill(&l, &vars);
                        if line.starts_with('(') {
                            e.comment(line.trim_matches(|c| c == '(' || c == ')'));
                        } else {
                            e.raw(&line);
                        }
                    }
                    e.last_g = None;
                    e.last = [None; 4];
                    current_tool = Some(*number);
                    first_tool = false;
                }
                Move::Spindle { rpm, on } => {
                    if post.laser_mode {
                        continue;
                    }
                    if *on {
                        let mut vars = base_vars.clone();
                        vars.push(("rpm", format!("{:.0}", rpm.min(post.max_rpm))));
                        let l = fill(&post.spindle_on, &vars);
                        e.raw(&l);
                    } else {
                        let l = post.spindle_off.clone();
                        e.raw(&l);
                    }
                }
                Move::Laser { power } => {
                    let s = (power.clamp(0.0, 1.0) * post.laser_s_max).round();
                    e.raw(&format!("S{s}"));
                }
                Move::Dwell { seconds } => {
                    let vars = vec![("seconds", e.num(*seconds)), ("ms", format!("{:.0}", seconds * 1000.0))];
                    let l = fill(&post.dwell, &vars);
                    e.raw(&l);
                }
                Move::Rapid(p) => {
                    let q = Pt { x: p.x + off.x, y: p.y + off.y, z: p.z + z_off, a: p.a };
                    let q = if tp.rotary { Pt { y: p.y, ..q } } else { q };
                    e.motion("G0", &q, None, tp.rotary);
                }
                Move::Feed { to, feed } => {
                    let q = Pt { x: to.x + off.x, y: to.y + off.y, z: to.z + z_off, a: to.a };
                    let q = if tp.rotary { Pt { y: to.y, ..q } } else { q };
                    e.motion("G1", &q, Some(*feed), tp.rotary);
                }
            }
        }
    }
    for f in &post.footer {
        let l = fill(f, &base_vars);
        e.raw(&l);
    }
    Ok(e.out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ezyx_cam::{ops::profile, CutDirection, CutParams};
    use ezyx_core::{path::shapes::rectangle, DVec2, XyDatum};
    use ezyx_tooldb::{feeds::MachineLimits, MaterialGroup, Operation, ToolLibrary};

    fn tp() -> Toolpath {
        let lib = ToolLibrary::generic_templates();
        let tool = lib.get("generic.flat_1_8_2f").unwrap().clone();
        let cutting = tool.cutting_for(MaterialGroup::Hardwood, Operation::Roughing, &MachineLimits::default());
        let p = CutParams { tool, cutting, start_depth: 0.0, cut_depth: 3.0, safe_z: 5.0, direction: CutDirection::Climb };
        profile::generate("Outline", &[rectangle(DVec2::ZERO, 50.0, 30.0, 0.0)], &p, &Default::default())
    }

    #[test]
    fn all_posts_emit() {
        let job = JobSetup::default();
        let t = tp();
        for post in builtin_posts() {
            let g = emit(&job, std::slice::from_ref(&t), &post, "test").unwrap();
            assert!(g.contains("G1"), "{}", post.id);
            assert!(g.lines().count() > 10);
            let parsed = crate::parse::parse(&g);
            assert!(!parsed.is_empty(), "{} parses", post.id);
        }
    }

    #[test]
    fn inch_output_and_datum() {
        let mut job = JobSetup { display_units: Units::Inches, xy_datum: XyDatum::Center, ..Default::default() };
        job.width = 100.0;
        let g = emit(&job, &[tp()], &find_post("grbl").unwrap(), "t").unwrap();
        assert!(g.contains("G20"));
        // Parsed back to mm, the cut must be centred on the datum shift.
        let moves = crate::parse::parse(&g);
        let minx = moves.iter().map(|m| m.to[0]).fold(f64::INFINITY, f64::min);
        assert!((minx - (-50.0 - 1.5875)).abs() < 0.05, "{minx}");
    }

    #[test]
    fn rotary_rejected_on_laser() {
        let mut t = tp();
        t.rotary = true;
        assert!(emit(&JobSetup::default(), &[t], &find_post("xtool_laser").unwrap(), "t").is_err());
    }
}
