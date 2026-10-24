//! The AI assistant's view of the project: tool specs + execution.

use crate::{commands::Command, ops, History, OpKind, OperationSpec, Project};
use ezyx_ai::{agent::AgentEvent, ToolHost, ToolSpec};
use ezyx_cam::ops::{
    pocket::PocketOptions,
    profile::{ProfileOptions, ProfileSide},
    raster3d::Raster3dOptions,
};
use ezyx_tooldb::{Operation, ToolLibrary};
use serde_json::{json, Value};

pub struct ProjectHost<'a> {
    pub project: &'a mut Project,
    pub history: &'a mut History,
    pub library: &'a ToolLibrary,
    /// Confirmation callback for dangerous tools (GUI shows a dialog). `None` = deny.
    pub confirm: Option<Box<dyn FnMut(&str, &Value) -> bool + 'a>>,
    pub events: Vec<AgentEvent>,
    /// Extra tools provided by the front-end (e.g. machine control in the GUI).
    pub extra: Option<Box<dyn ExtraTools + 'a>>,
}

pub trait ExtraTools {
    fn specs(&self) -> Vec<ToolSpec>;
    fn call(&mut self, name: &str, input: &Value) -> Option<Result<String, String>>;
}

fn spec(name: &str, description: &str, schema: Value, confirm: bool) -> ToolSpec {
    ToolSpec { name: name.into(), description: description.into(), input_schema: schema, requires_confirmation: confirm }
}

pub fn project_tool_specs() -> Vec<ToolSpec> {
    vec![
        spec(
            "get_project",
            "Summary of the current project: stock, machine, vectors (ids, bounding boxes) and operations.",
            json!({"type": "object", "properties": {}}),
            false,
        ),
        spec(
            "run_command",
            "Apply an editing command to the project (undoable). The input is a command object with a 'cmd' field. Available: \
             set_job{width,height,thickness,material,safe_z} (all optional), set_rotary{diameter,length}, set_flat, set_machine{id}, set_post{id}, \
             add_rectangle{x,y,w,h,corner_radius}, add_circle{cx,cy,r}, add_ellipse{cx,cy,rx,ry}, add_polygon{cx,cy,r,sides}, \
             add_star{cx,cy,r_outer,r_inner,points}, add_polyline{points:[[x,y]...],closed}, add_text{text,height,x,y,font_path?}, \
             delete_vectors{ids}, move_vectors{ids,dx,dy}, scale_vectors{ids,factor}, rotate_vectors{ids,degrees}, center_on_stock{ids}, \
             offset_vectors{ids,distance}, boolean{op:Union|Subtract|Intersect,ids}, image_to3d{path,width_mm,thickness}, \
             import_mesh{path,fit_to_stock,depth}, import_vectors{path} (svg/dxf), add_demo_part, remove_operation{id}, set_operation_enabled{id,enabled}. \
             Coordinates in mm, origin at stock lower-left.",
            json!({"type": "object", "properties": {"cmd": {"type": "string"}}, "required": ["cmd"], "additionalProperties": true}),
            false,
        ),
        spec(
            "search_tools",
            "Search the bit library (Genmitsu table + generic templates). Returns ids, shape, diameter, flutes.",
            json!({"type": "object", "properties": {"query": {"type": "string", "description": "e.g. '60 v', 'ball 1/8', 'flat 3.175'"}}, "required": ["query"]}),
            false,
        ),
        spec("list_materials", "List stock materials (ids like hardwood.walnut, metal.al6061).", json!({"type": "object", "properties": {}}), false),
        spec("list_machines", "List machine profiles and post-processors.", json!({"type": "object", "properties": {}}), false),
        spec(
            "feeds_and_speeds",
            "Recommended RPM/feed/stepdown/stepover for a tool in the project's current material on the current machine.",
            json!({"type": "object", "properties": {"tool_id": {"type": "string"}, "finishing": {"type": "boolean"}}, "required": ["tool_id"]}),
            false,
        ),
        spec(
            "add_toolpath",
            "Add a machining operation. kind: profile_outside | profile_inside | profile_on | pocket | pocket_raster | vcarve | drill | engrave | \
             finish_3d | rough_3d. For 3D kinds the source is all reliefs unless part_id/mesh_id/relief_id is given.",
            json!({
                "type": "object",
                "properties": {
                    "kind": {"type": "string"},
                    "name": {"type": "string"},
                    "tool_id": {"type": "string"},
                    "depth": {"type": "number", "description": "cut depth in mm (positive)"},
                    "vector_ids": {"type": "array", "items": {"type": "integer"}},
                    "tabs": {"type": "boolean"},
                    "flat_depth": {"type": "number"},
                    "part_id": {"type": "integer"},
                    "mesh_id": {"type": "integer"},
                    "relief_id": {"type": "integer"}
                },
                "required": ["kind", "tool_id", "depth"]
            }),
            false,
        ),
        spec(
            "compute_toolpaths",
            "Compute all enabled operations; returns per-operation time estimates, errors and machining advice.",
            json!({"type": "object", "properties": {}}),
            false,
        ),
        spec(
            "export_gcode",
            "Post-process all toolpaths for the selected machine and write a G-code file.",
            json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}),
            true,
        ),
        spec("undo", "Undo the last command.", json!({"type": "object", "properties": {}}), false),
    ]
}

