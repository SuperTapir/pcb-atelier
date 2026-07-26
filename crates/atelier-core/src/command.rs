use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    AtelierDocument, BoardFillContent, BoardOutline, CardFace, CardSide, ContentKind, ContentLayer,
    DocumentDiagnostic, DocumentError, FaceProductionLayer, ImageContent, LayerId, MappingId,
    ProductionMapping, StackupPreset, TextContent, TransformUm,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase")]
pub enum DocumentCommand {
    InsertLayer {
        side: CardSide,
        layer: ContentLayer,
        index: usize,
    },
    DeleteLayer {
        layer_id: LayerId,
    },
    TransformLayer {
        layer_id: LayerId,
        transform: TransformUm,
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
    ReorderLayer {
        layer_id: LayerId,
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
            Self::TransformLayer {
                layer_id,
                transform,
            } => {
                ensure_layer_unlocked(document, layer_id)?;
                transform_layer(document, layer_id, transform)?;
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
                document.stackup = stackup;
            }
            Self::ReorderLayer {
                layer_id,
                new_parent_id,
                new_index,
            } => reorder_layer(document, layer_id, new_parent_id, new_index)?,
            Self::GroupLayers {
                side,
                group,
                layer_ids,
            } => group_layers(document, side, group, layer_ids)?,
            Self::UngroupLayer { group_id } => ungroup_layer(document, group_id)?,
            Self::MapLayer { mapping } => {
                let (source_side, source_index) =
                    locate_layer(document, mapping.source_layer_id)
                        .ok_or(CommandError::LayerNotFound(mapping.source_layer_id))?;
                let source = &face(document, source_side).layers[source_index];
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
    #[error("content layer is locked: {0}")]
    LayerLocked(LayerId),
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

fn reorder_layer(
    document: &mut AtelierDocument,
    layer_id: LayerId,
    new_parent_id: Option<LayerId>,
    new_index: usize,
) -> Result<(), CommandError> {
    let (side, old_index) =
        locate_layer(document, layer_id).ok_or(CommandError::LayerNotFound(layer_id))?;
    ensure_unlocked(&face(document, side).layers[old_index])?;
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
    }
    let selected_face = face_mut(document, side);
    let mut selected = selected_face.layers.remove(old_index);
    selected.parent_id = new_parent_id;
    if new_index > selected_face.layers.len() {
        return Err(CommandError::InvalidLayerIndex(new_index));
    }
    selected_face.layers.insert(new_index, selected);
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

    let group_transform = bounds_for_layers(
        layer_ids
            .iter()
            .map(|layer_id| layer(document, *layer_id))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter(),
    );
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
    let mut min_x = i64::MAX;
    let mut min_y = i64::MAX;
    let mut max_x = i64::MIN;
    let mut max_y = i64::MIN;
    for layer in layers {
        let (layer_min_x, layer_min_y, layer_max_x, layer_max_y) =
            axis_aligned_bounds(layer.transform);
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
