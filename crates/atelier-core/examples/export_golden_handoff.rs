//! Generate the same asymmetric golden card used by integration tests for
//! manual JLCEDA acceptance.

#[path = "../tests/support/mod.rs"]
mod support;

use std::path::PathBuf;

use atelier_core::{
    ProjectBundleRasterizer, compile_fabrication_plan, export_easyeda_handoff,
    resolve_fabrication_plan,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: export_golden_handoff <output-directory>")?;
    let fixture = support::asymmetric_golden_card();
    let _source_markers = (fixture.front_text_id, fixture.back_text_id);
    let plan = compile_fabrication_plan(&fixture.bundle.document)?;
    let mut rasterizer = ProjectBundleRasterizer::new(&fixture.bundle)?;
    let board = resolve_fabrication_plan(&plan, 500, &mut rasterizer)?;
    let report = export_easyeda_handoff(&destination, "Golden card", &board)?;

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
