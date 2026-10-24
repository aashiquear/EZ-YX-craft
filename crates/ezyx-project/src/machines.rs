//! Machine profiles. Values marked `verify` are typical figures — the Machine tab can read the
//! real travel from GRBL `$130–$132` and update the profile automatically.

use ezyx_tooldb::feeds::MachineLimits;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MachineProfile {
    pub id: String,
    pub name: String,
    pub post_id: String,
    pub limits: MachineLimits,
    /// Work area X, Y, Z (mm).
    pub travel: [f64; 3],
    pub has_rotary: bool,
    pub has_laser: bool,
    pub atc: bool,
    /// Default connection hint shown in the Machine tab.
    pub connection_hint: String,
    /// Values are approximate; confirm against the machine.
    pub verify: bool,
}

pub fn builtin() -> Vec<MachineProfile> {
    let lim = |name: &str, min_rpm: f64, max_rpm: f64, feed: f64, zf: f64, w: f64| MachineLimits {
        name: name.into(),
        min_rpm,
        max_rpm,
        max_feed: feed,
        max_z_feed: zf,
        spindle_watts: w,
    };
    vec![
        MachineProfile {
            id: "genmitsu_4040_pro_max".into(),
            name: "SainSmart Genmitsu 4040 PRO MAX".into(),
            post_id: "genmitsu_grbl".into(),
            limits: lim("Genmitsu 4040 PRO MAX", 10000.0, 30000.0, 5000.0, 1500.0, 710.0),
            travel: [400.0, 400.0, 110.0],
            has_rotary: true,
            has_laser: true,
            atc: false,
            connection_hint: "USB (115200) or the Genmitsu Wi-Fi module on your LAN — try TCP port 23, then WebSocket :81.".into(),
            verify: true,
        },
        MachineProfile {
            id: "genmitsu_3018_prover".into(),
            name: "SainSmart Genmitsu 3018-PROVer".into(),
            post_id: "genmitsu_grbl".into(),
            limits: MachineLimits::genmitsu_3018(),
            travel: [300.0, 180.0, 45.0],
            has_rotary: false,
            has_laser: true,
            atc: false,
            connection_hint: "USB 115200 baud.".into(),
            verify: true,
        },
        MachineProfile {
            id: "genmitsu_4030_proverxl".into(),
            name: "SainSmart Genmitsu PROVerXL 4030".into(),
            post_id: "genmitsu_grbl".into(),
            limits: lim("Genmitsu PROVerXL 4030", 8000.0, 24000.0, 4000.0, 1000.0, 300.0),
            travel: [400.0, 300.0, 110.0],
            has_rotary: true,
            has_laser: true,
            atc: false,
            connection_hint: "USB 115200 baud.".into(),
            verify: true,
        },
        MachineProfile {
            id: "makera_carvera".into(),
            name: "Makera Carvera".into(),
            post_id: "makera_carvera".into(),
            limits: lim("Makera Carvera", 3000.0, 15000.0, 3000.0, 1000.0, 200.0),
            travel: [360.0, 240.0, 140.0],
            has_rotary: true,
            has_laser: true,
            atc: true,
            connection_hint: "Wi-Fi or USB via the Carvera controller; upload files then run.".into(),
            verify: true,
        },
        MachineProfile {
            id: "makera_carvera_air".into(),
            name: "Makera Carvera Air".into(),
            post_id: "makera_carvera".into(),
            limits: lim("Makera Carvera Air", 3000.0, 15000.0, 3000.0, 1000.0, 200.0),
            travel: [300.0, 200.0, 140.0],
            has_rotary: true,
            has_laser: true,
            atc: false,
            connection_hint: "Wi-Fi or USB.".into(),
            verify: true,
        },
        MachineProfile {
            id: "xtool_laser".into(),
            name: "xTool laser (GRBL-compatible mode)".into(),
            post_id: "xtool_laser".into(),
            limits: lim("xTool laser", 0.0, 1000.0, 6000.0, 1000.0, 10.0),
            travel: [430.0, 390.0, 0.0],
            has_rotary: true,
            has_laser: true,
            atc: false,
            connection_hint: "Model-dependent; many accept GRBL G-code over USB.".into(),
            verify: true,
        },
        MachineProfile {
            id: "generic_grbl".into(),
            name: "Generic GRBL router".into(),
            post_id: "grbl".into(),
            limits: MachineLimits::default(),
            travel: [600.0, 600.0, 80.0],
            has_rotary: false,
            has_laser: false,
            atc: false,
            connection_hint: "USB 115200 baud.".into(),
            verify: true,
        },
    ]
}

pub fn find(id: &str) -> Option<MachineProfile> {
    builtin().into_iter().find(|m| m.id == id)
}
