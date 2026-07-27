use std::io::Cursor;

use atelier_core::{
    AtelierDocument, BoardFillContent, CardSide, CombineMode, CommandHistory, ContentKind,
    ContentLayer, DocumentCommand, DocumentError, FabricationPrimitive, FaceProductionLayer,
    ProductionMapping, ProductionTarget, ProjectBundle, ProjectBundleRasterizer, TransformUm,
    compile_fabrication_plan, resolve_fabrication_plan,
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

fn solid_black_png() -> Vec<u8> {
    let mut image = RgbaImage::new(1, 1);
    image.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("encode test png");
    bytes.into_inner()
}

#[test]
fn board_fill_round_trips_as_a_versioned_source_content_kind() {
    let mut document = AtelierDocument::new_card("铺铜", 64_000, 100_000);
    let fill = ContentLayer::new_board_fill("正面铺铜", 750);
    document.front.layers.push(fill);

    let json = serde_json::to_string_pretty(&document).expect("serialize board fill");
    assert!(json.contains(r#""type": "boardFill""#));
    assert!(json.contains(r#""edgeClearanceUm": 750"#));

    let restored: AtelierDocument = serde_json::from_str(&json).expect("deserialize board fill");
    assert_eq!(restored, document);
    restored.validate().expect("restored board fill is valid");
}

#[test]
fn board_fill_insert_and_mapping_are_undoable_domain_commands() {
    let mut document = AtelierDocument::new_card("撤销铺铜", 64_000, 100_000);
    let fill = ContentLayer::new_board_fill("正面铺铜", 500);
    let fill_id = fill.id;
    let mapping = ProductionMapping::new(
        fill_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Copper),
        CombineMode::Add,
    );
    let mut history = CommandHistory::default();

    history
        .execute(
            &mut document,
            DocumentCommand::InsertLayer {
                side: CardSide::Front,
                layer: fill,
                index: 0,
            },
        )
        .expect("insert fill source");
    history
        .execute(&mut document, DocumentCommand::MapLayer { mapping })
        .expect("map fill source");
    history
        .execute(
            &mut document,
            DocumentCommand::SetBoardFillContent {
                layer_id: fill_id,
                fill: BoardFillContent {
                    edge_clearance_um: 900,
                },
            },
        )
        .expect("edit fill clearance");
    assert_eq!(document.front.layers[0].id, fill_id);
    assert_eq!(document.mappings.len(), 1);
    assert!(matches!(
        document.front.layers[0].kind,
        ContentKind::BoardFill(BoardFillContent {
            edge_clearance_um: 900
        })
    ));

    history.undo(&mut document).expect("undo fill edit");
    assert!(matches!(
        document.front.layers[0].kind,
        ContentKind::BoardFill(BoardFillContent {
            edge_clearance_um: 500
        })
    ));
    history.undo(&mut document).expect("undo fill mapping");
    assert!(document.mappings.is_empty());
    history.undo(&mut document).expect("undo fill insertion");
    assert!(document.front.layers.is_empty());
    history.redo(&mut document).expect("redo fill insertion");
    assert!(matches!(
        document.front.layers[0].kind,
        ContentKind::BoardFill(_)
    ));
}

#[test]
fn board_fill_must_fit_the_board_and_map_only_to_same_face_copper() {
    let mut document = AtelierDocument::new_card("铺铜约束", 10_000, 8_000);
    let fill = ContentLayer::new_board_fill("正面铺铜", 4_000);
    let fill_id = fill.id;
    document.front.layers.push(fill);

    let error = document
        .validate()
        .expect_err("clearance that consumes the board must fail");
    assert!(matches!(
        error,
        DocumentError::InvalidBoardFillClearance { layer_id, .. } if layer_id == fill_id
    ));

    let ContentKind::BoardFill(ref mut content) = document.front.layers[0].kind else {
        unreachable!()
    };
    content.edge_clearance_um = 500;
    document.mappings.push(ProductionMapping::new(
        fill_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Silkscreen),
        CombineMode::Add,
    ));
    let error = document
        .validate()
        .expect_err("board fill cannot map to silkscreen");
    assert!(matches!(
        error,
        DocumentError::BoardFillMustMapToCopper { layer_id, .. } if layer_id == fill_id
    ));

    document.mappings[0].target =
        ProductionTarget::new(CardSide::Back, FaceProductionLayer::Copper);
    assert!(matches!(
        document.validate().expect_err("cross-face fill must fail"),
        DocumentError::CrossFaceMapping { layer_id, .. } if layer_id == fill_id
    ));
}

#[test]
fn board_fill_compiles_to_inset_copper_and_later_subtract_mapping_carves_it() {
    let mut bundle = ProjectBundle::new(AtelierDocument::new_card("铺铜编译", 100, 80));
    bundle.document.board = atelier_core::BoardOutline::Rectangle {
        width_um: 100,
        height_um: 80,
    };
    let fill = ContentLayer::new_board_fill("正面铺铜", 20);
    let fill_id = fill.id;
    bundle.document.front.layers.push(fill);
    bundle.document.mappings.push(ProductionMapping::new(
        fill_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Copper),
        CombineMode::Add,
    ));

    let asset_id = bundle
        .embed_asset("cutout.png", "image/png", 1, 1, solid_black_png())
        .expect("embed cutout");
    let cutout = ContentLayer::new_image("挖空", asset_id, TransformUm::rect(40, 30, 20, 20));
    let cutout_id = cutout.id;
    bundle.document.front.layers.push(cutout);
    bundle.document.mappings.push(ProductionMapping::new(
        cutout_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Copper),
        CombineMode::Subtract,
    ));

    let plan = compile_fabrication_plan(&bundle.document).expect("compile board fill");
    let copper = plan
        .layer(ProductionTarget::new(
            CardSide::Front,
            FaceProductionLayer::Copper,
        ))
        .expect("top copper");
    assert!(matches!(
        copper.operations[0].primitive,
        FabricationPrimitive::BoardFill {
            edge_clearance_um: 20,
            ..
        }
    ));
    assert_eq!(copper.operations[1].combine, CombineMode::Subtract);

    let mut rasterizer = ProjectBundleRasterizer::new(&bundle).expect("rasterizer");
    let resolved = resolve_fabrication_plan(&plan, 10, &mut rasterizer).expect("resolve fill");
    let mask = &resolved
        .layers
        .iter()
        .find(|layer| {
            layer.target == ProductionTarget::new(CardSide::Front, FaceProductionLayer::Copper)
        })
        .expect("resolved top copper")
        .composite;
    assert_eq!(mask.active_pixel_count(), 20);
    assert!(!mask.get(1, 2).expect("outside clearance"));
    assert!(mask.get(2, 2).expect("inside fill"));
    assert!(!mask.get(4, 3).expect("subtracted opening"));
}

#[test]
fn board_fill_clearance_follows_a_rounded_board_outline() {
    let mut bundle = ProjectBundle::new(AtelierDocument::new_card("圆角铺铜", 100, 100));
    bundle.document.board = atelier_core::BoardOutline::RoundedRectangle {
        width_um: 100,
        height_um: 100,
        corner_radius_um: 30,
    };
    let fill = ContentLayer::new_board_fill("正面铺铜", 10);
    let fill_id = fill.id;
    bundle.document.front.layers.push(fill);
    bundle.document.mappings.push(ProductionMapping::new(
        fill_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Copper),
        CombineMode::Add,
    ));

    let plan = compile_fabrication_plan(&bundle.document).expect("compile rounded fill");
    let mut rasterizer = ProjectBundleRasterizer::new(&bundle).expect("rasterizer");
    let resolved = resolve_fabrication_plan(&plan, 10, &mut rasterizer).expect("resolve fill");
    let mask = &resolved
        .layers
        .iter()
        .find(|layer| {
            layer.target == ProductionTarget::new(CardSide::Front, FaceProductionLayer::Copper)
        })
        .expect("resolved top copper")
        .composite;

    assert!(!mask.get(0, 5).expect("edge clearance"));
    assert!(!mask.get(1, 1).expect("rounded inset corner"));
    assert!(mask.get(2, 2).expect("inside rounded inset"));
}

#[test]
fn project_rejects_an_older_unreleased_schema_before_interpreting_features() {
    let mut legacy = AtelierDocument::new_card("旧版本", 64_000, 100_000);
    legacy.schema_version = 1;
    legacy
        .front
        .layers
        .push(ContentLayer::new_board_fill("铺铜", 500));

    assert!(matches!(
        legacy
            .validate()
            .expect_err("old unreleased schema is unsupported"),
        DocumentError::UnsupportedSchema(1)
    ));
}
