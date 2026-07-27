use atelier_cli::execute;
use atelier_core::{
    AtelierDocument, CardSide, CombineMode, CommandHistory, ContentLayer, DocumentCommand,
    FaceProductionLayer, ImageTreatment, MechanicalFeature, ProductionMapping, ProductionTarget,
    ProjectBundle, ProjectBundleRasterizer, SamplingPurpose, TransformUm, TreatmentRecipe,
    compile_fabrication_plan, resolve_fabrication_plan, resolve_fabrication_plan_for_purpose,
};
use sha2::{Digest, Sha256};

#[test]
fn cli_creates_a_valid_physical_card_project() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let project_path = temp.path().join("card.pcba");
    let args = vec![
        "new".to_owned(),
        project_path.display().to_string(),
        "--title".to_owned(),
        "第一张卡".to_owned(),
        "--width-mm".to_owned(),
        "64".to_owned(),
        "--height-mm".to_owned(),
        "100.5".to_owned(),
    ];

    execute(&args).expect("create project from CLI");
    let bundle = ProjectBundle::open(&project_path).expect("open CLI project");

    assert_eq!(bundle.document.title, "第一张卡");
    assert_eq!(bundle.document.board.width_um(), 64_000);
    assert_eq!(bundle.document.board.height_um(), 100_500);
}

#[test]
fn cli_and_direct_core_command_produce_identical_document() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let project_path = temp.path().join("card.pcba");
    let commands_path = temp.path().join("commands.json");
    let initial = AtelierDocument::new_card("命令等价", 64_000, 100_000);
    ProjectBundle::new(initial.clone())
        .save(&project_path)
        .expect("save initial project");
    let layer = ContentLayer::new_text(
        "标题",
        "PCB",
        TransformUm::rect(2_000, 3_000, 12_000, 5_000),
    );
    let command = DocumentCommand::InsertLayer {
        side: CardSide::Front,
        layer,
        index: 0,
    };
    std::fs::write(
        &commands_path,
        serde_json::to_vec_pretty(&vec![command.clone()]).expect("serialize commands"),
    )
    .expect("write command file");

    let mut expected = initial;
    CommandHistory::default()
        .execute(&mut expected, command)
        .expect("apply core command");
    execute(&[
        "apply".to_owned(),
        project_path.display().to_string(),
        commands_path.display().to_string(),
    ])
    .expect("apply command through CLI");
    let actual = ProjectBundle::open(&project_path)
        .expect("open updated project")
        .document;

    assert_eq!(actual, expected);
}

#[test]
fn cli_rejects_ambiguous_or_imprecise_dimensions() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let project_path = temp.path().join("card.pcba");

    let error = execute(&[
        "new".to_owned(),
        project_path.display().to_string(),
        "--width-mm".to_owned(),
        "64.0001".to_owned(),
        "--height-mm".to_owned(),
        "100".to_owned(),
    ])
    .expect_err("sub-micrometre dimensions must fail");

    assert!(error.to_string().contains("micrometre"));
    assert!(!project_path.exists());
}

#[test]
fn production_inspect_matches_direct_core_for_the_same_pcba_fixture() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let project_path = temp.path().join("asymmetric.pcba");
    let bundle = asymmetric_cli_fixture();
    bundle.save(&project_path).expect("save fixture");

    let plan = compile_fabrication_plan(&bundle.document).expect("compile directly");
    let mut rasterizer = ProjectBundleRasterizer::new(&bundle).expect("embedded font");
    let expected = resolve_fabrication_plan(&plan, 25, &mut rasterizer).expect("resolve directly");
    let output = execute(&[
        "production-inspect".to_owned(),
        project_path.display().to_string(),
    ])
    .expect("inspect through CLI");
    let actual: serde_json::Value = serde_json::from_str(&output).expect("stable JSON");

    assert_eq!(actual["board"]["widthUm"], 64_000);
    assert_eq!(actual["board"]["heightUm"], 100_000);
    assert_eq!(
        actual["pixelPitchUm"],
        atelier_core::DEFAULT_PRODUCTION_PIXEL_PITCH_UM
    );
    let expected_document_sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&bundle.document).expect("serialize document"))
    );
    assert_eq!(actual["documentSha256"], expected_document_sha256);
    assert_eq!(actual["build"]["inputSha256"], expected.build.input_sha256);
    assert_eq!(
        actual["build"]["outputSha256"],
        expected.build.output_sha256
    );
    assert_eq!(actual["layers"].as_array().expect("six layers").len(), 6);
    for layer in &expected.layers {
        let actual_layer = actual["layers"]
            .as_array()
            .expect("layers")
            .iter()
            .find(|candidate| {
                serde_json::from_value::<atelier_core::ProductionTarget>(
                    candidate["target"].clone(),
                )
                .ok()
                    == Some(layer.target)
            })
            .expect("matching target");
        assert_eq!(actual_layer["compositeSha256"], layer.composite_sha256);
    }
}

