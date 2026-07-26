use atelier_core::{
    AssetReference, AtelierDocument, CardSide, CombineMode, CommandError, CommandHistory,
    ContentKind, ContentLayer, DocumentCommand, FaceProductionLayer, ImageContent,
    ProductionMapping, ProductionTarget, SolderMaskColor, StackupPreset, TextContent, TextLayout,
    TransformUm,
};

fn text_layer(name: &str, x_um: i64) -> ContentLayer {
    ContentLayer::new_text(name, name, TransformUm::rect(x_um, 2_000, 10_000, 5_000))
}

#[test]
fn insert_transform_undo_and_redo_preserve_identity() {
    let mut document = AtelierDocument::new_card("命令", 64_000, 100_000);
    let layer = text_layer("标题", 1_000);
    let layer_id = layer.id;
    let mut history = CommandHistory::default();

    history
        .execute(
            &mut document,
            DocumentCommand::InsertLayer {
                side: CardSide::Front,
                layer,
                index: 0,
            },
        )
        .expect("insert layer");
    history
        .execute(
            &mut document,
            DocumentCommand::TransformLayer {
                layer_id,
                transform: TransformUm::rect(12_000, 15_000, 20_000, 8_000),
            },
        )
        .expect("transform layer");

    assert_eq!(document.front.layers[0].transform.x_um, 12_000);
    assert_eq!(history.undo_depth(), 2);

    history.undo(&mut document).expect("undo transform");
    assert_eq!(document.front.layers[0].id, layer_id);
    assert_eq!(document.front.layers[0].transform.x_um, 1_000);
    history.redo(&mut document).expect("redo transform");
    assert_eq!(document.front.layers[0].transform.x_um, 12_000);
}

#[test]
fn locked_layer_rejects_mutation_without_polluting_history() {
    let mut document = AtelierDocument::new_card("锁定", 64_000, 100_000);
    let layer = text_layer("锁定层", 1_000);
    let layer_id = layer.id;
    document.front.layers.push(layer);
    let mut history = CommandHistory::default();

    history
        .execute(
            &mut document,
            DocumentCommand::SetLayerLock {
                layer_id,
                locked: true,
            },
        )
        .expect("lock layer");
    let error = history
        .execute(
            &mut document,
            DocumentCommand::TransformLayer {
                layer_id,
                transform: TransformUm::rect(20_000, 20_000, 5_000, 5_000),
            },
        )
        .expect_err("locked layer must not transform");

    assert!(matches!(error, CommandError::LayerLocked(id) if id == layer_id));
    assert_eq!(history.undo_depth(), 1);
    assert_eq!(document.front.layers[0].transform.x_um, 1_000);
}

#[test]
fn locked_parent_rejects_child_transform_without_polluting_history() {
    let mut document = AtelierDocument::new_card("父级锁定", 64_000, 100_000);
    let mut group = ContentLayer::new_group("锁定组");
    group.locked = true;
    let group_id = group.id;
    let mut child = text_layer("子层", 1_000);
    let child_id = child.id;
    child.parent_id = Some(group_id);
    document.front.layers.extend([group, child]);
    let mut history = CommandHistory::default();

    let error = history
        .execute(
            &mut document,
            DocumentCommand::TransformLayer {
                layer_id: child_id,
                transform: TransformUm::rect(20_000, 20_000, 5_000, 5_000),
            },
        )
        .expect_err("child of locked group must not transform");

    assert!(matches!(error, CommandError::LayerLocked(id) if id == group_id));
    assert_eq!(history.undo_depth(), 0);
    assert_eq!(document.front.layers[1].transform.x_um, 1_000);
}

