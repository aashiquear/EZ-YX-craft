//! The pipeline viewer: stock (board, round blank or rotary cylinder), features, toolpaths
//! filtered by pass, the simulated result, and the Design step's interactive tools (selection,
//! drawing with snapping and quick keys, measuring).

use crate::app::EzApp;
use crate::ribbon::{DrawTool, Step};
use crate::theme;
use crate::viewport::{self, Draw, Projector, ViewPreset};
use egui::{Color32, Pos2, RichText, Sense, Stroke};
use ezyx_core::{DVec2, JobKind};
use ezyx_mesh::TriMesh;
use ezyx_project::pipeline::StockShape;
use ezyx_project::{Command, ReliefObject};
use glam::Vec3;

pub fn show(app: &mut EzApp, ui: &mut egui::Ui) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = resp.rect;
    painter.rect_filled(rect, 0.0, theme::viewport_bg(app.prefs.dark));
    let designing = app.step == Step::Design;
    let tool = if designing { app.tool } else { DrawTool::Select };
    let drawing = tool != DrawTool::Select;
    let lmb_reserved = app.mode2d || drawing;
    viewport::navigate(ui, &resp, &mut app.cam, rect, lmb_reserved);
    let pr = Projector::new(&app.cam, rect);
    let d = Draw { painter: &painter, pr: &pr };
    let job = app.project.job.clone();
    let rotary = match job.kind {
        JobKind::Rotary { diameter, length, .. } => Some((diameter as f32, length as f32)),
        _ => None,
    };
    let dark = app.prefs.dark;
    let (w, h, t) = (job.width as f32, job.height as f32, job.thickness as f32);
    let round = app.project.stock_shape == StockShape::Round;
    let stock_col = if dark { Color32::from_gray(150) } else { Color32::from_rgb(120, 100, 80) };

    // ---- stock
    if let (Some((dia, len)), false) = (rotary, app.mode2d) {
        let r = dia * 0.5;
        let n = 48;
        for k in 0..n {
            let a0 = std::f32::consts::TAU * k as f32 / n as f32;
            let a1 = std::f32::consts::TAU * (k + 1) as f32 / n as f32;
            for x in [0.0, len] {
                d.line(Vec3::new(x, r * a0.sin(), r * a0.cos()), Vec3::new(x, r * a1.sin(), r * a1.cos()), Stroke::new(1.0, stock_col));
            }
            if k % 6 == 0 {
                d.line(Vec3::new(0.0, r * a0.sin(), r * a0.cos()), Vec3::new(len, r * a0.sin(), r * a0.cos()), Stroke::new(0.5, stock_col));
            }
        }
        d.line(Vec3::new(-10.0, 0.0, 0.0), Vec3::new(len + 10.0, 0.0, 0.0), Stroke::new(1.0, theme::ACCENT));
    } else {
        let outline: Vec<Vec3> = if round {
            let (c, r) = (Vec3::new(w * 0.5, h * 0.5, 0.0), w.min(h) * 0.5);
            (0..96)
                .map(|k| c + Vec3::new((k as f32 / 96.0 * std::f32::consts::TAU).cos() * r, (k as f32 / 96.0 * std::f32::consts::TAU).sin() * r, 0.0))
                .collect()
        } else {
            vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(w, 0.0, 0.0), Vec3::new(w, h, 0.0), Vec3::new(0.0, h, 0.0)]
        };
        if app.preview.is_none() {
            let fill = if dark {
                Color32::from_rgba_unmultiplied(90, 80, 70, 120)
            } else {
                let c = ezyx_materials::find(&job.material).map(|m| m.appearance.base).unwrap_or([0.95, 0.9, 0.8]);
                // Light tint of the material colour so vectors stay readable.
                let mix = |v: f32| ((0.55 + 0.45 * v) * 255.0) as u8;
                Color32::from_rgba_unmultiplied(mix(c[0]), mix(c[1]), mix(c[2]), 235)
            };
            let pts: Vec<Pos2> = outline.iter().filter_map(|c| pr.to_screen(*c)).collect();
            if pts.len() == outline.len() {
                painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
            }
            // Grid.
            let step = if app.cam.dist > 1500.0 { 50.0 } else { 10.0 };
            let gcol = if dark { Color32::from_gray(70) } else { Color32::from_rgba_unmultiplied(0, 0, 0, 22) };
            let mut x = 0.0;
            while x <= w + 0.01 {
                d.line(Vec3::new(x, 0.0, 0.0), Vec3::new(x, h, 0.0), Stroke::new(if (x as i32) % 50 == 0 { 1.0 } else { 0.5 }, gcol));
                x += step;
            }
            let mut y = 0.0;
            while y <= h + 0.01 {
                d.line(Vec3::new(0.0, y, 0.0), Vec3::new(w, y, 0.0), Stroke::new(if (y as i32) % 50 == 0 { 1.0 } else { 0.5 }, gcol));
                y += step;
            }
        }
        let s = Stroke::new(1.2, stock_col);
        for z in [0.0, -t] {
            let ring: Vec<Vec3> = outline.iter().map(|p| Vec3::new(p.x, p.y, z)).collect();
            d.polyline(&ring, true, s);
        }
        let posts: Vec<Vec3> = if round { outline.iter().step_by(12).copied().collect() } else { outline.clone() };
        for p in posts {
            d.line(p, Vec3::new(p.x, p.y, -t), s);
        }
        // Datum marker.
        let o = job.xy_datum.fraction();
        if let Some(p) = pr.to_screen(Vec3::new(o.x as f32 * w, o.y as f32 * h, 0.0)) {
            painter.circle_filled(p, 4.0, theme::ACCENT);
        }
    }

    // ---- solid content (reliefs, 3D models) or the simulated result
    if let Some(m) = &app.preview {
        // Back faces (the stock bottom) are skipped on flat jobs: half the triangles to upload.
        d.mesh(m, [0.8, 0.7, 0.55], 255, rotary.is_none());
    } else {
        let feature_of = |p: &ezyx_project::Project, pred: &dyn Fn(&ezyx_project::FeatureKind) -> bool| p.features.iter().find(|f| pred(&f.kind)).map(|f| f.id);
        let reliefs: Vec<ReliefObject> = app.project.reliefs.iter().filter(|r| r.visible).cloned().collect();
        for r in &reliefs {
            let m = relief_mesh(app, r);
            let fid = feature_of(&app.project, &|k| matches!(k, ezyx_project::FeatureKind::Relief { relief_id } if *relief_id == r.id));
            let sel = fid.is_some() && fid == app.selected_feature;
            d.mesh(&m, if sel { [0.95, 0.78, 0.55] } else { [0.85, 0.75, 0.6] }, 255, false);
        }
        // Models over the per-mesh GPU budget are drawn from a simplified copy.
        let budget = app.budget.mesh_triangles;
        for m in app.project.meshes.iter().filter(|m| m.visible && m.mesh.indices.len() > budget) {
            let key = format!("mesh{}", m.id);
            let sig = (m.mesh.indices.len() as u64) << 32 ^ m.mesh.positions.len() as u64 ^ budget as u64;
            if app.display.get(&key).map(|(r, _)| *r != sig).unwrap_or(true) {
                app.display.insert(key, (sig, m.mesh.decimated(budget)));
            }
        }
        for m in app.project.meshes.iter().filter(|m| m.visible) {
            let shown = app.display.get(&format!("mesh{}", m.id)).filter(|_| m.mesh.indices.len() > budget).map(|(_, d)| d).unwrap_or(&m.mesh);
            let fid = feature_of(&app.project, &|k| matches!(k, ezyx_project::FeatureKind::Model3D { mesh_id } if *mesh_id == m.id));
            let suppressed = fid.and_then(|f| app.project.features.iter().find(|x| x.id == f)).map(|f| f.suppressed).unwrap_or(false);
            let sel = fid.is_some() && fid == app.selected_feature;
            d.mesh(shown, if sel { [0.95, 0.7, 0.45] } else { [0.75, 0.78, 0.82] }, if suppressed { 90 } else { 255 }, true);
        }
    }

    // ---- vectors (hidden under the simulated result)
    let hidden_layers: Vec<usize> = app.project.layers.iter().enumerate().filter(|(_, l)| !l.visible).map(|(i, _)| i).collect();
    let feat_vecs: Vec<u64> = app
        .selected_feature
        .and_then(|id| app.project.features.iter().find(|f| f.id == id))
        .map(|f| match &f.kind {
            ezyx_project::FeatureKind::Cut2D { vector_ids, .. } => vector_ids.clone(),
            _ => vec![],
        })
        .unwrap_or_default();
    if app.preview.is_none() {
        for v in app.project.vectors.iter().filter(|v| !hidden_layers.contains(&v.layer)) {
            let sel = app.selection.contains(&v.id);
            let col = if sel {
                theme::VECTOR_SEL
            } else if feat_vecs.contains(&v.id) {
                theme::OK
            } else if dark {
                Color32::from_gray(220)
            } else {
                theme::VECTOR
            };
            let pts: Vec<Vec3> = v.path.pts.iter().map(|p| Vec3::new(p.x as f32, p.y as f32, 0.02)).collect();
            d.polyline(&pts, v.path.closed, Stroke::new(if sel { 2.2 } else { 1.3 }, col));
        }
        // Photo V-carve grooves are features without vectors: draw a light hint of them.
        for f in &app.project.features {
            if let ezyx_project::FeatureKind::Grooves { paths, .. } = &f.kind {
                for p in paths.iter().step_by(2) {
                    let pts: Vec<Vec3> = p.iter().step_by(4).map(|q| Vec3::new(q.x as f32, q.y as f32, 0.02)).collect();
                    d.polyline(&pts, false, Stroke::new(0.6, theme::STEEL));
                }
            }
        }
    }

    // ---- toolpaths (filtered by pass) — shown in Toolpaths/Output, faintly in Design
    if app.show_toolpaths && app.preview.is_none() && app.step != Step::Stock {
        draw_toolpaths(app, &d, rotary, if app.step == Step::Design { 0.35 } else { 1.0 });
    }

    // ---- interaction
    let hover = resp.hover_pos();
    let alt = ui.input(|i| i.modifiers.alt);
    let raw = hover.and_then(|s| pr.ground(s, 0.0));
    let (snapped, snap_hit) = match raw {
        Some(g) if !alt => snap(app, g, &pr),
        Some(g) => (g, false),
        None => (Vec3::ZERO, false),
    };
    app.cursor_world = raw.map(|_| snapped);
    if snap_hit && designing && drawing {
        if let Some(p) = pr.to_screen(snapped) {
            painter.circle_stroke(p, 6.0, Stroke::new(1.5, theme::OK));
        }
    }
    if designing {
        interact(app, ui, &resp, &painter, &d, &pr, tool, hover);
    } else if resp.clicked() {
        // Click a feature's shape to select the feature in other steps.
        if let Some(s) = hover {
            if let Some(vid) = pick_vector(app, &pr, s) {
                app.selected_feature = app
                    .project
                    .features
                    .iter()
                    .find(|f| matches!(&f.kind, ezyx_project::FeatureKind::Cut2D { vector_ids, .. } if vector_ids.contains(&vid)))
                    .map(|f| f.id);
                app.selected_op = None;
            }
        }
    }

    d.gizmo();
    overlay(app, ui, rect);
    if let Some(c) = app.cursor_world {
        let u = job.display_units;
        painter.text(
            rect.right_bottom() + egui::vec2(-10.0, -8.0),
            egui::Align2::RIGHT_BOTTOM,
            format!("X {}  Y {}", u.fmt(c.x as f64), u.fmt(c.y as f64)),
            egui::FontId::monospace(12.0),
            theme::STEEL,
        );
    }
    if !app.draw.typed.is_empty() {
        painter.text(
            rect.center_bottom() + egui::vec2(0.0, -30.0),
            egui::Align2::CENTER_BOTTOM,
            format!("⌨ {}  (Enter to apply, Esc to cancel)", app.draw.typed),
            egui::FontId::monospace(15.0),
            theme::ACCENT,
        );
    }
    if app.project.vectors.is_empty() && app.project.features.is_empty() && app.project.reliefs.is_empty() && app.project.meshes.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            match app.step {
                Step::Stock => "① Choose the material, shape and size of your stock on the right — or pick a Quick stock preset.",
                _ => "② Draw with the Draw tools · import SVG/DXF/STL · or open the 🖼 Image Studio / 🧊 Solid Modeler\nAsk the 🤖 Assistant: \"make a 200×100 walnut sign that says Workshop\"",
            },
            egui::FontId::proportional(15.0),
            theme::STEEL,
        );
    }
}

