//! Image → 3D mesh.
//!
//! [`Backend`] is the plug-in point. Shipped backends:
//! * [`InflateBackend`] – offline, instant: silhouette inflation + luminance emboss
//!   (see `ezyx_image::inflate`). Good for toys, ornaments, cookie-cutter carvings.
//!
//! Planned/optional backends (implemented outside this crate so it stays dependency-light):
//! * Remote API (`ezyx-project::img3d_remote`) – POST the image to a user-configured endpoint
//!   (Tripo3D, Meshy, a Hugging Face Space, or a self-hosted TripoSR/TRELLIS server) and receive
//!   GLB/OBJ/STL. Requires the user's own API key.
//! * Local ONNX (feature-gated) – run an open single-image reconstruction model on the GPU
//!   detected by `ezyx-gpu`.

use crate::{mt, TriMesh};
use ezyx_image::{inflate::InflateOptions, Bitmap, GrayImage};
use glam::Vec3;

pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    fn generate(&self, img: &image_input::ImageInput) -> Result<TriMesh, String>;
}

pub mod image_input {
    use super::*;
    /// What a backend receives: greyscale for classical methods, raw file bytes for remote ones.
    pub struct ImageInput {
        pub gray: GrayImage,
        pub silhouette: Bitmap,
        pub file_bytes: Vec<u8>,
        pub width_mm: f64,
    }
}

pub struct InflateBackend {
    pub options: InflateOptions,
}

impl Backend for InflateBackend {
    fn name(&self) -> &str {
        "Inflate (offline)"
    }
    fn generate(&self, input: &image_input::ImageInput) -> Result<TriMesh, String> {
        let mut o = self.options.clone();
        o.width_mm = input.width_mm;
        Ok(inflated_mesh(&input.gray, &input.silhouette, &o))
    }
}

/// Build a watertight mesh from the inflated front/back heightmaps via marching tetrahedra on
/// the implicit volume `back(x,y) ≤ z ≤ front(x,y)` inside the silhouette.
pub fn inflated_mesh(gray: &GrayImage, silhouette: &Bitmap, opt: &InflateOptions) -> TriMesh {
    let inf = ezyx_image::inflate::inflate(gray, silhouette, opt);
    let (front, back) = (&inf.front, &inf.back);
    let b = front.bounds();
    let h = opt.resolution as f32;
    let zmin = back.min_max().0 - h;
    let zmax = front.min_max().1 + h;
    let g = mt::Grid::sample(Vec3::new(b.min.x as f32, b.min.y as f32, zmin), Vec3::new(b.max.x as f32, b.max.y as f32, zmax), h, |p| {
        let q = ezyx_core::DVec2::new(p.x as f64, p.y as f64);
        let gp = front.to_grid(q);
        let (gx, gy) = (gp.x.round() as isize, gp.y.round() as isize);
        if gx < 0 || gy < 0 || gx >= front.width as isize || gy >= front.height as isize {
            return h;
        }
        let i = gy as usize * front.width + gx as usize;
        if !inf.inside[i] {
            return h;
        }
        let f = front.sample(q);
        let bk = back.sample(q);
        (bk - p.z).max(p.z - f)
    });
    let mut m = mt::extract(&g, 0.0);
    m.smooth_taubin(4, 0.5, -0.53);
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disc_to_closed_mesh() {
        let n = 48;
        let mut g = GrayImage::new(n, n, 1.0);
        let mut b = Bitmap::new(n, n);
        for y in 0..n {
            for x in 0..n {
                if ((x as f32 - 24.0).powi(2) + (y as f32 - 24.0).powi(2)).sqrt() < 18.0 {
                    b.set(x, y, true);
                    g.data[y * n + x] = 0.4;
                }
            }
        }
        let m = inflated_mesh(&g, &b, &InflateOptions { width_mm: 48.0, resolution: 1.0, thickness: 6.0, ..Default::default() });
        assert!(!m.is_empty());
        assert_eq!(m.boundary_edge_count(), 0);
        assert!(m.volume() > 0.0);
    }
}