#[test]
fn grouping_and_ungrouping_keep_selected_layers_ordered() {
    let mut document = AtelierDocument::new_card("分组", 64_000, 100_000);
    let first = text_layer("第一层", 1_000);
    let second = text_layer("第二层", 2_000);
    let first_id = first.id;
    let second_id = second.id;
    document.front.layers.extend([first, second]);
    let group = ContentLayer::new_group("组合");
    let group_id = group.id;
    let mut history = CommandHistory::default();

    history
        .execute(
            &mut document,
            DocumentCommand::GroupLayers {
                side: CardSide::Front,
                group,
                layer_ids: vec![first_id, second_id],
            },
        )
        .expect("group layers");

    assert_eq!(document.front.layers[0].id, group_id);
    assert_eq!(document.front.layers[1].parent_id, Some(group_id));
    assert_eq!(document.front.layers[2].parent_id, Some(group_id));
    assert_eq!(
        document.front.layers[0].transform,
        TransformUm::rect(1_000, 2_000, 11_000, 5_000)
    );

    history
        .execute(&mut document, DocumentCommand::UngroupLayer { group_id })
        .expect("ungroup layers");

    assert_eq!(
        document
            .front
            .layers
            .iter()
            .map(|layer| layer.id)
            .collect::<Vec<_>>(),
        vec![first_id, second_id]
    );
    assert!(
        document
            .front
            .layers
            .iter()
            .all(|layer| layer.parent_id.is_none())
    );
}

#[test]
fn transforming_group_updates_all_members_as_one_atomic_edit() {
    let mut document = AtelierDocument::new_card("组合变换", 100_000, 100_000);
    let first = ContentLayer::new_text(
        "第一层",
        "A",
        TransformUm::rect(1_000, 2_000, 10_000, 5_000),
    );
    let second = ContentLayer::new_text(
        "第二层",
        "B",
        TransformUm::rect(20_000, 2_000, 10_000, 5_000),
    );
    let first_id = first.id;
    let second_id = second.id;
    let group = ContentLayer::new_group("组合");
    let group_id = group.id;
    document.front.layers.extend([first, second]);
    let mut history = CommandHistory::default();

    history
        .execute(
            &mut document,
            DocumentCommand::GroupLayers {
                side: CardSide::Front,
                group,
                layer_ids: vec![first_id, second_id],
            },
        )
        .expect("group layers");
    history
        .execute(
            &mut document,
            DocumentCommand::TransformLayer {
                layer_id: group_id,
                transform: TransformUm::rect(6_000, 7_000, 58_000, 10_000),
            },
        )
        .expect("transform group");

    assert_eq!(
        document.front.layers[0].transform,
        TransformUm::rect(6_000, 7_000, 58_000, 10_000)
    );
    assert_eq!(
        document.front.layers[1].transform,
        TransformUm::rect(6_000, 7_000, 20_000, 10_000)
    );
    assert_eq!(
        document.front.layers[2].transform,
        TransformUm::rect(44_000, 7_000, 20_000, 10_000)
    );
    assert_eq!(history.undo_depth(), 2);

    history.undo(&mut document).expect("undo group transform");
    assert_eq!(
        document.front.layers[1].transform,
        TransformUm::rect(1_000, 2_000, 10_000, 5_000)
    );
    assert_eq!(
        document.front.layers[2].transform,
        TransformUm::rect(20_000, 2_000, 10_000, 5_000)
    );

    let mut rotated_group = document.front.layers[0].transform;
    rotated_group.rotation_mdeg = 90_000;
    history
        .execute(
            &mut document,
            DocumentCommand::TransformLayer {
                layer_id: group_id,
                transform: rotated_group,
            },
        )
        .expect("rotate group");
    assert_eq!(document.front.layers[1].transform.rotation_mdeg, 90_000);
    assert_eq!(document.front.layers[2].transform.rotation_mdeg, 90_000);
    assert_eq!(document.front.layers[1].transform.x_um, 10_500);
    assert_eq!(document.front.layers[1].transform.y_um, -7_500);
    assert_eq!(document.front.layers[2].transform.x_um, 10_500);
    assert_eq!(document.front.layers[2].transform.y_um, 11_500);
}

#[test]
fn grouping_requires_at_least_two_layers() {
    let mut document = AtelierDocument::new_card("无效组合", 64_000, 100_000);
    let layer = text_layer("单层", 1_000);
    let layer_id = layer.id;
    document.front.layers.push(layer);

    let error = DocumentCommand::GroupLayers {
        side: CardSide::Front,
        group: ContentLayer::new_group("组合"),
        layer_ids: vec![layer_id],
    }
    .apply(&mut document)
    .expect_err("single layer must not form a group");

    assert!(matches!(error, CommandError::InsufficientGroupMembers));
}

