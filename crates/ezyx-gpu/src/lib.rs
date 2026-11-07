//! # ezyx-gpu — find the best GPU and the right graphics API for it
//!
//! EZ-YX-craft renders with `wgpu`, which speaks Vulkan, DirectX 12, Metal and OpenGL/GLES.
//! Older GPUs are the tricky part, so we classify each adapter:
//!
//! | Family | Examples | Best backend | Tier |
//! |---|---|---|---|
//! | NVIDIA RTX (Turing+) | RTX 2060 … 5090 | Vulkan / DX12 | Modern |
//! | NVIDIA GTX 9xx/10xx/16xx | GTX 1060, 1660 | Vulkan / DX12 | Capable |
//! | NVIDIA Kepler (GTX 6xx/7xx) | GTX 760 | Vulkan (≤ driver 470) | Legacy |
//! | AMD RDNA 1–4 | RX 5700, 6800, 7900, 9070 | Vulkan / DX12 | Modern |
//! | AMD GCN (incl. Polaris, Vega) | HD 7970, R9 290, RX 480/580, Vega 56 | Vulkan | Capable |
//! | AMD TeraScale | HD 5xxx / 6xxx, some HD 7xxx mobile | OpenGL 3.3 (no Vulkan) | Legacy |
//! | Intel Xe / Arc | Iris Xe, Arc A770 | Vulkan / DX12 | Capable |
//! | Apple Silicon / AMD on macOS | M1–M4 | Metal | Modern |
//! | Apple iPad GPUs | A12–A17, M1–M4 | Metal | Modern (A9–A11: Capable) |
//! | Qualcomm Adreno | Adreno 6xx/7xx/8xx (5xx and older: Legacy) | Vulkan (GLES fallback) | Capable |
//! | Arm Mali / Immortalis | Mali-G52…G720, Immortalis-G715+ (Mali-T/4xx: Legacy) | Vulkan (GLES fallback) | Capable |
//! | Samsung Xclipse (RDNA) | Xclipse 920/940 | Vulkan | Capable |
//! | Imagination PowerVR | GE8320, BXM | GLES / Vulkan | Legacy |
//!
//! *Tier* drives defaults: simulation resolution, viewport LOD, and whether compute shaders are
//! used for drop-cutter/simulation (Modern/Capable) or the rayon CPU path (Legacy/None).
//! Users can force a backend with `EZYX_GPU_BACKEND=vulkan|dx12|metal|gl` (mapped to `WGPU_BACKEND`)
//! or [`GpuBackend`] in Settings › Graphics (the only way on tablets). Phones and tablets get
//! [`RenderBudget::mobile`] on top: less memory and a slower CPU tessellating the viewport.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Qualcomm,
    /// Arm Mali / Immortalis.
    Arm,
    /// Imagination PowerVR.
    ImgTec,
    /// Samsung Xclipse (AMD RDNA based).
    Samsung,
    Software,
    Other,
}

impl Vendor {
    pub fn from_pci(id: u32) -> Self {
        match id {
            0x10DE => Vendor::Nvidia,
            0x1002 | 0x1022 => Vendor::Amd,
            0x8086 => Vendor::Intel,
            0x106B => Vendor::Apple,
            0x5143 => Vendor::Qualcomm,
            0x13B5 => Vendor::Arm,
            0x1010 => Vendor::ImgTec,
            0x144D => Vendor::Samsung,
            0x10005 | 0x1414 | 0x1AE0 => Vendor::Software, // Mesa llvmpipe / Microsoft WARP / SwiftShader
            _ => Vendor::Other,
        }
    }

    /// Vendor from the adapter name, for drivers that report no PCI id (Metal on iPad, many
    /// Android GLES drivers).
    pub fn from_name(name: &str) -> Self {
        let n = name.to_ascii_lowercase();
        let has = |k: &[&str]| k.iter().any(|k| n.contains(k));
        if has(&["llvmpipe", "lavapipe", "swiftshader", "softpipe", "microsoft basic render"]) {
            Vendor::Software
        } else if has(&["adreno"]) {
            Vendor::Qualcomm
        } else if has(&["mali", "immortalis"]) {
            Vendor::Arm
        } else if has(&["powervr"]) {
            Vendor::ImgTec
        } else if has(&["xclipse"]) {
            Vendor::Samsung
        } else if has(&["apple"]) {
            Vendor::Apple
        } else if has(&["nvidia", "geforce", "quadro"]) {
            Vendor::Nvidia
        } else if has(&["radeon", "amd"]) {
            Vendor::Amd
        } else if has(&["intel"]) {
            Vendor::Intel
        } else {
            Vendor::Other
        }
    }