fn draw_toolpaths(app: &EzApp, d: &Draw, rotary: Option<(f32, f32)>, alpha: f32) {
    let maxd = app.results.iter().flat_map(|r| r.toolpaths.iter()).flat_map(|t| t.points()).map(|(p, _)| -p.z).fold(0.1f64, f64::max) as f32;
    let mode2d = app.mode2d;
    let to3 = |p: &ezyx_cam::Pt, rot: bool| -> Vec3 {
        if rot && !mode2d {
            let r = rotary.map(|(d, _)| d * 0.5).unwrap_or(0.0) + p.z as f32;
            let a = (p.a as f32).to_radians();
            Vec3::new(p.x as f32, r * a.sin(), r * a.cos())
        } else if rot {
            let circ = rotary.map(|(d, _)| std::f32::consts::PI * d).unwrap_or(1.0);
            Vec3::new(p.x as f32, p.a as f32 / 360.0 * circ, p.z as f32)
        } else {
            Vec3::new(p.x as f32, p.y as f32, p.z as f32)
        }
    };
    // Reserve this frame's GPU vertex budget for everything shown, then thin evenly to fit
    // (decimated paths connect from the last drawn point, so nothing is left with gaps).
    let shown = |op: &ezyx_project::OperationSpec, tp: &ezyx_cam::Toolpath| app.passes.shows(ezyx_project::pipeline::toolpath_pass(op, tp));
    let (mut cuts, mut rapids) = (0usize, 0usize);
    for r in &app.results {
        let Some(op) = app.project.operations.iter().find(|o| o.id == r.op_id) else { continue };
        for tp in r.toolpaths.iter().filter(|tp| shown(op, tp)) {
            for (_, rapid) in tp.points() {
                if rapid {
                    rapids += 1;
                } else {
                    cuts += 1;
                }
            }
        }
    }
    let ctx = d.painter.ctx();
    let cut_ok = crate::gpu_guard::take_segments(ctx, cuts.min(250_000));
    let cut_stride = cuts.div_ceil(cut_ok.max(1)).max(1);
    // A dashed rapid costs up to ~10 segments.
    let rapids_on = app.show_rapids && alpha > 0.5;
    let rapid_ok = if rapids_on { crate::gpu_guard::take_segments(ctx, rapids.min(20_000) * 10) / 10 } else { 0 };
    let rapid_stride = rapids.div_ceil(rapid_ok.max(1)).max(1);
    let fade = |c: Color32| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (255.0 * alpha) as u8);
    let (mut nc, mut nr) = (0usize, 0usize);
    for r in &app.results {
        let Some(op) = app.project.operations.iter().find(|o| o.id == r.op_id) else { continue };
        let highlighted = app.selected_op == Some(r.op_id) || (app.selected_op.is_none() && op.feature_id.is_some() && op.feature_id == app.selected_feature);
        for tp in r.toolpaths.iter().filter(|tp| shown(op, tp)) {
            let rough = ezyx_project::pipeline::toolpath_pass(op, tp) == ezyx_tooldb::Operation::Roughing;
            let mut last: Option<Vec3> = None;
            for (p, rapid) in tp.points() {
                let q = to3(&p, tp.rotary);
                let Some(a) = last else {
                    last = Some(q);
                    continue;
                };
                if rapid {
                    nr += 1;
                    if rapids_on && rapid_ok > 0 && nr % rapid_stride == 0 {
                        d.dashed_reserved(a, q, Stroke::new(0.7, theme::RAPID));
                    }
                    // Even when a rapid isn't drawn, the next cut starts where it ended.
                    last = Some(q);
                } else {
                    nc += 1;
                    if cut_ok > 0 && nc % cut_stride == 0 {
                        // Roughing in warm tones, finishing in the cool depth palette.
                        let c = if rough { theme::rough_color(-(p.z as f32) / maxd) } else { theme::depth_color(-(p.z as f32) / maxd) };
                        d.line_reserved(a, q, Stroke::new(if highlighted { 1.8 } else { 1.0 }, fade(c)));
                        last = Some(q);
                    }
                }
            }
        }
    }
}

