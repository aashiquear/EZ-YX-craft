pub mod drill;
pub mod engrave;
pub mod pocket;
pub mod polyline3d;
pub mod profile;
pub mod raster3d;
pub mod rotary;
pub mod vcarve;

#[cfg(test)]
pub(crate) mod testutil {
    use crate::{CutDirection, CutParams};
    use ezyx_tooldb::{feeds::MachineLimits, MaterialGroup, Operation, ToolLibrary};

    pub fn params(tool_id: &str, depth: f64) -> CutParams {
        let lib = ToolLibrary::generic_templates();
        let tool = lib.get(tool_id).unwrap().clone();
        let cutting = tool.cutting_for(MaterialGroup::Hardwood, Operation::Roughing, &MachineLimits::default());
        CutParams { tool, cutting, start_depth: 0.0, cut_depth: depth, safe_z: 5.0, direction: CutDirection::Climb }
    }
}