    /// PCI id first, the name when the id is missing or unknown.
    pub fn detect(pci: u32, name: &str) -> Self {
        match Self::from_pci(pci) {
            Vendor::Other => Self::from_name(name),
            v => v,
        }
    }

    /// Phone/tablet GPU vendors (unified memory, thermally limited).
    pub fn is_mobile(self) -> bool {
        matches!(self, Vendor::Qualcomm | Vendor::Arm | Vendor::ImgTec | Vendor::Samsung)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tier {
    /// Software rasteriser or nothing usable.
    None,
    /// OpenGL-only / very old: CPU compute, reduced viewport detail.
    Legacy,
    /// Vulkan/DX12/Metal capable discrete or integrated GPU.
    Capable,
    /// Recent high-end GPU.
    Modern,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Family {
    NvidiaRtx,
    NvidiaGtxModern,
    NvidiaKepler,
    AmdRdna,
    AmdGcn,
    AmdTeraScale,
    IntelXeArc,
    IntelLegacy,
    AppleSilicon,
    Adreno,
    Mali,
    Xclipse,
    PowerVr,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: Vendor,
    pub family: Family,
    pub tier: Tier,
    /// "Vulkan", "Dx12", "Metal", "Gl"
    pub backend: String,
    pub device_type: String,
    pub driver: String,
    pub notes: Vec<String>,
}

/// Classify a GPU from its vendor and marketing name (pure function, unit-tested).
pub fn classify(vendor: Vendor, name: &str) -> (Family, Tier, Vec<String>) {
    let n = name.to_ascii_lowercase();
    let num = |prefix: &str| -> Option<u32> {
        let i = n.find(prefix)? + prefix.len();
        n[i..].trim_start().chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()
    };
    match vendor {
        Vendor::Nvidia => {
            if n.contains("rtx") {
                (Family::NvidiaRtx, Tier::Modern, vec![])
            } else if let Some(g) = num("gtx") {
                if g >= 900 || (1000..2000).contains(&g) {
                    (Family::NvidiaGtxModern, Tier::Capable, vec![])
                } else {
                    (Family::NvidiaKepler, Tier::Legacy, vec!["Kepler/Fermi: Vulkan only with legacy drivers (≤ 470); OpenGL fallback otherwise.".into()])
                }
            } else if n.contains("quadro") || n.contains("tesla") || n.contains(" a") {
                (Family::NvidiaGtxModern, Tier::Capable, vec![])
            } else {
                (Family::Unknown, Tier::Capable, vec![])
            }
        }
        Vendor::Amd => {
            if let Some(rx) = num("rx ") {
                if rx >= 5000 {
                    (Family::AmdRdna, Tier::Modern, vec![])
                } else {
                    (Family::AmdGcn, Tier::Capable, vec!["Polaris/GCN: Vulkan via RADV (Linux) or Adrenalin (Windows).".into()])
                }
            } else if n.contains("vega") || n.contains("r9 ") || n.contains("r7 ") || n.contains("fury") || n.contains("polaris") {
                (Family::AmdGcn, Tier::Capable, vec![])
            } else if let Some(hd) = num("hd ") {
                if (7700..=8999).contains(&hd) || hd == 7970 || hd == 7950 {
                    (Family::AmdGcn, Tier::Capable, vec!["GCN 1.0: Vulkan on Linux needs `radeon.si_support=0 amdgpu.si_support=1`.".into()])
                } else {
                    (Family::AmdTeraScale, Tier::Legacy, vec!["TeraScale: no Vulkan — OpenGL 3.3 backend, CPU simulation.".into()])
                }
            } else if n.contains("radeon") && (n.contains("graphics") || n.contains("780m") || n.contains("680m")) {
                (Family::AmdRdna, Tier::Capable, vec!["Integrated Radeon (APU).".into()])
            } else if n.contains("navi") || n.contains("rdna") {
                (Family::AmdRdna, Tier::Modern, vec![])
            } else {
                (Family::Unknown, Tier::Capable, vec![])
            }
        }
        Vendor::Intel => {
            if n.contains("arc") || n.contains("xe") {
                (Family::IntelXeArc, Tier::Capable, vec![])
            } else {
                (Family::IntelLegacy, Tier::Legacy, vec!["Older Intel HD: reduced viewport detail by default.".into()])
            }
        }
        // iPad A9–A11 (2015–2017) are slower; A12+ and M-series are fine.
        Vendor::Apple => match num("apple a") {
            Some(a) if a < 12 => (Family::AppleSilicon, Tier::Capable, vec![]),
            _ => (Family::AppleSilicon, Tier::Modern, vec![]),
        },
        Vendor::Qualcomm => match num("adreno (tm)").or_else(|| num("adreno")) {
            Some(a) if a < 600 => (Family::Adreno, Tier::Legacy, vec!["Adreno 5xx or older: reduced viewport detail.".into()]),
            _ => (Family::Adreno, Tier::Capable, vec![]),
        },
        Vendor::Arm => {
            if n.contains("immortalis") || num("mali-g").is_some_and(|g| g >= 52) {
                (Family::Mali, Tier::Capable, vec!["Mali: if the 3D view glitches, choose Settings › Graphics › OpenGL ES.".into()])
            } else {
                (Family::Mali, Tier::Legacy, vec!["Older Mali (T-series / G31): OpenGL ES, reduced viewport detail.".into()])
            }
        }
        Vendor::Samsung => (Family::Xclipse, Tier::Capable, vec![]),
        Vendor::ImgTec => (Family::PowerVr, Tier::Legacy, vec!["PowerVR: OpenGL ES recommended (Settings › Graphics).".into()]),
        Vendor::Software => (Family::Unknown, Tier::None, vec!["Software renderer: install the GPU driver for usable 3D.".into()]),
        _ => (Family::Unknown, Tier::Capable, vec![]),
    }
}

/// Recommended defaults for a tier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PerfDefaults {
    pub sim_cell_mm: f64,
    pub viewport_lod: usize,
    pub gpu_compute: bool,
}

pub fn defaults_for(t: Tier) -> PerfDefaults {
    match t {
        Tier::Modern => PerfDefaults { sim_cell_mm: 0.08, viewport_lod: 1, gpu_compute: true },
        Tier::Capable => PerfDefaults { sim_cell_mm: 0.15, viewport_lod: 2, gpu_compute: true },
        Tier::Legacy => PerfDefaults { sim_cell_mm: 0.3, viewport_lod: 3, gpu_compute: false },
        Tier::None => PerfDefaults { sim_cell_mm: 0.5, viewport_lod: 4, gpu_compute: false },
    }
}

// ------------------------------------------------------------------------------------------------
// Rendering safety: adapter choice, CPU (software) fallback and buffer budgets

/// How the user wants the viewport rendered (Settings › Graphics, or `EZYX_RENDER=auto|gpu|cpu`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RenderMode {
    /// Best compatible GPU; software rendering only when no GPU is usable.
    #[default]
    Auto,
    /// Always a hardware GPU, even one with low limits (warnings only).
    Gpu,
    /// Software rasteriser (Mesa llvmpipe/lavapipe, Microsoft WARP): slow but always works.
    Cpu,
}

impl RenderMode {
    pub fn from_env() -> Option<Self> {
        match std::env::var("EZYX_RENDER").ok()?.to_ascii_lowercase().as_str() {
            "auto" => Some(RenderMode::Auto),
            "gpu" => Some(RenderMode::Gpu),
            "cpu" | "software" => Some(RenderMode::Cpu),
            _ => None,
        }
    }
}

/// Graphics API preference (Settings › Graphics). `EZYX_GPU_BACKEND` / `WGPU_BACKEND` win over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GpuBackend {
    /// Every API this platform has; the best adapter wins.
    #[default]
    Auto,
    Vulkan,
    Metal,
    Dx12,
    /// OpenGL / OpenGL ES — the compatibility choice (old GPUs, buggy mobile Vulkan drivers).
    Gl,
}

