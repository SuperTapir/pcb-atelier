use atelier_core::{
    BitMask, BoardOutline, CardSide, FabricationBuildManifest, FaceProductionLayer, LayerId,
    LayerPolarity, MappingId, PreviewPalette, ProductionTarget, RasterGrid,
    ResolvedFabricationBoard, ResolvedFabricationLayer, ResolvedOperationMask, SolderMaskColor,
    StackupPreset, SurfaceFinish,
};

fn board(width: u32, height: u32) -> ResolvedFabricationBoard {
    let layers = [CardSide::Front, CardSide::Back]
        .into_iter()
        .flat_map(|side| {
            [
                FaceProductionLayer::Copper,
                FaceProductionLayer::SolderMaskOpen,
                FaceProductionLayer::Silkscreen,
            ]
            .into_iter()
            .map(move |production_layer| {
                ResolvedFabricationLayer::empty(
                    ProductionTarget::new(side, production_layer),
                    if production_layer == FaceProductionLayer::SolderMaskOpen {
                        LayerPolarity::Opening
                    } else {
                        LayerPolarity::Positive
                    },
                    width,
                    height,
                )
                .expect("empty resolved layer")
            })
        })
        .collect();
    ResolvedFabricationBoard {
        outline: BoardOutline::Rectangle {
            width_um: width * 100,
            height_um: height * 100,
        },
        stackup: StackupPreset {
            solder_mask_color: SolderMaskColor::Green,
            surface_finish: SurfaceFinish::Enig,
            ..StackupPreset::default()
        },
        grid: RasterGrid {
            origin_x_um: 0,
            origin_y_um: 0,
            width_um: width * 100,
            height_um: height * 100,
            pixel_pitch_um: 100,
            width_px: width,
            height_px: height,
        },
        layers,
        mechanical_features: Vec::new(),
        build: FabricationBuildManifest {
            compiler_version: "test".to_owned(),
            input_sha256: "input".to_owned(),
            output_sha256: "output".to_owned(),
            font_fingerprint: "font".to_owned(),
            pixel_pitch_um: 100,
            sampling_purpose: None,
        },
    }
}

fn layer_mut(
    board: &mut ResolvedFabricationBoard,
    side: CardSide,
    production_layer: FaceProductionLayer,
) -> &mut ResolvedFabricationLayer {
    board
        .layers
        .iter_mut()
        .find(|layer| layer.target == ProductionTarget::new(side, production_layer))
        .expect("canonical resolved layer")
}

#[test]
fn changing_a_final_mask_changes_the_corresponding_preview_pixel() {
    let mut board = board(2, 1);
    let before = board.preview_textures().expect("preview before");

    layer_mut(&mut board, CardSide::Front, FaceProductionLayer::Silkscreen)
        .composite
        .set(0, 0, true)
        .expect("set silk pixel");
    let after = board.preview_textures().expect("preview after");

    assert_ne!(
        before.front.pixel(0, 0).expect("before pixel"),
        after.front.pixel(0, 0).expect("after pixel")
    );
    assert_eq!(
        after.front.pixel(0, 0).expect("silk pixel"),
        after.palette.silkscreen
    );
    assert_eq!(before.back, after.back, "front edit must not affect back");
}

#[test]
fn front_and_back_masks_are_composed_into_independent_textures() {
    let mut board = board(2, 1);
    layer_mut(&mut board, CardSide::Front, FaceProductionLayer::Silkscreen)
        .composite
        .set(0, 0, true)
        .expect("front silk");
    layer_mut(&mut board, CardSide::Back, FaceProductionLayer::Copper)
        .composite
        .set(1, 0, true)
        .expect("back copper");
    layer_mut(
        &mut board,
        CardSide::Back,
        FaceProductionLayer::SolderMaskOpen,
    )
    .composite
    .set(1, 0, true)
    .expect("back opening");

    let preview = board.preview_textures().expect("preview");

    assert_eq!(
        preview.front.pixel(0, 0).expect("front"),
        preview.palette.silkscreen
    );
    assert_ne!(
        preview.back.pixel(0, 0).expect("back untouched"),
        preview.palette.silkscreen
    );
    assert_eq!(
        preview.back.pixel(1, 0).expect("back exposed copper"),
        preview.palette.exposed_copper
    );
    assert_ne!(
        preview.front.pixel(1, 0).expect("front untouched"),
        preview.palette.exposed_copper
    );
}

#[test]
fn production_layer_textures_preserve_six_masks_in_physical_coordinates() {
    let mut board = board(2, 1);
    layer_mut(&mut board, CardSide::Back, FaceProductionLayer::Copper)
        .composite
        .set(1, 0, true)
        .expect("back copper");

    let textures = board
        .production_layer_textures()
        .expect("production textures");

    assert_eq!(textures.len(), 6);
    let back_copper = textures
        .iter()
        .find(|texture| {
            texture.side == CardSide::Back && texture.layer == FaceProductionLayer::Copper
        })
        .expect("back copper texture");
    assert_eq!(
        back_copper.pixel(0, 0).expect("physical left").a,
        0,
        "the backend must not mirror a back-face mask"
    );
    assert_eq!(
        back_copper.pixel(1, 0).expect("physical right"),
        board
            .preview_textures()
            .expect("palette")
            .palette
            .exposed_copper
    );
}

#[test]
fn solder_mask_opening_reveals_copper_instead_of_mask_color() {
    let mut board = board(2, 1);
    let copper = layer_mut(&mut board, CardSide::Front, FaceProductionLayer::Copper);
    copper.composite.set(0, 0, true).expect("covered copper");
    copper.composite.set(1, 0, true).expect("exposed copper");
    layer_mut(
        &mut board,
        CardSide::Front,
        FaceProductionLayer::SolderMaskOpen,
    )
    .composite
    .set(1, 0, true)
    .expect("opening");

    let preview = board.preview_textures().expect("preview");
    let covered = preview.front.pixel(0, 0).expect("covered");
    let exposed = preview.front.pixel(1, 0).expect("exposed");

    assert_eq!(exposed, preview.palette.exposed_copper);
    assert_ne!(covered, preview.palette.exposed_copper);
    assert_ne!(covered, preview.palette.solder_mask);
}

#[test]
fn provenance_operation_masks_do_not_change_preview_until_composite_changes() {
    let mut board = board(1, 1);
    let baseline = board.preview_textures().expect("baseline");
    let mut operation_mask = BitMask::new(1, 1).expect("operation mask");
    operation_mask.set(0, 0, true).expect("operation pixel");
    layer_mut(&mut board, CardSide::Front, FaceProductionLayer::Copper)
        .operations
        .push(ResolvedOperationMask::new(
            MappingId::new(),
            LayerId::new(),
            atelier_core::CombineMode::Add,
            operation_mask,
        ));

    let with_provenance = board.preview_textures().expect("preview");

    assert_eq!(baseline, with_provenance);
}

#[test]
fn stackup_palette_distinguishes_substrate_finish_mask_and_silkscreen() {
    let palette = PreviewPalette::from_stackup(&StackupPreset {
        solder_mask_color: SolderMaskColor::White,
        surface_finish: SurfaceFinish::HaslLeadFree,
        ..StackupPreset::default()
    });

    assert_ne!(palette.substrate, palette.exposed_copper);
    assert_ne!(palette.exposed_copper, palette.solder_mask);
    assert_ne!(palette.solder_mask, palette.silkscreen);
}
