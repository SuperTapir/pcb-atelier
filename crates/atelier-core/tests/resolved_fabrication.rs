use atelier_core::{
    AtelierDocument, BitMask, BoardOutline, CardSide, CombineMode, ContentLayer,
    FabricationOperation, FabricationRasterizer, FaceProductionLayer, LayerId, LayerPolarity,
    MappingId, ProductionMapping, ProductionTarget, RasterGrid, ResolvedFabricationLayer,
    ResolvedOperationMask, TransformUm, compile_fabrication_plan, resolve_fabrication_plan,
};

#[test]
fn production_grid_uses_fixed_physical_pitch_and_rounds_outward() {
    let board = BoardOutline::Rectangle {
        width_um: 64_010,
        height_um: 100_001,
    };

    let grid = RasterGrid::for_board(&board, 25).expect("create production grid");

    assert_eq!(grid.origin_x_um, 0);
    assert_eq!(grid.origin_y_um, 0);
    assert_eq!(grid.pixel_pitch_um, 25);
    assert_eq!(grid.width_px, 2_561);
    assert_eq!(grid.height_px, 4_001);
}

#[test]
fn bit_mask_is_bit_packed_and_combines_add_then_subtract_in_order() {
    let mut first = BitMask::new(9, 2).expect("first mask");
    first.set(0, 0, true).expect("set first pixel");
    first.set(8, 1, true).expect("set last pixel");
    assert_eq!(first.bytes().len(), 3);

    let mut add = BitMask::new(9, 2).expect("add mask");
    add.set(4, 0, true).expect("set add pixel");
    let mut subtract = BitMask::new(9, 2).expect("subtract mask");
    subtract.set(0, 0, true).expect("set subtract pixel");

    first
        .combine(&add, CombineMode::Add)
        .expect("add operation");
    first
        .combine(&subtract, CombineMode::Subtract)
        .expect("subtract operation");

    assert!(!first.get(0, 0).expect("first pixel"));
    assert!(first.get(4, 0).expect("added pixel"));
    assert!(first.get(8, 1).expect("last pixel"));
    assert_eq!(first.active_pixel_count(), 2);
}

#[test]
fn resolved_layer_rebuilds_composite_from_provenance_masks() {
    let mut add_mask = BitMask::new(4, 1).expect("add mask");
    add_mask.set(0, 0, true).expect("pixel");
    add_mask.set(1, 0, true).expect("pixel");
    let mut cut_mask = BitMask::new(4, 1).expect("cut mask");
    cut_mask.set(1, 0, true).expect("pixel");

    let mut layer = ResolvedFabricationLayer::empty(
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::SolderMaskOpen),
        LayerPolarity::Opening,
        4,
        1,
    )
    .expect("resolved layer");
    layer.operations = vec![
        ResolvedOperationMask::new(MappingId::new(), LayerId::new(), CombineMode::Add, add_mask),
        ResolvedOperationMask::new(
            MappingId::new(),
            LayerId::new(),
            CombineMode::Subtract,
            cut_mask,
        ),
    ];

    layer.rebuild_composite().expect("rebuild composite");

    assert!(layer.composite.get(0, 0).expect("remaining opening"));
    assert!(!layer.composite.get(1, 0).expect("cut opening"));
    assert_eq!(layer.polarity, LayerPolarity::Opening);
}

struct DeterministicRasterizer;

impl FabricationRasterizer for DeterministicRasterizer {
    fn fingerprint(&self) -> String {
        "test-rasterizer-v1".to_owned()
    }

    fn rasterize(
        &mut self,
        operation: &FabricationOperation,
        grid: &RasterGrid,
    ) -> Result<BitMask, String> {
        let mut mask =
            BitMask::new(grid.width_px, grid.height_px).map_err(|error| error.to_string())?;
        mask.set(0, 0, true).map_err(|error| error.to_string())?;
        if operation.combine == CombineMode::Add {
            mask.set(1, 0, true).map_err(|error| error.to_string())?;
        }
        Ok(mask)
    }
}

#[test]
fn resolver_builds_six_final_masks_and_a_reproducible_manifest() {
    let mut document = AtelierDocument::new_card("解析", 1_000, 1_000);
    let add = ContentLayer::new_text("添加", "F", TransformUm::rect(0, 0, 500, 500));
    let subtract = ContentLayer::new_text("挖空", "F", TransformUm::rect(0, 0, 500, 500));
    let add_id = add.id;
    let subtract_id = subtract.id;
    document.front.layers.extend([add, subtract]);
    let target = ProductionTarget::new(CardSide::Front, FaceProductionLayer::Copper);
    document.mappings = vec![
        ProductionMapping::new(add_id, target, CombineMode::Add),
        ProductionMapping::new(subtract_id, target, CombineMode::Subtract),
    ];
    let plan = compile_fabrication_plan(&document).expect("compile plan");

    let first =
        resolve_fabrication_plan(&plan, 25, &mut DeterministicRasterizer).expect("resolve board");
    let second =
        resolve_fabrication_plan(&plan, 25, &mut DeterministicRasterizer).expect("resolve again");
    let copper = first
        .layers
        .iter()
        .find(|layer| layer.target == target)
        .expect("top copper");

    assert_eq!(first.layers.len(), 6);
    assert_eq!(copper.operations.len(), 2);
    assert!(!copper.composite.get(0, 0).expect("subtracted pixel"));
    assert!(copper.composite.get(1, 0).expect("remaining pixel"));
    assert_eq!(first.build.input_sha256, second.build.input_sha256);
    assert_eq!(first.build.output_sha256, second.build.output_sha256);
    assert_eq!(first.build.font_fingerprint, "test-rasterizer-v1");
}
