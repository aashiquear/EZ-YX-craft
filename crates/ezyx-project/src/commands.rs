//! Commands: the single mutation API used by GUI, CLI and AI. Serde-tagged so the AI assistant
//! and macros/scripts can produce them as JSON.

use crate::{ops::OperationSpec, text, Id, MeshObject, PartObject, Project, ReliefObject, VectorSource};
use ezyx_core::{path::shapes, region::Region, DAffine2, DVec2, JobSetup};
use ezyx_image::{photo_vcarve::PhotoVCarveOptions, relief::ReliefOptions, trace::TraceOptions};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    SetJob {
        width: Option<f64>,
        height: Option<f64>,
        thickness: Option<f64>,
        material: Option<String>,
        safe_z: Option<f64>,
    },
    SetRotary {
        diameter: f64,
        length: f64,
    },
    SetFlat,
    SetMachine {
        id: String,
    },
    SetPost {
        id: String,
    },
    AddRectangle {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        corner_radius: f64,
    },
    AddCircle {
        cx: f64,
        cy: f64,
        r: f64,
    },
    AddEllipse {
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
    },
    AddPolygon {
        cx: f64,
        cy: f64,
        r: f64,
        sides: usize,
    },
    AddStar {
        cx: f64,
        cy: f64,
        r_outer: f64,
        r_inner: f64,
        points: usize,
    },
    AddPolyline {
        points: Vec<[f64; 2]>,
        closed: bool,
    },
    AddText {
        text: String,
        height: f64,
        x: f64,
        y: f64,
        font_path: Option<String>,
    },
    DeleteVectors {
        ids: Vec<Id>,
    },
    MoveVectors {
        ids: Vec<Id>,
        dx: f64,
        dy: f64,
    },
    ScaleVectors {
        ids: Vec<Id>,
        factor: f64,
    },
    RotateVectors {
        ids: Vec<Id>,
        degrees: f64,
    },
    CenterOnStock {
        ids: Vec<Id>,
    },
    OffsetVectors {
        ids: Vec<Id>,
        distance: f64,
    },
    Boolean {
        op: BoolOp,
        ids: Vec<Id>,
    },
    TraceImage {
        path: String,
        options: TraceOptions,
        centerline: bool,
    },
    ImageRelief {
        path: String,
        options: ReliefOptions,
    },
    PhotoVCarve {
        path: String,
        options: PhotoVCarveOptions,
        tool_id: String,
    },
    ImageTo3d {
        path: String,
        width_mm: f64,
        thickness: f64,
    },
    ImportMesh {
        path: String,
        fit_to_stock: bool,
        depth: f64,
    },
    /// SVG or DXF.
    ImportVectors {
        path: String,
    },
    AddDemoPart,
    AddPart {
        part: ezyx_cad::Part,
    },
    AddOperation {
        spec: OperationSpec,
    },
    RemoveOperation {
        id: Id,
    },
    SetOperationEnabled {
        id: Id,
        enabled: bool,
    },
}

fn vec_center(p: &Project, ids: &[Id]) -> DVec2 {
    let b = p.vectors.iter().filter(|v| ids.is_empty() || ids.contains(&v.id)).fold(ezyx_core::Bounds2::EMPTY, |a, v| a.union(&v.path.bounds()));
    b.center()
}

fn transform(p: &mut Project, ids: &[Id], t: DAffine2) -> usize {
    let mut n = 0;
    for v in p.vectors.iter_mut().filter(|v| ids.is_empty() || ids.contains(&v.id)) {
        v.path = v.path.transform(&t);
        v.source = VectorSource::Derived { op: "transform".into() };
        n += 1;
    }
    n
}

fn add_region(p: &mut Project, r: &Region, name: &str, op: &str) -> Vec<Id> {
    r.rings().into_iter().map(|ring| p.add_vector(name, ring, VectorSource::Derived { op: op.into() })).collect()
}

