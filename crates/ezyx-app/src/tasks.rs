//! Background jobs (toolpath calculation, simulation, meshing) so the UI never freezes.

use crossbeam_channel::{unbounded, Receiver, Sender};

pub enum TaskResult {
    Toolpaths(Vec<ezyx_project::ops::OpResult>),
    Simulation { mesh: ezyx_mesh::TriMesh, removed_cm3: f64, warnings: Vec<String>, seconds: f64 },
    PartMesh { id: u64, mesh: ezyx_mesh::TriMesh },
    Error(String),
}

pub struct Tasks {
    tx: Sender<TaskResult>,
    pub rx: Receiver<TaskResult>,
    pub running: usize,
}

impl Default for Tasks {
    fn default() -> Self {
        let (tx, rx) = unbounded();
        Self { tx, rx, running: 0 }
    }
}

impl Tasks {
    pub fn spawn(&mut self, ctx: &egui::Context, f: impl FnOnce() -> TaskResult + Send + 'static) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.running += 1;
        std::thread::spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| TaskResult::Error("background task panicked".into()));
            let _ = tx.send(r);
            ctx.request_repaint();
        });
    }
}
