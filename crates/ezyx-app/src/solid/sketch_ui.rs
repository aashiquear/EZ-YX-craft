//! Sketch mode of the Solid Modeler: a 2D canvas on the sketch plane with line / rectangle /
//! circle / arc / spline / point tools, point snapping (shared points = coincident), automatic
//! horizontal / vertical inference, the constraint toolbar, driving dimensions (values or
//! parameter expressions) and live solver status.

use super::finish_sketch;
use crate::app::EzApp;
use crate::ribbon::tip;
use crate::theme;
use egui::{Color32, Pos2, RichText, Stroke, Ui};
use ezyx_cad::{BooleanOp, Constraint, Dim, Entity, SPoint, Sketch, SketchPlane, SolveReport, SolveStatus};
use ezyx_core::DVec2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkTool {
    Select,
    Line,
    Rect,
    Circle,
    Arc,
    Spline,
    Point,
}

pub struct SketchEdit {
    /// Feature whose sketch is being edited (`None` = a new feature on finish).
    pub feature: Option<usize>,
    pub sketch: Sketch,
    pub tool: SkTool,
    pub sel_pts: Vec<usize>,
    pub sel_ents: Vec<usize>,
    chain: Option<usize>,
    pending: Vec<usize>,
    pub report: Option<SolveReport>,
    center: DVec2,
    scale: f32,
    fitted: bool,
    drag: Option<usize>,
    pub op: BooleanOp,
    pub depth: f64,
    pub finish_as_revolve: bool,
    pub error: Option<String>,
}

impl SketchEdit {
    pub fn new(feature: Option<usize>, sketch: Sketch) -> Self {
        Self {
            feature,
            sketch,
            tool: SkTool::Line,
            sel_pts: vec![],
            sel_ents: vec![],
            chain: None,
            pending: vec![],
            report: None,
            center: DVec2::ZERO,
            scale: 4.0,
            fitted: false,
            drag: None,
            op: BooleanOp::Add,
            depth: 10.0,
            finish_as_revolve: false,
            error: None,
        }
    }
}

fn solve(app: &mut EzApp) {
    let vars = app.solid.draft.as_ref().and_then(|p| p.parameter_values().ok()).unwrap_or_default();
    if let Some(e) = app.solid.sketch.as_mut() {
        match e.sketch.solve_with(&vars) {
            Ok(r) => {
                e.report = Some(r);
                e.error = None;
            }
            Err(err) => e.error = Some(err.to_string()),
        }
    }
}