impl Command {
    /// Apply to the project. Returns a human-readable result (shown in status bar / sent to AI).
    pub fn apply(&self, p: &mut Project) -> Result<String, String> {
        use Command::*;
        Ok(match self {
            SetJob { width, height, thickness, material, safe_z } => {
                if let Some(v) = width {
                    p.job.width = *v;
                }
                if let Some(v) = height {
                    p.job.height = *v;
                }
                if let Some(v) = thickness {
                    p.job.thickness = *v;
                }
                if let Some(m) = material {
                    if ezyx_materials::find(m).is_none() {
                        return Err(format!("unknown material '{m}'"));
                    }
                    p.job.material = m.clone();
                }
                if let Some(v) = safe_z {
                    p.job.safe_z = *v;
                }
                format!("Stock {:.1} x {:.1} x {:.1} mm, {}", p.job.width, p.job.height, p.job.thickness, p.job.material)
            }
            SetRotary { diameter, length } => {
                let mat = p.job.material.clone();
                p.job = JobSetup { material: mat, ..JobSetup::rotary(*diameter, *length) };
                format!("Rotary job Ø{diameter} × {length} mm (design height = circumference {:.1} mm)", p.job.height)
            }
            SetFlat => {
                p.job.kind = ezyx_core::JobKind::Flat;
                "Flat job".into()
            }
            SetMachine { id } => {
                let m = crate::machines::find(id).ok_or_else(|| format!("unknown machine '{id}'"))?;
                p.post_id = m.post_id.clone();
                p.machine = m;
                format!("Machine: {} (post {})", p.machine.name, p.post_id)
            }
            SetPost { id } => {
                ezyx_gcode::post::find_post(id).ok_or_else(|| format!("unknown post '{id}'"))?;
                p.post_id = id.clone();
                format!("Post: {id}")
            }
            AddRectangle { x, y, w, h, corner_radius } => {
                let id = p.add_vector(
                    "Rectangle",
                    shapes::rectangle(DVec2::new(*x, *y), *w, *h, *corner_radius),
                    VectorSource::Rectangle { x: *x, y: *y, w: *w, h: *h, r: *corner_radius },
                );
                format!("Added rectangle #{id}")
            }
            AddCircle { cx, cy, r } => {
                let id = p.add_vector("Circle", shapes::circle(DVec2::new(*cx, *cy), *r), VectorSource::Circle { cx: *cx, cy: *cy, r: *r });
                format!("Added circle #{id}")
            }
            AddEllipse { cx, cy, rx, ry } => {
                let id =
                    p.add_vector("Ellipse", shapes::ellipse(DVec2::new(*cx, *cy), *rx, *ry, 0.0), VectorSource::Ellipse { cx: *cx, cy: *cy, rx: *rx, ry: *ry });
                format!("Added ellipse #{id}")
            }
            AddPolygon { cx, cy, r, sides } => {
                let id = p.add_vector(
                    "Polygon",
                    shapes::polygon(DVec2::new(*cx, *cy), *r, *sides, 0.0),
                    VectorSource::Polygon { cx: *cx, cy: *cy, r: *r, sides: *sides },
                );
                format!("Added polygon #{id}")
            }
            AddStar { cx, cy, r_outer, r_inner, points } => {
                let id = p.add_vector(
                    "Star",
                    shapes::star(DVec2::new(*cx, *cy), *r_outer, *r_inner, *points, 0.0),
                    VectorSource::Star { cx: *cx, cy: *cy, r_outer: *r_outer, r_inner: *r_inner, points: *points },
                );
                format!("Added star #{id}")
            }
            AddPolyline { points, closed } => {
                if points.len() < 2 {
                    return Err("need at least 2 points".into());
                }
                let pl = ezyx_core::Polyline::new(points.iter().map(|q| DVec2::new(q[0], q[1])).collect(), *closed);
                let id = p.add_vector("Polyline", pl, VectorSource::Drawn);
                format!("Added polyline #{id}")
            }
            AddText { text: t, height, x, y, font_path } => {
                let path = font_path.clone().map(std::path::PathBuf::from).or_else(text::find_system_font).ok_or("no font found: pass font_path")?;
                let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                let loops = text::text_outlines(&data, t, *height, DVec2::new(*x, *y), 0.0)?;
                let n = loops.len();
                for l in loops {
                    p.add_vector(format!("Text '{t}'"), l, VectorSource::Text { text: t.clone(), height: *height });
                }
                format!("Added text '{t}' ({n} outlines)")
            }
            DeleteVectors { ids } => {
                let before = p.vectors.len();
                p.vectors.retain(|v| !ids.contains(&v.id));
                format!("Deleted {} vectors", before - p.vectors.len())
            }
            MoveVectors { ids, dx, dy } => {
                let n = transform(p, ids, DAffine2::from_translation(DVec2::new(*dx, *dy)));
                format!("Moved {n} vectors")
            }
            ScaleVectors { ids, factor } => {
                let c = vec_center(p, ids);
                let t = DAffine2::from_translation(c) * DAffine2::from_scale(DVec2::splat(*factor)) * DAffine2::from_translation(-c);
                let n = transform(p, ids, t);
                format!("Scaled {n} vectors by {factor}")
            }
            RotateVectors { ids, degrees } => {
                let c = vec_center(p, ids);
                let t = DAffine2::from_translation(c) * DAffine2::from_angle(degrees.to_radians()) * DAffine2::from_translation(-c);
                let n = transform(p, ids, t);
                format!("Rotated {n} vectors by {degrees}°")
            }
            CenterOnStock { ids } => {
                let c = vec_center(p, ids);
                let target = DVec2::new(p.job.width, p.job.height) * 0.5;
                let n = transform(p, ids, DAffine2::from_translation(target - c));
                format!("Centred {n} vectors on the stock")
            }
            OffsetVectors { ids, distance } => {
                let r = p.region_for(ids).offset(*distance);
                let new = add_region(p, &r, "Offset", "offset");
                format!("Created {} offset vectors: {:?}", new.len(), new)
            }
            Boolean { op, ids } => {
                if ids.len() < 2 {
                    return Err("select at least two vectors".into());
                }
                let first = p.region_for(&ids[..1]);
                let rest = p.region_for(&ids[1..]);
                let r = match op {
                    BoolOp::Union => first.union(&rest),
                    BoolOp::Subtract => first.difference(&rest),
                    BoolOp::Intersect => first.intersection(&rest),
                };
                p.vectors.retain(|v| !ids.contains(&v.id));
                let new = add_region(p, &r, &format!("{op:?}"), "boolean");
                format!("{op:?} → {} vectors {:?}", new.len(), new)
            }
            TraceImage { path, options, centerline } => {
                let img = ezyx_image::load_gray(std::path::Path::new(path)).map_err(|e| e.to_string())?;
                let polys = if *centerline { ezyx_image::trace::trace_centerlines(&img, options) } else { ezyx_image::trace::trace_outlines(&img, options) };
                let n = polys.len();
                for pl in polys {
                    p.add_vector("Traced", pl, VectorSource::Traced { image: path.clone() });
                }
                format!("Traced {n} {} from {path}", if *centerline { "centrelines" } else { "outlines" })
            }
            ImageRelief { path, options } => {
                let img = ezyx_image::load_gray(std::path::Path::new(path)).map_err(|e| e.to_string())?;
                let hm = ezyx_image::relief::image_to_relief(&img, options);
                let id = p.alloc_id();
                p.reliefs.push(ReliefObject { id, name: format!("Relief from {path}"), heightmap: hm, visible: true });
                format!("Added relief #{id}")
            }
            PhotoVCarve { path, options, tool_id } => {
                let img = ezyx_image::load_gray(std::path::Path::new(path)).map_err(|e| e.to_string())?;
                let paths = ezyx_image::photo_vcarve::grooves(&img, options);
                let id = p.alloc_id();
                let n = paths.len();
                p.operations.push(OperationSpec {
                    id,
                    name: "Photo V-carve".into(),
                    enabled: true,
                    kind: crate::OpKind::Paths3d { paths, tolerance: 0.01 },
                    tool_id: tool_id.clone(),
                    pass: ezyx_tooldb::Operation::Finishing,
                    start_depth: 0.0,
                    cut_depth: options.max_depth(),
                    vector_ids: vec![],
                    direction: Default::default(),
                    cutting_override: None,
                });
                format!("Added photo V-carve operation #{id} ({n} grooves, max depth {:.2} mm)", options.max_depth())
            }
            ImageTo3d { path, width_mm, thickness } => {
                let dynimg = image::open(path).map_err(|e| e.to_string())?;
                let gray = ezyx_image::GrayImage::from_dynamic_limited(&dynimg, 512);
                let has_alpha = dynimg.color().has_alpha();
                let sil = if has_alpha {
                    let small = dynimg.resize(gray.width as u32, gray.height as u32, image::imageops::FilterType::Triangle);
                    ezyx_image::GrayImage::alpha_mask(&small, 16)
                } else {
                    ezyx_image::trace::mask(&gray, &TraceOptions::default())
                };
                let opt = ezyx_image::inflate::InflateOptions { width_mm: *width_mm, thickness: *thickness, ..Default::default() };
                let mut mesh = ezyx_mesh::img3d::inflated_mesh(&gray, &sil, &opt);
                place_mesh(&mut mesh, p, false, 0.0);
                let id = p.alloc_id();
                p.meshes.push(MeshObject { id, name: format!("3D from {path}"), mesh, visible: true });
                format!("Generated 3D mesh #{id} from image (inflation backend)")
            }
            ImportMesh { path, fit_to_stock, depth } => {
                let mut mesh = ezyx_mesh::io::load(std::path::Path::new(path)).map_err(|e| e.to_string())?;
                place_mesh(&mut mesh, p, *fit_to_stock, *depth);
                let id = p.alloc_id();
                let tris = mesh.indices.len();
                p.meshes.push(MeshObject { id, name: path.clone(), mesh, visible: true });
                format!("Imported mesh #{id} ({tris} triangles)")
            }
            ImportVectors { path } => {
                let pth = std::path::Path::new(path);
                let ext = pth.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                let polys = match ext.as_str() {
                    "svg" => crate::import::import_svg(&std::fs::read(pth).map_err(|e| e.to_string())?)?,
                    "dxf" => crate::import::import_dxf(pth)?,
                    other => return Err(format!("unsupported vector format '.{other}' (svg, dxf)")),
                };
                let n = polys.len();
                for pl in polys {
                    p.add_vector("Imported", pl, VectorSource::Imported { file: path.clone() });
                }
                format!("Imported {n} vectors from {path}")
            }
            AddDemoPart => {
                let id = p.alloc_id();
                p.parts.push(PartObject { id, part: ezyx_cad::demo_part(), visible: true });
                format!("Added demo part #{id}")
            }
            AddPart { part } => {
                let id = p.alloc_id();
                p.parts.push(PartObject { id, part: part.clone(), visible: true });
                format!("Added part #{id} '{}'", part.name)
            }
            AddOperation { spec } => {
                let mut s = spec.clone();
                s.id = p.alloc_id();
                let id = s.id;
                p.operations.push(s);
                format!("Added operation #{id}")
            }
            RemoveOperation { id } => {
                p.operations.retain(|o| o.id != *id);
                format!("Removed operation #{id}")
            }
            SetOperationEnabled { id, enabled } => {
                let o = p.operations.iter_mut().find(|o| o.id == *id).ok_or("no such operation")?;
                o.enabled = *enabled;
                format!("Operation #{id} enabled={enabled}")
            }
        })
    }
}