impl GpuBackend {
    pub fn label(self) -> &'static str {
        match self {
            GpuBackend::Auto => "Auto",
            GpuBackend::Vulkan => "Vulkan",
            GpuBackend::Metal => "Metal",
            GpuBackend::Dx12 => "DirectX 12",
            GpuBackend::Gl => {
                if cfg!(any(target_os = "android", target_os = "ios")) {
                    "OpenGL ES"
                } else {
                    "OpenGL"
                }
            }
        }
    }

    /// The choices that make sense on the platform this was built for.
    pub fn choices() -> &'static [GpuBackend] {
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            &[GpuBackend::Auto, GpuBackend::Metal]
        } else if cfg!(target_os = "windows") {
            &[GpuBackend::Auto, GpuBackend::Dx12, GpuBackend::Vulkan, GpuBackend::Gl]
        } else {
            &[GpuBackend::Auto, GpuBackend::Vulkan, GpuBackend::Gl]
        }
    }

    /// wgpu backends to enumerate (`None` = wgpu's default set).
    #[cfg(feature = "wgpu")]
    pub fn backends(self) -> Option<wgpu::Backends> {
        match self {
            GpuBackend::Auto => None,
            GpuBackend::Vulkan => Some(wgpu::Backends::VULKAN),
            GpuBackend::Metal => Some(wgpu::Backends::METAL),
            GpuBackend::Dx12 => Some(wgpu::Backends::DX12),
            GpuBackend::Gl => Some(wgpu::Backends::GL),
        }
    }
}