/// Catmull-Rom samples through control points (display only; the kernel does the same).
fn spline_pts(p: &[DVec2], closed: bool) -> Vec<DVec2> {
    let n = p.len();
    if n < 3 {
        return p.to_vec();
    }
    let get = |i: isize| -> DVec2 {
        if closed {
            p[i.rem_euclid(n as isize) as usize]
        } else {
            p[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let segs = if closed { n } else { n - 1 };
    let mut out = Vec::new();
    for i in 0..segs as isize {
        let (p0, p1, p2, p3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
        for k in 0..12 {
            let t = k as f64 / 12.0;
            let (t2, t3) = (t * t, t * t * t);
            out.push(0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3));
        }
    }
    if !closed {
        out.push(p[n - 1]);
    }
    out
}

fn arc_pts(c: DVec2, s: DVec2, e: DVec2) -> Vec<DVec2> {
    let r = (s - c).length();
    let a0 = (s - c).y.atan2((s - c).x);
    let mut a1 = (e - c).y.atan2((e - c).x);
    if a1 <= a0 {
        a1 += std::f64::consts::TAU;
    }
    (0..=32).map(|k| c + DVec2::from_angle(a0 + (a1 - a0) * k as f64 / 32.0) * r).collect()
}

/// Screen-space polyline of an entity.
fn entity_pts(sk: &Sketch, e: &Entity) -> (Vec<DVec2>, bool) {
    let p = |i: usize| sk.points.get(i).map(|q| q.pos()).unwrap_or_default();
    match e {
        Entity::Line { a, b, .. } => (vec![p(*a), p(*b)], false),
        Entity::Circle { center, radius, .. } => {
            ((0..64).map(|k| p(*center) + DVec2::from_angle(k as f64 / 64.0 * std::f64::consts::TAU) * *radius).collect(), true)
        }
        Entity::Arc { center, start, end, .. } => (arc_pts(p(*center), p(*start), p(*end)), false),
        Entity::Spline { ctrl, closed, .. } => (spline_pts(&ctrl.iter().map(|i| p(*i)).collect::<Vec<_>>(), *closed), *closed),
        Entity::Polyline { pts, closed, .. } => (pts.iter().map(|q| DVec2::new(q[0], q[1])).collect(), *closed),
    }
}

fn is_line(sk: &Sketch, e: usize) -> bool {
    matches!(sk.entities.get(e), Some(Entity::Line { .. }))
}
fn is_round(sk: &Sketch, e: usize) -> bool {
    matches!(sk.entities.get(e), Some(Entity::Circle { .. } | Entity::Arc { .. }))
}

fn line_len(sk: &Sketch, e: usize) -> f64 {
    match sk.entities.get(e) {
        Some(Entity::Line { a, b, .. }) => (sk.point(*a) - sk.point(*b)).length(),
        _ => 0.0,
    }
}
fn radius_of(sk: &Sketch, e: usize) -> f64 {
    match sk.entities.get(e) {
        Some(Entity::Circle { radius, .. }) => *radius,
        Some(Entity::Arc { center, start, .. }) => (sk.point(*start) - sk.point(*center)).length(),
        _ => 0.0,
    }
}
fn line_dir(sk: &Sketch, e: usize) -> DVec2 {
    match sk.entities.get(e) {
        Some(Entity::Line { a, b, .. }) => (sk.point(*b) - sk.point(*a)).normalize_or_zero(),
        _ => DVec2::X,
    }
}

/// Re-index constraints after deleting entities / points (`None` = drop the constraint).
fn remap(c: &Constraint, em: &dyn Fn(usize) -> Option<usize>, pm: &dyn Fn(usize) -> Option<usize>) -> Option<Constraint> {
    use Constraint::*;
    Some(match c {
        Coincident(p, q) => Coincident(pm(*p)?, pm(*q)?),
        Horizontal(e) => Horizontal(em(*e)?),
        Vertical(e) => Vertical(em(*e)?),
        Distance(p, q, d) => Distance(pm(*p)?, pm(*q)?, d.clone()),
        HorizontalDistance(p, q, d) => HorizontalDistance(pm(*p)?, pm(*q)?, d.clone()),
        VerticalDistance(p, q, d) => VerticalDistance(pm(*p)?, pm(*q)?, d.clone()),
        Length(e, d) => Length(em(*e)?, d.clone()),
        Radius(e, d) => Radius(em(*e)?, d.clone()),
        Diameter(e, d) => Diameter(em(*e)?, d.clone()),
        Angle(a, b, d) => Angle(em(*a)?, em(*b)?, d.clone()),
        Parallel(a, b) => Parallel(em(*a)?, em(*b)?),
        Perpendicular(a, b) => Perpendicular(em(*a)?, em(*b)?),
        Equal(a, b) => Equal(em(*a)?, em(*b)?),
        Tangent(a, b) => Tangent(em(*a)?, em(*b)?),
        Concentric(a, b) => Concentric(em(*a)?, em(*b)?),
        PointOnLine(p, e) => PointOnLine(pm(*p)?, em(*e)?),
        PointOnCircle(p, e) => PointOnCircle(pm(*p)?, em(*e)?),
        Midpoint(p, e) => Midpoint(pm(*p)?, em(*e)?),
        Symmetric(p, q, e) => Symmetric(pm(*p)?, pm(*q)?, em(*e)?),
        Fix(p) => Fix(pm(*p)?),
    })
}

fn delete_selection(e: &mut SketchEdit) {
    let sk = &mut e.sketch;
    let dead_e: Vec<usize> = e.sel_ents.clone();
    // Points go when selected, or when only dead entities used them.
    let mut dead_p: Vec<usize> = e.sel_pts.clone();
    let used = |sk: &Sketch, p: usize, skip: &[usize]| {
        sk.entities.iter().enumerate().filter(|(i, _)| !skip.contains(i)).any(|(_, en)| match en {
            Entity::Line { a, b, .. } => *a == p || *b == p,
            Entity::Circle { center, .. } => *center == p,
            Entity::Arc { center, start, end, .. } => *center == p || *start == p || *end == p,
            Entity::Spline { ctrl, .. } => ctrl.contains(&p),
            Entity::Polyline { .. } => false,
        })
    };
    for p in 0..sk.points.len() {
        if !used(sk, p, &dead_e) && used(sk, p, &[]) {
            dead_p.push(p);
        }
    }
    // Entities that use a deleted point go too.
    let mut dead_e = dead_e;
    for (i, en) in sk.entities.iter().enumerate() {
        let pts: Vec<usize> = match en {
            Entity::Line { a, b, .. } => vec![*a, *b],
            Entity::Circle { center, .. } => vec![*center],
            Entity::Arc { center, start, end, .. } => vec![*center, *start, *end],
            Entity::Spline { ctrl, .. } => ctrl.clone(),
            Entity::Polyline { .. } => vec![],
        };
        if pts.iter().any(|p| dead_p.contains(p)) && !dead_e.contains(&i) {
            dead_e.push(i);
        }
    }
    let emap = |i: usize| (!dead_e.contains(&i)).then(|| i - dead_e.iter().filter(|d| **d < i).count());
    let pmap = |i: usize| (!dead_p.contains(&i)).then(|| i - dead_p.iter().filter(|d| **d < i).count());
    sk.constraints = sk.constraints.iter().filter_map(|c| remap(c, &emap, &pmap)).collect();
    let fix = |p: &mut usize| *p = pmap(*p).unwrap_or(0);
    let mut ents = Vec::new();
    for (i, mut en) in std::mem::take(&mut sk.entities).into_iter().enumerate() {
        if dead_e.contains(&i) {
            continue;
        }
        match &mut en {
            Entity::Line { a, b, .. } => {
                fix(a);
                fix(b)
            }
            Entity::Circle { center, .. } => fix(center),
            Entity::Arc { center, start, end, .. } => {
                fix(center);
                fix(start);
                fix(end)
            }
            Entity::Spline { ctrl, .. } => ctrl.iter_mut().for_each(fix),
            Entity::Polyline { .. } => {}
        }
        ents.push(en);
    }
    sk.entities = ents;
    let mut i = 0;
    sk.points.retain(|_| {
        let keep = !dead_p.contains(&i);
        i += 1;
        keep
    });
    e.sel_pts.clear();
    e.sel_ents.clear();
    e.chain = None;
    e.pending.clear();
}

pub fn canvas(app: &mut EzApp, ui: &mut Ui) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
    let rect = resp.rect;
    painter.rect_filled(rect, 0.0, theme::viewport_bg(app.prefs.dark));
    let dark = app.prefs.dark;
    let mut changed = false;
    {
        let Some(e) = app.solid.sketch.as_mut() else { return };
        if !e.fitted {
            e.fitted = true;
            let mut lo = DVec2::splat(-40.0);
            let mut hi = DVec2::splat(40.0);
            for p in &e.sketch.points {
                lo = lo.min(p.pos() - 10.0);
                hi = hi.max(p.pos() + 10.0);
            }
            e.center = (lo + hi) * 0.5;
            e.scale = ((rect.width() as f64 / (hi.x - lo.x)).min(rect.height() as f64 / (hi.y - lo.y)) * 0.8) as f32;
        }
        // Navigation: RMB/MMB pan, wheel zoom.
        if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) {
            let d = resp.drag_delta();
            e.center -= DVec2::new(d.x as f64, -d.y as f64) / e.scale as f64;
        }
        if resp.hovered() {
            let z = ui.input(|i| i.smooth_scroll_delta.y);
            if z.abs() > 0.0 {
                e.scale = (e.scale * (z * 0.0015).exp()).clamp(0.05, 200.0);
            }
        }
    }
    let e = app.solid.sketch.as_ref().unwrap();
    let (center, scale) = (e.center, e.scale);
    let to_s = |p: DVec2| Pos2::new(rect.center().x + ((p.x - center.x) as f32) * scale, rect.center().y - ((p.y - center.y) as f32) * scale);
    let to_w = |s: Pos2| DVec2::new(center.x + ((s.x - rect.center().x) / scale) as f64, center.y - ((s.y - rect.center().y) / scale) as f64);
    // Grid + axes.
    let step = if scale > 4.0 {
        5.0
    } else if scale > 1.0 {
        10.0
    } else {
        50.0
    };
    let gcol = if dark { Color32::from_gray(60) } else { Color32::from_rgba_unmultiplied(0, 0, 0, 20) };
    let (wlo, whi) = (to_w(rect.left_bottom()), to_w(rect.right_top()));
    let mut x = (wlo.x / step).floor() * step;
    while x < whi.x {
        painter.line_segment([to_s(DVec2::new(x, wlo.y)), to_s(DVec2::new(x, whi.y))], Stroke::new(0.5, gcol));
        x += step;
    }
    let mut y = (wlo.y / step).floor() * step;
    while y < whi.y {
        painter.line_segment([to_s(DVec2::new(wlo.x, y)), to_s(DVec2::new(whi.x, y))], Stroke::new(0.5, gcol));
        y += step;
    }
    painter.line_segment([to_s(DVec2::new(wlo.x, 0.0)), to_s(DVec2::new(whi.x, 0.0))], Stroke::new(1.0, Color32::from_rgb(220, 90, 90)));
    painter.line_segment([to_s(DVec2::new(0.0, wlo.y)), to_s(DVec2::new(0.0, whi.y))], Stroke::new(1.0, Color32::from_rgb(90, 180, 90)));
    let sk = &e.sketch;
    let fully = matches!(e.report.as_ref().map(|r| &r.status), Some(SolveStatus::FullyDefined));
    let base = if fully {
        if dark {
            Color32::from_gray(235)
        } else {
            Color32::BLACK
        }
    } else {
        Color32::from_rgb(40, 110, 220)
    };
    for (i, en) in sk.entities.iter().enumerate() {
        let (pts, closed) = entity_pts(sk, en);
        let mut s: Vec<Pos2> = pts.iter().map(|p| to_s(*p)).collect();
        if closed && !s.is_empty() {
            s.push(s[0]);
        }
        let col = if e.sel_ents.contains(&i) { theme::ACCENT } else { base };
        if en.construction() {
            for w in s.windows(2) {
                painter.extend(egui::Shape::dashed_line(&[w[0], w[1]], Stroke::new(1.0, col), 5.0, 4.0));
            }
        } else {
            painter.add(egui::Shape::line(s, Stroke::new(if e.sel_ents.contains(&i) { 2.6 } else { 1.8 }, col)));
        }
    }
    for (i, p) in sk.points.iter().enumerate() {
        let s = to_s(p.pos());
        let col = if e.sel_pts.contains(&i) { theme::ACCENT } else { base };
        if p.fixed {
            painter.rect_filled(egui::Rect::from_center_size(s, egui::vec2(7.0, 7.0)), 0.0, col);
        } else {
            painter.circle_filled(s, 3.0, col);
        }
    }
    // Constraint glyphs / dimensions.
    let mid = |en: usize| -> DVec2 {
        match sk.entities.get(en) {
            Some(Entity::Line { a, b, .. }) => (sk.point(*a) + sk.point(*b)) * 0.5,
            Some(Entity::Circle { center, radius, .. }) => sk.point(*center) + DVec2::new(radius * 0.7, radius * 0.7),
            Some(Entity::Arc { center, start, .. }) => (sk.point(*center) + sk.point(*start)) * 0.5,
            _ => DVec2::ZERO,
        }
    };
    let gcolr = Color32::from_rgb(30, 140, 80);
    // Stack labels that share an anchor so they don't overprint.
    let mut used: Vec<(Pos2, u32)> = Vec::new();
    for c in &sk.constraints {
        let (at, txt) = match c {
            Constraint::Horizontal(e) => (mid(*e), "H".to_string()),
            Constraint::Vertical(e) => (mid(*e), "V".to_string()),
            Constraint::Parallel(a, _) => (mid(*a), "∥".into()),
            Constraint::Perpendicular(a, _) => (mid(*a), "⊥".into()),
            Constraint::Equal(a, _) => (mid(*a), "=".into()),
            Constraint::Tangent(a, _) => (mid(*a), "T".into()),
            Constraint::Concentric(a, _) => (mid(*a), "◎".into()),
            Constraint::Fix(p) => (sk.point(*p), "⚓".into()),
            Constraint::Length(e, d) => (mid(*e), format!("{:.2}{}", d.value, d.expr.as_ref().map(|x| format!(" = {x}")).unwrap_or_default())),
            Constraint::Radius(e, d) => (mid(*e), format!("R{:.2}", d.value)),
            Constraint::Diameter(e, d) => (mid(*e), format!("⌀{:.2}", d.value)),
            Constraint::Angle(a, _, d) => (mid(*a), format!("∠{:.1}°", d.value)),
            Constraint::Distance(p, q, d) | Constraint::HorizontalDistance(p, q, d) | Constraint::VerticalDistance(p, q, d) => {
                ((sk.point(*p) + sk.point(*q)) * 0.5, format!("{:.2}", d.value))
            }
            _ => continue,
        };
        let s = to_s(at);
        let k = match used.iter_mut().find(|(p, _)| (*p - s).length() < 20.0) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                used.push((s, 0));
                0
            }
        };
        painter.text(s + egui::vec2(6.0, -6.0 + 15.0 * k as f32), egui::Align2::LEFT_BOTTOM, txt, egui::FontId::proportional(12.0), gcolr);
    }
    // Status.
    let status = match (&e.report, &e.error) {
        (_, Some(err)) => (format!("⚠ {err}"), theme::DANGER),
        (Some(r), _) => match r.status {
            SolveStatus::FullyDefined => ("✔ Fully defined".to_string(), theme::OK),
            SolveStatus::UnderDefined(n) => (format!("Under-defined: {n} degree(s) of freedom left (blue)"), Color32::from_rgb(40, 110, 220)),
            SolveStatus::OverDefined => ("⚠ Over-defined: remove a redundant or conflicting constraint".to_string(), theme::DANGER),
            SolveStatus::Failed => ("✖ Solver failed — check the dimensions".to_string(), theme::DANGER),
        },
        _ => ("Draw, then constrain and dimension".to_string(), theme::STEEL),
    };
    painter.text(rect.left_top() + egui::vec2(10.0, 8.0), egui::Align2::LEFT_TOP, &status.0, egui::FontId::proportional(14.0), status.1);
    painter.text(
        rect.left_bottom() + egui::vec2(10.0, -8.0),
        egui::Align2::LEFT_BOTTOM,
        format!("Plane {:?} · LMB draw/select · RMB/MMB pan · wheel zoom · Esc ends a chain · Del deletes", e.sketch.plane),
        egui::FontId::proportional(11.0),
        theme::STEEL,
    );

    // ---- interaction
    let hover = resp.hover_pos();
    let pick_point = |sk: &Sketch, s: Pos2| -> Option<usize> {
        sk.points
            .iter()
            .enumerate()
            .map(|(i, p)| (i, (to_s(p.pos()) - s).length()))
            .filter(|(_, d)| *d < 9.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let pick_entity = |sk: &Sketch, s: Pos2| -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, en) in sk.entities.iter().enumerate() {
            let (pts, closed) = entity_pts(sk, en);
            let sp: Vec<Pos2> = pts.iter().map(|p| to_s(*p)).collect();
            let n = sp.len();
            let segs = if closed { n } else { n.saturating_sub(1) };
            for k in 0..segs {
                let (a, b) = (sp[k], sp[(k + 1) % n]);
                let ab = b - a;
                let t = ((s - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                let d = (a + ab * t - s).length();
                if d < 7.0 && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                    best = Some((i, d));
                }
            }
        }
        best.map(|(i, _)| i)
    };
    let tool = e.tool;
    let shift = ui.input(|i| i.modifiers.shift);
    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let del = ui.input(|i| i.key_pressed(egui::Key::Delete)) && !ui.ctx().egui_wants_keyboard_input();
    let grid = 0.5;
    let snap = |w: DVec2| DVec2::new((w.x / grid).round() * grid, (w.y / grid).round() * grid);
    let e = app.solid.sketch.as_mut().unwrap();
    if esc {
        if e.chain.is_some() || !e.pending.is_empty() {
            e.chain = None;
            e.pending.clear();
        } else {
            e.tool = SkTool::Select;
        }
    }
    if del && (!e.sel_pts.is_empty() || !e.sel_ents.is_empty()) {
        delete_selection(e);
        changed = true;
    }
    let get_or_add = |e: &mut SketchEdit, s: Pos2| -> usize {
        if let Some(i) = pick_point(&e.sketch, s) {
            return i;
        }
        let w = snap(to_w(s));
        e.sketch.points.push(SPoint::new(w.x, w.y));
        e.sketch.points.len() - 1
    };
    // Rubber band preview.
    if let Some(h) = hover {
        let anchor = e.chain.or_else(|| e.pending.first().copied());
        if let Some(a) = anchor.filter(|_| matches!(tool, SkTool::Line | SkTool::Rect | SkTool::Circle | SkTool::Arc | SkTool::Spline)) {
            let pa = e.sketch.point(a);
            let ph = snap(to_w(h));
            let st = Stroke::new(1.0, theme::ACCENT);
            match tool {
                SkTool::Rect => {
                    let r = egui::Rect::from_two_pos(to_s(pa), to_s(ph));
                    painter.rect_stroke(r, 0.0, st, egui::StrokeKind::Middle);
                }
                SkTool::Circle => {
                    painter.circle_stroke(to_s(pa), ((ph - pa).length() as f32) * scale, st);
                }
                _ => {
                    painter.line_segment([to_s(pa), to_s(ph)], st);
                }
            }
            painter.text(
                h + egui::vec2(12.0, -12.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{:.2}", (ph - pa).length()),
                egui::FontId::monospace(12.0),
                theme::ACCENT,
            );
        }
    }
    match tool {
        SkTool::Select => {
            if resp.drag_started_by(egui::PointerButton::Primary) {
                e.drag = hover.and_then(|s| pick_point(&e.sketch, s)).filter(|i| !e.sketch.points[*i].fixed);
            }
            if let (Some(i), Some(h)) = (e.drag, hover) {
                if resp.dragged_by(egui::PointerButton::Primary) {
                    let w = to_w(h);
                    e.sketch.points[i].x = w.x;
                    e.sketch.points[i].y = w.y;
                    changed = true;
                }
            }
            if resp.drag_stopped() {
                e.drag = None;
            }
            if resp.clicked() {
                if let Some(s) = hover {
                    if !shift {
                        e.sel_pts.clear();
                        e.sel_ents.clear();
                    }
                    if let Some(p) = pick_point(&e.sketch, s) {
                        if let Some(k) = e.sel_pts.iter().position(|x| *x == p) {
                            e.sel_pts.remove(k);
                        } else {
                            e.sel_pts.push(p);
                        }
                    } else if let Some(en) = pick_entity(&e.sketch, s) {
                        if let Some(k) = e.sel_ents.iter().position(|x| *x == en) {
                            e.sel_ents.remove(k);
                        } else {
                            e.sel_ents.push(en);
                        }
                    }
                }
            }
        }
        SkTool::Point => {
            if let (true, Some(s)) = (resp.clicked(), hover) {
                get_or_add(e, s);
                changed = true;
            }
        }
        SkTool::Line => {
            if resp.secondary_clicked() {
                e.chain = None;
            }
            if let (true, Some(s)) = (resp.clicked(), hover) {
                let p = get_or_add(e, s);
                if let Some(prev) = e.chain.filter(|prev| *prev != p) {
                    let li = e.sketch.add_line(prev, p);
                    // Inference: nearly horizontal / vertical lines get the constraint.
                    let d = e.sketch.point(p) - e.sketch.point(prev);
                    if d.y.abs() < d.length() * 0.035 {
                        e.sketch.constrain(Constraint::Horizontal(li));
                    } else if d.x.abs() < d.length() * 0.035 {
                        e.sketch.constrain(Constraint::Vertical(li));
                    }
                    changed = true;
                }
                // Clicking back on an existing point that already had lines closes the chain.
                let closes =
                    e.chain.is_some() && e.sketch.entities.iter().filter(|en| matches!(en, Entity::Line { a, b, .. } if *a == p || *b == p)).count() >= 2;
                e.chain = if closes { None } else { Some(p) };
            }
        }
        SkTool::Rect => {
            if let (true, Some(s)) = (resp.clicked(), hover) {
                let p = get_or_add(e, s);
                if let Some(a) = e.pending.pop() {
                    let (pa, pb) = (e.sketch.point(a), e.sketch.point(p));
                    let b = e.sketch.add_point(pb.x, pa.y);
                    let d = e.sketch.add_point(pa.x, pb.y);
                    let l0 = e.sketch.add_line(a, b);
                    let l1 = e.sketch.add_line(b, p);
                    let l2 = e.sketch.add_line(p, d);
                    let l3 = e.sketch.add_line(d, a);
                    for c in [Constraint::Horizontal(l0), Constraint::Vertical(l1), Constraint::Horizontal(l2), Constraint::Vertical(l3)] {
                        e.sketch.constrain(c);
                    }
                    changed = true;
                } else {
                    e.pending.push(p);
                }
            }
        }
        SkTool::Circle => {
            if let (true, Some(s)) = (resp.clicked(), hover) {
                if let Some(c) = e.pending.pop() {
                    let r = (snap(to_w(s)) - e.sketch.point(c)).length().max(0.1);
                    e.sketch.entities.push(Entity::Circle { center: c, radius: r, construction: false });
                    changed = true;
                } else {
                    let c = get_or_add(e, s);
                    e.pending.push(c);
                }
            }
        }
        SkTool::Arc => {
            if let (true, Some(s)) = (resp.clicked(), hover) {
                if e.pending.len() == 2 {
                    let (c, st) = (e.pending[0], e.pending[1]);
                    let r = (e.sketch.point(st) - e.sketch.point(c)).length();
                    let dir = (to_w(s) - e.sketch.point(c)).normalize_or_zero();
                    let end = pick_point(&e.sketch, s).unwrap_or_else(|| {
                        let q = e.sketch.point(c) + dir * r;
                        e.sketch.add_point(q.x, q.y)
                    });
                    e.sketch.add_arc(c, st, end);
                    e.pending.clear();
                    changed = true;
                } else {
                    let p = get_or_add(e, s);
                    e.pending.push(p);
                }
            }
        }
        SkTool::Spline => {
            if let (true, Some(s)) = (resp.clicked(), hover) {
                let p = get_or_add(e, s);
                e.pending.push(p);
            }
            if (resp.double_clicked() || resp.secondary_clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter))) && e.pending.len() >= 2 {
                let mut ctrl = std::mem::take(&mut e.pending);
                ctrl.dedup();
                let closed = ctrl.len() > 3 && ctrl.first() == ctrl.last();
                if closed {
                    ctrl.pop();
                }
                e.sketch.add_spline(ctrl, closed);
                changed = true;
            }
        }
    }
    if changed {
        solve(app);
    }
}

