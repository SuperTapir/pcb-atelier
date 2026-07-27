use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    AssetId, AssetReference, AtelierDocument, BoardFillContent, BoardOutline, CardFace, CardSide,
    ContentKind, ContentLayer, DocumentDiagnostic, DocumentError, FaceProductionLayer,
    ImageContent, ImageProductionMode, ImageTreatment, LayerId, ManufacturerProfileSnapshot,
    MappingId, ProductionMapping, ProductionTarget, StackupPreset, TextContent, TransformUm,
    TreatmentId, TreatmentRecipe,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase")]
// Commands intentionally own complete snapshots so serialization and
// undo/redo remain atomic; boxing selected variants would only move this cost
// to heap allocation without reducing history payloads.
#[allow(clippy::large_enum_variant)]
pub enum DocumentCommand {
    InsertLayer {
        side: CardSide,
        layer: ContentLayer,
        index: usize,
    },
    DeleteLayer {
        layer_id: LayerId,
    },
    DeleteLayers {
        layer_ids: Vec<LayerId>,
    },
    DuplicateLayer {
        layer_id: LayerId,
        duplicate_layer_id: LayerId,
        duplicate_mapping_ids: Vec<MappingId>,
        offset_um: i64,
    },
    TransformLayer {
        layer_id: LayerId,
        transform: TransformUm,
    },
    TransformLayers {
        transforms: Vec<LayerTransform>,
    },
    SetLayerLock {
        layer_id: LayerId,
        locked: bool,
    },
    SetLayerVisibility {
        layer_id: LayerId,
        visible: bool,
    },
    SetLayerExportEnabled {
        layer_id: LayerId,
        export_enabled: bool,
    },
    SetLayerName {
        layer_id: LayerId,
        name: String,
    },
    SetImageContent {
        layer_id: LayerId,
        image: ImageContent,
    },
    ReplaceImageInstanceAsset {
        layer_id: LayerId,
        asset_id: AssetId,
    },
    SetTextContent {
        layer_id: LayerId,
        text: TextContent,
    },
    SetBoardFillContent {
        layer_id: LayerId,
        fill: BoardFillContent,
    },
    CreateBoardFill {
        side: CardSide,
        layer_id: LayerId,
        name: String,
        edge_clearance_um: u32,
    },
    SetBoardOutline {
        outline: BoardOutline,
    },
    SetStackup {
        stackup: StackupPreset,
    },
    InsertImageTreatment {
        treatment: ImageTreatment,
    },
    InsertProcessedImage {
        asset: Option<AssetReference>,
        side: CardSide,
        layer: ContentLayer,
        index: usize,
        treatment: ImageTreatment,
        mapping: ProductionMapping,
    },
    SetTreatmentRecipe {
        treatment_id: TreatmentId,
        recipe: TreatmentRecipe,
    },
    SetImageProductionMode {
        treatment_id: TreatmentId,
        production_mode: ImageProductionMode,
    },
    DeleteImageTreatment {
        treatment_id: TreatmentId,
    },
    SetManufacturerProfile {
        profile: ManufacturerProfileSnapshot,
    },
    ReorderLayer {
        layer_id: LayerId,
        new_parent_id: Option<LayerId>,
        new_index: usize,
    },
    MoveLayer {
        layer_id: LayerId,
        new_parent_id: Option<LayerId>,
        new_index: usize,
        from_target: ProductionTarget,
        to_target: ProductionTarget,
    },
    TransferLayers {
        layer_ids: Vec<LayerId>,
        target: ProductionTarget,
        new_parent_id: Option<LayerId>,
        new_index: usize,
        mode: LayerTransferMode,
        duplicate_layer_ids: Vec<LayerId>,
        duplicate_mapping_ids: Vec<MappingId>,
        offset_um: i64,
    },
    PasteLayers {
        layers: Vec<ContentLayer>,
        mappings: Vec<ProductionMapping>,
        target: ProductionTarget,
        new_parent_id: Option<LayerId>,
        new_index: usize,
    },
    GroupLayers {
        side: CardSide,
        group: ContentLayer,
        layer_ids: Vec<LayerId>,
    },
    UngroupLayer {
        group_id: LayerId,
    },
    MapLayer {
        mapping: ProductionMapping,
    },
    UnmapLayer {
        mapping_id: MappingId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerTransform {
    pub layer_id: LayerId,
    pub transform: TransformUm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayerTransferMode {
    Copy,
    Move,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum CommandOutcome {
    Applied,
    BoardOutlineUpdated {
        diagnostics: Vec<DocumentDiagnostic>,
    },
    BoardFillReady {
        layer_id: LayerId,
        created: bool,
    },
}

impl DocumentCommand {
    pub fn apply(self, document: &mut AtelierDocument) -> Result<CommandOutcome, CommandError> {
        let mut outcome = CommandOutcome::Applied;
        match self {
            Self::InsertLayer { side, layer, index } => {
                if index > face(document, side).layers.len() {
                    return Err(CommandError::InvalidLayerIndex(index));
                }
                face_mut(document, side).layers.insert(index, layer);
            }
            Self::DeleteLayer { layer_id } => delete_layer(document, layer_id)?,
            Self::DeleteLayers { layer_ids } => {
                let selected = layer_ids.iter().copied().collect::<HashSet<_>>();
                if selected.len() != layer_ids.len() {
                    return Err(CommandError::DuplicateDeleteLayer);
                }
                for layer_id in &layer_ids {
                    ensure_unlocked(layer(document, *layer_id)?)?;
                }
                let mut roots = Vec::new();
                for layer_id in layer_ids {
                    let mut parent_id = layer(document, layer_id)?.parent_id;
                    let mut covered_by_selected_ancestor = false;
                    while let Some(parent) = parent_id {
                        if selected.contains(&parent) {
                            covered_by_selected_ancestor = true;
                            break;
                        }
                        parent_id = layer(document, parent)?.parent_id;
                    }
                    if !covered_by_selected_ancestor {
                        roots.push(layer_id);
                    }
                }
                for layer_id in roots {
                    delete_layer(document, layer_id)?;
                }
            }
            Self::DuplicateLayer {
                layer_id,
                duplicate_layer_id,
                duplicate_mapping_ids,
                offset_um,
            } => duplicate_layer(
                document,
                layer_id,
                duplicate_layer_id,
                &duplicate_mapping_ids,
                offset_um,
            )?,
            Self::TransformLayer {
                layer_id,
                transform,
            } => {
                ensure_layer_unlocked(document, layer_id)?;
                transform_layer(document, layer_id, transform)?;
            }
            Self::TransformLayers { transforms } => {
                let mut seen = HashSet::new();
                for update in &transforms {
                    if !seen.insert(update.layer_id) {
                        return Err(CommandError::DuplicateTransformLayer(update.layer_id));
                    }
                    ensure_layer_unlocked(document, update.layer_id)?;
                }
                for update in transforms {
                    transform_layer(document, update.layer_id, update.transform)?;
                }
            }
            Self::SetLayerLock { layer_id, locked } => {
                layer_mut(document, layer_id)?.locked = locked;
            }
            Self::SetLayerVisibility { layer_id, visible } => {
                layer_mut(document, layer_id)?.visible = visible;
            }
            Self::SetLayerExportEnabled {
                layer_id,
                export_enabled,
            } => {
                layer_mut(document, layer_id)?.export_enabled = export_enabled;
            }
            Self::SetLayerName { layer_id, name } => {
                let layer = layer_mut(document, layer_id)?;
                ensure_unlocked(layer)?;
                layer.name = name;
            }
            Self::SetImageContent { layer_id, image } => {
                let layer = layer_mut(document, layer_id)?;
                ensure_unlocked(layer)?;
                if !matches!(&layer.kind, ContentKind::Image(_)) {
                    return Err(CommandError::UnexpectedLayerKind {
                        layer_id,
                        expected: "image",
                    });
                }
                layer.kind = ContentKind::Image(image);
            }
            Self::ReplaceImageInstanceAsset { layer_id, asset_id } => {
                replace_image_instance_asset(document, layer_id, asset_id)?;
            }
            Self::SetTextContent { layer_id, text } => {
                let layer = layer_mut(document, layer_id)?;
                ensure_unlocked(layer)?;
                if !matches!(&layer.kind, ContentKind::Text(_)) {
                    return Err(CommandError::UnexpectedLayerKind {
                        layer_id,
                        expected: "text",
                    });
                }
                layer.kind = ContentKind::Text(text);
            }
            Self::SetBoardFillContent { layer_id, fill } => {
                let layer = layer_mut(document, layer_id)?;
                ensure_unlocked(layer)?;
                if !matches!(&layer.kind, ContentKind::BoardFill(_)) {
                    return Err(CommandError::UnexpectedLayerKind {
                        layer_id,
                        expected: "board fill",
                    });
                }
                layer.kind = ContentKind::BoardFill(fill);
            }
            Self::CreateBoardFill {
                side,
                layer_id,
                name,
                edge_clearance_um,
            } => {
                if let Some(existing) = face(document, side)
                    .layers
                    .iter()
                    .find(|layer| matches!(layer.kind, ContentKind::BoardFill(_)))
                {
                    outcome = CommandOutcome::BoardFillReady {
                        layer_id: existing.id,
                        created: false,
                    };
                } else {
                    let mut layer = ContentLayer::new_board_fill(name, edge_clearance_um);
                    layer.id = layer_id;
                    face_mut(document, side).layers.push(layer);
                    outcome = CommandOutcome::BoardFillReady {
                        layer_id,
                        created: true,
                    };
                }
            }
            Self::SetBoardOutline { outline } => {
                document.board = outline;
                outcome = CommandOutcome::BoardOutlineUpdated {
                    diagnostics: document.content_bounds_diagnostics(),
                };
            }
            Self::SetStackup { stackup } => {
                document.manufacturer_profile.substrate = stackup.substrate;
                document.manufacturer_profile.thickness_um = stackup.thickness_um;
                document.manufacturer_profile.solder_mask = stackup.solder_mask_color;
                document.manufacturer_profile.surface_finish = stackup.surface_finish;
                document.stackup = stackup;
            }
            Self::InsertImageTreatment { treatment } => {
                document.image_treatments.push(treatment);
            }
            Self::InsertProcessedImage {
                asset,
                side,
                layer,
                index,
                treatment,
                mapping,
            } => {
                if index > face(document, side).layers.len() {
                    return Err(CommandError::InvalidLayerIndex(index));
                }
                if let Some(asset) = asset {
                    document.assets.push(asset);
                }
                document.image_treatments.push(treatment);
                face_mut(document, side).layers.insert(index, layer);
                document.mappings.push(mapping);
            }
            Self::SetTreatmentRecipe {
                treatment_id,
                recipe,
            } => {
                let treatment = document
                    .image_treatments
                    .iter_mut()
                    .find(|treatment| treatment.id == treatment_id)
                    .ok_or(CommandError::TreatmentNotFound(treatment_id))?;
                treatment.recipe = recipe;
            }
            Self::SetImageProductionMode {
                treatment_id,
                production_mode,
            } => {
                let treatment = document
                    .image_treatments
                    .iter_mut()
                    .find(|treatment| treatment.id == treatment_id)
                    .ok_or(CommandError::TreatmentNotFound(treatment_id))?;
                treatment.production_mode = production_mode;
            }
            Self::DeleteImageTreatment { treatment_id } => {
                let index = document
                    .image_treatments
                    .iter()
                    .position(|treatment| treatment.id == treatment_id)
                    .ok_or(CommandError::TreatmentNotFound(treatment_id))?;
                document.image_treatments.remove(index);
            }
            Self::SetManufacturerProfile { profile } => {
                document.stackup.substrate = profile.substrate;
                document.stackup.thickness_um = profile.thickness_um;
                document.stackup.solder_mask_color = profile.solder_mask;
                document.stackup.surface_finish = profile.surface_finish;
                document.manufacturer_profile = profile;
            }
            Self::ReorderLayer {
                layer_id,
                new_parent_id,
                new_index,
            } => reorder_layer(document, layer_id, new_parent_id, new_index)?,
            Self::MoveLayer {
                layer_id,
                new_parent_id,
                new_index,
                from_target,
                to_target,
            } => move_layer(
                document,
                layer_id,
                new_parent_id,
                new_index,
                from_target,
                to_target,
            )?,
            Self::TransferLayers {
                layer_ids,
                target,
                new_parent_id,
                new_index,
                mode,
                duplicate_layer_ids,
                duplicate_mapping_ids,
                offset_um,
            } => transfer_layers(
                document,
                &layer_ids,
                target,
                new_parent_id,
                new_index,
                mode,
                &duplicate_layer_ids,
                &duplicate_mapping_ids,
                offset_um,
            )?,
            Self::PasteLayers {
                layers,
                mappings,
                target,
                new_parent_id,
                new_index,
            } => paste_layers(document, layers, mappings, target, new_parent_id, new_index)?,
            Self::GroupLayers {
                side,
                group,
                layer_ids,
            } => group_layers(document, side, group, layer_ids)?,
            Self::UngroupLayer { group_id } => ungroup_layer(document, group_id)?,
            Self::MapLayer { mut mapping } => {
                let (source_side, source_index) =
                    locate_layer(document, mapping.source_layer_id)
                        .ok_or(CommandError::LayerNotFound(mapping.source_layer_id))?;
                let source = &face(document, source_side).layers[source_index];
                let image_asset_id = match &source.kind {
                    ContentKind::Image(image) => Some(image.asset_id),
                    _ => None,
                };
                if source.locked {
                    return Err(CommandError::LayerLocked(source.id));
                }
                if source_side != mapping.target.side {
                    return Err(CommandError::LayerOnWrongFace {
                        layer_id: source.id,
                        expected_side: mapping.target.side,
                        actual_side: source_side,
                    });
                }
                if matches!(source.kind, ContentKind::BoardFill(_))
                    && mapping.target.layer != FaceProductionLayer::Copper
                {
                    return Err(CommandError::InvalidBoardFillTarget {
                        layer_id: source.id,
                        target: mapping.target.canonical_name(),
                    });
                }
                if let Some(image_asset_id) = image_asset_id
                    && mapping.treatment_id.is_none()
                {
                    let treatment_id = document
                        .image_treatments
                        .iter()
                        .find(|treatment| treatment.asset_id == image_asset_id)
                        .map(|treatment| treatment.id)
                        .unwrap_or_else(|| {
                            let treatment = ImageTreatment::new(
                                image_asset_id,
                                TreatmentRecipe::standard_monochrome(),
                            );
                            let treatment_id = treatment.id;
                            document.image_treatments.push(treatment);
                            treatment_id
                        });
                    mapping.treatment_id = Some(treatment_id);
                }
                document.mappings.push(mapping);
            }
            Self::UnmapLayer { mapping_id } => {
                let index = document
                    .mappings
                    .iter()
                    .position(|mapping| mapping.id == mapping_id)
                    .ok_or(CommandError::MappingNotFound(mapping_id))?;
                let source_layer_id = document.mappings[index].source_layer_id;
                if layer(document, source_layer_id)?.locked {
                    return Err(CommandError::LayerLocked(source_layer_id));
                }
                document.mappings.remove(index);
            }
        }
        Ok(outcome)
    }
}

#[derive(Debug, Default)]
pub struct CommandHistory {
    undo_stack: Vec<AtelierDocument>,
    redo_stack: Vec<AtelierDocument>,
}

impl CommandHistory {
    pub fn execute(
        &mut self,
        document: &mut AtelierDocument,
        command: DocumentCommand,
    ) -> Result<CommandOutcome, CommandError> {
        let previous = document.clone();
        let outcome = match command.apply(document) {
            Ok(outcome) => outcome,
            Err(error) => {
                *document = previous;
                return Err(error);
            }
        };
        if let Err(error) = document.validate() {
            *document = previous;
            return Err(CommandError::from(error));
        }
        if *document != previous {
            self.undo_stack.push(previous);
            self.redo_stack.clear();
        }
        Ok(outcome)
    }

    pub fn undo(&mut self, document: &mut AtelierDocument) -> Result<(), CommandError> {
        let previous = self.undo_stack.pop().ok_or(CommandError::NothingToUndo)?;
        let current = std::mem::replace(document, previous);
        self.redo_stack.push(current);
        Ok(())
    }

    pub fn redo(&mut self, document: &mut AtelierDocument) -> Result<(), CommandError> {
        let next = self.redo_stack.pop().ok_or(CommandError::NothingToRedo)?;
        let current = std::mem::replace(document, next);
        self.undo_stack.push(current);
        Ok(())
    }

    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }
}

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("content layer not found: {0}")]
    LayerNotFound(LayerId),
    #[error("production mapping not found: {0}")]
    MappingNotFound(MappingId),
    #[error("duplicate layer needs {expected} mapping ids, received {actual}")]
    DuplicateMappingIdCount { expected: usize, actual: usize },
    #[error("batch delete contains a duplicate layer id")]
    DuplicateDeleteLayer,
    #[error("image treatment not found: {0}")]
    TreatmentNotFound(TreatmentId),
    #[error("content layer is locked: {0}")]
    LayerLocked(LayerId),
    #[error("batch transform contains duplicate layer id: {0}")]
    DuplicateTransformLayer(LayerId),
    #[error("layer transfer requires at least one content layer")]
    EmptyLayerTransfer,
    #[error("layer transfer contains a duplicate layer id: {0}")]
    DuplicateTransferLayer(LayerId),
    #[error("layer transfer spans more than one card face")]
    MixedFaceLayerTransfer,
    #[error("layer transfer needs {expected} duplicate layer ids, received {actual}")]
    TransferLayerIdCount { expected: usize, actual: usize },
    #[error("layer transfer needs {expected} duplicate mapping ids, received {actual}")]
    TransferMappingIdCount { expected: usize, actual: usize },
    #[error("content layer {layer_id} is not a {expected} layer")]
    UnexpectedLayerKind {
        layer_id: LayerId,
        expected: &'static str,
    },
    #[error("layer index is out of bounds: {0}")]
    InvalidLayerIndex(usize),
    #[error("group command requires at least one content layer")]
    EmptyGroup,
    #[error("group command requires at least two content layers")]
    InsufficientGroupMembers,
    #[error("group layer {0} is not a group")]
    NotAGroup(LayerId),
    #[error("grouping selection contains duplicate layer id: {0}")]
    DuplicateGroupMember(LayerId),
    #[error("content layer {layer_id} belongs to {actual_side}, not {expected_side}")]
    LayerOnWrongFace {
        layer_id: LayerId,
        expected_side: CardSide,
        actual_side: CardSide,
    },
    #[error("board fill layer {layer_id} cannot map to {target}")]
    InvalidBoardFillTarget {
        layer_id: LayerId,
        target: &'static str,
    },
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
    #[error(transparent)]
    InvalidDocument(#[from] DocumentError),
}

fn face(document: &AtelierDocument, side: CardSide) -> &CardFace {
    match side {
        CardSide::Front => &document.front,
        CardSide::Back => &document.back,
    }
}

fn face_mut(document: &mut AtelierDocument, side: CardSide) -> &mut CardFace {
    match side {
        CardSide::Front => &mut document.front,
        CardSide::Back => &mut document.back,
    }
}

fn locate_layer(document: &AtelierDocument, layer_id: LayerId) -> Option<(CardSide, usize)> {
    document
        .front
        .layers
        .iter()
        .position(|layer| layer.id == layer_id)
        .map(|index| (CardSide::Front, index))
        .or_else(|| {
            document
                .back
                .layers
                .iter()
                .position(|layer| layer.id == layer_id)
                .map(|index| (CardSide::Back, index))
        })
}

fn layer(document: &AtelierDocument, layer_id: LayerId) -> Result<&ContentLayer, CommandError> {
    let (side, index) =
        locate_layer(document, layer_id).ok_or(CommandError::LayerNotFound(layer_id))?;
    Ok(&face(document, side).layers[index])
}

fn layer_mut(
    document: &mut AtelierDocument,
    layer_id: LayerId,
) -> Result<&mut ContentLayer, CommandError> {
    let (side, index) =
        locate_layer(document, layer_id).ok_or(CommandError::LayerNotFound(layer_id))?;
    Ok(&mut face_mut(document, side).layers[index])
}

fn ensure_unlocked(layer: &ContentLayer) -> Result<(), CommandError> {
    if layer.locked {
        Err(CommandError::LayerLocked(layer.id))
    } else {
        Ok(())
    }
}

fn ensure_layer_unlocked(
    document: &AtelierDocument,
    layer_id: LayerId,
) -> Result<(), CommandError> {
    let mut current = layer(document, layer_id)?;
    ensure_unlocked(current)?;
    while let Some(parent_id) = current.parent_id {
        current = layer(document, parent_id)?;
        ensure_unlocked(current)?;
    }
    Ok(())
}

fn delete_layer(document: &mut AtelierDocument, layer_id: LayerId) -> Result<(), CommandError> {
    let (side, _) =
        locate_layer(document, layer_id).ok_or(CommandError::LayerNotFound(layer_id))?;
    ensure_unlocked(layer(document, layer_id)?)?;

    let layers = &face(document, side).layers;
    let mut deleted = HashSet::from([layer_id]);
    loop {
        let before = deleted.len();
        for candidate in layers {
            if candidate
                .parent_id
                .is_some_and(|parent_id| deleted.contains(&parent_id))
            {
                if candidate.locked {
                    return Err(CommandError::LayerLocked(candidate.id));
                }
                deleted.insert(candidate.id);
            }
        }
        if deleted.len() == before {
            break;
        }
    }

    face_mut(document, side)
        .layers
        .retain(|candidate| !deleted.contains(&candidate.id));
    document
        .mappings
        .retain(|mapping| !deleted.contains(&mapping.source_layer_id));
    Ok(())
}

fn duplicate_layer(
    document: &mut AtelierDocument,
    layer_id: LayerId,
    duplicate_layer_id: LayerId,
    duplicate_mapping_ids: &[MappingId],
    offset_um: i64,
) -> Result<(), CommandError> {
    let (side, index) =
        locate_layer(document, layer_id).ok_or(CommandError::LayerNotFound(layer_id))?;
    let source = layer(document, layer_id)?;
    ensure_unlocked(source)?;
    if matches!(source.kind, ContentKind::BoardFill(_) | ContentKind::Group) {
        return Err(CommandError::UnexpectedLayerKind {
            layer_id,
            expected: "image or text",
        });
    }
    let source_mappings = document
        .mappings
        .iter()
        .filter(|mapping| mapping.source_layer_id == layer_id)
        .cloned()
        .collect::<Vec<_>>();
    if source_mappings.len() != duplicate_mapping_ids.len() {
        return Err(CommandError::DuplicateMappingIdCount {
            expected: source_mappings.len(),
            actual: duplicate_mapping_ids.len(),
        });
    }

    let mut duplicate = source.clone();
    duplicate.id = duplicate_layer_id;
    duplicate.name = format!("{} 副本", source.name);
    duplicate.transform.x_um += offset_um;
    duplicate.transform.y_um += offset_um;
    face_mut(document, side).layers.insert(index + 1, duplicate);

    for (source_mapping, mapping_id) in source_mappings
        .into_iter()
        .zip(duplicate_mapping_ids.iter().copied())
    {
        let mut duplicate_mapping = source_mapping;
        duplicate_mapping.id = mapping_id;
        duplicate_mapping.source_layer_id = duplicate_layer_id;
        document.mappings.push(duplicate_mapping);
    }
    Ok(())
}

fn replace_image_instance_asset(
    document: &mut AtelierDocument,
    layer_id: LayerId,
    replacement_asset_id: AssetId,
) -> Result<(), CommandError> {
    ensure_layer_unlocked(document, layer_id)?;
    let (side, index) =
        locate_layer(document, layer_id).ok_or(CommandError::LayerNotFound(layer_id))?;
    let ContentKind::Image(image) = &face(document, side).layers[index].kind else {
        return Err(CommandError::UnexpectedLayerKind {
            layer_id,
            expected: "image",
        });
    };
    if !document
        .assets
        .iter()
        .any(|asset| asset.id == replacement_asset_id)
    {
        return Err(CommandError::InvalidDocument(
            DocumentError::MissingImageAsset {
                layer_id,
                asset_id: replacement_asset_id,
            },
        ));
    }
    let original_asset_id = image.asset_id;
    let mapped_treatments = document
        .mappings
        .iter()
        .filter(|mapping| mapping.source_layer_id == layer_id)
        .filter_map(|mapping| mapping.treatment_id)
        .collect::<HashSet<_>>();
    let mut replacements = std::collections::HashMap::new();
    for treatment_id in mapped_treatments {
        let Some(existing) = document
            .image_treatments
            .iter()
            .find(|treatment| treatment.id == treatment_id)
        else {
            continue;
        };
        if existing.asset_id != original_asset_id {
            continue;
        }
        let replacement = ImageTreatment::new(replacement_asset_id, existing.recipe.clone());
        replacements.insert(treatment_id, replacement.id);
        document.image_treatments.push(replacement);
    }
    for mapping in document
        .mappings
        .iter_mut()
        .filter(|mapping| mapping.source_layer_id == layer_id)
    {
        if let Some(replacement) = mapping
            .treatment_id
            .and_then(|treatment_id| replacements.get(&treatment_id))
        {
            mapping.treatment_id = Some(*replacement);
        }
    }
    let ContentKind::Image(image) = &mut face_mut(document, side).layers[index].kind else {
        unreachable!("image kind was checked before replacement")
    };
    image.asset_id = replacement_asset_id;
    Ok(())
}

fn reorder_layer(
    document: &mut AtelierDocument,
    layer_id: LayerId,
    new_parent_id: Option<LayerId>,
    new_index: usize,
) -> Result<(), CommandError> {
    let (side, old_index) =
        locate_layer(document, layer_id).ok_or(CommandError::LayerNotFound(layer_id))?;
    ensure_unlocked(&face(document, side).layers[old_index])?;
    let mut moving_ids = descendant_ids(face(document, side), layer_id);
    moving_ids.insert(layer_id);
    if let Some(parent_id) = new_parent_id {
        let (parent_side, parent_index) =
            locate_layer(document, parent_id).ok_or(CommandError::LayerNotFound(parent_id))?;
        if parent_side != side {
            return Err(CommandError::LayerOnWrongFace {
                layer_id: parent_id,
                expected_side: side,
                actual_side: parent_side,
            });
        }
        if !matches!(
            face(document, side).layers[parent_index].kind,
            ContentKind::Group
        ) {
            return Err(CommandError::NotAGroup(parent_id));
        }
        if moving_ids.contains(&parent_id) {
            return Err(CommandError::InvalidDocument(DocumentError::LayerCycle(
                layer_id,
            )));
        }
    }
    let selected_face = face_mut(document, side);
    let mut moving = Vec::new();
    let mut remaining = Vec::with_capacity(selected_face.layers.len() - moving_ids.len());
    for layer in selected_face.layers.drain(..) {
        if moving_ids.contains(&layer.id) {
            moving.push(layer);
        } else {
            remaining.push(layer);
        }
    }
    if new_index > remaining.len() {
        return Err(CommandError::InvalidLayerIndex(new_index));
    }
    moving
        .iter_mut()
        .find(|layer| layer.id == layer_id)
        .expect("selected layer must be included in its moving subtree")
        .parent_id = new_parent_id;
    remaining.splice(new_index..new_index, moving);
    selected_face.layers = remaining;
    Ok(())
}

fn move_layer(
    document: &mut AtelierDocument,
    layer_id: LayerId,
    new_parent_id: Option<LayerId>,
    new_index: usize,
    from_target: ProductionTarget,
    to_target: ProductionTarget,
) -> Result<(), CommandError> {
    let (side, index) =
        locate_layer(document, layer_id).ok_or(CommandError::LayerNotFound(layer_id))?;
    let mut moving_ids = if matches!(face(document, side).layers[index].kind, ContentKind::Group) {
        descendant_ids(face(document, side), layer_id)
    } else {
        HashSet::new()
    };
    moving_ids.insert(layer_id);
    if to_target.layer != FaceProductionLayer::Copper
        && face(document, side).layers.iter().any(|layer| {
            moving_ids.contains(&layer.id) && matches!(layer.kind, ContentKind::BoardFill(_))
        })
    {
        return Err(CommandError::InvalidBoardFillTarget {
            layer_id,
            target: to_target.canonical_name(),
        });
    }

    reorder_layer(document, layer_id, new_parent_id, new_index)?;
    if from_target == to_target {
        return Ok(());
    }
    for mapping in &mut document.mappings {
        if moving_ids.contains(&mapping.source_layer_id) && mapping.target == from_target {
            mapping.target = to_target;
        }
    }
    let mut seen = HashSet::new();
    document
        .mappings
        .retain(|mapping| seen.insert((mapping.source_layer_id, mapping.target, mapping.combine)));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn transfer_layers(
    document: &mut AtelierDocument,
    layer_ids: &[LayerId],
    target: ProductionTarget,
    new_parent_id: Option<LayerId>,
    new_index: usize,
    mode: LayerTransferMode,
    duplicate_layer_ids: &[LayerId],
    duplicate_mapping_ids: &[MappingId],
    offset_um: i64,
) -> Result<(), CommandError> {
    if layer_ids.is_empty() {
        return Err(CommandError::EmptyLayerTransfer);
    }
    let mut selected = HashSet::new();
    let mut source_side = None;
    for layer_id in layer_ids {
        if !selected.insert(*layer_id) {
            return Err(CommandError::DuplicateTransferLayer(*layer_id));
        }
        let (side, _) =
            locate_layer(document, *layer_id).ok_or(CommandError::LayerNotFound(*layer_id))?;
        if source_side.is_some_and(|source| source != side) {
            return Err(CommandError::MixedFaceLayerTransfer);
        }
        source_side = Some(side);
    }
    let source_side = source_side.expect("non-empty transfer has a source face");

    let roots = layer_ids
        .iter()
        .copied()
        .filter(|layer_id| {
            let mut parent_id = layer(document, *layer_id)
                .expect("selected layer was located")
                .parent_id;
            while let Some(parent) = parent_id {
                if selected.contains(&parent) {
                    return false;
                }
                parent_id = layer(document, parent)
                    .expect("validated document has every parent")
                    .parent_id;
            }
            true
        })
        .collect::<Vec<_>>();

    let mut transferring_ids = HashSet::new();
    for root_id in &roots {
        ensure_layer_unlocked(document, *root_id)?;
        transferring_ids.insert(*root_id);
        transferring_ids.extend(descendant_ids(face(document, source_side), *root_id));
    }
    for transferring_id in &transferring_ids {
        ensure_unlocked(layer(document, *transferring_id)?)?;
    }

    if target.layer != FaceProductionLayer::Copper
        && face(document, source_side).layers.iter().any(|candidate| {
            transferring_ids.contains(&candidate.id)
                && matches!(candidate.kind, ContentKind::BoardFill(_))
        })
    {
        return Err(CommandError::InvalidBoardFillTarget {
            layer_id: roots[0],
            target: target.canonical_name(),
        });
    }
    if mode == LayerTransferMode::Copy
        && face(document, source_side).layers.iter().any(|candidate| {
            transferring_ids.contains(&candidate.id)
                && matches!(candidate.kind, ContentKind::BoardFill(_))
        })
    {
        return Err(CommandError::UnexpectedLayerKind {
            layer_id: roots[0],
            expected: "image, text, or group",
        });
    }

    if let Some(parent_id) = new_parent_id {
        let (parent_side, parent_index) =
            locate_layer(document, parent_id).ok_or(CommandError::LayerNotFound(parent_id))?;
        if parent_side != target.side {
            return Err(CommandError::LayerOnWrongFace {
                layer_id: parent_id,
                expected_side: target.side,
                actual_side: parent_side,
            });
        }
        if !matches!(
            face(document, parent_side).layers[parent_index].kind,
            ContentKind::Group
        ) {
            return Err(CommandError::NotAGroup(parent_id));
        }
        if transferring_ids.contains(&parent_id) {
            return Err(CommandError::InvalidDocument(DocumentError::LayerCycle(
                roots[0],
            )));
        }
    }

    let source_layers = face(document, source_side)
        .layers
        .iter()
        .filter(|candidate| transferring_ids.contains(&candidate.id))
        .cloned()
        .collect::<Vec<_>>();
    let source_mappings = document
        .mappings
        .iter()
        .filter(|mapping| transferring_ids.contains(&mapping.source_layer_id))
        .cloned()
        .collect::<Vec<_>>();

    match mode {
        LayerTransferMode::Copy => {
            if source_layers.len() != duplicate_layer_ids.len() {
                return Err(CommandError::TransferLayerIdCount {
                    expected: source_layers.len(),
                    actual: duplicate_layer_ids.len(),
                });
            }
            if source_mappings.len() != duplicate_mapping_ids.len() {
                return Err(CommandError::TransferMappingIdCount {
                    expected: source_mappings.len(),
                    actual: duplicate_mapping_ids.len(),
                });
            }
            if new_index > face(document, target.side).layers.len() {
                return Err(CommandError::InvalidLayerIndex(new_index));
            }

            let id_map = source_layers
                .iter()
                .map(|source| source.id)
                .zip(duplicate_layer_ids.iter().copied())
                .collect::<HashMap<_, _>>();
            let mut duplicates = source_layers;
            for duplicate in &mut duplicates {
                let source_id = duplicate.id;
                duplicate.id = id_map[&source_id];
                duplicate.parent_id = if roots.contains(&source_id) {
                    new_parent_id
                } else {
                    duplicate
                        .parent_id
                        .and_then(|parent| id_map.get(&parent).copied())
                };
                if roots.contains(&source_id) {
                    duplicate.name = format!("{} 副本", duplicate.name);
                }
                duplicate.transform.x_um += offset_um;
                duplicate.transform.y_um += offset_um;
            }
            face_mut(document, target.side)
                .layers
                .splice(new_index..new_index, duplicates);

            for (mut mapping, duplicate_mapping_id) in source_mappings
                .into_iter()
                .zip(duplicate_mapping_ids.iter().copied())
            {
                mapping.id = duplicate_mapping_id;
                mapping.source_layer_id = id_map[&mapping.source_layer_id];
                mapping.target = target;
                document.mappings.push(mapping);
            }
            let mut seen = HashSet::new();
            document.mappings.retain(|mapping| {
                seen.insert((mapping.source_layer_id, mapping.target, mapping.combine))
            });
        }
        LayerTransferMode::Move => {
            if !duplicate_layer_ids.is_empty() {
                return Err(CommandError::TransferLayerIdCount {
                    expected: 0,
                    actual: duplicate_layer_ids.len(),
                });
            }
            if !duplicate_mapping_ids.is_empty() {
                return Err(CommandError::TransferMappingIdCount {
                    expected: 0,
                    actual: duplicate_mapping_ids.len(),
                });
            }

            face_mut(document, source_side)
                .layers
                .retain(|candidate| !transferring_ids.contains(&candidate.id));
            let target_len = face(document, target.side).layers.len();
            if new_index > target_len {
                return Err(CommandError::InvalidLayerIndex(new_index));
            }
            let mut moving = source_layers;
            for candidate in &mut moving {
                if roots.contains(&candidate.id) {
                    candidate.parent_id = new_parent_id;
                }
            }
            face_mut(document, target.side)
                .layers
                .splice(new_index..new_index, moving);
            for mapping in &mut document.mappings {
                if transferring_ids.contains(&mapping.source_layer_id) {
                    mapping.target = target;
                }
            }
            let mut seen = HashSet::new();
            document.mappings.retain(|mapping| {
                seen.insert((mapping.source_layer_id, mapping.target, mapping.combine))
            });
        }
    }
    Ok(())
}

fn paste_layers(
    document: &mut AtelierDocument,
    mut layers: Vec<ContentLayer>,
    mut mappings: Vec<ProductionMapping>,
    target: ProductionTarget,
    new_parent_id: Option<LayerId>,
    new_index: usize,
) -> Result<(), CommandError> {
    if layers.is_empty() {
        return Err(CommandError::EmptyLayerTransfer);
    }
    if new_index > face(document, target.side).layers.len() {
        return Err(CommandError::InvalidLayerIndex(new_index));
    }
    let pasted_ids = layers.iter().map(|layer| layer.id).collect::<HashSet<_>>();
    if pasted_ids.len() != layers.len() {
        return Err(CommandError::DuplicateTransferLayer(layers[0].id));
    }
    if let Some(parent_id) = new_parent_id {
        let (parent_side, parent_index) =
            locate_layer(document, parent_id).ok_or(CommandError::LayerNotFound(parent_id))?;
        if parent_side != target.side {
            return Err(CommandError::LayerOnWrongFace {
                layer_id: parent_id,
                expected_side: target.side,
                actual_side: parent_side,
            });
        }
        if !matches!(
            face(document, parent_side).layers[parent_index].kind,
            ContentKind::Group
        ) {
            return Err(CommandError::NotAGroup(parent_id));
        }
    }
    for layer in &mut layers {
        if layer
            .parent_id
            .is_none_or(|parent| !pasted_ids.contains(&parent))
        {
            layer.parent_id = new_parent_id;
        }
    }
    for mapping in &mut mappings {
        if !pasted_ids.contains(&mapping.source_layer_id) {
            return Err(CommandError::LayerNotFound(mapping.source_layer_id));
        }
        mapping.target = target;
    }
    face_mut(document, target.side)
        .layers
        .splice(new_index..new_index, layers);
    document.mappings.extend(mappings);
    Ok(())
}

fn group_layers(
    document: &mut AtelierDocument,
    side: CardSide,
    group: ContentLayer,
    layer_ids: Vec<LayerId>,
) -> Result<(), CommandError> {
    if layer_ids.len() < 2 {
        return Err(if layer_ids.is_empty() {
            CommandError::EmptyGroup
        } else {
            CommandError::InsufficientGroupMembers
        });
    }
    if !matches!(group.kind, ContentKind::Group) {
        return Err(CommandError::NotAGroup(group.id));
    }
    let mut unique = HashSet::new();
    for layer_id in &layer_ids {
        if !unique.insert(*layer_id) {
            return Err(CommandError::DuplicateGroupMember(*layer_id));
        }
        let (actual_side, index) =
            locate_layer(document, *layer_id).ok_or(CommandError::LayerNotFound(*layer_id))?;
        if actual_side != side {
            return Err(CommandError::LayerOnWrongFace {
                layer_id: *layer_id,
                expected_side: side,
                actual_side,
            });
        }
        ensure_unlocked(&face(document, side).layers[index])?;
    }

    let group_transform = bounds_for_selection(document, side, &layer_ids)?;
    let mut group = group;
    group.transform = group_transform;
    let selected_face = face_mut(document, side);
    let insert_index = selected_face
        .layers
        .iter()
        .position(|layer| unique.contains(&layer.id))
        .expect("validated non-empty selection");
    let mut selected = Vec::new();
    let mut remaining = Vec::with_capacity(selected_face.layers.len() - unique.len());
    for mut layer in selected_face.layers.drain(..) {
        if unique.contains(&layer.id) {
            layer.parent_id = Some(group.id);
            selected.push(layer);
        } else {
            remaining.push(layer);
        }
    }
    remaining.insert(insert_index, group);
    remaining.splice(insert_index + 1..insert_index + 1, selected);
    selected_face.layers = remaining;
    Ok(())
}

fn bounds_for_selection(
    document: &AtelierDocument,
    side: CardSide,
    layer_ids: &[LayerId],
) -> Result<TransformUm, CommandError> {
    let selected_face = face(document, side);
    let mut transforms = Vec::new();
    for layer_id in layer_ids {
        let selected = layer(document, *layer_id)?;
        if matches!(selected.kind, ContentKind::Group)
            && (selected.transform.width_um == 0 || selected.transform.height_um == 0)
        {
            let descendants = descendant_ids(selected_face, selected.id);
            transforms.extend(
                selected_face
                    .layers
                    .iter()
                    .filter(|candidate| descendants.contains(&candidate.id))
                    .map(|candidate| candidate.transform),
            );
        } else {
            transforms.push(selected.transform);
        }
    }
    Ok(bounds_for_transforms(transforms))
}

fn transform_layer(
    document: &mut AtelierDocument,
    layer_id: LayerId,
    transform: TransformUm,
) -> Result<(), CommandError> {
    let (side, index) =
        locate_layer(document, layer_id).ok_or(CommandError::LayerNotFound(layer_id))?;
    if !matches!(face(document, side).layers[index].kind, ContentKind::Group) {
        face_mut(document, side).layers[index].transform = transform;
        return Ok(());
    }

    let group_id = face(document, side).layers[index].id;
    let descendant_ids = descendant_ids(face(document, side), group_id);
    for descendant_id in &descendant_ids {
        ensure_unlocked(layer(document, *descendant_id)?)?;
    }
    let old_group = {
        let group = &face(document, side).layers[index];
        if group.transform.width_um == 0 || group.transform.height_um == 0 {
            bounds_for_layers(
                face(document, side)
                    .layers
                    .iter()
                    .filter(|candidate| descendant_ids.contains(&candidate.id)),
            )
        } else {
            group.transform
        }
    };
    let original = face(document, side)
        .layers
        .iter()
        .filter(|candidate| descendant_ids.contains(&candidate.id))
        .map(|candidate| (candidate.id, candidate.transform))
        .collect::<Vec<_>>();

    for (descendant_id, descendant_transform) in original {
        let next = transform_relative_to_group(descendant_transform, old_group, transform);
        let descendant_index = face(document, side)
            .layers
            .iter()
            .position(|candidate| candidate.id == descendant_id)
            .expect("collected descendant must still exist");
        face_mut(document, side).layers[descendant_index].transform = next;
    }
    face_mut(document, side).layers[index].transform = transform;
    Ok(())
}

fn descendant_ids(face: &CardFace, group_id: LayerId) -> HashSet<LayerId> {
    let mut descendants = HashSet::new();
    loop {
        let before = descendants.len();
        for candidate in &face.layers {
            if candidate
                .parent_id
                .is_some_and(|parent_id| parent_id == group_id || descendants.contains(&parent_id))
            {
                descendants.insert(candidate.id);
            }
        }
        if descendants.len() == before {
            return descendants;
        }
    }
}

fn bounds_for_layers<'a>(layers: impl Iterator<Item = &'a ContentLayer>) -> TransformUm {
    bounds_for_transforms(layers.map(|layer| layer.transform))
}

fn bounds_for_transforms(transforms: impl IntoIterator<Item = TransformUm>) -> TransformUm {
    let mut min_x = i64::MAX;
    let mut min_y = i64::MAX;
    let mut max_x = i64::MIN;
    let mut max_y = i64::MIN;
    for transform in transforms {
        let (layer_min_x, layer_min_y, layer_max_x, layer_max_y) = axis_aligned_bounds(transform);
        min_x = min_x.min(layer_min_x);
        min_y = min_y.min(layer_min_y);
        max_x = max_x.max(layer_max_x);
        max_y = max_y.max(layer_max_y);
    }
    if min_x == i64::MAX {
        return TransformUm::default();
    }
    TransformUm::rect(
        min_x,
        min_y,
        u32::try_from((max_x - min_x).max(1)).unwrap_or(u32::MAX),
        u32::try_from((max_y - min_y).max(1)).unwrap_or(u32::MAX),
    )
}

fn axis_aligned_bounds(transform: TransformUm) -> (i64, i64, i64, i64) {
    let center_x = transform.x_um as f64 + f64::from(transform.width_um) / 2.0;
    let center_y = transform.y_um as f64 + f64::from(transform.height_um) / 2.0;
    let half_width = f64::from(transform.width_um) / 2.0;
    let half_height = f64::from(transform.height_um) / 2.0;
    let radians = (f64::from(transform.rotation_mdeg) / 1_000.0).to_radians();
    let (sin, cos) = radians.sin_cos();
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (x, y) in [
        (-half_width, -half_height),
        (half_width, -half_height),
        (half_width, half_height),
        (-half_width, half_height),
    ] {
        let board_x = center_x + x * cos - y * sin;
        let board_y = center_y + x * sin + y * cos;
        min_x = min_x.min(board_x);
        min_y = min_y.min(board_y);
        max_x = max_x.max(board_x);
        max_y = max_y.max(board_y);
    }
    (
        min_x.floor() as i64,
        min_y.floor() as i64,
        max_x.ceil() as i64,
        max_y.ceil() as i64,
    )
}

fn transform_relative_to_group(
    child: TransformUm,
    old_group: TransformUm,
    new_group: TransformUm,
) -> TransformUm {
    let old_width = f64::from(old_group.width_um.max(1));
    let old_height = f64::from(old_group.height_um.max(1));
    let scale_x = f64::from(new_group.width_um) / old_width;
    let scale_y = f64::from(new_group.height_um) / old_height;
    let old_center = (
        old_group.x_um as f64 + old_width / 2.0,
        old_group.y_um as f64 + old_height / 2.0,
    );
    let new_center = (
        new_group.x_um as f64 + f64::from(new_group.width_um) / 2.0,
        new_group.y_um as f64 + f64::from(new_group.height_um) / 2.0,
    );
    let child_center = (
        child.x_um as f64 + f64::from(child.width_um) / 2.0,
        child.y_um as f64 + f64::from(child.height_um) / 2.0,
    );
    let old_radians = -(f64::from(old_group.rotation_mdeg) / 1_000.0).to_radians();
    let (old_sin, old_cos) = old_radians.sin_cos();
    let dx = child_center.0 - old_center.0;
    let dy = child_center.1 - old_center.1;
    let mut local_x = dx * old_cos - dy * old_sin;
    let mut local_y = dx * old_sin + dy * old_cos;
    if old_group.flip_x {
        local_x = -local_x;
    }
    if old_group.flip_y {
        local_y = -local_y;
    }
    local_x *= scale_x;
    local_y *= scale_y;
    if new_group.flip_x {
        local_x = -local_x;
    }
    if new_group.flip_y {
        local_y = -local_y;
    }
    let new_radians = (f64::from(new_group.rotation_mdeg) / 1_000.0).to_radians();
    let (new_sin, new_cos) = new_radians.sin_cos();
    let next_center_x = new_center.0 + local_x * new_cos - local_y * new_sin;
    let next_center_y = new_center.1 + local_x * new_sin + local_y * new_cos;
    let next_width = (f64::from(child.width_um) * scale_x.abs()).round().max(1.0) as u32;
    let next_height = (f64::from(child.height_um) * scale_y.abs())
        .round()
        .max(1.0) as u32;
    TransformUm {
        x_um: (next_center_x - f64::from(next_width) / 2.0).round() as i64,
        y_um: (next_center_y - f64::from(next_height) / 2.0).round() as i64,
        width_um: next_width,
        height_um: next_height,
        rotation_mdeg: child.rotation_mdeg + new_group.rotation_mdeg - old_group.rotation_mdeg,
        flip_x: child.flip_x ^ old_group.flip_x ^ new_group.flip_x,
        flip_y: child.flip_y ^ old_group.flip_y ^ new_group.flip_y,
    }
}

fn ungroup_layer(document: &mut AtelierDocument, group_id: LayerId) -> Result<(), CommandError> {
    let (side, group_index) =
        locate_layer(document, group_id).ok_or(CommandError::LayerNotFound(group_id))?;
    let group = &face(document, side).layers[group_index];
    ensure_unlocked(group)?;
    if !matches!(group.kind, ContentKind::Group) {
        return Err(CommandError::NotAGroup(group_id));
    }

    let selected_face = face_mut(document, side);
    selected_face.layers.remove(group_index);
    let mut children = Vec::new();
    let mut remaining = Vec::with_capacity(selected_face.layers.len());
    for mut layer in selected_face.layers.drain(..) {
        if layer.parent_id == Some(group_id) {
            if layer.locked {
                return Err(CommandError::LayerLocked(layer.id));
            }
            layer.parent_id = None;
            children.push(layer);
        } else {
            remaining.push(layer);
        }
    }
    remaining.splice(group_index..group_index, children);
    selected_face.layers = remaining;
    Ok(())
}
