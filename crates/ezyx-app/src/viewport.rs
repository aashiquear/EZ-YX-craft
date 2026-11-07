//! 3D/2D viewport: camera, navigation (orbit / pan / zoom-to-cursor / view presets),
//! projection helpers and CPU tessellation onto egui's painter.
//!
//! Rendering note: v0.1 projects geometry on the CPU and paints it through egui (works on every
//! backend incl. OpenGL-only TeraScale GPUs). Triangles are depth-sorted (painter's algorithm)
//! with back-face culling, inside the per-frame triangle budget of `gpu_guard`. The planned upgrade is an `egui_wgpu` paint callback with a real depth
//! buffer and GPU-resident meshes — the `Projector`/`Camera` API stays the same.

use egui::{epaint::Mesh, Color32, Painter, Pos2, Rect, Response, Shape, Stroke, Ui};
use ezyx_mesh::TriMesh;
use glam::{Mat4, Vec3, Vec4};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub ortho: bool,
    pub fov: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewPreset {
    Top,
    Front,
    Right,
    Iso,
}

impl Default for Camera {
    fn default() -> Self {
        Self { target: Vec3::new(150.0, 100.0, 0.0), yaw: -90f32.to_radians(), pitch: 90f32.to_radians(), dist: 400.0, ortho: true, fov: 40f32.to_radians() }
    }
}

impl Camera {
    pub fn dir(&self) -> Vec3 {
        Vec3::new(self.pitch.cos() * self.yaw.cos(), self.pitch.cos() * self.yaw.sin(), self.pitch.sin())
    }
    pub fn eye(&self) -> Vec3 {
        self.target + self.dir() * self.dist
    }
    pub fn up(&self) -> Vec3 {
        Vec3::new(-self.pitch.sin() * self.yaw.cos(), -self.pitch.sin() * self.yaw.sin(), self.pitch.cos())
    }
    pub fn view(&self) -> Mat4 {
        look_at_rh(self.eye(), self.target, self.up())
    }
    pub fn proj(&self, aspect: f32) -> Mat4 {
        if self.ortho {
            let h = self.dist * (self.fov * 0.5).tan();
            orthographic_rh(-h * aspect, h * aspect, -h, h, -self.dist * 50.0, self.dist * 50.0)
        } else {
            perspective_rh(self.fov, aspect, (self.dist * 0.01).max(0.05), self.dist * 100.0)
        }
    }
    pub fn preset(&mut self, p: ViewPreset) {
        let (yaw, pitch) = match p {
            ViewPreset::Top => (-90.0, 90.0),
            ViewPreset::Front => (-90.0, 0.0),
            ViewPreset::Right => (0.0, 0.0),
            ViewPreset::Iso => (-60.0, 35.0),
        };
        self.yaw = f32::to_radians(yaw);
        self.pitch = f32::to_radians(pitch);
    }
    /// Frame an axis-aligned box.
    pub fn fit(&mut self, lo: Vec3, hi: Vec3) {
        self.target = (lo + hi) * 0.5;
        let r = (hi - lo).length().max(10.0) * 0.5;
        self.dist = r / (self.fov * 0.5).tan() * 1.15;
    }
}

/// Right-handed view matrix (camera looks down -Z in view space).
pub fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let f = (target - eye).normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);
    Mat4::from_cols(
        Vec4::new(s.x, u.x, -f.x, 0.0),
        Vec4::new(s.y, u.y, -f.y, 0.0),
        Vec4::new(s.z, u.z, -f.z, 0.0),
        Vec4::new(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
    )
}

/// Right-handed perspective, depth mapped to [0, 1].
pub fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let h = 1.0 / (fov_y * 0.5).tan();
    let w = h / aspect;
    let r = far / (near - far);
    Mat4::from_cols(Vec4::new(w, 0.0, 0.0, 0.0), Vec4::new(0.0, h, 0.0, 0.0), Vec4::new(0.0, 0.0, r, -1.0), Vec4::new(0.0, 0.0, r * near, 0.0))
}