/// What we know about an adapter before creating a device (filled from `wgpu` in [`facts`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterFacts {
    pub name: String,
    pub vendor: Vendor,
    /// `true` for software rasterisers (`wgpu::DeviceType::Cpu`).
    pub software: bool,
    pub discrete: bool,
    pub backend: String,
    pub max_buffer_size: u64,
    pub max_texture_2d: u32,
    pub compute: bool,
    /// Can present to the app's window (`None` = unknown).
    pub surface_ok: Option<bool>,
}

/// Smallest limits the app needs for a usable viewport.
pub const MIN_BUFFER_BYTES: u64 = 64 << 20;
pub const MIN_TEXTURE_2D: u32 = 4096;

/// Why an adapter can't be used for the viewport (`None` = fine).
pub fn incompatibility(f: &AdapterFacts) -> Option<String> {
    if f.surface_ok == Some(false) {
        return Some(format!("{} can't present to this window", f.name));
    }
    if f.max_buffer_size < MIN_BUFFER_BYTES {
        return Some(format!("{}: buffers limited to {} MiB (need {} MiB)", f.name, f.max_buffer_size >> 20, MIN_BUFFER_BYTES >> 20));
    }
    if f.max_texture_2d < MIN_TEXTURE_2D {
        return Some(format!("{}: textures limited to {} px (need {MIN_TEXTURE_2D})", f.name, f.max_texture_2d));
    }
    None
}

/// The adapter to render with, and what the user should be told about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterChoice {
    pub index: usize,
    pub software: bool,
    pub warnings: Vec<String>,
}

