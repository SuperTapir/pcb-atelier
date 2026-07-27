use atelier_core::{
    AssetId, AtelierDocument, BoardOutline, CardSide, CombineMode, ContentKind, ContentLayer,
    CropRect, FaceProductionLayer, ImageTreatment, MechanicalFeature, ProductionMapping,
    ProductionTarget, ProjectBundle, SolderMaskColor, SurfaceFinish, TransformUm, TreatmentRecipe,
};

#[test]
fn new_card_has_independent_front_and_back_faces() {
    let document = AtelierDocument::new_card("双面卡片", 64_000, 100_000);

    assert_eq!(document.board.width_um(), 64_000);
    assert_eq!(document.board.height_um(), 100_000);
    assert_eq!(document.front.side, CardSide::Front);
    assert_eq!(document.back.side, CardSide::Back);
    assert!(document.front.layers.is_empty());
    assert!(document.back.layers.is_empty());
    document.validate().expect("default card must be valid");
}

#[test]
fn document_round_trip_preserves_physical_transform_and_layer_identity() {
    let mut document = AtelierDocument::new_card("正面人物", 64_000, 100_000);
    let layer = ContentLayer::new_text(
        "标题",
        "PCB Atelier",
        TransformUm {
            x_um: 4_250,
            y_um: 8_500,
            width_um: 32_000,
            height_um: 6_000,
            rotation_mdeg: 12_500,
            flip_x: false,
            flip_y: true,
        },
    );
    let layer_id = layer.id;
    document.front.layers.push(layer);

    let json = serde_json::to_string_pretty(&document).expect("serialize document");
    let restored: AtelierDocument = serde_json::from_str(&json).expect("deserialize document");

    assert_eq!(restored, document);
    assert_eq!(restored.front.layers[0].id, layer_id);
    assert_eq!(restored.front.layers[0].transform.x_um, 4_250);
    assert_eq!(restored.front.layers[0].transform.rotation_mdeg, 12_500);
    assert!(restored.front.layers[0].transform.flip_y);
    restored
        .validate()
        .expect("round-tripped document is valid");
}

#[test]
fn one_content_layer_can_map_to_copper_and_solder_mask_opening() {
    let mut document = AtelierDocument::new_card("沉金线稿", 64_000, 100_000);
    let layer =
        ContentLayer::new_text("线稿", "F", TransformUm::rect(5_000, 7_000, 12_000, 18_000));
    let layer_id = layer.id;
    document.front.layers.push(layer);
    document.mappings = vec![
        ProductionMapping::new(
            layer_id,
            ProductionTarget::new(CardSide::Front, FaceProductionLayer::Copper),
            CombineMode::Add,
        ),
        ProductionMapping::new(
            layer_id,
            ProductionTarget::new(CardSide::Front, FaceProductionLayer::SolderMaskOpen),
            CombineMode::Add,
        ),
    ];

    document
        .validate()
        .expect("explicit multi-layer mapping is valid");
    assert_eq!(document.mappings.len(), 2);
    assert!(
        document
            .mappings
            .iter()
            .any(|mapping| { mapping.target.layer == FaceProductionLayer::SolderMaskOpen })
    );
}

#[test]
fn production_layer_names_encode_face_and_mask_opening_polarity() {
    let top_mask = ProductionTarget::new(CardSide::Front, FaceProductionLayer::SolderMaskOpen);
    let bottom_silk = ProductionTarget::new(CardSide::Back, FaceProductionLayer::Silkscreen);

    assert_eq!(top_mask.canonical_name(), "topSolderMaskOpen");
    assert_eq!(bottom_silk.canonical_name(), "bottomSilkscreen");
    assert_eq!(top_mask.layer.polarity_description(), "opening");
}

#[test]
fn back_content_remains_in_board_coordinates() {
    let mut document = AtelierDocument::new_card("背面方向", 64_000, 100_000);
    document.back.layers.push(ContentLayer::new_text(
        "背面 B",
        "B",
        TransformUm::rect(3_000, 9_000, 10_000, 15_000),
    ));

    let persisted_x = document.back.layers[0].transform.x_um;
    let viewed_x = document
        .board
        .mirror_x_for_back_view(&document.back.layers[0].transform);

    assert_eq!(persisted_x, 3_000);
    assert_eq!(viewed_x, 51_000);
    assert_eq!(document.back.layers[0].transform.x_um, persisted_x);
}

