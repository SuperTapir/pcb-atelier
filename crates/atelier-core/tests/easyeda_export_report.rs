mod support;

use atelier_core::{
    CharacterProcess, EasyedaOrderSupportStatus, FaceProductionLayer, ImageProductionMode,
    ImageTreatment, ProjectBundleRasterizer, SamplingPurpose, build_production_trace,
    compile_fabrication_plan, export_easyeda_handoff, resolve_fabrication_plan,
    resolve_fabrication_plan_for_purpose,
};

use support::asymmetric_golden_card;

#[test]
fn handoff_writes_a_versioned_traceable_manifest_after_two_validated_outputs() {
    let mut fixture = asymmetric_golden_card();
    let _asymmetric_text_markers = (fixture.front_text_id, fixture.back_text_id);
    let front_asset_id = match fixture.bundle.document.front.layers[0].kind {
        atelier_core::ContentKind::Image(ref image) => image.asset_id,
        _ => panic!("fixture front layer is an image"),
    };
    let treatment = ImageTreatment::new(front_asset_id, Default::default());
    let treatment_id = treatment.id;
    fixture.bundle.document.image_treatments.push(treatment);
    fixture.bundle.document.mappings[0].treatment_id = Some(treatment_id);
    let plan = compile_fabrication_plan(&fixture.bundle.document).expect("compile golden plan");
    let mut rasterizer = ProjectBundleRasterizer::new(&fixture.bundle).expect("embedded font");
    let board = resolve_fabrication_plan_for_purpose(
        &plan,
        SamplingPurpose::FormalProduction,
        &mut rasterizer,
    )
    .expect("resolve formal masks");
    let directory = tempfile::tempdir().expect("temporary directory");

    let first = export_easyeda_handoff(directory.path(), &fixture.bundle, &board)
        .expect("export a traceable handoff");
    let second = export_easyeda_handoff(directory.path(), &fixture.bundle, &board)
        .expect("re-export as a new downstream version");

    assert_eq!(first.export_format_version, "atelier-easyeda-handoff-v4");
    assert_eq!(first.production_source, SamplingPurpose::FormalProduction);
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
    assert!(first.manufacturing.validated);
    assert!(first.order_support.direct_order_supported);
    assert_eq!(
        first.order_support.status,
        EasyedaOrderSupportStatus::DirectOrderSupported
    );
    let front_trace = first
        .image_graphics
        .iter()
        .find(|trace| trace.treatment_id == Some(treatment_id))
        .expect("treated image trace");
    assert_eq!(front_trace.asset_id, front_asset_id);
    assert_eq!(
        front_trace.algorithm_version.as_deref(),
        Some("atelier-image-treatment-v2")
    );
    assert!(front_trace.recipe_fingerprint.is_some());
    assert_eq!(front_trace.asset_sha256.len(), 64);
    assert_eq!(front_trace.mask_sha256.len(), 64);

    let manifest = std::fs::read_to_string(&first.manifest_path).expect("read manifest");
    assert!(manifest.contains(&first.export_version));
    assert!(manifest.contains(&first.fabrication_input_sha256));
    assert!(manifest.contains(&first.fabrication_output_sha256));
    assert!(manifest.contains(&first.public_archive_sha256));
    assert!(manifest.contains(&first.native_project_sha256));
    assert!(manifest.contains("formalProduction"));
    assert!(manifest.contains("sourceInstanceId"));
    assert!(manifest.contains("assetSha256"));
    assert!(manifest.contains("manufacturing"));
    assert!(manifest.contains("directOrderSupported"));
    assert!(manifest.contains("layerStrategies"));
    assert!(manifest.contains("nativeImage"));
    assert!(manifest.contains("nativeString"));
}