/// Pick the rendering adapter. Hardware GPUs that meet the minimum limits win (best tier, then
/// discrete); if none does — no GPU, an incompatible one, or the driver only exposes a software
/// rasteriser — fall back to the CPU renderer with a warning. Pure, unit-tested.
pub fn choose_adapter(adapters: &[AdapterFacts], mode: RenderMode) -> Option<AdapterChoice> {
    let tier = |f: &AdapterFacts| classify(f.vendor, &f.name).1;
    let score = |f: &AdapterFacts| (tier(f), f.backend != "Gl", f.discrete, f.compute);
    let hardware: Vec<usize> = (0..adapters.len()).filter(|i| !adapters[*i].software).collect();
    let software: Vec<usize> = (0..adapters.len()).filter(|i| adapters[*i].software && adapters[*i].surface_ok != Some(false)).collect();
    let best = |ids: &[usize]| ids.iter().copied().max_by(|a, b| score(&adapters[*a]).cmp(&score(&adapters[*b])));
    let mut warnings = Vec::new();
    let pick_soft = |mut warnings: Vec<String>| -> Option<AdapterChoice> {
        let i = best(&software)?;
        warnings.push(format!("Using CPU (software) rendering on {}: the 3D view is slower and previews are simplified.", adapters[i].name));
        Some(AdapterChoice { index: i, software: true, warnings })
    };
    if mode == RenderMode::Cpu {
        warnings.push("CPU rendering is selected (Settings › Graphics or EZYX_RENDER=cpu).".into());
        return pick_soft(warnings.clone()).or_else(|| {
            // No software rasteriser installed: run on the GPU anyway rather than not at all.
            let i = best(&hardware)?;
            Some(AdapterChoice {
                index: i,
                software: false,
                warnings: vec![format!("No software renderer found (install Mesa llvmpipe/lavapipe); using {}.", adapters[i].name)],
            })
        });
    }
    let ok: Vec<usize> = hardware.iter().copied().filter(|i| incompatibility(&adapters[*i]).is_none()).collect();
    for i in &hardware {
        if let Some(why) = incompatibility(&adapters[*i]) {
            warnings.push(format!("Skipping GPU {why}."));
        }
    }
    let chosen = match (best(&ok), mode) {
        (Some(i), _) => Some(i),
        // Forced GPU: take the best hardware adapter even if it is under the limits.
        (None, RenderMode::Gpu) => best(&hardware.iter().copied().filter(|i| adapters[*i].surface_ok != Some(false)).collect::<Vec<_>>()),
        (None, _) => None,
    };
    match chosen {
        Some(i) => {
            let f = &adapters[i];
            if f.backend == "Gl" && tier(f) > Tier::Legacy {
                warnings.push(format!("{}: the driver only offers OpenGL — install the vendor's Vulkan/DirectX driver for full speed.", f.name));
            }
            if incompatibility(f).is_some() {
                warnings.push(format!("{} is below the recommended limits; previews are simplified.", f.name));
            }
            Some(AdapterChoice { index: i, software: false, warnings })
        }
        None => {
            if hardware.is_empty() {
                warnings.push("No GPU driver found (the system fell back to a software renderer).".into());
            }
            pick_soft(warnings)
        }
    }
}

/// Memory budgets that keep the CPU-tessellated viewport inside the GPU's buffer limit (egui
/// uploads every frame's triangles into one vertex buffer: 3 × 20-byte vertices per triangle).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RenderBudget {
    /// Largest single GPU buffer (bytes) on the device.
    pub max_buffer_bytes: u64,
    /// Vertices one window may emit per frame for meshes, lines and toolpaths together (egui
    /// vertices are 20 bytes; 30 % of the buffer is left for the UI itself).
    pub frame_vertices: usize,
    /// Triangles one window may draw per frame (all meshes together).
    pub frame_triangles: usize,
    /// Triangles for the simulated-result preview mesh.
    pub preview_triangles: usize,
    /// Triangles for any other single mesh (models, reliefs, parts).
    pub mesh_triangles: usize,
    /// Z-map cells the material-removal simulation may allocate.
    pub sim_max_cells: usize,
}

/// Vertex-buffer bytes per displayed triangle (3 unshared 20-byte egui vertices).
pub const BYTES_PER_TRIANGLE: u64 = 60;
/// Bytes per egui vertex.
pub const BYTES_PER_VERTEX: u64 = 20;
/// Vertices egui needs for one anti-aliased line segment (conservative).
pub const VERTICES_PER_SEGMENT: usize = 6;

pub fn render_budget(max_buffer_bytes: u64, software: bool, tier: Tier) -> RenderBudget {
    let frame_vertices = ((max_buffer_bytes * 7 / 10 / BYTES_PER_VERTEX) as usize).max(30_000);
    // Meshes may use half of the vertex budget, lines and toolpaths the rest.
    let by_buffer = frame_vertices / 2 / 3;
    // CPU tessellation (and software rasterising) cost also caps what's sensible.
    let by_speed = match (software, tier) {
        (true, _) | (_, Tier::None) => 400_000,
        (_, Tier::Legacy) => 1_000_000,
        (_, Tier::Capable) => 2_500_000,
        (_, Tier::Modern) => 4_000_000,
    };
    let frame = by_buffer.min(by_speed).max(5_000);
    let sim_max_cells = match (software, tier) {
        (true, _) | (_, Tier::None) => 8_000_000,
        (_, Tier::Legacy) => 16_000_000,
        _ => 40_000_000,
    };
    RenderBudget { max_buffer_bytes, frame_vertices, frame_triangles: frame, preview_triangles: frame * 6 / 10, mesh_triangles: frame * 4 / 10, sim_max_cells }
}

