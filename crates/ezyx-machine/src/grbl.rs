//! GRBL 1.1 protocol (also grblHAL / FluidNC which are protocol compatible).

use serde::{Deserialize, Serialize};

/// Realtime command bytes (sent immediately, not queued).
pub mod rt {
    pub const STATUS: u8 = b'?';
    pub const HOLD: u8 = b'!';
    pub const RESUME: u8 = b'~';
    pub const RESET: u8 = 0x18;
    pub const SAFETY_DOOR: u8 = 0x84;
    pub const JOG_CANCEL: u8 = 0x85;
    pub const FEED_100: u8 = 0x90;
    pub const FEED_PLUS_10: u8 = 0x91;
    pub const FEED_MINUS_10: u8 = 0x92;
    pub const FEED_PLUS_1: u8 = 0x93;
    pub const FEED_MINUS_1: u8 = 0x94;
    pub const RAPID_100: u8 = 0x95;
    pub const RAPID_50: u8 = 0x96;
    pub const RAPID_25: u8 = 0x97;
    pub const SPINDLE_100: u8 = 0x99;
    pub const SPINDLE_PLUS_10: u8 = 0x9A;
    pub const SPINDLE_MINUS_10: u8 = 0x9B;
    pub const SPINDLE_STOP: u8 = 0x9E;
    pub const FLOOD_TOGGLE: u8 = 0xA0;
    pub const MIST_TOGGLE: u8 = 0xA1;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MachineState {
    #[default]
    Unknown,
    Idle,
    Run,
    Hold,
    Jog,
    Alarm,
    Door,
    Check,
    Home,
    Sleep,
    Tool,
}

impl MachineState {
    fn parse(s: &str) -> Self {
        match s.split(':').next().unwrap_or("") {
            "Idle" => Self::Idle,
            "Run" => Self::Run,
            "Hold" => Self::Hold,
            "Jog" => Self::Jog,
            "Alarm" => Self::Alarm,
            "Door" => Self::Door,
            "Check" => Self::Check,
            "Home" => Self::Home,
            "Sleep" => Self::Sleep,
            "Tool" => Self::Tool,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatusReport {
    pub state: MachineState,
    pub raw_state: String,
    pub mpos: Option<[f64; 4]>,
    pub wpos: Option<[f64; 4]>,
    pub wco: Option<[f64; 4]>,
    pub feed: f64,
    pub spindle: f64,
    /// feed / rapid / spindle overrides (%).
    pub overrides: Option<[u32; 3]>,
    /// planner blocks free, rx bytes free.
    pub buffer: Option<[u32; 2]>,
    pub pins: String,
    pub line: Option<u32>,
}

impl StatusReport {
    /// Work position, deriving it from MPos − WCO when only MPos is reported.
    pub fn work_pos(&self) -> Option<[f64; 4]> {
        self.wpos.or_else(|| {
            let m = self.mpos?;
            let w = self.wco.unwrap_or_default();
            Some([m[0] - w[0], m[1] - w[1], m[2] - w[2], m[3] - w[3]])
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Ok,
    Error(u32),
    Alarm(u32),
    Status(StatusReport),
    /// `Grbl 1.1h ['$' for help]` or FluidNC/grblHAL banners.
    Welcome(String),
    /// `$100=800.000`
    Setting(u32, String),
    /// `[MSG:...]`, `[GC:...]`, `[PRB:...]`, `[VER:...]` …
    Feedback(String),
    Other(String),
}

fn parse_vec(s: &str) -> Option<[f64; 4]> {
    let mut v = [0.0; 4];
    let mut n = 0;
    for (i, p) in s.split(',').enumerate().take(4) {
        v[i] = p.trim().parse().ok()?;
        n += 1;
    }
    (n >= 3).then_some(v)
}

pub fn parse_status(s: &str) -> Option<StatusReport> {
    let inner = s.trim().strip_prefix('<')?.strip_suffix('>')?;
    let mut parts = inner.split('|');
    let raw_state = parts.next()?.to_string();
    let mut r = StatusReport { state: MachineState::parse(&raw_state), raw_state, ..Default::default() };
    for p in parts {
        let (k, v) = p.split_once(':').unwrap_or((p, ""));
        match k {
            "MPos" => r.mpos = parse_vec(v),
            "WPos" => r.wpos = parse_vec(v),
            "WCO" => r.wco = parse_vec(v),
            "FS" | "F" => {
                let mut it = v.split(',');
                r.feed = it.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
                r.spindle = it.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
            }
            "Ov" => {
                let o: Vec<u32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                if o.len() == 3 {
                    r.overrides = Some([o[0], o[1], o[2]]);
                }
            }
            "Bf" => {
                let o: Vec<u32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                if o.len() == 2 {
                    r.buffer = Some([o[0], o[1]]);
                }
            }
            "Pn" => r.pins = v.to_string(),
            "Ln" => r.line = v.parse().ok(),
            _ => {}
        }
    }
    Some(r)
}

pub fn parse_line(line: &str) -> Response {
    let l = line.trim();
    if l == "ok" {
        return Response::Ok;
    }
    if let Some(e) = l.strip_prefix("error:") {
        return Response::Error(e.trim().parse().unwrap_or(0));
    }
    if let Some(a) = l.strip_prefix("ALARM:") {
        return Response::Alarm(a.trim().parse().unwrap_or(0));
    }
    if l.starts_with('<') {
        if let Some(s) = parse_status(l) {
            return Response::Status(s);
        }
    }
    if l.starts_with("Grbl") || l.starts_with("GrblHAL") || l.contains("FluidNC") {
        return Response::Welcome(l.to_string());
    }
    if let Some(rest) = l.strip_prefix('$') {
        if let Some((k, v)) = rest.split_once('=') {
            if let Ok(n) = k.parse() {
                return Response::Setting(n, v.to_string());
            }
        }
    }
    if l.starts_with('[') {
        return Response::Feedback(l.to_string());
    }
    Response::Other(l.to_string())
}

pub fn error_text(code: u32) -> &'static str {
    match code {
        1 => "G-code words consist of a letter and a value. Letter was not found.",
        2 => "Numeric value format is not valid or missing an expected value.",
        3 => "Grbl '$' system command was not recognized or supported.",
        4 => "Negative value received for an expected positive value.",
        5 => "Homing cycle is not enabled via settings.",
        8 => "Grbl '$' command cannot be used unless Grbl is IDLE.",
        9 => "G-code locked out during alarm or jog state.",
        10 => "Soft limits cannot be enabled without homing also enabled.",
        11 => "Max characters per line exceeded.",
        15 => "Jog target exceeds machine travel. Command ignored.",
        16 => "Jog command with no '=' or contains prohibited g-code.",
        17 => "Laser mode requires PWM output.",
        20 => "Unsupported or invalid g-code command found in block.",
        22 => "Feed rate has not yet been set or is undefined.",
        24 => "Two G-code commands that both require the use of the XYZ axis words were detected in the block.",
        33 => "The motion command has an invalid target (arc).",
        _ => "See GRBL documentation for this error code.",
    }
}

pub fn alarm_text(code: u32) -> &'static str {
    match code {
        1 => "Hard limit triggered. Position likely lost — re-home.",
        2 => "Soft limit: motion target exceeds machine travel.",
        3 => "Reset while in motion. Position likely lost — re-home.",
        4 => "Probe fail: probe not in expected initial state.",
        5 => "Probe fail: probe did not contact the workpiece.",
        6 => "Homing fail: reset during active homing cycle.",
        7 => "Homing fail: safety door opened during homing.",
        8 => "Homing fail: could not clear limit switch on pull-off.",
        9 => "Homing fail: could not find limit switch.",
        _ => "See GRBL documentation for this alarm code.",
    }
}

/// Command builders.
pub mod cmd {
    /// Incremental jog (mm). `None` axes are not moved.
    pub fn jog(dx: Option<f64>, dy: Option<f64>, dz: Option<f64>, da: Option<f64>, feed: f64) -> String {
        let mut s = String::from("$J=G91 G21");
        for (l, v) in [('X', dx), ('Y', dy), ('Z', dz), ('A', da)] {
            if let Some(v) = v {
                s.push_str(&format!(" {l}{v:.3}"));
            }
        }
        s.push_str(&format!(" F{feed:.0}"));
        s
    }
    pub fn jog_to(x: Option<f64>, y: Option<f64>, z: Option<f64>, feed: f64) -> String {
        let mut s = String::from("$J=G90 G21");
        for (l, v) in [('X', x), ('Y', y), ('Z', z)] {
            if let Some(v) = v {
                s.push_str(&format!(" {l}{v:.3}"));
            }
        }
        s.push_str(&format!(" F{feed:.0}"));
        s
    }
    pub const HOME: &str = "$H";
    pub const UNLOCK: &str = "$X";
    pub const SETTINGS: &str = "$$";
    pub const PARSER_STATE: &str = "$G";
    pub const BUILD_INFO: &str = "$I";
    pub const CHECK_MODE: &str = "$C";

    /// Set current position as work zero for the given axes (G10 L20 P1).
    pub fn zero(x: bool, y: bool, z: bool, a: bool) -> String {
        let mut s = String::from("G10 L20 P1");
        for (l, on) in [('X', x), ('Y', y), ('Z', z), ('A', a)] {
            if on {
                s.push_str(&format!(" {l}0"));
            }
        }
        s
    }

    /// Z touch-plate probe sequence: probe down, set Z to plate thickness, retract.
    pub fn probe_z(plate_thickness: f64, max_travel: f64, feed: f64, retract: f64) -> Vec<String> {
        vec![
            "G21 G91".into(),
            format!("G38.2 Z-{max_travel:.3} F{feed:.0}"),
            "G90".into(),
            format!("G10 L20 P1 Z{plate_thickness:.3}"),
            "G91".into(),
            format!("G0 Z{retract:.3}"),
            "G90".into(),
        ]
    }
}

/// Machine travel from `$130`, `$131`, `$132` settings (mm) — used to auto-configure the profile.
pub fn travel_from_settings(settings: &[(u32, String)]) -> Option<[f64; 3]> {
    let get = |k| settings.iter().find(|(n, _)| *n == k).and_then(|(_, v)| v.parse::<f64>().ok());
    Some([get(130)?, get(131)?, get(132)?])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_mpos() {
        let s = parse_status("<Idle|MPos:1.000,2.000,-3.500|FS:0,0|WCO:1.000,1.000,0.000>").unwrap();
        assert_eq!(s.state, MachineState::Idle);
        assert_eq!(s.work_pos().unwrap()[1], 1.0);
    }
    #[test]
    fn status_hold_with_sub() {
        let s = parse_status("<Hold:0|WPos:0,0,0|Bf:15,128|Ov:100,100,100|Pn:Z>").unwrap();
        assert_eq!(s.state, MachineState::Hold);
        assert_eq!(s.buffer, Some([15, 128]));
        assert_eq!(s.pins, "Z");
    }
    #[test]
    fn lines() {
        assert_eq!(parse_line("ok"), Response::Ok);
        assert_eq!(parse_line("error:22"), Response::Error(22));
        assert_eq!(parse_line("ALARM:1"), Response::Alarm(1));
        assert!(matches!(parse_line("Grbl 1.1h ['$' for help]"), Response::Welcome(_)));
        assert_eq!(parse_line("$130=400.000"), Response::Setting(130, "400.000".into()));
    }
    #[test]
    fn jog_cmd() {
        assert_eq!(cmd::jog(Some(1.0), None, Some(-0.1), None, 500.0), "$J=G91 G21 X1.000 Z-0.100 F500");
    }
}