#[test]
fn grouping_a_legacy_zero_sized_group_uses_its_descendant_bounds() {
    let mut document = AtelierDocument::new_card("嵌套组合", 64_000, 100_000);
    let legacy_group = ContentLayer::new_group("旧组合");
    let legacy_group_id = legacy_group.id;
    let mut child = text_layer("旧组合子层", 8_000);
    child.parent_id = Some(legacy_group_id);
    let peer = text_layer("同级对象", 20_000);
    let peer_id = peer.id;
    document.front.layers.extend([legacy_group, child, peer]);
    let outer_group = ContentLayer::new_group("外层组合");
    let outer_group_id = outer_group.id;

    DocumentCommand::GroupLayers {
        side: CardSide::Front,
        group: outer_group,
        layer_ids: vec![legacy_group_id, peer_id],
    }
    .apply(&mut document)
    .expect("group legacy parent with peer");

    let outer = document
        .front
        .layers
        .iter()
        .find(|layer| layer.id == outer_group_id)
        .expect("outer group");
    assert_eq!(
        outer.transform,
        TransformUm::rect(8_000, 2_000, 22_000, 5_000)
    );
}

#[test]
fn visibility_reorder_and_mapping_are_reversible_domain_commands() {
    let mut document = AtelierDocument::new_card("映射", 64_000, 100_000);
    let first = text_layer("第一层", 1_000);
    let second = text_layer("第二层", 2_000);
    let first_id = first.id;
    let second_id = second.id;
    document.front.layers.extend([first, second]);
    let mut history = CommandHistory::default();

    history
        .execute(
            &mut document,
            DocumentCommand::SetLayerVisibility {
                layer_id: first_id,
                visible: false,
            },
        )
        .expect("hide layer");
    history
        .execute(
            &mut document,
            DocumentCommand::ReorderLayer {
                layer_id: second_id,
                new_parent_id: None,
                new_index: 0,
            },
        )
        .expect("reorder layer");
    history
        .execute(
            &mut document,
            DocumentCommand::MapLayer {
                mapping: ProductionMapping::new(
                    first_id,
                    ProductionTarget::new(CardSide::Front, FaceProductionLayer::Silkscreen),
                    CombineMode::Add,
                ),
            },
        )
        .expect("map layer");

    assert!(!document.front.layers[1].visible);
    assert_eq!(document.front.layers[0].id, second_id);
    assert_eq!(document.mappings.len(), 1);

    history.undo(&mut document).expect("undo mapping");
    history.undo(&mut document).expect("undo reorder");
    history.undo(&mut document).expect("undo visibility");
    assert!(document.front.layers[0].visible);
    assert_eq!(document.front.layers[0].id, first_id);
    assert!(document.mappings.is_empty());
}

#[test]
fn deleting_group_removes_descendants_and_their_mappings() {
    let mut document = AtelierDocument::new_card("删除组", 64_000, 100_000);
    let group = ContentLayer::new_group("组");
    let group_id = group.id;
    let mut child = text_layer("子层", 1_000);
    child.parent_id = Some(group_id);
    let child_id = child.id;
    document.front.layers.extend([group, child]);
    document.mappings.push(ProductionMapping::new(
        child_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Copper),
        CombineMode::Add,
    ));
    let mut history = CommandHistory::default();

    history
        .execute(
            &mut document,
            DocumentCommand::DeleteLayer { layer_id: group_id },
        )
        .expect("delete group");

    assert!(document.front.layers.is_empty());
    assert!(document.mappings.is_empty());
    history.undo(&mut document).expect("undo group deletion");
    assert_eq!(document.front.layers.len(), 2);
    assert_eq!(document.mappings.len(), 1);
}

#[test]
fn commands_are_serializable_for_cli_and_future_agent_use() {
    let command = DocumentCommand::SetLayerVisibility {
        layer_id: text_layer("测试", 0).id,
        visible: false,
    };

    let json = serde_json::to_string(&command).expect("serialize command");
    let restored: DocumentCommand = serde_json::from_str(&json).expect("deserialize command");

    assert_eq!(restored, command);
}

