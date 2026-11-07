//! Byte transports. All are used from the controller's IO thread with short read timeouts.

#[cfg(serial)]
mod serial;
mod sim;
mod tcp;
mod ws;

#[cfg(serial)]
pub use serial::SerialTransport;
pub use sim::SimulatedGrbl;
pub use tcp::TcpTransport;
pub use ws::WsTransport;

use crate::MachineError;
use serde::{Deserialize, Serialize};

pub trait Transport: Send {
    fn write_all(&mut self, data: &[u8]) -> Result<(), MachineError>;
    /// Read whatever is available (may return 0 after a short timeout).
    fn read_some(&mut self, buf: &mut [u8]) -> Result<usize, MachineError>;
    fn describe(&self) -> String;
    /// Receive-buffer size of the controller (GRBL: 128, grblHAL: often 1024).
    fn rx_buffer_hint(&self) -> usize {
        127
    }
}

/// How to reach a machine. Serialisable so connection profiles can be saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Endpoint {
    Serial {
        port: String,
        baud: u32,
    },
    /// Raw TCP / telnet (ESP32 GRBL bridges, FluidNC, grblHAL Wi-Fi). Usually port 23.
    Tcp {
        host: String,
        port: u16,
    },
    /// WebSocket (ESP3D-style web UIs). Usually `ws://<ip>:81/`.
    WebSocket {
        url: String,
    },
    /// Built-in GRBL simulator for trying the app without hardware.
    Simulator,
}

impl Endpoint {
    pub fn open(&self) -> Result<Box<dyn Transport>, MachineError> {
        Ok(match self {
            #[cfg(serial)]
            Endpoint::Serial { port, baud } => Box::new(SerialTransport::open(port, *baud)?),
            #[cfg(not(serial))]
            Endpoint::Serial { .. } => {
                return Err(MachineError::Serial("USB serial is not available in this build (on iPad/Android connect over Wi-Fi)".into()))
            }
            Endpoint::Tcp { host, port } => Box::new(TcpTransport::connect(host, *port)?),
            Endpoint::WebSocket { url } => Box::new(WsTransport::connect(url)?),
            Endpoint::Simulator => Box::new(SimulatedGrbl::new()),
        })
    }
    pub fn label(&self) -> String {
        match self {
            Endpoint::Serial { port, baud } => format!("USB {port} @ {baud}"),
            Endpoint::Tcp { host, port } => format!("Wi-Fi TCP {host}:{port}"),
            Endpoint::WebSocket { url } => format!("Wi-Fi WebSocket {url}"),
            Endpoint::Simulator => "Simulator".into(),
        }
    }
}
