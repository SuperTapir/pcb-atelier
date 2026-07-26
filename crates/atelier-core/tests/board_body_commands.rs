use atelier_core::{
    AssetId, AtelierDocument, BoardOutline, CardSide, CombineMode, CommandError, CommandHistory,
    CommandOutcome, ContentKind, ContentLayer, DocumentCommand, DocumentDiagnostic, DocumentError,
    FaceProductionLayer, LayerId, ProductionMapping, ProductionTarget, SolderMaskColor,
    StackupPreset, TransformUm,
};

#[test]
fn changing_outline_preserves_physical_transforms_and_returns_overflow_diagnostics() {
    let mut document = AtelierDocument::new_card("缩小板框", 100_000, 80_000);
    let layer = ContentLayer::new_text(
        "越界文字",
        "EDGE",
        TransformUm::rect(55_000, 10_000, 30_000, 12_000),
    );
    let layer_id = layer.id;
    let original_transform = layer.transform;
    document.front.layers.push(layer);
    let original_outline = document.board.clone();
    let mut history = CommandHistory::default();

    let outcome = history
        .execute(
            &mut document,
            DocumentCommand::SetBoardOutline {
                outline: BoardOutline::RoundedRectangle {
                    width_um: 64_000,
                    height_um: 80_000,
                    corner_radius_um: 2_000,
                },
            },
        )
        .expect("valid smaller outline remains editable");

    let CommandOutcome::BoardOutlineUpdated { diagnostics } = outcome else {
        panic!("expected outline diagnostics");
    };
    assert_eq!(document.front.layers[0].transform, original_transform);
    assert!(diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        DocumentDiagnostic::ContentOutsideBoard {
            side: CardSide::Front,
            layer_id: id,
            ..
        } if *id == layer_id
    )));

    history.undo(&mut document).expect("undo board outline");
    assert_eq!(document.board, original_outline);
    assert_eq!(document.front.layers[0].transform, original_transform);
}

#[test]
fn outline_diagnostics_use_rotated_physical_bounds() {
    let mut document = AtelierDocument::new_card("旋转越界", 64_000, 100_000);
    let mut rotated =
        ContentLayer::new_text("旋转文字", "R", TransformUm::rect(0, 0, 20_000, 20_000));
    rotated.transform.rotation_mdeg = 45_000;
    let layer_id = rotated.id;
    document.front.layers.push(rotated);

    let diagnostics = document.content_bounds_diagnostics();
    let Some(DocumentDiagnostic::ContentOutsideBoard { bounds, .. }) =
        diagnostics.iter().find(|diagnostic| {
            matches!(
                diagnostic,
                DocumentDiagnostic::ContentOutsideBoard {
                    layer_id: id,
                    ..
                } if *id == layer_id
            )
        })
    else {
        panic!("rotated object should cross the top-left board bounds");
    };
    assert!(bounds.min_x_um < 0);
    assert!(bounds.min_y_um < 0);
}

#[test]
fn invalid_stackup_is_rejected_without_mutating_the_document_or_history() {
    let mut document = AtelierDocument::new_card("叠层", 64_000, 100_000);
    let original = document.stackup.clone();
    let mut history = CommandHistory::default();

    let error = history
        .execute(
            &mut document,
            DocumentCommand::SetStackup {
                stackup: StackupPreset {
                    thickness_um: 0,
                    solder_mask_color: SolderMaskColor::Purple,
                    ..StackupPreset::default()
                },
            },
        )
        .expect_err("zero board thickness is invalid");

    assert!(matches!(
        error,
        CommandError::InvalidDocument(DocumentError::InvalidBoardThickness)
    ));
    assert_eq!(document.stackup, original);
    assert_eq!(history.undo_depth(), 0);
}