/// Ceiling for phones and tablets: the viewport is tessellated by a phone CPU and GPU memory is
/// shared with the OS (the app is killed, not warned, when it takes too much).
pub const MOBILE_FRAME_TRIANGLES: usize = 900_000;
pub const MOBILE_SIM_CELLS: usize = 12_000_000;

impl RenderBudget {
    /// Cap the budget for a phone/tablet (never raises it).
    pub fn mobile(self) -> Self {
        let f = self.frame_triangles.min(MOBILE_FRAME_TRIANGLES);
        RenderBudget {
            frame_vertices: self.frame_vertices.min(MOBILE_FRAME_TRIANGLES * 3 * 2),
            frame_triangles: f,
            preview_triangles: f * 6 / 10,
            mesh_triangles: f * 4 / 10,
            sim_max_cells: self.sim_max_cells.min(MOBILE_SIM_CELLS),
            ..self
        }
    }

    /// After an out-of-memory / oversize-buffer error: halve everything (never below a floor).
    pub fn reduced(self) -> Self {
        let f = (self.frame_triangles / 2).max(5_000);
        RenderBudget {
            frame_vertices: (self.frame_vertices / 2).max(30_000),
            frame_triangles: f,
            preview_triangles: f * 6 / 10,
            mesh_triangles: f * 4 / 10,
            sim_max_cells: (self.sim_max_cells / 2).max(2_000_000),
            ..self
        }
    }
}

/// Facts about every adapter wgpu can see (used by the app's adapter selector).
#[cfg(feature = "wgpu")]
pub fn facts(adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>) -> Vec<AdapterFacts> {
    adapters
        .iter()
        .map(|a| {
            let info = a.get_info();
            let limits = a.limits();
            let software = info.device_type == wgpu::DeviceType::Cpu || info.name.to_ascii_lowercase().contains("llvmpipe");
            AdapterFacts {
                name: info.name.clone(),
                vendor: if software { Vendor::Software } else { Vendor::detect(info.vendor, &info.name) },
                software,
                discrete: info.device_type == wgpu::DeviceType::DiscreteGpu,
                backend: format!("{:?}", info.backend),
                max_buffer_size: limits.max_buffer_size,
                max_texture_2d: limits.max_texture_dimension_2d,
                compute: a.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::COMPUTE_SHADERS),
                surface_ok: surface.map(|s| a.is_surface_supported(s)),
            }
        })
        .collect()
}

/// Map `EZYX_GPU_BACKEND` to wgpu's `WGPU_BACKEND` before the window is created.
pub fn apply_backend_override() -> Option<String> {
    let v = std::env::var("EZYX_GPU_BACKEND").ok()?;
    let mapped = match v.to_ascii_lowercase().as_str() {
        "vulkan" | "vk" => "vulkan",
        "dx12" | "d3d12" => "dx12",
        "metal" => "metal",
        "gl" | "opengl" | "gles" => "gl",
        _ => return None,
    };
    // SAFETY: called at startup before any other threads are spawned.
    unsafe { std::env::set_var("WGPU_BACKEND", mapped) };
    Some(mapped.into())
}

/// Enumerate all adapters on all backends and classify them, best first.
#[cfg(feature = "wgpu")]
pub fn probe() -> Vec<GpuInfo> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
    let mut out: Vec<GpuInfo> = adapters
        .into_iter()
        .map(|a| {
            let info = a.get_info();
            let vendor = if info.device_type == wgpu::DeviceType::Cpu { Vendor::Software } else { Vendor::detect(info.vendor, &info.name) };
            let (family, mut tier, mut notes) = classify(vendor, &info.name);
            if info.backend == wgpu::Backend::Gl && tier > Tier::Legacy {
                notes.push("Only exposed through OpenGL on this system — check drivers for Vulkan/DX12.".into());
                tier = Tier::Legacy;
            }
            let dl = a.get_downlevel_capabilities();
            if !dl.flags.contains(wgpu::DownlevelFlags::COMPUTE_SHADERS) {
                notes.push("No compute shaders: CPU simulation path.".into());
                tier = tier.min(Tier::Legacy);
            }
            GpuInfo {
                name: info.name,
                vendor,
                family,
                tier,
                backend: format!("{:?}", info.backend),
                device_type: format!("{:?}", info.device_type),
                driver: format!("{} {}", info.driver, info.driver_info).trim().to_string(),
                notes,
            }
        })
        .collect();
    out.sort_by(|a, b| b.tier.cmp(&a.tier).then_with(|| (a.device_type != "DiscreteGpu").cmp(&(b.device_type != "DiscreteGpu"))));
    out
}

