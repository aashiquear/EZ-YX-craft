use super::Transport;
use crate::MachineError;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub struct TcpTransport {
    stream: TcpStream,
    label: String,
}

impl TcpTransport {
    pub fn connect(host: &str, port: u16) -> Result<Self, MachineError> {
        let addr = (host, port).to_socket_addrs()?.next().ok_or_else(|| MachineError::Io(std::io::Error::new(ErrorKind::NotFound, "host not found")))?;
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_millis(10)))?;
        Ok(Self { stream, label: format!("tcp://{host}:{port}") })
    }
}

impl Transport for TcpTransport {
    fn write_all(&mut self, data: &[u8]) -> Result<(), MachineError> {
        self.stream.write_all(data)?;
        Ok(())
    }
    fn read_some(&mut self, buf: &mut [u8]) -> Result<usize, MachineError> {
        match self.stream.read(buf) {
            Ok(0) => Err(MachineError::Io(std::io::Error::new(ErrorKind::ConnectionAborted, "connection closed"))),
            Ok(n) => Ok(n),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Ok(0),
            Err(e) => Err(e.into()),
        }
    }
    fn describe(&self) -> String {
        self.label.clone()
    }
}
