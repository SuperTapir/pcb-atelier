use atelier_core::{
    AssetId, AtelierDocument, BoardOutline, CardSide, CombineMode, ContentKind, ContentLayer,
    CropRect, FaceProductionLayer, MechanicalFeature, ProductionMapping, ProductionTarget,
    ProjectBundle, TransformUm,
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
