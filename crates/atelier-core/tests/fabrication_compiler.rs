mod support;

use atelier_core::{
    AtelierDocument, CardSide, CombineMode, ContentLayer, FaceProductionLayer, LayerId,
    LayerPolarity, ProductionMapping, ProductionTarget, TransformUm, compile_fabrication_plan,
};
use support::asymmetric_golden_card;

fn mapped_text(
    document: &mut AtelierDocument,
    side: CardSide,
    name: &str,
    target: FaceProductionLayer,
) -> LayerId {
    let layer = ContentLayer::new_text(name, name, TransformUm::rect(4_000, 7_000, 12_000, 6_000));
    let layer_id = layer.id;
    match side {
        CardSide::Front => document.front.layers.push(layer),
        CardSide::Back => document.back.layers.push(layer),
    }
    document.mappings.push(ProductionMapping::new(
        layer_id,
        ProductionTarget::new(side, target),
        CombineMode::Add,
    ));
    layer_id
}

#[test]
fn compiler_always_emits_the_six_canonical_production_layers() {
    let document = AtelierDocument::new_card("六层", 64_000, 100_000);

    let board = compile_fabrication_plan(&document).expect("compile empty plan");
    let names = board
        .layers
        .iter()
        .map(|layer| layer.target.canonical_name())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "topCopper",
            "topSolderMaskOpen",
            "topSilkscreen",
            "bottomCopper",
            "bottomSolderMaskOpen",
            "bottomSilkscreen",
        ]
    );
    assert_eq!(board.layers[1].polarity, LayerPolarity::Opening);
}

#[test]
fn content_visibility_does_not_change_export_but_export_enabled_does() {
    let mut document = AtelierDocument::new_card("显隐", 64_000, 100_000);
    let first_id = mapped_text(
        &mut document,
        CardSide::Front,
        "仅画布隐藏",
        FaceProductionLayer::Silkscreen,
    );
    let second_id = mapped_text(
        &mut document,
        CardSide::Front,
        "不参与导出",
        FaceProductionLayer::Silkscreen,
    );
    document.front.layers[0].visible = false;
    document.front.layers[1].export_enabled = false;

    let board = compile_fabrication_plan(&document).expect("compile plan");
    let silk = board
        .layer(ProductionTarget::new(
            CardSide::Front,
            FaceProductionLayer::Silkscreen,
        ))
        .expect("top silk");

    assert_eq!(silk.operations.len(), 1);
    assert_eq!(silk.operations[0].source_layer_id, first_id);
    assert_ne!(silk.operations[0].source_layer_id, second_id);
}

#[test]
fn back_content_keeps_board_coordinates_in_compiled_geometry() {
    let mut document = AtelierDocument::new_card("背面", 64_000, 100_000);
    let layer_id = mapped_text(
        &mut document,
        CardSide::Back,
        "B",
        FaceProductionLayer::Copper,
    );
    document.back.layers[0].transform.x_um = 3_250;

    let board = compile_fabrication_plan(&document).expect("compile plan");
    let copper = board
        .layer(ProductionTarget::new(
            CardSide::Back,
            FaceProductionLayer::Copper,
        ))
        .expect("bottom copper");

    assert_eq!(copper.operations[0].source_layer_id, layer_id);
    assert_eq!(copper.operations[0].transform.x_um, 3_250);
}

#[test]
fn asymmetric_golden_card_preserves_physical_geometry_and_layer_semantics() {
    let fixture = asymmetric_golden_card();
    let board =
        compile_fabrication_plan(&fixture.bundle.document).expect("compile golden card plan");

    assert_eq!(board.outline.width_um(), 64_000);
    assert_eq!(board.outline.height_um(), 100_000);
    assert_eq!(board.mechanical_features.len(), 1);

    let top_silk = board
        .layer(ProductionTarget::new(
            CardSide::Front,
            FaceProductionLayer::Silkscreen,
        ))
        .expect("top silk");
    assert_eq!(
        top_silk.operations[0].source_layer_id,
        fixture.front_text_id
    );
    assert_eq!(top_silk.operations[0].transform.x_um, 43_000);
    assert!(top_silk.operations[0].clip_to_board);

    let bottom_silk = board
        .layer(ProductionTarget::new(
            CardSide::Back,
            FaceProductionLayer::Silkscreen,
        ))
        .expect("bottom silk");
    assert_eq!(
        bottom_silk.operations[0].source_layer_id,
        fixture.back_text_id
    );
    assert_eq!(bottom_silk.operations[0].transform.x_um, 7_000);

    let top_mask = board
        .layer(ProductionTarget::new(
            CardSide::Front,
            FaceProductionLayer::SolderMaskOpen,
        ))
        .expect("top mask opening");
    let bottom_mask = board
        .layer(ProductionTarget::new(
            CardSide::Back,
            FaceProductionLayer::SolderMaskOpen,
        ))
        .expect("bottom mask opening");
    assert_eq!(top_mask.polarity, LayerPolarity::Opening);
    assert_eq!(bottom_mask.polarity, LayerPolarity::Opening);
    assert_eq!(top_mask.operations.len(), 1);
    assert_eq!(bottom_mask.operations.len(), 1);
}
