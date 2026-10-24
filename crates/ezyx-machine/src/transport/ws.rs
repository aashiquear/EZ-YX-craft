use super::Transport;
use crate::MachineError;
use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::Duration;
use tungstenite::{stream::MaybeTlsStream, Message, WebSocket};

/// WebSocket transport (ESP3D-style). Text and binary frames are both treated as GRBL output.
pub struct WsTransport {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    pending: VecDeque<u8>,
    url: String,
}

impl WsTransport {
    pub fn connect(url: &str) -> Result<Self, MachineError> {
        let (ws, _resp) = tungstenite::connect(url).map_err(|e| MachineError::Ws(e.to_string()))?;
        if let MaybeTlsStream::Plain(s) = ws.get_ref() {
            s.set_read_timeout(Some(Duration::from_millis(10)))?;
            s.set_nodelay(true)?;
        }
        Ok(Self { ws, pending: VecDeque::new(), url: url.into() })
    }
}

impl Transport for WsTransport {
    fn write_all(&mut self, data: &[u8]) -> Result<(), MachineError> {
        // ESP3D accepts commands as text frames; realtime bytes as binary.
        let msg = match std::str::from_utf8(data) {
            Ok(s) if data.iter().all(|b| *b < 0x80) => Message::text(s.to_string()),
            _ => Message::binary(data.to_vec()),
        };
        self.ws.send(msg).map_err(|e| MachineError::Ws(e.to_string()))
    }
    fn read_some(&mut self, buf: &mut [u8]) -> Result<usize, MachineError> {
        if self.pending.is_empty() {
            match self.ws.read() {
                Ok(Message::Text(t)) => self.pending.extend(t.as_bytes()),
                Ok(Message::Binary(b)) => self.pending.extend(b.iter()),
                Ok(Message::Close(_)) => return Err(MachineError::Ws("closed".into())),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => return Ok(0),
                Err(e) => return Err(MachineError::Ws(e.to_string())),
            }
        }
        let n = buf.len().min(self.pending.len());
        for b in buf.iter_mut().take(n) {
            *b = self.pending.pop_front().unwrap();
        }
        Ok(n)
    }
    fn describe(&self) -> String {
        self.url.clone()
    }
}