/// Centre a mesh on the stock with its top at Z = 0; optionally scale to fit the stock and depth.
pub fn place_mesh(mesh: &mut ezyx_mesh::TriMesh, p: &Project, fit: bool, depth: f64) {
    use glam::{Mat4, Vec3};
    let (lo, hi) = mesh.bounds();
    let size = hi - lo;
    let mut s = 1.0f32;
    if fit && size.x > 0.0 && size.y > 0.0 {
        s = (p.job.width as f32 * 0.9 / size.x).min(p.job.height as f32 * 0.9 / size.y);
        if depth > 0.0 && size.z > 0.0 {
            s = s.min(depth as f32 / size.z);
        }
    }
    let center = (lo + hi) * 0.5;
    let target = Vec3::new(p.job.width as f32 * 0.5, p.job.height as f32 * 0.5, 0.0);
    let m = Mat4::from_translation(target) * Mat4::from_scale(Vec3::splat(s)) * Mat4::from_translation(Vec3::new(-center.x, -center.y, -hi.z));
    mesh.transform(&m);
}

/// Snapshot-based undo/redo (projects are small enough that this is the most robust choice).
#[derive(Default)]
pub struct History {
    undo: Vec<Project>,
    redo: Vec<Project>,
    pub log: Vec<String>,
}

