pub mod ai;
pub mod image;
pub mod inspector;
pub mod machine;
pub mod tree;

/// Length field shown in the job's display units but stored in mm.
pub fn len_field(ui: &mut egui::Ui, mm: &mut f64, units: ezyx_core::Units, range: std::ops::RangeInclusive<f64>) -> egui::Response {
    let mut v = units.from_mm(*mm);
    let (lo, hi) = (units.from_mm(*range.start()), units.from_mm(*range.end()));
    let speed = if units == ezyx_core::Units::Inches { 0.01 } else { 0.25 };
    let r = ui.add(egui::DragValue::new(&mut v).range(lo..=hi).speed(speed).max_decimals(4).suffix(format!(" {}", units.suffix())));
    if r.changed() {
        *mm = units.to_mm(v);
    }
    r
}
