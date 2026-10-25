//! Keeps graphics trouble from crashing the app.
//!
//! * **Adapter choice at start-up** ([`wgpu_options`]): our own selector ranks the adapters with
//!   `ezyx_gpu::choose_adapter` — incompatible GPUs (tiny buffers/textures, can't present) are
//!   skipped and, if nothing usable is left (no driver, driver fell back), the software rasteriser
//!   is used. What happened is recorded in a [`StartupReport`] the app shows to the user.
//! * **Non-fatal wgpu errors** ([`install`]): wgpu treats uncaptured errors as fatal (panic) by
//!   default. We log them as [`GpuEvent`]s instead; the app reacts (smaller budgets, simplified
//!   previews, offer to restart with CPU rendering). Device loss is reported the same way.
//! * **Per-frame vertex budget** ([`take_triangles`], [`take_segments`]): egui uploads all of a window's vertices
//!   into one vertex buffer per frame, so the viewport never asks for more than the device can
//!   hold (the reported crash was a 321 MB buffer on a 256 MiB limit).
//! * **Restart in another mode** ([`restart`]).

use eframe::{egui_wgpu, wgpu};
use ezyx_gpu::{AdapterChoice, RenderMode};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct StartupReport {
    pub mode: RenderMode,
    pub adapter: String,
    pub backend: String,
    pub software: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GpuEvent {
    OutOfMemory(String),
    /// Includes "buffer too large" — the limit the reported crash hit.
    Validation(String),
    Internal(String),
    DeviceLost(String),
}

impl GpuEvent {
    pub fn message(&self) -> String {
        match self {
            GpuEvent::OutOfMemory(m) => format!("The GPU ran out of memory ({m})."),
            GpuEvent::Validation(m) => format!("The GPU rejected a request: {m}"),
            GpuEvent::Internal(m) => format!("GPU driver error: {m}"),
            GpuEvent::DeviceLost(m) => format!("The GPU device was lost ({m})."),
        }
    }
    /// Errors that mean "draw less": out of memory or a buffer over the device limit.
    pub fn is_memory(&self) -> bool {
        match self {
            GpuEvent::OutOfMemory(_) => true,
            GpuEvent::Validation(m) => m.contains("maximum buffer size") || m.contains("Buffer size") || m.to_ascii_lowercase().contains("out of memory"),
            _ => false,
        }
    }
}

static STARTUP: Mutex<Option<StartupReport>> = Mutex::new(None);
static EVENTS: Mutex<Vec<GpuEvent>> = Mutex::new(Vec::new());
/// Vertices (20 bytes each in egui's vertex buffer) a window may emit per frame.
static FRAME_BUDGET: AtomicUsize = AtomicUsize::new(6_000_000);
static EXCEEDED: AtomicBool = AtomicBool::new(false);
static RECOVERY: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

/// Where the latest autosaved recovery copy of the project is (reopened after a GPU restart).
pub fn set_recovery_path(p: std::path::PathBuf) {
    if let Ok(mut r) = RECOVERY.lock() {
        *r = Some(p);
    }
}

/// Last line of defence: if a graphics panic still happens (inside the driver or egui's
/// renderer), start the app again with CPU rendering on the latest recovery copy.
pub fn install_panic_fallback(mode: RenderMode) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        let msg = info.payload().downcast_ref::<String>().cloned().or_else(|| info.payload().downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
        let gpu = ["wgpu", "vertex buffer", "staging buffer", "index buffer", "surface", "Device"].iter().any(|k| msg.contains(k));
        if !gpu || mode == RenderMode::Cpu || std::env::var_os("EZYX_GPU_FAILED").is_some() {
            return;
        }
        if let Ok(exe) = std::env::current_exe() {
            let mut cmd = std::process::Command::new(exe);
            // Safe defaults in the new process: drop test overrides that may have caused the crash.
            cmd.env("EZYX_RENDER", "cpu").env("EZYX_GPU_FAILED", msg.lines().next().unwrap_or("graphics error")).env_remove("EZYX_FRAME_BUDGET");
            if let Some(p) = RECOVERY.lock().ok().and_then(|r| r.clone()) {
                cmd.arg(p);
            }
            let _ = cmd.spawn();
        }
    }));
}

pub fn startup() -> Option<StartupReport> {
    STARTUP.lock().ok().and_then(|s| s.clone())
}

fn push(e: GpuEvent) {
    log::warn!("{}", e.message());
    if let Ok(mut v) = EVENTS.lock() {
        if v.len() < 64 {
            v.push(e);
        }
    }
}

