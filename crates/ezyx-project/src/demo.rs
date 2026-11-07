//! The demo sign project (CLI `ezyx demo`, docs screenshots, first-run examples), built through
//! the pipeline so it shows features with automatically planned toolpaths.

use crate::pipeline::CutStrategy;
use crate::{Command, History, Project};

pub fn sign(text: &str) -> Result<Project, String> {
    let mut p = Project::default();
    let mut h = History::default();
    let mut run = |p: &mut Project, c: Command| h.execute(p, &c);
    run(&mut p, Command::SetJob { width: Some(300.0), height: Some(140.0), thickness: Some(18.0), material: Some("hardwood.walnut".into()), safe_z: None })?;
    run(&mut p, Command::AddRectangle { x: 10.0, y: 10.0, w: 280.0, h: 120.0, corner_radius: 12.0 })?;
    let outline = p.vectors.last().map(|v| v.id).unwrap_or_default();
    run(&mut p, Command::AddRectangle { x: 20.0, y: 20.0, w: 260.0, h: 100.0, corner_radius: 8.0 })?;
    let border = p.vectors.last().map(|v| v.id).unwrap_or_default();
    let before: Vec<u64> = p.vectors.iter().map(|v| v.id).collect();
    if crate::text::find_system_font().is_some() {
        run(&mut p, Command::AddText { text: text.into(), height: 50.0, x: 0.0, y: 0.0, font_path: None })?;
    } else {
        run(&mut p, Command::AddStar { cx: 150.0, cy: 70.0, r_outer: 40.0, r_inner: 18.0, points: 5 })?;
    }
    let letters: Vec<u64> = p.vectors.iter().filter(|v| !before.contains(&v.id)).map(|v| v.id).collect();
    run(&mut p, Command::CenterOnStock { ids: letters.clone() })?;
    run(&mut p, Command::CarveVectors { ids: letters, strategy: CutStrategy::VCarve, depth: Some(4.0), name: Some("Lettering".into()) })?;
    run(&mut p, Command::CarveVectors { ids: vec![border], strategy: CutStrategy::Engrave, depth: Some(1.5), name: Some("Border groove".into()) })?;
    run(&mut p, Command::CarveVectors { ids: vec![outline], strategy: CutStrategy::CutOut, depth: None, name: Some("Cut out".into()) })?;
    p.name = format!("{text} sign");
    Ok(p)
}

#[cfg(test)]
mod tests {
    #[test]
    fn demo_has_three_features_and_planned_ops() {
        let p = super::sign("OAK").unwrap();
        assert_eq!(p.features.len(), 3);
        // V-carve (1, maybe with clearance inside), engrave (1), cut-out rough + finish (2).
        assert_eq!(p.operations.len(), 4, "{:?}", p.operations.iter().map(|o| &o.name).collect::<Vec<_>>());
        assert_eq!(p.operations.last().unwrap().kind.label(), "Profile");
        let (g, _) = crate::ops::export_gcode(&p, crate::pipeline::default_library()).unwrap();
        assert!(g.contains("G1"));
    }
}
