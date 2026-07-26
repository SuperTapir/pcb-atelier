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
                let layer = layer_mut(document, layer_id)?;
                layer.transform = transform;
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
    if layer_ids.is_empty() {
        return Err(CommandError::EmptyGroup);
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
