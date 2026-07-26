mod support;

use atelier_core::{
    ProjectBundleRasterizer, compile_fabrication_plan, export_easyeda_handoff,
    resolve_fabrication_plan,
};

use support::asymmetric_golden_card;

#[test]
fn handoff_writes_a_versioned_traceable_manifest_after_two_validated_outputs() {
    let fixture = asymmetric_golden_card();
    let _asymmetric_text_markers = (fixture.front_text_id, fixture.back_text_id);
    let plan = compile_fabrication_plan(&fixture.bundle.document).expect("compile golden plan");
    let mut rasterizer = ProjectBundleRasterizer::new(&fixture.bundle).expect("embedded font");
    let board = resolve_fabrication_plan(&plan, 500, &mut rasterizer).expect("resolve masks");
    let directory = tempfile::tempdir().expect("temporary directory");

    let first = export_easyeda_handoff(directory.path(), "Golden card", &board)
        .expect("export a traceable handoff");
    let second = export_easyeda_handoff(directory.path(), "Golden card", &board)
        .expect("re-export as a new downstream version");

    assert_eq!(first.export_format_version, "atelier-easyeda-handoff-v1");
    assert_eq!(first.fabrication_input_sha256, board.build.input_sha256);
    assert_eq!(first.fabrication_output_sha256, board.build.output_sha256);
    assert_ne!(first.export_version, second.export_version);
    assert_ne!(first.public_archive_path, second.public_archive_path);
    assert_ne!(first.native_project_path, second.native_project_path);
    assert_eq!(
        first.fabrication_input_sha256,
        second.fabrication_input_sha256
    );
    assert_eq!(
        first.fabrication_output_sha256,
        second.fabrication_output_sha256
    );
    assert_eq!(first.primitives, second.primitives);
    assert_eq!(
        first.public_validation.filled_layer_ids,
        second.public_validation.filled_layer_ids
    );
    assert!(first.public_archive_path.is_file());
    assert!(first.native_project_path.is_file());
    assert!(first.manifest_path.is_file());
    assert!(first.public_validation.is_valid);
    assert!(first.native_validation.is_valid);
    assert_eq!(
        first.primitives.fill_count,
        first.public_validation.fill_count
    );
    assert_eq!(first.primitives.hole_count, 1);
    assert_eq!(first.primitives.filled_layer_ids, vec![1, 2, 3, 4, 5, 6]);
    assert_ne!(first.public_archive_sha256, first.native_project_sha256);

    let manifest = std::fs::read_to_string(&first.manifest_path).expect("read manifest");
    assert!(manifest.contains(&first.export_version));
    assert!(manifest.contains(&first.fabrication_input_sha256));
    assert!(manifest.contains(&first.fabrication_output_sha256));
    assert!(manifest.contains(&first.public_archive_sha256));
    assert!(manifest.contains(&first.native_project_sha256));
}