impl History {
    pub fn execute(&mut self, p: &mut Project, c: &Command) -> Result<String, String> {
        let snapshot = p.clone();
        match c.apply(p) {
            Ok(msg) => {
                self.undo.push(snapshot);
                if self.undo.len() > 200 {
                    self.undo.remove(0);
                }
                self.redo.clear();
                self.log.push(msg.clone());
                Ok(msg)
            }
            Err(e) => {
                *p = snapshot;
                Err(e)
            }
        }
    }
    /// Record the current state as an undo point before an external bulk change (e.g. the AI
    /// assistant returning a modified project).
    pub fn checkpoint(&mut self, p: &Project, note: impl Into<String>) {
        self.undo.push(p.clone());
        self.redo.clear();
        self.log.push(note.into());
    }

    pub fn undo(&mut self, p: &mut Project) -> bool {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(p, prev));
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self, p: &mut Project) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(p, next));
            true
        } else {
            false
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn undo_redo_and_boolean() {
        let mut p = Project::default();
        let mut h = History::default();
        h.execute(&mut p, &Command::AddRectangle { x: 0.0, y: 0.0, w: 20.0, h: 20.0, corner_radius: 0.0 }).unwrap();
        h.execute(&mut p, &Command::AddCircle { cx: 20.0, cy: 10.0, r: 5.0 }).unwrap();
        let ids: Vec<Id> = p.vectors.iter().map(|v| v.id).collect();
        h.execute(&mut p, &Command::Boolean { op: BoolOp::Union, ids }).unwrap();
        assert_eq!(p.vectors.len(), 1);
        assert!(h.undo(&mut p));
        assert_eq!(p.vectors.len(), 2);
        assert!(h.redo(&mut p));
        assert_eq!(p.vectors.len(), 1);
        // Failed command leaves the project untouched.
        assert!(h.execute(&mut p, &Command::SetMachine { id: "nope".into() }).is_err());
        assert_eq!(p.vectors.len(), 1);
    }
    #[test]
    fn json_commands() {
        let c: Command = serde_json::from_str(r#"{"cmd":"add_circle","cx":5,"cy":5,"r":2}"#).unwrap();
        let mut p = Project::default();
        c.apply(&mut p).unwrap();
        assert_eq!(p.vectors.len(), 1);
    }
}