#[test]
fn validation_rejects_cross_face_mapping_and_invalid_board_geometry() {
    let mut document = AtelierDocument::new_card("错误映射", 64_000, 100_000);
    let layer = ContentLayer::new_text(
        "正面文字",
        "F",
        TransformUm::rect(1_000, 2_000, 5_000, 8_000),
    );
    let layer_id = layer.id;
    document.front.layers.push(layer);
    document.mappings.push(ProductionMapping::new(
        layer_id,
        ProductionTarget::new(CardSide::Back, FaceProductionLayer::Silkscreen),
        CombineMode::Add,
    ));

    let error = document
        .validate()
        .expect_err("cross-face mapping must fail");
    assert!(error.to_string().contains("opposite face"));

    document.mappings.clear();
    document.board = BoardOutline::RoundedRectangle {
        width_um: 10_000,
        height_um: 8_000,
        corner_radius_um: 5_000,
    };
    let error = document.validate().expect_err("oversized radius must fail");
    assert!(error.to_string().contains("corner radius"));
}

#[test]
fn validation_rejects_non_group_parent_and_out_of_board_drill() {
    let mut document = AtelierDocument::new_card("层级和孔", 64_000, 100_000);
    let parent = ContentLayer::new_text(
        "不是组",
        "parent",
        TransformUm::rect(1_000, 1_000, 10_000, 5_000),
    );
    let parent_id = parent.id;
    let mut child = ContentLayer::new_text(
        "子层",
        "child",
        TransformUm::rect(2_000, 2_000, 10_000, 5_000),
    );
    child.parent_id = Some(parent_id);
    document.front.layers.extend([parent, child]);

    let error = document.validate().expect_err("parent must be a group");
    assert!(error.to_string().contains("is not a group"));

    document.front.layers[0].kind = ContentKind::Group;
    document
        .mechanical_features
        .push(MechanicalFeature::NpthRound {
            center_x_um: 70_000,
            center_y_um: 10_000,
            diameter_um: 3_000,
        });
    let error = document.validate().expect_err("drill must fit board");
    assert!(error.to_string().contains("outside board"));
}

#[test]
fn validation_rejects_image_layer_with_missing_asset() {
    let mut document = AtelierDocument::new_card("缺失素材", 64_000, 100_000);
    let image = ContentLayer::new_image(
        "不存在的图片",
        AssetId::new(),
        TransformUm::rect(2_000, 3_000, 20_000, 30_000),
    );
    document.front.layers.push(image);

    let error = document
        .validate()
        .expect_err("image asset reference must exist");
    assert!(error.to_string().contains("missing image asset"));
}

#[test]
fn validation_rejects_invalid_or_ambiguous_asset_metadata() {
    let first_id = AssetId::new();
    let second_id = AssetId::new();
    let mut document = AtelierDocument::new_card("素材完整性", 64_000, 100_000);
    document.assets.push(atelier_core::ProjectAsset::fixture(
        first_id,
        "portrait.png",
        &"a".repeat(64),
    ));

    document.assets[0].original_filename = " ".to_owned();
    assert!(
        document.validate().is_err(),
        "asset filename must not be empty"
    );
    document.assets[0].original_filename = "portrait.png".to_owned();
    document.assets[0].media_type = " ".to_owned();
    assert!(
        document.validate().is_err(),
        "asset media type must not be empty"
    );
    document.assets[0].media_type = "image/png".to_owned();
    document.assets[0].pixel_width = 0;
    assert!(
        document.validate().is_err(),
        "asset dimensions must be positive"
    );
    document.assets[0].pixel_width = 1;
    document.assets[0].sha256 = "not-a-sha256".to_owned();
    assert!(
        document.validate().is_err(),
        "asset hash must use canonical lowercase SHA-256"
    );

    document.assets[0].sha256 = "a".repeat(64);
    let mut duplicate =
        atelier_core::ProjectAsset::fixture(second_id, "portrait-copy.png", &"b".repeat(64));
    duplicate.embedded_path = document.assets[0].embedded_path.clone();
    document.assets.push(duplicate);
    assert!(
        document.validate().is_err(),
        "two asset records must not claim the same embedded path"
    );

    document.assets[1].embedded_path = format!("assets/{second_id}.png");
    document.assets[1].sha256 = document.assets[0].sha256.clone();
    assert!(
        document.validate().is_err(),
        "content-addressed assets must not contain duplicate hashes"
    );
}

#[test]
fn validation_rejects_zero_or_out_of_bounds_image_crop() {
    let mut bundle = ProjectBundle::new(AtelierDocument::new_card("裁切", 64_000, 100_000));
    let asset_id = bundle
        .embed_asset("pixel.png", "image/png", 1, 1, vec![0])
        .expect("asset metadata");
    let mut image = ContentLayer::new_image(
        "图片",
        asset_id,
        TransformUm::rect(2_000, 3_000, 20_000, 30_000),
    );
    let ContentKind::Image(ref mut content) = image.kind else {
        unreachable!()
    };
    content.crop = Some(CropRect {
        x_millionths: 900_000,
        y_millionths: 0,
        width_millionths: 200_000,
        height_millionths: 1_000_000,
    });
    bundle.document.front.layers.push(image);

    let error = bundle
        .document
        .validate()
        .expect_err("crop must remain inside normalized image bounds");
    assert!(error.to_string().contains("invalid image crop"));
}