#[test]
fn text_edit_and_layer_rename_are_undoable_and_respect_locking() {
    let mut document = AtelierDocument::new_card("文字", 64_000, 100_000);
    let layer = text_layer("旧标题", 1_000);
    let layer_id = layer.id;
    document.front.layers.push(layer);
    let mut history = CommandHistory::default();

    history
        .execute(
            &mut document,
            DocumentCommand::SetLayerName {
                layer_id,
                name: "新标题".to_owned(),
            },
        )
        .expect("rename layer");
    history
        .execute(
            &mut document,
            DocumentCommand::SetTextContent {
                layer_id,
                text: TextContent {
                    text: "PCB 艺术".to_owned(),
                    font_family: "Inter".to_owned(),
                    font_size_um: 5_000,
                    layout: TextLayout::FixedFrame,
                },
            },
        )
        .expect("edit text");

    assert_eq!(document.front.layers[0].name, "新标题");
    assert!(matches!(
        &document.front.layers[0].kind,
        ContentKind::Text(text) if text.text == "PCB 艺术"
    ));
    history.undo(&mut document).expect("undo text edit");
    assert!(matches!(
        &document.front.layers[0].kind,
        ContentKind::Text(text) if text.text == "旧标题"
    ));

    document.front.layers[0].locked = true;
    let error = history
        .execute(
            &mut document,
            DocumentCommand::SetLayerName {
                layer_id,
                name: "不可修改".to_owned(),
            },
        )
        .expect_err("locked layer must reject rename");
    assert!(matches!(error, CommandError::LayerLocked(id) if id == layer_id));
}

#[test]
fn replacing_image_asset_preserves_layer_identity_transform_and_mapping() {
    let mut document = AtelierDocument::new_card("替换图片", 64_000, 100_000);
    let first_asset = atelier_core::AssetId::new();
    let replacement_asset = atelier_core::AssetId::new();
    for (id, name) in [
        (first_asset, "first.png"),
        (replacement_asset, "second.png"),
    ] {
        document.assets.push(AssetReference {
            id,
            embedded_path: format!("assets/{id}.png"),
            original_filename: name.to_owned(),
            media_type: "image/png".to_owned(),
            sha256: name.to_owned(),
            pixel_width: 100,
            pixel_height: 100,
        });
    }
    let transform = TransformUm::rect(7_000, 9_000, 30_000, 40_000);
    let layer = ContentLayer::new_image("图片", first_asset, transform);
    let layer_id = layer.id;
    document.front.layers.push(layer);
    document.mappings.push(ProductionMapping::new(
        layer_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Copper),
        CombineMode::Add,
    ));
    let mut history = CommandHistory::default();

    history
        .execute(
            &mut document,
            DocumentCommand::SetImageContent {
                layer_id,
                image: ImageContent {
                    asset_id: replacement_asset,
                    crop: None,
                },
            },
        )
        .expect("replace image asset");

    assert_eq!(document.front.layers[0].id, layer_id);
    assert_eq!(document.front.layers[0].transform, transform);
    assert_eq!(document.mappings[0].source_layer_id, layer_id);
    assert!(matches!(
        &document.front.layers[0].kind,
        ContentKind::Image(image) if image.asset_id == replacement_asset
    ));
}

#[test]
fn content_specific_commands_reject_the_wrong_layer_kind() {
    let mut document = AtelierDocument::new_card("类型", 64_000, 100_000);
    let layer = text_layer("文字", 1_000);
    let layer_id = layer.id;
    document.front.layers.push(layer);
    let mut history = CommandHistory::default();

    let error = history
        .execute(
            &mut document,
            DocumentCommand::SetImageContent {
                layer_id,
                image: ImageContent {
                    asset_id: atelier_core::AssetId::new(),
                    crop: None,
                },
            },
        )
        .expect_err("text layer must reject image content");

    assert!(matches!(
        error,
        CommandError::UnexpectedLayerKind {
            layer_id: id,
            expected: "image"
        } if id == layer_id
    ));
}

#[test]
fn stackup_changes_are_undoable_domain_commands() {
    let mut document = AtelierDocument::new_card("叠层", 64_000, 100_000);
    let original = document.stackup.clone();
    let updated = StackupPreset {
        solder_mask_color: SolderMaskColor::Purple,
        ..StackupPreset::default()
    };
    let mut history = CommandHistory::default();

    history
        .execute(
            &mut document,
            DocumentCommand::SetStackup {
                stackup: updated.clone(),
            },
        )
        .expect("set stackup");
    assert_eq!(document.stackup, updated);

    history.undo(&mut document).expect("undo stackup");
    assert_eq!(document.stackup, original);
}