/// Snap to the grid, shape vertices / midpoints / centres and stock corners / edges / centre.
fn snap(app: &EzApp, g: Vec3, pr: &Projector) -> (Vec3, bool) {
    let p = DVec2::new(g.x as f64, g.y as f64);
    if app.prefs.snap_objects {
        // Screen-space radius → world.
        let tol = pr.to_screen(g).and_then(|s| pr.ground(s + egui::vec2(8.0, 0.0), 0.0)).map(|q| (q - g).length() as f64).unwrap_or(1.0);
        let j = &app.project.job;
        let mut cands = vec![
            DVec2::ZERO,
            DVec2::new(j.width, 0.0),
            DVec2::new(j.width, j.height),
            DVec2::new(0.0, j.height),
            DVec2::new(j.width, j.height) * 0.5,
            DVec2::new(j.width * 0.5, 0.0),
            DVec2::new(j.width * 0.5, j.height),
            DVec2::new(0.0, j.height * 0.5),
            DVec2::new(j.width, j.height * 0.5),
        ];
        for v in app.project.vectors.iter().take(2000) {
            if v.path.pts.len() <= 64 {
                cands.extend(v.path.pts.iter().copied());
                cands.extend(v.path.segments().map(|(a, b)| (a + b) * 0.5));
            }
            if v.path.closed {
                cands.push(v.path.bounds().center());
            }
        }
        if let Some(best) = cands.into_iter().filter(|c| (*c - p).length() < tol).min_by(|a, b| (*a - p).length().total_cmp(&(*b - p).length())) {
            return (Vec3::new(best.x as f32, best.y as f32, 0.0), true);
        }
    }
    if app.prefs.snap_grid && app.prefs.grid > 0.0 {
        let s = app.prefs.grid;
        return (Vec3::new(((p.x / s).round() * s) as f32, ((p.y / s).round() * s) as f32, 0.0), false);
    }
    (Vec3::new(g.x, g.y, 0.0), false)
}

