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
//!
//! *Tier* drives defaults: simulation resolution, viewport LOD, and whether compute shaders are
//! used for drop-cutter/simulation (Modern/Capable) or the rayon CPU path (Legacy/None).
//! Users can force a backend with `EZYX_GPU_BACKEND=vulkan|dx12|metal|gl` (mapped to `WGPU_BACKEND`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Qualcomm,
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
            0x10005 | 0x1414 => Vendor::Software, // Mesa llvmpipe / Microsoft WARP
            _ => Vendor::Other,
        }
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
        Vendor::Apple => (Family::AppleSilicon, Tier::Modern, vec![]),
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
            let vendor = if info.device_type == wgpu::DeviceType::Cpu { Vendor::Software } else { Vendor::from_pci(info.vendor) };
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
    fn probe_does_not_panic() {
        // Headless CI may have zero adapters; just make sure enumeration is safe.
        let _ = probe();
    }
}