#[test]
fn export_easyeda_uses_the_same_resolved_board_and_reports_versioned_artifacts() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let project_path = temp.path().join("asymmetric.pcba");
    let destination = temp.path().join("handoff");
    let bundle = asymmetric_cli_fixture();
    bundle.save(&project_path).expect("save fixture");
    let plan = compile_fabrication_plan(&bundle.document).expect("compile directly");
    let mut rasterizer = ProjectBundleRasterizer::new(&bundle).expect("embedded font");
    let expected = resolve_fabrication_plan_for_purpose(
        &plan,
        SamplingPurpose::FormalProduction,
        &mut rasterizer,
    )
    .expect("resolve formal production");

    let error = execute(&[
        "export-easyeda".to_owned(),
        project_path.display().to_string(),
        destination.display().to_string(),
        "--pitch-um".to_owned(),
        "500".to_owned(),
    ])
    .expect_err("non-formal export pitch is rejected");
    assert!(error.to_string().contains("formalProduction"));

    let output = execute(&[
        "export-easyeda".to_owned(),
        project_path.display().to_string(),
        destination.display().to_string(),
    ])
    .expect("export through CLI");
    let report: serde_json::Value = serde_json::from_str(&output).expect("report JSON");

    assert_eq!(
        report["fabricationInputSha256"].as_str().map(str::len),
        Some(64)
    );
    assert_eq!(
        report["fabricationOutputSha256"].as_str().map(str::len),
        Some(64)
    );
    assert_eq!(
        report["fabricationOutputSha256"],
        expected.build.output_sha256
    );
    assert_eq!(
        report["primitives"]["filledLayerIds"],
        serde_json::json!([1, 2, 3, 4, 5, 6])
    );
    assert_eq!(report["nativeValidation"]["boardWidthUm"], 64_000);
    assert_eq!(report["nativeValidation"]["boardHeightUm"], 100_000);
    assert!(
        report["nativeValidation"]["isValid"]
            .as_bool()
            .expect("valid native")
    );
    assert!(
        std::path::Path::new(report["publicArchivePath"].as_str().expect("archive path")).is_file()
    );
    assert!(
        std::path::Path::new(report["nativeProjectPath"].as_str().expect("native path")).is_file()
    );
}