#[allow(clippy::too_many_arguments)]
fn interact(app: &mut EzApp, ui: &egui::Ui, resp: &egui::Response, painter: &egui::Painter, d: &Draw, pr: &Projector, tool: DrawTool, hover: Option<Pos2>) {
    quick_keys(app, ui, tool);
    let stroke = Stroke::new(1.5, theme::ACCENT);
    match tool {
        DrawTool::Select => {
            if resp.clicked() {
                if let Some(s) = hover {
                    let hit = pick_vector(app, pr, s);
                    let shift = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                    match (hit, shift) {
                        (Some(id), true) => {
                            if let Some(i) = app.selection.iter().position(|x| *x == id) {
                                app.selection.remove(i);
                            } else {
                                app.selection.push(id);
                            }
                        }
                        (Some(id), false) => app.selection = vec![id],
                        (None, false) => {
                            app.selection.clear();
                            app.selected_feature = None;
                        }
                        _ => {}
                    }
                    if let Some(id) = hit {
                        // Selecting a shape that belongs to a feature selects the feature too.
                        app.selected_feature = app
                            .project
                            .features
                            .iter()
                            .find(|f| matches!(&f.kind, ezyx_project::FeatureKind::Cut2D { vector_ids, .. } if vector_ids.contains(&id)))
                            .map(|f| f.id);
                        app.selected_op = None;
                    }
                }
            }
            if app.mode2d {
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    app.draw.box_start = hover;
                }
                if let (Some(a), Some(b)) = (app.draw.box_start, hover) {
                    if resp.dragged_by(egui::PointerButton::Primary) {
                        painter.rect_stroke(egui::Rect::from_two_pos(a, b), 0.0, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Middle);
                    }
                    if resp.drag_stopped() {
                        let r = egui::Rect::from_two_pos(a, b);
                        let locked: Vec<usize> = app.project.layers.iter().enumerate().filter(|(_, l)| l.locked || !l.visible).map(|(i, _)| i).collect();
                        let ids: Vec<u64> = app
                            .project
                            .vectors
                            .iter()
                            .filter(|v| !locked.contains(&v.layer))
                            .filter(|v| v.path.pts.iter().all(|p| pr.to_screen(Vec3::new(p.x as f32, p.y as f32, 0.0)).map(|s| r.contains(s)).unwrap_or(false)))
                            .map(|v| v.id)
                            .collect();
                        if ui.input(|i| i.modifiers.shift) {
                            app.selection.extend(ids);
                        } else {
                            app.selection = ids;
                        }
                        app.draw.box_start = None;
                    }
                }
            }
        }
        DrawTool::Rect | DrawTool::Circle | DrawTool::Ellipse => {
            // Drag to draw, or click once to anchor and type exact sizes.
            if resp.drag_started_by(egui::PointerButton::Primary) || (resp.clicked() && app.draw.start.is_none()) {
                app.draw.start = app.cursor_world;
            }
            if let (Some(a), Some(b)) = (app.draw.start, app.cursor_world) {
                preview_shape(d, tool, a, b, stroke);
                let u = app.project.job.display_units;
                let label = match tool {
                    DrawTool::Circle => format!("r {}", u.fmt((b - a).length() as f64)),
                    _ => format!("{} × {}", u.fmt((b.x - a.x).abs() as f64), u.fmt((b.y - a.y).abs() as f64)),
                };
                if let Some(s) = pr.to_screen(b) {
                    painter.text(s + egui::vec2(12.0, -12.0), egui::Align2::LEFT_BOTTOM, label, egui::FontId::monospace(12.0), theme::ACCENT);
                }
                if resp.drag_stopped() {
                    make_shape(app, tool, a, b);
                }
            }
        }
        DrawTool::Polyline => {
            if let Some(p) = app.cursor_world {
                if resp.double_clicked() {
                    finish_polyline(app);
                } else if resp.clicked() {
                    app.draw.pts.push(p);
                }
                if !app.draw.pts.is_empty() {
                    let mut pts = app.draw.pts.clone();
                    pts.push(p);
                    d.polyline(&pts, false, stroke);
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) && app.draw.typed.is_empty() {
                finish_polyline(app);
            }
        }
        DrawTool::Text => {
            if resp.clicked() {
                app.dialogs.text_at = app.cursor_world;
            }
        }
        DrawTool::Measure => {
            if resp.clicked() {
                if let Some(p) = app.cursor_world {
                    app.draw.measure = match app.draw.measure {
                        Some((a, None)) => Some((a, Some(p))),
                        _ => Some((p, None)),
                    };
                }
            }
            if let Some((a, b)) = app.draw.measure {
                let b = b.or(app.cursor_world).unwrap_or(a);
                d.line(a, b, Stroke::new(1.8, theme::OK));
                let v = b - a;
                let u = app.project.job.display_units;
                let txt = format!("{}  ∠ {:.1}°  (Δx {}, Δy {})", u.fmt(v.length() as f64), v.y.atan2(v.x).to_degrees(), u.fmt(v.x as f64), u.fmt(v.y as f64));
                if let Some(s) = pr.to_screen((a + b) * 0.5) {
                    painter.text(s + egui::vec2(8.0, -8.0), egui::Align2::LEFT_BOTTOM, txt, egui::FontId::monospace(13.0), theme::OK);
                }
            }
        }
    }
}