impl ProjectHost<'_> {
    fn add_toolpath(&mut self, i: &Value) -> Result<String, String> {
        let kind_s = i["kind"].as_str().ok_or("kind required")?;
        let tool_id = i["tool_id"].as_str().ok_or("tool_id required")?.to_string();
        if self.library.get(&tool_id).is_none() {
            return Err(format!("unknown tool_id '{tool_id}' — use search_tools"));
        }
        let depth = i["depth"].as_f64().ok_or("depth required")?.abs();
        let ids: Vec<u64> = i["vector_ids"].as_array().map(|a| a.iter().filter_map(|v| v.as_u64()).collect()).unwrap_or_default();
        let tabs = i["tabs"].as_bool().unwrap_or(false).then(Default::default);
        let source = if let Some(id) = i["part_id"].as_u64() {
            ops::ReliefSource::Part(id)
        } else if let Some(id) = i["mesh_id"].as_u64() {
            ops::ReliefSource::Mesh(id)
        } else if let Some(id) = i["relief_id"].as_u64() {
            ops::ReliefSource::Relief(id)
        } else {
            ops::ReliefSource::AllReliefs
        };
        let (kind, pass) = match kind_s {
            "profile_outside" => (OpKind::Profile(ProfileOptions { side: ProfileSide::Outside, tabs, ..Default::default() }), Operation::Roughing),
            "profile_inside" => (OpKind::Profile(ProfileOptions { side: ProfileSide::Inside, tabs, ..Default::default() }), Operation::Roughing),
            "profile_on" => (OpKind::Profile(ProfileOptions { side: ProfileSide::On, ..Default::default() }), Operation::Roughing),
            "pocket" => (OpKind::Pocket(PocketOptions::default()), Operation::Roughing),
            "pocket_raster" => (OpKind::Pocket(PocketOptions { style: ezyx_cam::ops::pocket::PocketStyle::Raster, ..Default::default() }), Operation::Roughing),
            "vcarve" => (OpKind::VCarve { flat_depth: i["flat_depth"].as_f64(), step: 0.2, clearance_tool_id: None }, Operation::Finishing),
            "drill" => (OpKind::Drill(Default::default()), Operation::Roughing),
            "engrave" => (OpKind::Engrave, Operation::Finishing),
            "finish_3d" => (OpKind::Raster3d { source, options: Raster3dOptions::default(), resolution: 0.2 }, Operation::Finishing),
            "rough_3d" => (
                OpKind::Raster3d { source, options: Raster3dOptions { roughing: true, allowance: 0.3, ..Default::default() }, resolution: 0.4 },
                Operation::Roughing,
            ),
            other => return Err(format!("unknown kind '{other}'")),
        };
        let spec = OperationSpec {
            id: 0,
            name: i["name"].as_str().unwrap_or(kind_s).to_string(),
            enabled: true,
            kind,
            tool_id,
            pass,
            start_depth: 0.0,
            cut_depth: depth,
            vector_ids: ids,
            direction: Default::default(),
            cutting_override: None,
        };
        self.history.execute(self.project, &Command::AddOperation { spec })
    }
}

