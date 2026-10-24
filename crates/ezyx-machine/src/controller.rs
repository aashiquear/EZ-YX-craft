//! Background controller: owns the transport on an IO thread, streams jobs with GRBL's
//! character-counting protocol, polls status, and publishes [`Event`]s.

use crate::grbl::{self, rt, Response, StatusReport};
use crate::transport::{Endpoint, Transport};
use crate::MachineError;
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::collections::VecDeque;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum Command {
    Line(String),
    Realtime(u8),
    StartJob(Vec<String>),
    Pause,
    Resume,
    Stop,
    Disconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JobState {
    #[default]
    Idle,
    Running,
    Paused,
    Done,
    Failed,
}

#[derive(Debug, Clone)]
pub enum Event {
    Connected(String),
    Disconnected(String),
    Status(StatusReport),
    Response(Response),
    /// Raw text line from the controller (console).
    Rx(String),
    /// A line we sent (console).
    Tx(String),
    Progress {
        acked: usize,
        total: usize,
        state: JobState,
    },
    Error(String),
}

pub struct Controller {
    tx: Sender<Command>,
    pub events: Receiver<Event>,
    handle: Option<JoinHandle<()>>,
    pub endpoint: Endpoint,
}

impl Controller {
    pub fn connect(endpoint: Endpoint) -> Result<Self, MachineError> {
        let transport = endpoint.open()?;
        Ok(Self::with_transport(endpoint, transport))
    }

    pub fn with_transport(endpoint: Endpoint, transport: Box<dyn Transport>) -> Self {
        let (tx, rx_cmd) = unbounded();
        let (ev_tx, events) = unbounded();
        let handle = std::thread::Builder::new().name("ezyx-machine-io".into()).spawn(move || io_loop(transport, rx_cmd, ev_tx)).expect("spawn io thread");
        Self { tx, events, handle: Some(handle), endpoint }
    }

    /// A cloneable handle for other threads (e.g. the AI assistant) to queue commands.
    pub fn sender(&self) -> Sender<Command> {
        self.tx.clone()
    }

    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }
    pub fn line(&self, s: impl Into<String>) {
        self.send(Command::Line(s.into()));
    }
    pub fn realtime(&self, b: u8) {
        self.send(Command::Realtime(b));
    }
    pub fn start_job(&self, gcode: &str) {
        self.send(Command::StartJob(prepare_job(gcode)));
    }
}

/// Strip comments/whitespace and blank lines so lines use the controller RX buffer efficiently.
pub fn prepare_job(gcode: &str) -> Vec<String> {
    gcode
        .lines()
        .map(|l| {
            // Strip comments and whitespace to save RX buffer.
            let mut s = String::new();
            let mut depth = 0;
            for c in l.chars() {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    ';' if depth == 0 => break,
                    _ if depth == 0 && !c.is_whitespace() => s.push(c),
                    _ => {}
                }
            }
            s
        })
        .filter(|l| !l.is_empty() && l != "%")
        .collect()
}

impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Disconnect);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn io_loop(mut t: Box<dyn Transport>, rx: Receiver<Command>, ev: Sender<Event>) {
    let _ = ev.send(Event::Connected(t.describe()));
    let rx_size = t.rx_buffer_hint();
    let mut buf = [0u8; 1024];
    let mut acc: Vec<u8> = Vec::new();
    // Streaming state.
    let mut job: VecDeque<String> = VecDeque::new();
    let mut inflight: VecDeque<usize> = VecDeque::new(); // byte lengths awaiting "ok"
    let mut total = 0usize;
    let mut acked = 0usize;
    let mut state = JobState::Idle;
    let mut last_poll = Instant::now() - Duration::from_secs(1);
    let mut manual_inflight = 0usize; // manual lines awaiting ok (not counted in job)

    let fail = |ev: &Sender<Event>, e: MachineError| {
        let _ = ev.send(Event::Disconnected(e.to_string()));
    };

    loop {
        // 1. Commands from the UI.
        while let Ok(c) = rx.try_recv() {
            match c {
                Command::Line(l) => {
                    let _ = ev.send(Event::Tx(l.clone()));
                    if let Err(e) = t.write_all(format!("{l}\n").as_bytes()) {
                        return fail(&ev, e);
                    }
                    manual_inflight += 1;
                }
                Command::Realtime(b) => {
                    if let Err(e) = t.write_all(&[b]) {
                        return fail(&ev, e);
                    }
                }
                Command::StartJob(lines) => {
                    total = lines.len();
                    acked = 0;
                    job = lines.into();
                    inflight.clear();
                    state = JobState::Running;
                    let _ = ev.send(Event::Progress { acked, total, state });
                }
                Command::Pause => {
                    let _ = t.write_all(&[rt::HOLD]);
                    if state == JobState::Running {
                        state = JobState::Paused;
                    }
                    let _ = ev.send(Event::Progress { acked, total, state });
                }
                Command::Resume => {
                    let _ = t.write_all(&[rt::RESUME]);
                    if state == JobState::Paused {
                        state = JobState::Running;
                    }
                    let _ = ev.send(Event::Progress { acked, total, state });
                }
                Command::Stop => {
                    // Feed hold then soft reset: the safe way to abort a GRBL job.
                    let _ = t.write_all(&[rt::HOLD]);
                    std::thread::sleep(Duration::from_millis(150));
                    let _ = t.write_all(&[rt::RESET]);
                    job.clear();
                    inflight.clear();
                    state = JobState::Idle;
                    let _ = ev.send(Event::Progress { acked, total, state });
                }
                Command::Disconnect => {
                    let _ = ev.send(Event::Disconnected("user".into()));
                    return;
                }
            }
        }

        // 2. Fill the controller's RX buffer (character counting).
        if state == JobState::Running {
            while let Some(next) = job.front() {
                let used: usize = inflight.iter().sum();
                let need = next.len() + 1;
                if used + need > rx_size && !inflight.is_empty() {
                    break;
                }
                let l = job.pop_front().unwrap();
                if let Err(e) = t.write_all(format!("{l}\n").as_bytes()) {
                    return fail(&ev, e);
                }
                inflight.push_back(need);
            }
        }

        // 3. Status polling (5 Hz).
        if last_poll.elapsed() >= Duration::from_millis(200) {
            let _ = t.write_all(&[rt::STATUS]);
            last_poll = Instant::now();
        }

        // 4. Read responses.
        match t.read_some(&mut buf) {
            Ok(n) if n > 0 => acc.extend_from_slice(&buf[..n]),
            Ok(_) => {}
            Err(e) => return fail(&ev, e),
        }
        while let Some(pos) = acc.iter().position(|b| *b == b'\n') {
            let raw: Vec<u8> = acc.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&raw).trim().to_string();
            if line.is_empty() {
                continue;
            }
            let resp = grbl::parse_line(&line);
            match &resp {
                Response::Status(s) => {
                    let _ = ev.send(Event::Status(s.clone()));
                    continue;
                }
                Response::Ok | Response::Error(_) => {
                    if manual_inflight > 0 && inflight.is_empty() {
                        manual_inflight -= 1;
                    } else if inflight.pop_front().is_some() {
                        acked += 1;
                        if let Response::Error(code) = resp {
                            let _ = ev.send(Event::Error(format!("line {acked}: error {code}: {}", grbl::error_text(code))));
                        }
                        if state == JobState::Running && job.is_empty() && inflight.is_empty() {
                            state = JobState::Done;
                        }
                        if acked % 10 == 0 || state == JobState::Done {
                            let _ = ev.send(Event::Progress { acked, total, state });
                        }
                    } else if manual_inflight > 0 {
                        manual_inflight -= 1;
                    }
                }
                Response::Alarm(code) => {
                    let _ = ev.send(Event::Error(format!("ALARM {code}: {}", grbl::alarm_text(*code))));
                    if state == JobState::Running {
                        state = JobState::Failed;
                        job.clear();
                        inflight.clear();
                        let _ = ev.send(Event::Progress { acked, total, state });
                    }
                }
                _ => {}
            }
            let _ = ev.send(Event::Rx(line));
            let _ = ev.send(Event::Response(resp));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::SimulatedGrbl;

    #[test]
    fn streams_job_to_simulator() {
        let c = Controller::with_transport(Endpoint::Simulator, Box::new(SimulatedGrbl::new()));
        let mut g = String::from("G21 G90\n");
        for i in 0..300 {
            g.push_str(&format!("G1 X{} Y{} F1000 (comment)\n", i % 50, i % 30));
        }
        c.start_job(&g);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut done = false;
        let mut saw_status = false;
        while Instant::now() < deadline && !(done && saw_status) {
            if let Ok(e) = c.events.recv_timeout(Duration::from_millis(100)) {
                match e {
                    Event::Progress { acked, total, state: JobState::Done } => {
                        assert_eq!(acked, total);
                        assert_eq!(total, 301);
                        done = true;
                    }
                    Event::Status(_) => saw_status = true,
                    _ => {}
                }
            }
        }
        assert!(done, "job did not complete");
        assert!(saw_status, "no status reports");
    }

    #[test]
    fn jog_moves_simulator() {
        let c = Controller::with_transport(Endpoint::Simulator, Box::new(SimulatedGrbl::new()));
        c.line(grbl::cmd::jog(Some(10.0), Some(5.0), None, None, 1000.0));
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(Event::Status(s)) = c.events.recv_timeout(Duration::from_millis(100)) {
                if let Some(p) = s.work_pos() {
                    if (p[0] - 10.0).abs() < 1e-6 && (p[1] - 5.0).abs() < 1e-6 {
                        return;
                    }
                }
            }
        }
        panic!("simulator did not report jogged position");
    }
}