fn preview_shape(d: &Draw, tool: DrawTool, a: Vec3, b: Vec3, stroke: Stroke) {
    match tool {
        DrawTool::Rect => d.polyline(&[a, Vec3::new(b.x, a.y, 0.0), b, Vec3::new(a.x, b.y, 0.0)], true, stroke),
        DrawTool::Circle => {
            let r = (b - a).length();
            let pts: Vec<Vec3> = (0..64)
                .map(|k| a + Vec3::new((k as f32 / 64.0 * std::f32::consts::TAU).cos(), (k as f32 / 64.0 * std::f32::consts::TAU).sin(), 0.0) * r)
                .collect();
            d.polyline(&pts, true, stroke);
        }
        _ => {
            let c = (a + b) * 0.5;
            let (rx, ry) = ((b.x - a.x).abs() * 0.5, (b.y - a.y).abs() * 0.5);
            let pts: Vec<Vec3> = (0..64)
                .map(|k| c + Vec3::new((k as f32 / 64.0 * std::f32::consts::TAU).cos() * rx, (k as f32 / 64.0 * std::f32::consts::TAU).sin() * ry, 0.0))
                .collect();
            d.polyline(&pts, true, stroke);
        }
    }
}

fn make_shape(app: &mut EzApp, tool: DrawTool, a: Vec3, b: Vec3) {
    let cmd = match tool {
        DrawTool::Rect => {
            let (x0, y0) = (a.x.min(b.x) as f64, a.y.min(b.y) as f64);
            let (ww, hh) = ((a.x - b.x).abs() as f64, (a.y - b.y).abs() as f64);
            (ww > 0.2 && hh > 0.2).then_some(Command::AddRectangle { x: x0, y: y0, w: ww, h: hh, corner_radius: 0.0 })
        }
        DrawTool::Circle => {
            let r = (b - a).length() as f64;
            (r > 0.2).then_some(Command::AddCircle { cx: a.x as f64, cy: a.y as f64, r })
        }
        _ => {
            let c = (a + b) * 0.5;
            let (rx, ry) = ((b.x - a.x).abs() as f64 * 0.5, (b.y - a.y).abs() as f64 * 0.5);
            (rx > 0.1 && ry > 0.1).then_some(Command::AddEllipse { cx: c.x as f64, cy: c.y as f64, rx, ry })
        }
    };
    if let Some(c) = cmd {
        app.exec(c);
        app.selection = app.project.vectors.last().map(|v| vec![v.id]).unwrap_or_default();
    }
    app.draw.start = None;
    app.draw.typed.clear();
}