#[cfg(not(feature = "wgpu"))]
pub fn probe() -> Vec<GpuInfo> {
    vec![]
}

pub fn best() -> Option<GpuInfo> {
    probe().into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classification() {
        assert_eq!(classify(Vendor::Nvidia, "NVIDIA GeForce RTX 4070").1, Tier::Modern);
        assert_eq!(classify(Vendor::Nvidia, "NVIDIA GeForce GTX 1060 6GB").0, Family::NvidiaGtxModern);
        assert_eq!(classify(Vendor::Nvidia, "GeForce GTX 760").1, Tier::Legacy);
        assert_eq!(classify(Vendor::Amd, "AMD Radeon RX 7900 XTX").0, Family::AmdRdna);
        assert_eq!(classify(Vendor::Amd, "Radeon RX 580 Series").0, Family::AmdGcn);
        assert_eq!(classify(Vendor::Amd, "AMD Radeon HD 7970").0, Family::AmdGcn);
        assert_eq!(classify(Vendor::Amd, "AMD Radeon HD 6870").0, Family::AmdTeraScale);
        assert_eq!(classify(Vendor::Amd, "AMD Radeon R9 290").0, Family::AmdGcn);
        assert_eq!(classify(Vendor::Intel, "Intel(R) Arc(TM) A770").0, Family::IntelXeArc);
        assert_eq!(classify(Vendor::Apple, "Apple M2").1, Tier::Modern);
    }

    #[test]
    fn mobile_gpus() {
        // Metal on iPad and many Android GLES drivers report no PCI vendor id.
        assert_eq!(Vendor::detect(0, "Apple A15 GPU"), Vendor::Apple);
        assert_eq!(Vendor::detect(0, "Adreno (TM) 740"), Vendor::Qualcomm);
        assert_eq!(Vendor::detect(0x13B5, "Mali-G78"), Vendor::Arm);
        assert_eq!(Vendor::detect(0, "Mali-G610 MC6"), Vendor::Arm);
        assert_eq!(Vendor::detect(0x1AE0, "SwiftShader Device (Subzero)"), Vendor::Software);
        assert_eq!(Vendor::detect(0, "Google SwiftShader"), Vendor::Software);
        assert_eq!(Vendor::detect(0x10DE, "anything"), Vendor::Nvidia);
        assert!(Vendor::Qualcomm.is_mobile() && !Vendor::Apple.is_mobile());

        assert_eq!(classify(Vendor::Apple, "Apple A15 GPU").1, Tier::Modern);
        assert_eq!(classify(Vendor::Apple, "Apple A10X GPU").1, Tier::Capable);
        assert_eq!(classify(Vendor::Qualcomm, "Adreno (TM) 740"), (Family::Adreno, Tier::Capable, vec![]));
        assert_eq!(classify(Vendor::Qualcomm, "Adreno (TM) 530").1, Tier::Legacy);
        assert_eq!(classify(Vendor::Arm, "Mali-G78").1, Tier::Capable);
        assert_eq!(classify(Vendor::Arm, "Mali-G31").1, Tier::Legacy);
        assert_eq!(classify(Vendor::Arm, "Mali-T880").1, Tier::Legacy);
        assert_eq!(classify(Vendor::Arm, "Immortalis-G720").1, Tier::Capable);
        assert_eq!(classify(Vendor::ImgTec, "PowerVR Rogue GE8320").1, Tier::Legacy);
    }

    #[test]
    fn mobile_budget_caps() {
        // A 1 GiB-buffer iPad still tessellates at most 900 k triangles per frame and keeps the
        // simulation under 60 MB.
        let b = render_budget(1 << 30, false, Tier::Modern).mobile();
        assert_eq!(b.frame_triangles, MOBILE_FRAME_TRIANGLES);
        assert!(b.preview_triangles + b.mesh_triangles <= b.frame_triangles);
        assert!(b.frame_triangles * 3 <= b.frame_vertices / 2);
        assert!(b.sim_max_cells * 5 <= 60_000_000);
        // Never raises a smaller budget.
        let small = render_budget(64 << 20, false, Tier::Legacy);
        assert_eq!(small.mobile().frame_triangles, small.frame_triangles);
        assert_eq!(small.mobile().frame_vertices, small.frame_vertices);
    }
    fn hw(name: &str, vendor: Vendor, backend: &str, buf_mib: u64) -> AdapterFacts {
        AdapterFacts {
            name: name.into(),
            vendor,
            software: false,
            discrete: true,
            backend: backend.into(),
            max_buffer_size: buf_mib << 20,
            max_texture_2d: 16384,
            compute: true,
            surface_ok: Some(true),
        }
    }
    fn llvmpipe() -> AdapterFacts {
        AdapterFacts {
            name: "llvmpipe (LLVM 17, 256 bits)".into(),
            vendor: Vendor::Software,
            software: true,
            discrete: false,
            ..hw("", Vendor::Software, "Vulkan", 2048)
        }
    }

    #[test]
    fn adapter_choice_and_fallbacks() {
        // Best compatible GPU wins over the software renderer and a weaker GPU.
        let a = [llvmpipe(), hw("Intel(R) UHD Graphics 620", Vendor::Intel, "Vulkan", 2048), hw("NVIDIA GeForce RTX 3060", Vendor::Nvidia, "Vulkan", 4096)];
        let c = choose_adapter(&a, RenderMode::Auto).unwrap();
        assert_eq!((c.index, c.software), (2, false));
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        // A GPU with tiny buffers is incompatible → CPU fallback with an explanation.
        let a = [hw("Old GPU", Vendor::Other, "Gl", 16), llvmpipe()];
        let c = choose_adapter(&a, RenderMode::Auto).unwrap();
        assert!(c.software && c.index == 1);
        assert!(c.warnings.iter().any(|w| w.contains("16 MiB")) && c.warnings.iter().any(|w| w.contains("CPU (software) rendering")), "{:?}", c.warnings);
        // …unless the user forces the GPU.
        let c = choose_adapter(&a, RenderMode::Gpu).unwrap();
        assert!(!c.software && c.index == 0 && c.warnings.iter().any(|w| w.contains("below the recommended")));
        // Driver fell back to software only.
        let c = choose_adapter(&[llvmpipe()], RenderMode::Auto).unwrap();
        assert!(c.software && c.warnings.iter().any(|w| w.contains("No GPU driver")));
        // Modern GPU exposed only through OpenGL: usable, but the driver is flagged.
        let c = choose_adapter(&[hw("AMD Radeon RX 6800", Vendor::Amd, "Gl", 2048)], RenderMode::Auto).unwrap();
        assert!(c.warnings.iter().any(|w| w.contains("only offers OpenGL")));
        // CPU mode picks the software renderer even when a GPU exists.
        let a = [hw("NVIDIA GeForce RTX 3060", Vendor::Nvidia, "Vulkan", 4096), llvmpipe()];
        assert!(choose_adapter(&a, RenderMode::Cpu).unwrap().software);
        assert!(choose_adapter(&[], RenderMode::Auto).is_none());
    }

    #[test]
    fn budgets_fit_the_buffer_limit() {
        // The reported crash: a 321 MB vertex buffer on a device limited to 256 MiB.
        let b = render_budget(256 << 20, false, Tier::Modern);
        // Everything we draw stays within 70 % of the buffer; meshes within half of that.
        assert!(b.frame_vertices as u64 * BYTES_PER_VERTEX <= (256 << 20) * 7 / 10);
        assert!(b.frame_triangles * 3 <= b.frame_vertices / 2);
        assert!(b.preview_triangles + b.mesh_triangles <= b.frame_triangles);
        assert_eq!(b.frame_triangles, 1_565_873);
        // A tiny (8 MiB) device still gets a usable, safe budget.
        let t = render_budget(8 << 20, false, Tier::Capable);
        assert!(t.frame_vertices as u64 * BYTES_PER_VERTEX <= 8 << 20);
        let s = render_budget(2 << 30, true, Tier::None);
        assert_eq!(s.frame_triangles, 400_000);
        let r = b.reduced();
        assert_eq!(r.frame_triangles, b.frame_triangles / 2);
        assert!(r.sim_max_cells < b.sim_max_cells);
    }

    #[test]
    fn probe_does_not_panic() {
        // Headless CI may have zero adapters; just make sure enumeration is safe.
        let _ = probe();
    }
}
