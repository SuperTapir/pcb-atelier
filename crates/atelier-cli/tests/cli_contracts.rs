use atelier_cli::execute;
use atelier_core::{
    AtelierDocument, CardSide, CombineMode, CommandHistory, ContentLayer, DocumentCommand,
    FaceProductionLayer, MechanicalFeature, ProductionMapping, ProductionTarget, ProjectBundle,
    ProjectBundleRasterizer, TransformUm, compile_fabrication_plan, resolve_fabrication_plan,
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
    assert_eq!(actual["pixelPitchUm"], 25);
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
            .find(|candidate| candidate["target"] == layer.target.canonical_name())
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
    let expected =
        resolve_fabrication_plan(&plan, 500, &mut rasterizer).expect("resolve explicit pitch");

    let output = execute(&[
        "export-easyeda".to_owned(),
        project_path.display().to_string(),
        destination.display().to_string(),
        "--pitch-um".to_owned(),
        "500".to_owned(),
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