/// VCarve-style quick keys: with a shape anchored, type "w,h" (rectangle / ellipse) or "r"
/// (circle) in the display units and press Enter.
fn quick_keys(app: &mut EzApp, ui: &egui::Ui, tool: DrawTool) {
    if !matches!(tool, DrawTool::Rect | DrawTool::Circle | DrawTool::Ellipse) || app.draw.start.is_none() || ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let (typed, enter, back, esc) = ui.input(|i| {
        let mut t = String::new();
        for e in &i.events {
            if let egui::Event::Text(s) = e {
                t.extend(s.chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-' | ' ' | 'x')));
            }
        }
        (t, i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Backspace), i.key_pressed(egui::Key::Escape))
    });
    app.draw.typed.push_str(&typed);
    if back {
        app.draw.typed.pop();
    }
    if esc {
        app.draw.typed.clear();
        app.draw.start = None;
    }
    if enter && !app.draw.typed.is_empty() {
        let u = app.project.job.display_units;
        let nums: Vec<f64> = app.draw.typed.split([',', ' ', 'x']).filter_map(|s| s.trim().parse::<f64>().ok()).map(|v| u.to_mm(v)).collect();
        if let (Some(a), Some(&first)) = (app.draw.start, nums.first()) {
            let second = nums.get(1).copied().unwrap_or(first);
            let b = match tool {
                DrawTool::Circle => a + Vec3::new(first as f32, 0.0, 0.0),
                DrawTool::Ellipse => a + Vec3::new(first as f32 * 2.0, second as f32 * 2.0, 0.0),
                _ => a + Vec3::new(first as f32, second as f32, 0.0),
            };
            make_shape(app, tool, a, b);
        }
        app.draw.typed.clear();
    }
}