#[test]
fn validation_rejects_non_current_or_invalid_persisted_treatment_recipe() {
    let mut document = AtelierDocument::new_card("处理配方", 64_000, 100_000);
    let asset_id = AssetId::new();
    document.assets.push(atelier_core::ProjectAsset::fixture(
        asset_id,
        "portrait.png",
        &"a".repeat(64),
    ));
    document.image_treatments.push(ImageTreatment::new(
        asset_id,
        TreatmentRecipe::standard_monochrome(),
    ));

    document.image_treatments[0].recipe.algorithm_version =
        "atelier-image-treatment-v999".to_owned();
    let error = document
        .validate()
        .expect_err("only the current treatment algorithm is valid");
    assert!(error.to_string().contains("algorithm version"));

    document.image_treatments[0].recipe = TreatmentRecipe::standard_monochrome();
    document.image_treatments[0].recipe.crop = Some(CropRect {
        x_millionths: u32::MAX,
        y_millionths: 0,
        width_millionths: 1,
        height_millionths: 1_000_000,
    });
    let error = document
        .validate()
        .expect_err("overflowing treatment crop coordinates must fail");
    assert!(error.to_string().contains("crop"));

    document.image_treatments[0].recipe.crop = Some(CropRect {
        x_millionths: 0,
        y_millionths: 0,
        width_millionths: 0,
        height_millionths: 1_000_000,
    });
    let error = document
        .validate()
        .expect_err("empty treatment crop dimensions must fail");
    assert!(error.to_string().contains("crop"));
}

#[test]
fn validation_requires_stackup_to_mirror_the_manufacturer_profile() {
    let mut document = AtelierDocument::new_card("制造镜像", 64_000, 100_000);

    document.stackup.thickness_um = 2_000;
    let error = document
        .validate()
        .expect_err("stackup thickness must mirror manufacturer profile");
    assert!(error.to_string().contains("manufacturer profile"));

    document.stackup.thickness_um = document.manufacturer_profile.thickness_um;
    document.stackup.solder_mask_color = SolderMaskColor::White;
    let error = document
        .validate()
        .expect_err("stackup solder mask must mirror manufacturer profile");
    assert!(error.to_string().contains("manufacturer profile"));

    document.stackup.solder_mask_color = document.manufacturer_profile.solder_mask;
    document.stackup.surface_finish = SurfaceFinish::HaslLeadFree;
    let error = document
        .validate()
        .expect_err("stackup finish must mirror manufacturer profile");
    assert!(error.to_string().contains("manufacturer profile"));
}

#[test]
fn persisted_document_payloads_reject_unknown_fields() {
    let document = AtelierDocument::new_card("严格 schema", 64_000, 100_000);
    let mut json = serde_json::to_value(&document).expect("serialize document");
    json.as_object_mut()
        .expect("document object")
        .insert("legacyThreshold".to_owned(), serde_json::json!(128));
    assert!(
        serde_json::from_value::<AtelierDocument>(json).is_err(),
        "document root must reject unknown fields"
    );

    let mut json = serde_json::to_value(&document).expect("serialize document");
    json["front"]
        .as_object_mut()
        .expect("face object")
        .insert("legacyLayers".to_owned(), serde_json::json!([]));
    assert!(
        serde_json::from_value::<AtelierDocument>(json).is_err(),
        "nested persisted structs must reject unknown fields"
    );

    let mut content = serde_json::to_value(ContentKind::Image(atelier_core::ImageContent {
        asset_id: AssetId::new(),
        crop: None,
    }))
    .expect("serialize content kind");
    content
        .as_object_mut()
        .expect("tagged content object")
        .insert("legacyCrop".to_owned(), serde_json::Value::Null);
    assert!(
        serde_json::from_value::<ContentKind>(content).is_err(),
        "internally tagged content payload must reject unknown fields"
    );

    let mut outline = serde_json::to_value(BoardOutline::RoundedRectangle {
        width_um: 64_000,
        height_um: 100_000,
        corner_radius_um: 2_000,
    })
    .expect("serialize board outline");
    outline
        .as_object_mut()
        .expect("tagged outline object")
        .insert("legacyRadius".to_owned(), serde_json::json!(2_000));
    assert!(
        serde_json::from_value::<BoardOutline>(outline).is_err(),
        "internally tagged board payload must reject unknown fields"
    );

    let mut feature = serde_json::to_value(MechanicalFeature::NpthRound {
        center_x_um: 10_000,
        center_y_um: 10_000,
        diameter_um: 3_000,
    })
    .expect("serialize mechanical feature");
    feature
        .as_object_mut()
        .expect("tagged feature object")
        .insert("plated".to_owned(), serde_json::json!(false));
    assert!(
        serde_json::from_value::<MechanicalFeature>(feature).is_err(),
        "internally tagged mechanical payload must reject unknown fields"
    );
}
