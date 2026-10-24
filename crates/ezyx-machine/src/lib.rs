//! # ezyx-machine
//!
//! Talk to the CNC: connect, jog, home, zero, probe, stream jobs, apply overrides.
//!
//! * [`grbl`] – GRBL 1.1 / grblHAL / FluidNC protocol: response parsing, realtime bytes,
//!   command builders, error/alarm text.
//! * [`transport`] – byte pipes: USB serial, raw TCP ("telnet", port 23 — ESP32 GRBL bridges,
//!   FluidNC, grblHAL Wi-Fi), WebSocket (ESP3D-style, port 81) and a built-in simulator.
//! * [`controller`] – background IO thread with character-counting streaming, status polling and
//!   an event channel the UI subscribes to.
//! * [`discovery`] – list serial ports, scan the LAN for Wi-Fi controllers.
//!
//! ### About the Genmitsu Wi-Fi module
//! SainSmart's wireless module/app protocol is not publicly documented. Most ESP32 GRBL bridges
//! expose GRBL over raw TCP or WebSocket, which are supported here. If a module uses something
//! else, capture its traffic with Wireshark while using the vendor app and add a [`transport`]
//! implementation — see `docs/MACHINE_CONTROL.md`.

pub mod controller;
pub mod discovery;
pub mod grbl;
pub mod transport;

pub use controller::{Controller, Event, JobState};
pub use grbl::{MachineState, Response, StatusReport};
pub use transport::{Endpoint, Transport};

#[derive(Debug, thiserror::Error)]
pub enum MachineError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("websocket: {0}")]
    Ws(String),
    #[error("serial: {0}")]
    Serial(String),
    #[error("not connected")]
    NotConnected,
}