#[test]
fn board_fill_creation_is_idempotent_per_face_and_returns_the_existing_id() {
    let mut document = AtelierDocument::new_card("双面铺铜", 64_000, 100_000);
    let front_id = LayerId::new();
    let ignored_duplicate_id = LayerId::new();
    let back_id = LayerId::new();
    let mut history = CommandHistory::default();

    let created = history
        .execute(
            &mut document,
            DocumentCommand::CreateBoardFill {
                side: CardSide::Front,
                layer_id: front_id,
                name: "正面基础铺铜".into(),
                edge_clearance_um: 500,
            },
        )
        .expect("create front fill");
    assert_eq!(
        created,
        CommandOutcome::BoardFillReady {
            layer_id: front_id,
            created: true,
        }
    );
    assert_eq!(history.undo_depth(), 1);

    let duplicate = history
        .execute(
            &mut document,
            DocumentCommand::CreateBoardFill {
                side: CardSide::Front,
                layer_id: ignored_duplicate_id,
                name: "不会创建".into(),
                edge_clearance_um: 900,
            },
        )
        .expect("duplicate returns existing fill");
    assert_eq!(
        duplicate,
        CommandOutcome::BoardFillReady {
            layer_id: front_id,
            created: false,
        }
    );
    assert_eq!(
        history.undo_depth(),
        1,
        "idempotent command adds no history"
    );
    assert_eq!(document.front.layers.len(), 1);
    assert!(matches!(
        document.front.layers[0].kind,
        ContentKind::BoardFill(_)
    ));

    let back = history
        .execute(
            &mut document,
            DocumentCommand::CreateBoardFill {
                side: CardSide::Back,
                layer_id: back_id,
                name: "背面基础铺铜".into(),
                edge_clearance_um: 500,
            },
        )
        .expect("back has its own fill");
    assert_eq!(
        back,
        CommandOutcome::BoardFillReady {
            layer_id: back_id,
            created: true,
        }
    );
    assert_eq!(document.back.layers.len(), 1);
}

#[test]
fn document_validation_rejects_multiple_board_fills_on_one_face() {
    let mut document = AtelierDocument::new_card("非法重复铺铜", 64_000, 100_000);
    let first = ContentLayer::new_board_fill("铺铜一", 500);
    let duplicate = ContentLayer::new_board_fill("铺铜二", 700);
    let first_id = first.id;
    let duplicate_id = duplicate.id;
    document.front.layers.extend([first, duplicate]);

    assert!(matches!(
        document.validate().expect_err("one fill per face"),
        DocumentError::MultipleBoardFills {
            side: CardSide::Front,
            first_layer_id,
            duplicate_layer_id,
        } if first_layer_id == first_id && duplicate_layer_id == duplicate_id
    ));
}

#[test]
fn map_command_rejects_cross_face_and_non_copper_board_fill_targets() {
    let mut document = AtelierDocument::new_card("映射边界", 64_000, 100_000);
    let fill = ContentLayer::new_board_fill("正面铺铜", 500);
    let fill_id = fill.id;
    document.front.layers.push(fill);

    let cross_face = DocumentCommand::MapLayer {
        mapping: ProductionMapping::new(
            fill_id,
            ProductionTarget::new(CardSide::Back, FaceProductionLayer::Copper),
            CombineMode::Add,
        ),
    }
    .apply(&mut document)
    .expect_err("command itself rejects cross-face mapping");
    assert!(matches!(
        cross_face,
        CommandError::LayerOnWrongFace {
            layer_id,
            expected_side: CardSide::Back,
            actual_side: CardSide::Front,
        } if layer_id == fill_id
    ));

    let non_copper = DocumentCommand::MapLayer {
        mapping: ProductionMapping::new(
            fill_id,
            ProductionTarget::new(CardSide::Front, FaceProductionLayer::Silkscreen),
            CombineMode::Add,
        ),
    }
    .apply(&mut document)
    .expect_err("board fill maps only to same-face copper");
    assert!(matches!(
        non_copper,
        CommandError::InvalidBoardFillTarget { layer_id, .. } if layer_id == fill_id
    ));

    let text = ContentLayer::new_text(
        "普通内容",
        "F",
        TransformUm::rect(1_000, 1_000, 5_000, 5_000),
    );
    let text_id = text.id;
    document.front.layers.push(text);
    for layer in [
        FaceProductionLayer::Copper,
        FaceProductionLayer::SolderMaskOpen,
        FaceProductionLayer::Silkscreen,
    ] {
        DocumentCommand::MapLayer {
            mapping: ProductionMapping::new(
                text_id,
                ProductionTarget::new(CardSide::Front, layer),
                CombineMode::Add,
            ),
        }
        .apply(&mut document)
        .expect("image/text mapping target remains supported");
    }

    let image = ContentLayer::new_image(
        "普通图片",
        AssetId::new(),
        TransformUm::rect(2_000, 2_000, 5_000, 5_000),
    );
    let image_id = image.id;
    document.front.layers.push(image);
    for layer in [
        FaceProductionLayer::Copper,
        FaceProductionLayer::SolderMaskOpen,
        FaceProductionLayer::Silkscreen,
    ] {
        DocumentCommand::MapLayer {
            mapping: ProductionMapping::new(
                image_id,
                ProductionTarget::new(CardSide::Front, layer),
                CombineMode::Add,
            ),
        }
        .apply(&mut document)
        .expect("image mapping target remains supported");
    }
}