/// Right-handed orthographic, depth mapped to [0, 1].
pub fn orthographic_rh(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Mat4 {
    let rw = 1.0 / (right - left);
    let rh = 1.0 / (top - bottom);
    let r = 1.0 / (near - far);
    Mat4::from_cols(
        Vec4::new(2.0 * rw, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 2.0 * rh, 0.0, 0.0),
        Vec4::new(0.0, 0.0, r, 0.0),
        Vec4::new(-(left + right) * rw, -(top + bottom) * rh, r * near, 1.0),
    )
}

pub struct Projector {
    pub vp: Mat4,
    pub inv: Mat4,
    pub rect: Rect,
    pub cam: Camera,
}

impl Projector {
    pub fn new(cam: &Camera, rect: Rect) -> Self {
        let aspect = rect.width() / rect.height().max(1.0);
        let vp = cam.proj(aspect) * cam.view();
        Self { vp, inv: vp.inverse(), rect, cam: *cam }
    }
    /// World → screen; depth in 0..1 (0 = near).
    pub fn project(&self, p: Vec3) -> Option<(Pos2, f32)> {
        let c = self.vp * p.extend(1.0);
        if c.w <= 1e-6 {
            return None;
        }
        let n = c.truncate() / c.w;
        if n.z < -0.01 || n.z > 1.01 {
            return None;
        }
        let s = Pos2::new(self.rect.center().x + n.x * self.rect.width() * 0.5, self.rect.center().y - n.y * self.rect.height() * 0.5);
        Some((s, n.z))
    }
    pub fn to_screen(&self, p: Vec3) -> Option<Pos2> {
        self.project(p).map(|(s, _)| s)
    }
    pub fn ray(&self, s: Pos2) -> (Vec3, Vec3) {
        let x = (s.x - self.rect.center().x) / (self.rect.width() * 0.5);
        let y = -(s.y - self.rect.center().y) / (self.rect.height() * 0.5);
        let a = self.inv * Vec4::new(x, y, 0.0, 1.0);
        let b = self.inv * Vec4::new(x, y, 1.0, 1.0);
        let a = a.truncate() / a.w;
        let b = b.truncate() / b.w;
        (a, (b - a).normalize_or_zero())
    }
    /// Intersection of the pick ray with the plane Z = z.
    pub fn ground(&self, s: Pos2, z: f32) -> Option<Vec3> {
        let (o, d) = self.ray(s);
        if d.z.abs() < 1e-6 {
            return None;
        }
        let t = (z - o.z) / d.z;
        Some(o + d * t)
    }
}

/// Navigation. Returns true when the camera changed.
/// * 3D: LMB-drag orbit (unless `lmb_reserved`), RMB/MMB or Shift+LMB pan, wheel/pinch zoom to cursor.
/// * 2D: LMB is reserved for selection/drawing; RMB/MMB pan, wheel zoom.
/// * Touch (tablets, touch screens): one finger acts as LMB, two fingers pan + pinch-zoom, and
///   twisting two fingers turns the 3D view.
pub fn navigate(ui: &Ui, resp: &Response, cam: &mut Camera, rect: Rect, lmb_reserved: bool) -> bool {
    let mut changed = false;
    let shift = ui.input(|i| i.modifiers.shift);
    if let Some(t) = ui.input(|i| i.multi_touch()).filter(|t| rect.contains(t.center_pos)) {
        // Two fingers: pan with the centre, turn with the twist (zoom comes through zoom_delta).
        let pr = Projector::new(cam, rect);
        let z = cam.target.z;
        if let (Some(a), Some(b)) = (pr.ground(t.center_pos - t.translation_delta, z), pr.ground(t.center_pos, z)) {
            cam.target -= b - a;
        }
        if !cam.ortho_locked_2d() {
            cam.yaw -= t.rotation_delta;
        }
        let factor = 1.0 / t.zoom_delta.max(1e-3);
        if (factor - 1.0).abs() > 1e-5 {
            cam.dist = (cam.dist * factor).clamp(1.0, 50_000.0);
        }
        return true;
    }
    let pan_drag = resp.dragged_by(egui::PointerButton::Secondary)
        || resp.dragged_by(egui::PointerButton::Middle)
        || (shift && resp.dragged_by(egui::PointerButton::Primary));
    if pan_drag {
        let d = resp.drag_delta();
        let pr = Projector::new(cam, rect);
        if let Some(p) = resp.interact_pointer_pos() {
            let z = cam.target.z;
            if let (Some(a), Some(b)) = (pr.ground(p - d, z), pr.ground(p, z)) {
                cam.target -= b - a;
                changed = true;
            } else {
                // Looking parallel to the plane: screen-space pan.
                let right = cam.dir().cross(cam.up()).normalize_or_zero();
                let s = cam.dist * 0.0025;
                cam.target += (right * d.x - cam.up() * -d.y) * s;
                changed = true;
            }
        }
    } else if resp.dragged_by(egui::PointerButton::Primary) && !lmb_reserved && !cam.ortho_locked_2d() {
        let d = resp.drag_delta();
        cam.yaw -= d.x * 0.008;
        cam.pitch = (cam.pitch + d.y * 0.008).clamp(-89.5f32.to_radians(), 89.9f32.to_radians());
        changed = true;
    }
    if resp.hovered() {
        let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        let mut factor = (-scroll * 0.0015).exp();
        if (zoom - 1.0).abs() > 1e-4 {
            factor /= zoom;
        }
        if (factor - 1.0).abs() > 1e-5 {
            let pr = Projector::new(cam, rect);
            let cursor = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
            let before = pr.ground(cursor, cam.target.z);
            cam.dist = (cam.dist * factor).clamp(1.0, 50_000.0);
            if let Some(b) = before {
                let pr2 = Projector::new(cam, rect);
                if let Some(a) = pr2.ground(cursor, cam.target.z) {
                    cam.target += b - a;
                }
            }
            changed = true;
        }
    }
    changed
}

impl Camera {
    /// In 2D mode the camera is locked to the top view.
    pub fn ortho_locked_2d(&self) -> bool {
        self.ortho && (self.pitch - 90f32.to_radians()).abs() < 1e-3
    }
}

pub struct Draw<'a> {
    pub painter: &'a Painter,
    pub pr: &'a Projector,
}