pub fn props(app: &mut EzApp, ui: &mut Ui) {
    if app.solid.sketch.as_ref().map(|e| e.report.is_none() && e.error.is_none()).unwrap_or(false) {
        solve(app);
    }
    let units = app.project.job.display_units;
    let Some(e) = app.solid.sketch.as_mut() else { return };
    ui.heading(if e.feature.is_some() { "Edit sketch" } else { "New sketch" });
    ui.label(RichText::new(tip("solid.sketch")).small().color(theme::STEEL));
    ui.separator();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Plane");
        for pl in [SketchPlane::XY, SketchPlane::XZ, SketchPlane::YZ] {
            let l = match pl {
                SketchPlane::XY => "Top (XY)",
                SketchPlane::XZ => "Front (XZ)",
                SketchPlane::YZ => "Right (YZ)",
            };
            changed |= ui.selectable_value(&mut e.sketch.plane, pl, l).changed();
        }
    });
    ui.horizontal(|ui| {
        ui.label("Offset");
        changed |= crate::panels::len_field(ui, &mut e.sketch.offset, units, -1000.0..=1000.0).changed();
    });
    ui.label(RichText::new("Tools").strong());
    ui.horizontal_wrapped(|ui| {
        for (t, l) in [
            (SkTool::Select, "⬉ Select"),
            (SkTool::Line, "╱ Line"),
            (SkTool::Rect, "▭ Rectangle"),
            (SkTool::Circle, "◯ Circle"),
            (SkTool::Arc, "◠ Arc"),
            (SkTool::Spline, "∿ Spline"),
            (SkTool::Point, "• Point"),
        ] {
            if ui.selectable_label(e.tool == t, l).clicked() {
                e.tool = t;
                e.chain = None;
                e.pending.clear();
            }
        }
    });
    // Constraint toolbar from the current selection.
    ui.label(RichText::new(format!("Constrain ({} point(s), {} entit(ies) selected)", e.sel_pts.len(), e.sel_ents.len())).strong());
    let sk = &e.sketch;
    let (pts, ents) = (e.sel_pts.clone(), e.sel_ents.clone());
    let lines: Vec<usize> = ents.iter().copied().filter(|i| is_line(sk, *i)).collect();
    let rounds: Vec<usize> = ents.iter().copied().filter(|i| is_round(sk, *i)).collect();
    let mut add: Vec<Constraint> = Vec::new();
    ui.horizontal_wrapped(|ui| {
        let mut b = |ui: &mut Ui, ok: bool, label: &str, c: &mut dyn FnMut() -> Vec<Constraint>| {
            if ui.add_enabled(ok, egui::Button::new(label)).clicked() {
                add.extend(c());
            }
        };
        b(ui, !lines.is_empty() && pts.is_empty() && rounds.is_empty(), "H", &mut || lines.iter().map(|l| Constraint::Horizontal(*l)).collect());
        b(ui, !lines.is_empty() && pts.is_empty() && rounds.is_empty(), "V", &mut || lines.iter().map(|l| Constraint::Vertical(*l)).collect());
        b(ui, lines.len() == 2 && ents.len() == 2, "∥ Parallel", &mut || vec![Constraint::Parallel(lines[0], lines[1])]);
        b(ui, lines.len() == 2 && ents.len() == 2, "⊥ Perpendicular", &mut || vec![Constraint::Perpendicular(lines[0], lines[1])]);
        b(ui, ents.len() == 2 && (lines.len() == 2 || rounds.len() == 2), "= Equal", &mut || vec![Constraint::Equal(ents[0], ents[1])]);
        b(ui, lines.len() == 1 && rounds.len() == 1 || rounds.len() == 2, "T Tangent", &mut || vec![Constraint::Tangent(ents[0], ents[1])]);
        b(ui, rounds.len() == 2, "◎ Concentric", &mut || vec![Constraint::Concentric(rounds[0], rounds[1])]);
        b(ui, pts.len() == 2 && ents.is_empty(), "⊙ Coincident", &mut || vec![Constraint::Coincident(pts[0], pts[1])]);
        b(ui, pts.len() == 1 && lines.len() == 1, "Midpoint", &mut || vec![Constraint::Midpoint(pts[0], lines[0])]);
        b(ui, pts.len() == 1 && lines.len() == 1, "On line", &mut || vec![Constraint::PointOnLine(pts[0], lines[0])]);
        b(ui, pts.len() == 1 && rounds.len() == 1, "On circle", &mut || vec![Constraint::PointOnCircle(pts[0], rounds[0])]);
        b(ui, pts.len() == 2 && lines.len() == 1, "Symmetric", &mut || vec![Constraint::Symmetric(pts[0], pts[1], lines[0])]);
        b(ui, !pts.is_empty() && ents.is_empty(), "⚓ Fix", &mut || pts.iter().map(|p| Constraint::Fix(*p)).collect());
    });
    ui.label(RichText::new("Dimensions (start from the current size, edit below)").strong());
    ui.horizontal_wrapped(|ui| {
        let mut b = |ui: &mut Ui, ok: bool, label: &str, c: &mut dyn FnMut() -> Constraint| {
            if ui.add_enabled(ok, egui::Button::new(label)).clicked() {
                add.push(c());
            }
        };
        b(ui, lines.len() == 1 && ents.len() == 1, "↔ Length", &mut || Constraint::Length(lines[0], Dim::new(line_len(sk, lines[0]))));
        b(ui, rounds.len() == 1 && ents.len() == 1, "R Radius", &mut || Constraint::Radius(rounds[0], Dim::new(radius_of(sk, rounds[0]))));
        b(ui, rounds.len() == 1 && ents.len() == 1, "⌀ Diameter", &mut || Constraint::Diameter(rounds[0], Dim::new(2.0 * radius_of(sk, rounds[0]))));
        b(ui, pts.len() == 2 && ents.is_empty(), "Distance", &mut || {
            Constraint::Distance(pts[0], pts[1], Dim::new((sk.point(pts[0]) - sk.point(pts[1])).length()))
        });
        b(ui, pts.len() == 2 && ents.is_empty(), "Horiz. dist", &mut || {
            Constraint::HorizontalDistance(pts[0], pts[1], Dim::new((sk.point(pts[0]).x - sk.point(pts[1]).x).abs()))
        });
        b(ui, pts.len() == 2 && ents.is_empty(), "Vert. dist", &mut || {
            Constraint::VerticalDistance(pts[0], pts[1], Dim::new((sk.point(pts[0]).y - sk.point(pts[1]).y).abs()))
        });
        b(ui, lines.len() == 2 && ents.len() == 2, "∠ Angle", &mut || {
            Constraint::Angle(lines[0], lines[1], Dim::new(line_dir(sk, lines[0]).dot(line_dir(sk, lines[1])).clamp(-1.0, 1.0).acos().to_degrees()))
        });
    });
    if !add.is_empty() {
        for c in add {
            e.sketch.constrain(c);
        }
        e.sel_pts.clear();
        e.sel_ents.clear();
        changed = true;
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!e.sel_ents.is_empty(), egui::Button::new("┄ Construction"))
            .on_hover_text("Toggle construction geometry (not part of the profile)")
            .clicked()
        {
            for i in e.sel_ents.clone() {
                if let Some(en) = e.sketch.entities.get_mut(i) {
                    let v = !en.construction();
                    en.set_construction(v);
                }
            }
            changed = true;
        }
        if ui.add_enabled(!e.sel_ents.is_empty() || !e.sel_pts.is_empty(), egui::Button::new("🗑 Delete")).clicked() {
            delete_selection(e);
            changed = true;
        }
    });
    ui.separator();
    ui.label(RichText::new(format!("Constraints ({})", e.sketch.constraints.len())).strong());
    let mut remove = None;
    for (i, c) in e.sketch.constraints.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let name = format!("{c:?}");
            let short: String = name.split('(').next().unwrap_or("").to_string();
            ui.label(RichText::new(short).small());
            if let Some(d) = c.dim_mut() {
                let r = ui.add(egui::DragValue::new(&mut d.value).speed(0.1).max_decimals(3));
                changed |= r.drag_stopped() || r.lost_focus();
                let mut ex = d.expr.clone().unwrap_or_default();
                let r = ui
                    .add(egui::TextEdit::singleline(&mut ex).hint_text("= expr").desired_width(80.0))
                    .on_hover_text("Parameter expression, e.g. width/2 or 3 in");
                if r.changed() {
                    d.expr = (!ex.trim().is_empty()).then(|| ex.trim().to_string());
                }
                changed |= r.lost_focus();
            }
            if ui.small_button("✖").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        e.sketch.constraints.remove(i);
        changed = true;
    }
    ui.separator();
    let profiles = e.sketch.profiles().len();
    ui.label(format!("{profiles} closed profile(s) · {} open path(s)", e.sketch.open_paths().len()));
    ui.label(RichText::new("Finish").strong());
    ui.horizontal(|ui| {
        ui.selectable_value(&mut e.op, BooleanOp::Add, "Boss (add)");
        ui.selectable_value(&mut e.op, BooleanOp::Cut, "Cut");
        ui.selectable_value(&mut e.op, BooleanOp::Intersect, "Intersect");
    });
    ui.horizontal(|ui| {
        ui.label("Depth");
        crate::panels::len_field(ui, &mut e.depth, units, 0.1..=1000.0);
    });
    let mut finish: Option<bool> = None;
    let mut cancel = false;
    ui.horizontal(|ui| {
        let editing = e.feature.is_some();
        if editing {
            if ui.add(egui::Button::new(RichText::new("✔ Apply sketch").strong()).fill(theme::ACCENT_SOFT)).clicked() {
                finish = Some(false);
            }
        } else {
            if ui.add_enabled(profiles > 0, egui::Button::new(RichText::new("⬆ Extrude").strong()).fill(theme::ACCENT_SOFT)).clicked() {
                finish = Some(false);
            }
            if ui
                .add_enabled(profiles > 0, egui::Button::new(RichText::new("🔄 Revolve").strong()))
                .on_hover_text("About the sketch's local Y axis (x ≥ 0 is the radius)")
                .clicked()
            {
                finish = Some(true);
            }
        }
        if ui.button("Cancel").clicked() {
            cancel = true;
        }
    });
    if e.finish_as_revolve && e.feature.is_none() {
        ui.label(RichText::new("Profile for a revolve: draw on the Front plane with x ≥ 0 as the radius.").small().color(theme::STEEL));
    }
    if changed {
        solve(app);
    }
    if cancel {
        app.solid.sketch = None;
    } else if let Some(rev) = finish {
        if let Some(edit) = app.solid.sketch.take() {
            let (op, depth) = (edit.op, edit.depth);
            finish_sketch(app, edit, rev, op, depth);
        }
    }
}
