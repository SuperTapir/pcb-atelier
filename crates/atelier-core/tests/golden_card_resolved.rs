mod support;

use atelier_core::{
    CardSide, FaceProductionLayer, LayerPolarity, ProductionTarget, ProjectBundleRasterizer,
    compile_fabrication_plan, resolve_fabrication_plan,
};
use support::asymmetric_golden_card;

fn layer(
    board: &atelier_core::ResolvedFabricationBoard,
    side: CardSide,
    production_layer: FaceProductionLayer,
) -> &atelier_core::ResolvedFabricationLayer {
    board
        .layers
        .iter()
        .find(|layer| layer.target == ProductionTarget::new(side, production_layer))
        .expect("canonical production layer")
}

#[test]
fn asymmetric_golden_card_resolves_real_assets_into_six_reproducible_production_masks() {
    let fixture = asymmetric_golden_card();
    assert_eq!(fixture.bundle.assets.len(), 2, "both images are embedded");
    let plan = compile_fabrication_plan(&fixture.bundle.document).expect("compile plan");

    let mut first_rasterizer =
        ProjectBundleRasterizer::new(&fixture.bundle).expect("embedded font");
    let first =
        resolve_fabrication_plan(&plan, 500, &mut first_rasterizer).expect("resolve golden card");
    let mut second_rasterizer =
        ProjectBundleRasterizer::new(&fixture.bundle).expect("embedded font");
    let second =
        resolve_fabrication_plan(&plan, 500, &mut second_rasterizer).expect("resolve again");

    assert_eq!(first.layers.len(), 6);
    assert_eq!(
        first.mechanical_features,
        fixture.bundle.document.mechanical_features
    );
    assert_eq!(first.build.input_sha256, second.build.input_sha256);
    assert_eq!(first.build.output_sha256, second.build.output_sha256);
    assert_eq!(first.build.font_fingerprint, second.build.font_fingerprint);

    let top_copper = layer(&first, CardSide::Front, FaceProductionLayer::Copper);
    let top_mask = layer(&first, CardSide::Front, FaceProductionLayer::SolderMaskOpen);
    let top_silk = layer(&first, CardSide::Front, FaceProductionLayer::Silkscreen);
    let bottom_copper = layer(&first, CardSide::Back, FaceProductionLayer::Copper);
    let bottom_mask = layer(&first, CardSide::Back, FaceProductionLayer::SolderMaskOpen);
    let bottom_silk = layer(&first, CardSide::Back, FaceProductionLayer::Silkscreen);

    for production_layer in [
        top_copper,
        top_mask,
        top_silk,
        bottom_copper,
        bottom_mask,
        bottom_silk,
    ] {
        assert_eq!(production_layer.operations.len(), 1);
        assert!(production_layer.composite.active_pixel_count() > 0);
    }
    assert_eq!(top_mask.polarity, LayerPolarity::Opening);
    assert_eq!(bottom_mask.polarity, LayerPolarity::Opening);
    assert_eq!(
        top_silk.operations[0].source_layer_id,
        fixture.front_text_id
    );
    assert_eq!(
        bottom_silk.operations[0].source_layer_id,
        fixture.back_text_id
    );
    assert_ne!(top_copper.composite_sha256, bottom_copper.composite_sha256);
    assert_ne!(top_silk.composite_sha256, bottom_silk.composite_sha256);

    // The top-left source marker is wholly outside the board after its negative
    // X placement, while the opposite marker remains; rasterization therefore
    // proves board-frame clipping rather than just accepting an out-of-range transform.
    assert!(!top_copper.composite.get(0, 30).expect("clipped left edge"));
    assert!(top_copper.composite.active_pixel_count() > 100);
}