impl ToolHost for ProjectHost<'_> {
    fn tools(&self) -> Vec<ToolSpec> {
        let mut t = project_tool_specs();
        if let Some(e) = &self.extra {
            t.extend(e.specs());
        }
        t
    }

    fn call(&mut self, name: &str, i: &Value) -> Result<String, String> {
        if let Some(e) = self.extra.as_mut() {
            if let Some(r) = e.call(name, i) {
                return r;
            }
        }
        match name {
            "get_project" => Ok(self.project.summary()),
            "run_command" => {
                let c: Command = serde_json::from_value(i.clone()).map_err(|e| format!("invalid command: {e}"))?;
                self.history.execute(self.project, &c)
            }
            "search_tools" => {
                let q = i["query"].as_str().unwrap_or("");
                let hits: Vec<String> = self
                    .library
                    .search(q)
                    .into_iter()
                    .take(25)
                    .map(|t| format!("{} | {} | {} | Ø{:.3} mm | {} flutes", t.id, t.name, t.shape.label(), t.diameter, t.flutes))
                    .collect();
                Ok(if hits.is_empty() { "no matches".into() } else { hits.join("\n") })
            }
            "list_materials" => Ok(ezyx_materials::library().iter().map(|m| format!("{} — {} ({:?})", m.id, m.name, m.group)).collect::<Vec<_>>().join("\n")),
            "list_machines" => {
                let m: Vec<String> = crate::machines::builtin().iter().map(|m| format!("machine {} — {} (post {})", m.id, m.name, m.post_id)).collect();
                let p: Vec<String> = ezyx_gcode::builtin_posts().iter().map(|p| format!("post {} — {}", p.id, p.name)).collect();
                Ok(format!("{}\n{}", m.join("\n"), p.join("\n")))
            }
            "feeds_and_speeds" => {
                let id = i["tool_id"].as_str().unwrap_or("");
                let t = self.library.get(id).ok_or("unknown tool")?;
                let pass = if i["finishing"].as_bool().unwrap_or(false) { Operation::Finishing } else { Operation::Roughing };
                let c = ops::resolve_cutting(t, ops::material_group(self.project), pass, &self.project.machine.limits);
                Ok(format!(
                    "{}: {:.0} RPM, feed {:.0} mm/min, plunge {:.0}, stepdown {:.2} mm, stepover {:.2} mm. Source: {}",
                    t.name, c.rpm, c.feed, c.plunge_feed, c.stepdown, c.stepover, c.source
                ))
            }
            "add_toolpath" => self.add_toolpath(i),
            "compute_toolpaths" => {
                let res = ops::compute_all(self.project, self.library);
                let mut out = Vec::new();
                for r in res {
                    let name = self.project.operations.iter().find(|o| o.id == r.op_id).map(|o| o.name.clone()).unwrap_or_default();
                    if let Some(e) = r.error {
                        out.push(format!("#{} {name}: ERROR {e}", r.op_id));
                        continue;
                    }
                    let secs: f64 = r.toolpaths.iter().map(|t| t.stats(self.project.machine.limits.max_feed).est_seconds).sum();
                    out.push(format!("#{} {name}: {} toolpath(s), ~{:.1} min", r.op_id, r.toolpaths.len(), secs / 60.0));
                    for a in r.advice {
                        out.push(format!("   [{:?}] {}", a.severity, a.message));
                    }
                }
                Ok(if out.is_empty() { "no enabled operations".into() } else { out.join("\n") })
            }
            "export_gcode" => {
                let path = i["path"].as_str().ok_or("path required")?;
                let (g, warnings) = ops::export_gcode(self.project, self.library)?;
                std::fs::write(path, &g).map_err(|e| e.to_string())?;
                Ok(format!("Wrote {} lines to {path}. Warnings: {}", g.lines().count(), if warnings.is_empty() { "none".into() } else { warnings.join("; ") }))
            }
            "undo" => Ok(if self.history.undo(self.project) { "undone".into() } else { "nothing to undo".into() }),
            other => Err(format!("unknown tool {other}")),
        }
    }

    fn confirm(&mut self, name: &str, input: &Value) -> bool {
        self.confirm.as_mut().map(|f| f(name, input)).unwrap_or(false)
    }

    fn on_event(&mut self, e: &AgentEvent) {
        self.events.push(e.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tools_work_without_llm() {
        let mut p = Project::default();
        let mut h = History::default();
        let lib = ToolLibrary::bundled();
        let mut host = ProjectHost { project: &mut p, history: &mut h, library: &lib, confirm: None, events: vec![], extra: None };
        host.call("run_command", &json!({"cmd": "add_rectangle", "x": 10, "y": 10, "w": 80, "h": 40, "corner_radius": 4})).unwrap();
        let r = host.call("search_tools", &json!({"query": "flat 1/8"})).unwrap();
        assert!(r.contains("generic.flat_1_8_2f"));
        host.call("add_toolpath", &json!({"kind": "profile_outside", "tool_id": "generic.flat_1_8_2f", "depth": 6, "tabs": true})).unwrap();
        let c = host.call("compute_toolpaths", &json!({})).unwrap();
        assert!(c.contains("min"), "{c}");
        // export requires confirmation → agent loop would refuse; direct call is allowed for the host.
        assert!(!host.confirm("export_gcode", &json!({})));
    }
}