#[test]
fn handoff_rejects_non_formal_masks_and_unvalidated_manufacturing_before_writing() {
    let mut fixture = asymmetric_golden_card();
    let plan = compile_fabrication_plan(&fixture.bundle.document).expect("compile plan");
    let mut rasterizer = ProjectBundleRasterizer::new(&fixture.bundle).expect("rasterizer");
    let preview_board =
        resolve_fabrication_plan(&plan, 500, &mut rasterizer).expect("resolve preview board");
    let directory = tempfile::tempdir().expect("temporary directory");

    let error = export_easyeda_handoff(directory.path(), &fixture.bundle, &preview_board)
        .expect_err("preview resolution must not reach formal export");
    assert!(error.to_string().contains("formalProduction"));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);

    let plan = compile_fabrication_plan(&fixture.bundle.document).expect("compile valid plan");
    let mut rasterizer = ProjectBundleRasterizer::new(&fixture.bundle).expect("rasterizer");
    let formal_board = resolve_fabrication_plan_for_purpose(
        &plan,
        SamplingPurpose::FormalProduction,
        &mut rasterizer,
    )
    .expect("resolve formal board");
    fixture.bundle.document.manufacturer_profile.outer_copper = atelier_core::CopperWeight::Oz0_5;
    let error = export_easyeda_handoff(directory.path(), &fixture.bundle, &formal_board)
        .expect_err("unvalidated manufacturing must not be exported");
    assert!(error.to_string().contains("manufacturing"));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn multicolor_silkscreen_exports_with_an_actionable_non_direct_order_status() {
    let mut fixture = asymmetric_golden_card();
    let front_asset_id = match fixture.bundle.document.front.layers[0].kind {
        atelier_core::ContentKind::Image(ref image) => image.asset_id,
        _ => panic!("fixture front layer is an image"),
    };
    let mut treatment = ImageTreatment::new(front_asset_id, Default::default());
    treatment.production_mode = ImageProductionMode::ColorOriginal;
    let treatment_id = treatment.id;
    fixture.bundle.document.image_treatments.push(treatment);
    fixture.bundle.document.mappings[0].target.layer = FaceProductionLayer::Silkscreen;
    fixture.bundle.document.mappings[0].treatment_id = Some(treatment_id);
    fixture.bundle.document.stackup.solder_mask_color = atelier_core::SolderMaskColor::White;
    fixture.bundle.document.manufacturer_profile.solder_mask = atelier_core::SolderMaskColor::White;
    fixture
        .bundle
        .document
        .manufacturer_profile
        .character_process = CharacterProcess::Multicolor;
    let plan = compile_fabrication_plan(&fixture.bundle.document).expect("valid multicolor plan");
    let mut rasterizer = ProjectBundleRasterizer::new(&fixture.bundle).expect("rasterizer");
    let board = resolve_fabrication_plan_for_purpose(
        &plan,
        SamplingPurpose::FormalProduction,
        &mut rasterizer,
    )
    .expect("resolve formal board");
    let production_trace = build_production_trace(7, &fixture.bundle.document, &board);
    let operation = production_trace
        .operations
        .iter()
        .find(|operation| operation.treatment_id == Some(treatment_id))
        .expect("color-original operation trace");
    assert_eq!(
        operation.image_production_mode,
        Some(ImageProductionMode::ColorOriginal)
    );
    assert_eq!(operation.asset_media_type.as_deref(), Some("image/png"));
    let directory = tempfile::tempdir().expect("temporary directory");

    let report = export_easyeda_handoff(directory.path(), &fixture.bundle, &board)
        .expect("export with explicit downgrade status");

    assert!(!report.order_support.direct_order_supported);
    assert_eq!(
        report.order_support.status,
        EasyedaOrderSupportStatus::RequiresManualAdjustment
    );
    assert!(
        report
            .order_support
            .issues
            .iter()
            .any(|issue| issue.contains("彩色丝印"))
    );
    assert!(
        report
            .order_support
            .downgrade_actions
            .iter()
            .any(|action| action.contains("标准丝印"))
    );
    let resource = report
        .color_silkscreen_resources
        .iter()
        .find(|resource| resource.treatment_id == treatment_id)
        .expect("color original resource");
    assert_eq!(resource.target.layer, FaceProductionLayer::Silkscreen);
    assert_eq!(resource.asset_id, front_asset_id);
    assert_eq!(resource.media_type, "image/png");
    assert_eq!(resource.asset_sha256.len(), 64);
    assert_eq!(
        std::fs::read(&resource.resource_path).expect("read sidecar resource"),
        fixture
            .bundle
            .asset_bytes(front_asset_id)
            .expect("embedded source")
    );
    let manifest = std::fs::read_to_string(&report.manifest_path).expect("read manifest");
    assert!(manifest.contains("colorSilkscreenResources"));
    assert!(manifest.contains("requiresEasyedaProColorSilkscreenExport"));
}