/// New GPU events since the last call.
pub fn drain() -> Vec<GpuEvent> {
    EVENTS.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

/// Route the device's uncaptured errors and device loss to [`drain`] instead of panicking.
pub fn install(device: &wgpu::Device) {
    device.on_uncaptured_error(Arc::new(|e: wgpu::Error| {
        let text = e.to_string();
        let last = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
        push(match e {
            wgpu::Error::OutOfMemory { .. } => GpuEvent::OutOfMemory(last),
            wgpu::Error::Validation { .. } => GpuEvent::Validation(last),
            _ => GpuEvent::Internal(last),
        });
    }));
    device.set_device_lost_callback(|reason, msg| {
        if reason != wgpu::DeviceLostReason::Destroyed {
            push(GpuEvent::DeviceLost(format!("{reason:?}: {msg}")));
        }
    });
}

/// Set the per-window, per-frame vertex budget (`RenderBudget::frame_vertices`).
pub fn set_frame_budget(vertices: usize) {
    FRAME_BUDGET.store(vertices.max(10_000), Ordering::Relaxed);
}

pub fn frame_budget() -> usize {
    FRAME_BUDGET.load(Ordering::Relaxed)
}

/// Reserve vertices for `want` triangles of this window's per-frame budget; returns how many
/// triangles may be drawn.
pub fn take_triangles(ctx: &egui::Context, want: usize) -> usize {
    take_in_pass(ctx, ctx.cumulative_pass_nr(), want * 3) / 3
}

/// Reserve vertices for `want` line segments; returns how many segments may be drawn.
pub fn take_segments(ctx: &egui::Context, want: usize) -> usize {
    let v = ezyx_gpu::VERTICES_PER_SEGMENT;
    take_in_pass(ctx, ctx.cumulative_pass_nr(), want * v) / v
}

fn take_in_pass(ctx: &egui::Context, pass: u64, want: usize) -> usize {
    let id = egui::Id::unique(("ezyx_tri_budget", ctx.viewport_id()));
    let budget = frame_budget();
    let granted = ctx.data_mut(|d| {
        let (p, used) = d.get_temp::<(u64, usize)>(id).unwrap_or((pass, 0));
        let used = if p == pass { used } else { 0 };
        let g = want.min(budget.saturating_sub(used));
        d.insert_temp(id, (pass, used + g));
        g
    });
    if granted < want {
        EXCEEDED.store(true, Ordering::Relaxed);
    }
    granted
}

/// Whether a mesh had to be thinned out to fit the budget since the last call.
pub fn take_exceeded() -> bool {
    EXCEEDED.swap(false, Ordering::Relaxed)
}

/// eframe/wgpu options: our adapter selector, and device limits raised to what the adapter can do
/// (bigger buffers on capable GPUs) without ever asking for more than it supports.
pub fn wgpu_options(mode: RenderMode) -> egui_wgpu::WgpuConfiguration {
    let selector: egui_wgpu::NativeAdapterSelectorMethod = Arc::new(move |adapters, surface| {
        let facts = ezyx_gpu::facts(adapters, surface);
        for f in &facts {
            log::info!("GPU adapter: {} [{}] software={} buffer={} MiB", f.name, f.backend, f.software, f.max_buffer_size >> 20);
        }
        let AdapterChoice { index, software, warnings } = ezyx_gpu::choose_adapter(&facts, mode)
            .ok_or_else(|| "no graphics adapter can draw to this window (no GPU driver and no software renderer)".to_string())?;
        let f = &facts[index];
        if let Ok(mut s) = STARTUP.lock() {
            *s = Some(StartupReport { mode, adapter: f.name.clone(), backend: f.backend.clone(), software, warnings });
        }
        Ok(adapters[index].clone())
    });
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create.native_adapter_selector = Some(selector);
    create.device_descriptor = Arc::new(|adapter: &wgpu::Adapter| {
        let a = adapter.limits();
        let base = if adapter.get_info().backend == wgpu::Backend::Gl { wgpu::Limits::downlevel_webgl2_defaults() } else { wgpu::Limits::default() };
        wgpu::DeviceDescriptor {
            label: Some("ezyx wgpu device"),
            required_limits: wgpu::Limits {
                max_texture_dimension_2d: 8192.min(a.max_texture_dimension_2d),
                // Up to 1 GiB buffers when the adapter allows it (default is 256 MiB).
                max_buffer_size: a.max_buffer_size.min(1 << 30).max(base.max_buffer_size.min(a.max_buffer_size)).min(test_buffer_cap()),
                ..base
            },
            memory_hints: if adapter.get_info().device_type == wgpu::DeviceType::Cpu { wgpu::MemoryHints::MemoryUsage } else { wgpu::MemoryHints::Performance },
            ..Default::default()
        }
    });
    egui_wgpu::WgpuConfiguration { wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(create), ..Default::default() }
}

/// `EZYX_MAX_BUFFER_MB` caps the device's buffer limit — simulates a low-memory GPU for testing.
fn test_buffer_cap() -> u64 {
    std::env::var("EZYX_MAX_BUFFER_MB").ok().and_then(|v| v.parse::<u64>().ok()).map(|mb| (mb.max(4)) << 20).unwrap_or(u64::MAX)
}

/// Start a fresh copy of the app in `mode` (optionally opening `project`) and close this one.
pub fn restart(mode: RenderMode, project: Option<&std::path::Path>, ctx: &egui::Context) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.env(
        "EZYX_RENDER",
        match mode {
            RenderMode::Auto => "auto",
            RenderMode::Gpu => "gpu",
            RenderMode::Cpu => "cpu",
        },
    );
    if let Some(p) = project {
        cmd.arg(p);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_budget_is_shared_per_pass() {
        let ctx = egui::Context::default();
        set_frame_budget(100_000);
        assert_eq!(take_in_pass(&ctx, 1, 70_000), 70_000);
        // The same frame can only get the rest…
        assert_eq!(take_in_pass(&ctx, 1, 70_000), 30_000);
        assert!(take_exceeded());
        assert!(!take_exceeded());
        // …and the next pass starts over.
        assert_eq!(take_in_pass(&ctx, 2, 70_000), 70_000);
    }

    #[test]
    fn memory_events_are_recognised() {
        let e = GpuEvent::Validation("Buffer size 321298560 is greater than the maximum buffer size (268435456)".into());
        assert!(e.is_memory());
        assert!(GpuEvent::OutOfMemory("x".into()).is_memory());
        assert!(!GpuEvent::DeviceLost("x".into()).is_memory());
    }
}