#[test]
fn cli_import_compile_validate_and_trace_use_embedded_project_data() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let project_path = temp.path().join("media.pcba");
    let source_path = temp.path().join("source.png");
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .expect("encode source PNG");
    std::fs::write(&source_path, encoded.into_inner()).expect("write source");
    ProjectBundle::new(AtelierDocument::new_card("CLI 素材", 10_000, 10_000))
        .save(&project_path)
        .expect("save project");

    let imported = execute(&[
        "asset-import".to_owned(),
        project_path.display().to_string(),
        source_path.display().to_string(),
        "--media-type".to_owned(),
        "image/png".to_owned(),
        "--pixel-width".to_owned(),
        "2".to_owned(),
        "--pixel-height".to_owned(),
        "2".to_owned(),
    ])
    .expect("import asset");
    let imported: serde_json::Value = serde_json::from_str(&imported).expect("asset report JSON");
    assert_eq!(imported["reused"], false);

    let mut bundle = ProjectBundle::open(&project_path).expect("open imported project");
    let asset_id = bundle.document.assets[0].id;
    let treatment = ImageTreatment::new(asset_id, TreatmentRecipe::default());
    let treatment_id = treatment.id;
    let image = ContentLayer::new_image(
        "嵌入图片",
        asset_id,
        TransformUm::rect(1_000, 1_000, 4_000, 4_000),
    );
    let image_id = image.id;
    bundle.document.image_treatments.push(treatment);
    bundle.document.front.layers.push(image);
    let mut mapping = ProductionMapping::new(
        image_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Silkscreen),
        CombineMode::Add,
    );
    mapping.treatment_id = Some(treatment_id);
    bundle.document.mappings.push(mapping);
    bundle.save(&project_path).expect("save treatment fixture");
    std::fs::remove_file(&source_path).expect("remove original source");

    let compile = execute(&[
        "treatment-compile".to_owned(),
        project_path.display().to_string(),
        treatment_id.to_string(),
        "--width-um".to_owned(),
        "4000".to_owned(),
        "--height-um".to_owned(),
        "4000".to_owned(),
        "--purpose".to_owned(),
        "formal-production".to_owned(),
    ])
    .expect("compile embedded treatment without source file");
    let compile: serde_json::Value = serde_json::from_str(&compile).expect("compile report JSON");
    assert_eq!(compile["treatmentId"], treatment_id.to_string());
    assert_eq!(compile["purpose"], "formalProduction");
    assert_eq!(
        compile["recipeFingerprint"].as_str().map(str::len),
        Some(64)
    );
    assert_eq!(compile["maskSha256"].as_str().map(str::len), Some(64));

    let manufacturer = execute(&[
        "manufacturer-validate".to_owned(),
        project_path.display().to_string(),
    ])
    .expect("validate manufacturer");
    let manufacturer: serde_json::Value =
        serde_json::from_str(&manufacturer).expect("manufacturer JSON");
    assert_eq!(manufacturer["valid"], true);
    assert_eq!(manufacturer["profile"]["manufacturerId"], "jlcpcb");

    let trace = execute(&[
        "production-inspect".to_owned(),
        project_path.display().to_string(),
        "--pitch-um".to_owned(),
        "500".to_owned(),
    ])
    .expect("trace production");
    let trace: serde_json::Value = serde_json::from_str(&trace).expect("trace JSON");
    assert_eq!(trace["operations"].as_array().map(Vec::len), Some(1));
    assert_eq!(trace["operations"][0]["assetId"], asset_id.to_string());
    assert_eq!(
        trace["operations"][0]["treatmentId"],
        treatment_id.to_string()
    );
    assert_eq!(
        trace["operations"][0]["algorithmVersion"],
        atelier_core::TREATMENT_ALGORITHM_VERSION
    );
    assert_eq!(
        trace["operations"][0]["recipeFingerprint"]
            .as_str()
            .map(str::len),
        Some(64)
    );
}

fn asymmetric_cli_fixture() -> ProjectBundle {
    let mut bundle = ProjectBundle::new(AtelierDocument::new_card("CLI 非对称", 64_000, 100_000));
    let front = ContentLayer::new_text(
        "Front F",
        "F",
        TransformUm::rect(8_000, 12_000, 10_000, 12_000),
    );
    let back = ContentLayer::new_text(
        "Back B",
        "B",
        TransformUm::rect(42_000, 70_000, 10_000, 12_000),
    );
    let front_id = front.id;
    let back_id = back.id;
    bundle.document.front.layers.push(front);
    bundle.document.back.layers.push(back);
    bundle.document.mappings = vec![
        mapping(front_id, CardSide::Front, FaceProductionLayer::Copper),
        mapping(
            front_id,
            CardSide::Front,
            FaceProductionLayer::SolderMaskOpen,
        ),
        mapping(front_id, CardSide::Front, FaceProductionLayer::Silkscreen),
        mapping(back_id, CardSide::Back, FaceProductionLayer::Copper),
        mapping(back_id, CardSide::Back, FaceProductionLayer::SolderMaskOpen),
        mapping(back_id, CardSide::Back, FaceProductionLayer::Silkscreen),
    ];
    bundle
        .document
        .mechanical_features
        .push(MechanicalFeature::NpthRound {
            center_x_um: 54_000,
            center_y_um: 16_000,
            diameter_um: 3_000,
        });
    bundle
}

fn mapping(
    source_layer_id: atelier_core::LayerId,
    side: CardSide,
    layer: FaceProductionLayer,
) -> ProductionMapping {
    ProductionMapping::new(
        source_layer_id,
        ProductionTarget::new(side, layer),
        CombineMode::Add,
    )
}
