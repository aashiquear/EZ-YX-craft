use super::Transport;
use crate::MachineError;
use std::io::{ErrorKind, Read, Write};
use std::time::Duration;

pub struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
    name: String,
}

impl SerialTransport {
    pub fn open(name: &str, baud: u32) -> Result<Self, MachineError> {
        let port = serialport::new(name, baud).timeout(Duration::from_millis(10)).open().map_err(|e| MachineError::Serial(e.to_string()))?;
        Ok(Self { port, name: name.into() })
    }

    pub fn list() -> Vec<String> {
        serialport::available_ports().map(|v| v.into_iter().map(|p| p.port_name).collect()).unwrap_or_default()
    }
}

impl Transport for SerialTransport {
    fn write_all(&mut self, data: &[u8]) -> Result<(), MachineError> {
        self.port.write_all(data)?;
        Ok(())
    }
    fn read_some(&mut self, buf: &mut [u8]) -> Result<usize, MachineError> {
        match self.port.read(buf) {
            Ok(n) => Ok(n),
            Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => Ok(0),
            Err(e) => Err(e.into()),
        }
    }
    fn describe(&self) -> String {
        self.name.clone()
    }
}
