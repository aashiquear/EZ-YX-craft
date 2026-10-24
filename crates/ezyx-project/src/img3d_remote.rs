//! Remote image→3D backend (bring-your-own service).
//!
//! Protocol (deliberately minimal so any service can be adapted with a ~30-line proxy):
//! * `POST {endpoint}` with the raw image bytes, `Content-Type: image/png|image/jpeg`,
//!   optional `Authorization: Bearer <key>` (key read from the env var named in the config).
//! * Response body: a mesh as binary/ASCII STL or OBJ (detected from content).
//!
//! This lets users connect commercial services (Tripo3D, Meshy, …) through their own accounts, or
//! a self-hosted open model (TripoSR, TRELLIS, Hunyuan3D) running on their GPU box.

use ezyx_mesh::{
    img3d::{image_input::ImageInput, Backend},
    TriMesh,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteConfig {
    pub name: String,
    pub endpoint: String,
    pub api_key_env: String,
    pub timeout_s: u64,
}

pub struct RemoteBackend {
    pub cfg: RemoteConfig,
}

impl Backend for RemoteBackend {
    fn name(&self) -> &str {
        &self.cfg.name
    }
    fn generate(&self, input: &ImageInput) -> Result<TriMesh, String> {
        let agent: ureq::Agent =
            ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(self.cfg.timeout_s.max(10)))).build().into();
        let ct = if input.file_bytes.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else { "image/jpeg" };
        let mut req = agent.post(&self.cfg.endpoint).header("content-type", ct);
        if let Ok(k) = std::env::var(&self.cfg.api_key_env) {
            req = req.header("authorization", &format!("Bearer {k}"));
        }
        let mut resp = req.send(&input.file_bytes[..]).map_err(|e| e.to_string())?;
        if resp.status().as_u16() >= 400 {
            return Err(format!("service returned {}", resp.status()));
        }
        let bytes = resp.body_mut().with_config().limit(512 * 1024 * 1024).read_to_vec().map_err(|e| e.to_string())?;
        let text_head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]).to_string();
        let mut mesh = if text_head.lines().any(|l| l.starts_with("v ")) {
            ezyx_mesh::io::read_obj(&String::from_utf8_lossy(&bytes)).map_err(|e| e.to_string())?
        } else {
            ezyx_mesh::io::read_stl(&bytes).map_err(|e| e.to_string())?
        };
        // Scale to requested width.
        let (lo, hi) = mesh.bounds();
        let w = (hi.x - lo.x).max(1e-6);
        let s = input.width_mm as f32 / w;
        mesh.transform(&glam::Mat4::from_scale(glam::Vec3::splat(s)));
        Ok(mesh)
    }
}