impl Draw<'_> {
    /// Line segment inside this window's GPU vertex budget (silently skipped when it's used up).
    pub fn line(&self, a: Vec3, b: Vec3, stroke: Stroke) {
        if crate::gpu_guard::take_segments(self.painter.ctx(), 1) == 1 {
            self.line_reserved(a, b, stroke);
        }
    }
    /// Line segment whose budget the caller already reserved in bulk (`gpu_guard::take_segments`).
    pub fn line_reserved(&self, a: Vec3, b: Vec3, stroke: Stroke) {
        if let (Some(p), Some(q)) = (self.pr.to_screen(a), self.pr.to_screen(b)) {
            self.painter.line_segment([p, q], stroke);
        }
    }
    pub fn polyline(&self, pts: &[Vec3], closed: bool, stroke: Stroke) {
        let mut s: Vec<Pos2> = pts.iter().filter_map(|p| self.pr.to_screen(*p)).collect();
        if s.len() < 2 {
            return;
        }
        if closed {
            s.push(s[0]);
        }
        // Over budget: keep an even subset of the points (the shape stays recognisable).
        let granted = crate::gpu_guard::take_segments(self.painter.ctx(), s.len());
        if granted < s.len() {
            if granted < 2 {
                return;
            }
            let step = s.len() as f64 / granted as f64;
            let last = *s.last().unwrap();
            s = (0..granted).map(|k| s[(k as f64 * step) as usize]).chain(std::iter::once(last)).collect();
        }
        self.painter.add(Shape::line(s, stroke));
    }
    /// Dashed segment whose budget the caller already reserved.
    pub fn dashed_reserved(&self, a: Vec3, b: Vec3, stroke: Stroke) {
        if let (Some(p), Some(q)) = (self.pr.to_screen(a), self.pr.to_screen(b)) {
            // Dashes multiply the segment count: plain thin line when it's long on screen.
            if (q - p).length() > 400.0 {
                self.painter.line_segment([p, q], Stroke::new(stroke.width * 0.6, stroke.color.gamma_multiply(0.6)));
            } else {
                self.painter.extend(Shape::dashed_line(&[p, q], stroke, 4.0, 4.0));
            }
        }
    }

    /// Shaded triangle mesh. `base` is used when the mesh has no vertex colours.
    pub fn mesh(&self, m: &TriMesh, base: [f32; 3], alpha: u8, cull: bool) {
        if m.indices.is_empty() {
            return;
        }
        let proj: Vec<Option<(Pos2, f32)>> = m.positions.iter().map(|p| self.pr.project(*p)).collect();
        let eye_dir = self.pr.cam.dir();
        let key = Vec3::new(-0.4, -0.5, 0.75).normalize();
        let mut tris: Vec<(f32, [Pos2; 3], Color32)> = Vec::with_capacity(m.indices.len());
        let has_col = m.colors.len() == m.positions.len();
        for t in &m.indices {
            let (Some(a), Some(b), Some(c)) = (proj[t[0] as usize], proj[t[1] as usize], proj[t[2] as usize]) else { continue };
            let area = (b.0.x - a.0.x) * (c.0.y - a.0.y) - (b.0.y - a.0.y) * (c.0.x - a.0.x);
            if cull && area > 0.0 {
                continue; // screen Y is down: CCW in world → negative area
            }
            let [pa, pb, pc] = [m.positions[t[0] as usize], m.positions[t[1] as usize], m.positions[t[2] as usize]];
            let n = (pb - pa).cross(pc - pa).normalize_or_zero();
            let n = if n.dot(eye_dir) < 0.0 { -n } else { n };
            let light = 0.28 + 0.52 * n.dot(key).max(0.0) + 0.30 * n.dot(eye_dir).max(0.0);
            let col = if has_col {
                let ca = m.colors[t[0] as usize];
                let cb = m.colors[t[1] as usize];
                let cc = m.colors[t[2] as usize];
                [(ca[0] + cb[0] + cc[0]) / 3.0, (ca[1] + cb[1] + cc[1]) / 3.0, (ca[2] + cb[2] + cc[2]) / 3.0]
            } else {
                base
            };
            let to8 = |v: f32| (v * light * 255.0).clamp(0.0, 255.0) as u8;
            let color = Color32::from_rgba_unmultiplied(to8(col[0]), to8(col[1]), to8(col[2]), alpha);
            tris.push(((a.1 + b.1 + c.1) / 3.0, [a.0, b.0, c.0], color));
        }
        // Stay inside this window's per-frame GPU buffer budget: if the visible triangles don't
        // fit, keep an even subset (the caller should hand us an already-decimated mesh; this is
        // the last line of defence against an over-size vertex buffer).
        let granted = crate::gpu_guard::take_triangles(self.painter.ctx(), tris.len());
        if granted < tris.len() {
            if granted == 0 {
                return;
            }
            let step = tris.len() as f64 / granted as f64;
            tris = (0..granted).map(|k| tris[(k as f64 * step) as usize]).collect();
        }
        tris.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
        // Chunk into several meshes to keep each under u16-friendly sizes for older backends.
        for chunk in tris.chunks(20_000) {
            let mut mesh = Mesh::default();
            for (_, p, c) in chunk {
                let i = mesh.vertices.len() as u32;
                mesh.colored_vertex(p[0], *c);
                mesh.colored_vertex(p[1], *c);
                mesh.colored_vertex(p[2], *c);
                mesh.add_triangle(i, i + 1, i + 2);
            }
            self.painter.add(Shape::mesh(mesh));
        }
    }

    /// XYZ axis triad in the bottom-left corner.
    pub fn gizmo(&self) {
        let o = self.pr.rect.left_bottom() + egui::vec2(42.0, -42.0);
        let v = self.pr.cam.view();
        for (axis, col, label) in
            [(Vec3::X, Color32::from_rgb(220, 60, 60), "X"), (Vec3::Y, Color32::from_rgb(60, 170, 70), "Y"), (Vec3::Z, Color32::from_rgb(60, 110, 230), "Z")]
        {
            let d = v.transform_vector3(axis);
            let e = o + egui::vec2(d.x, -d.y) * 28.0;
            self.painter.line_segment([o, e], Stroke::new(2.5, col));
            self.painter.text(e, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(12.0), col);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_unproject_ground() {
        let cam = Camera::default();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let pr = Projector::new(&cam, rect);
        let p = Vec3::new(120.0, 80.0, 0.0);
        let s = pr.to_screen(p).unwrap();
        let g = pr.ground(s, 0.0).unwrap();
        assert!((g - p).length() < 1e-2, "{g:?}");
    }
}