fn finish_polyline(app: &mut EzApp) {
    let mut pts = std::mem::take(&mut app.draw.pts);
    pts.dedup_by(|a, b| (*a - *b).length() < 1e-3);
    if pts.len() >= 2 {
        let closed = pts.len() > 2 && (pts[0] - *pts.last().unwrap()).length() < 2.0;
        if closed {
            pts.pop();
        }
        let points = pts.iter().map(|p| [p.x as f64, p.y as f64]).collect();
        app.exec(Command::AddPolyline { points, closed });
    }
}

fn pick_vector(app: &EzApp, pr: &Projector, s: Pos2) -> Option<u64> {
    let blocked: Vec<usize> = app.project.layers.iter().enumerate().filter(|(_, l)| l.locked || !l.visible).map(|(i, _)| i).collect();
    let mut best: Option<(f32, u64)> = None;
    for v in app.project.vectors.iter().filter(|v| !blocked.contains(&v.layer)) {
        let pts: Vec<Pos2> = v.path.pts.iter().filter_map(|p| pr.to_screen(Vec3::new(p.x as f32, p.y as f32, 0.0))).collect();
        let n = pts.len();
        if n < 2 {
            continue;
        }
        let segs = if v.path.closed { n } else { n - 1 };
        for i in 0..segs {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let ab = b - a;
            let t = ((s - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
            let dist = (a + ab * t - s).length();
            if dist < 8.0 && best.map(|(bd, _)| dist < bd).unwrap_or(true) {
                best = Some((dist, v.id));
            }
        }
        // Clicking inside a closed shape also selects it (beginner friendly).
        if v.path.closed {
            if let Some(g) = pr.ground(s, 0.0) {
                if v.path.contains(DVec2::new(g.x as f64, g.y as f64)) && best.is_none() {
                    best = Some((7.9, v.id));
                }
            }
        }
    }
    best.map(|(_, id)| id)
}

fn relief_mesh(app: &mut EzApp, r: &ReliefObject) -> TriMesh {
    let key = format!("relief{}", r.id);
    if let Some((rev, m)) = app.display.get(&key) {
        if *rev == app.rev {
            return m.clone();
        }
    }
    // ~2 triangles per displayed cell: pick the step that fits the per-mesh GPU budget.
    let cells = (r.heightmap.width * r.heightmap.height) as f64;
    let by_budget = (2.0 * cells / app.budget.mesh_triangles.max(1) as f64).sqrt().ceil() as usize;
    let lod = (r.heightmap.width.max(r.heightmap.height) / 400).max(by_budget).max(1);
    let m = ezyx_mesh::hm::surface(&r.heightmap, lod);
    app.display.insert(key, (app.rev, m.clone()));
    m
}

fn overlay(app: &mut EzApp, ui: &mut egui::Ui, rect: egui::Rect) {
    egui::Area::new(egui::Id::unique("pipeline_vp_overlay"))
        .fixed_pos(egui::pos2((rect.right() - 430.0).max(rect.left() + 8.0), rect.top() + 8.0))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::NONE.fill(ui.visuals().window_fill.gamma_multiply(0.92)).corner_radius(8).inner_margin(6).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let label = if app.mode2d { "2D" } else { "3D" };
                    if ui
                        .add(egui::Button::new(RichText::new(label).strong()).fill(theme::ACCENT_SOFT))
                        .on_hover_text("Toggle 2D drafting / 3D orbit (Tab)")
                        .clicked()
                    {
                        let m = !app.mode2d;
                        app.set_mode2d(m);
                    }
                    for (txt, v) in [("Top", ViewPreset::Top), ("Front", ViewPreset::Front), ("Right", ViewPreset::Right), ("Iso", ViewPreset::Iso)] {
                        if ui.small_button(txt).clicked() {
                            if app.mode2d && v != ViewPreset::Top {
                                app.mode2d = false;
                                app.cam.ortho = false;
                            }
                            app.cam.preset(v);
                        }
                    }
                    if ui.small_button("⛶").on_hover_text("Fit (F)").clicked() {
                        app.fit_view();
                    }
                    ui.toggle_value(&mut app.show_toolpaths, "🛠").on_hover_text("Show toolpaths");
                    let mut snap_changed = ui.toggle_value(&mut app.prefs.snap_grid, "#").on_hover_text("Snap to grid").changed();
                    snap_changed |= ui.toggle_value(&mut app.prefs.snap_objects, "⌖").on_hover_text("Snap to shapes and stock").changed();
                    if snap_changed {
                        app.prefs.save();
                    }
                    if !app.mode2d {
                        ui.toggle_value(&mut app.cam.ortho, "▦").on_hover_text("Orthographic");
                    }
                });
                let hint =
                    if app.mode2d { "LMB select/draw · RMB/MMB pan · wheel zoom · Alt: no snap" } else { "LMB orbit · RMB/MMB/Shift pan · wheel zoom" };
                ui.label(RichText::new(hint).small().color(theme::STEEL));
            });
        });
}
