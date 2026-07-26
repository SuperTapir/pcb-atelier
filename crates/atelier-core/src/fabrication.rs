use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    AssetId, AtelierDocument, BoardOutline, CardSide, CombineMode, ContentKind, ContentLayer,
    CropRect, DocumentError, FaceProductionLayer, LayerId, MappingId, MechanicalFeature,
    ProductionTarget, StackupPreset, TextContent, TransformUm,
};

pub const DEFAULT_PRODUCTION_PIXEL_PITCH_UM: u32 = 25;
pub const FABRICATION_COMPILER_VERSION: &str = "atelier-fabrication-v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricationPlan {
    pub outline: BoardOutline,
    pub stackup: StackupPreset,
    pub layers: Vec<FabricationLayer>,
    pub mechanical_features: Vec<MechanicalFeature>,
}

impl FabricationPlan {
    pub fn layer(&self, target: ProductionTarget) -> Option<&FabricationLayer> {
        self.layers.iter().find(|layer| layer.target == target)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricationLayer {
    pub target: ProductionTarget,
    pub polarity: LayerPolarity,
    pub operations: Vec<FabricationOperation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayerPolarity {
    Positive,
    Opening,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricationOperation {
    pub mapping_id: MappingId,
    pub source_layer_id: LayerId,
    pub source_name: String,
    pub combine: CombineMode,
    pub transform: TransformUm,
    pub primitive: FabricationPrimitive,
    pub clip_to_board: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FabricationPrimitive {
    Image {
        asset_id: AssetId,
        crop: Option<CropRect>,
    },
    Text(TextContent),
    BoardFill {
        outline: BoardOutline,
        edge_clearance_um: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RasterGrid {
    pub origin_x_um: i64,
    pub origin_y_um: i64,
    pub width_um: u32,
    pub height_um: u32,
    pub pixel_pitch_um: u32,
    pub width_px: u32,
    pub height_px: u32,
}

impl RasterGrid {
    pub fn for_board(
        board: &BoardOutline,
        pixel_pitch_um: u32,
    ) -> Result<Self, FabricationResolveError> {
        if pixel_pitch_um == 0 {
            return Err(FabricationResolveError::InvalidPixelPitch);
        }
        let width_px = board.width_um().div_ceil(pixel_pitch_um);
        let height_px = board.height_um().div_ceil(pixel_pitch_um);
        checked_mask_byte_len(width_px, height_px)?;
        Ok(Self {
            origin_x_um: 0,
            origin_y_um: 0,
            width_um: board.width_um(),
            height_um: board.height_um(),
            pixel_pitch_um,
            width_px,
            height_px,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitMask {
    width_px: u32,
    height_px: u32,
    bytes: Vec<u8>,
}

impl BitMask {
    pub fn new(width_px: u32, height_px: u32) -> Result<Self, FabricationResolveError> {
        let byte_len = checked_mask_byte_len(width_px, height_px)?;
        Ok(Self {
            width_px,
            height_px,
            bytes: vec![0; byte_len],
        })
    }

    pub const fn width_px(&self) -> u32 {
        self.width_px
    }

    pub const fn height_px(&self) -> u32 {
        self.height_px
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn get(&self, x: u32, y: u32) -> Result<bool, FabricationResolveError> {
        let bit_index = self.bit_index(x, y)?;
        Ok(self.bytes[bit_index / 8] & (1 << (bit_index % 8)) != 0)
    }

    pub fn set(&mut self, x: u32, y: u32, active: bool) -> Result<(), FabricationResolveError> {
        let bit_index = self.bit_index(x, y)?;
        let byte = &mut self.bytes[bit_index / 8];
        let bit = 1 << (bit_index % 8);
        if active {
            *byte |= bit;
        } else {
            *byte &= !bit;
        }
        Ok(())
    }

    pub fn combine(
        &mut self,
        operation: &Self,
        mode: CombineMode,
    ) -> Result<(), FabricationResolveError> {
        if self.width_px != operation.width_px || self.height_px != operation.height_px {
            return Err(FabricationResolveError::MaskDimensionsMismatch);
        }
        for (destination, source) in self.bytes.iter_mut().zip(&operation.bytes) {
            match mode {
                CombineMode::Add => *destination |= source,
                CombineMode::Subtract => *destination &= !source,
            }
        }
        self.clear_unused_tail_bits();
        Ok(())
    }

    pub fn active_pixel_count(&self) -> u64 {
        self.bytes
            .iter()
            .map(|byte| u64::from(byte.count_ones()))
            .sum()
    }

    pub fn sha256(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(self.width_px.to_le_bytes());
        digest.update(self.height_px.to_le_bytes());
        digest.update(&self.bytes);
        format!("{:x}", digest.finalize())
    }

    fn bit_index(&self, x: u32, y: u32) -> Result<usize, FabricationResolveError> {
        if x >= self.width_px || y >= self.height_px {
            return Err(FabricationResolveError::PixelOutOfBounds {
                x,
                y,
                width: self.width_px,
                height: self.height_px,
            });
        }
        let index = u64::from(y) * u64::from(self.width_px) + u64::from(x);
        usize::try_from(index).map_err(|_| FabricationResolveError::MaskTooLarge)
    }

    fn clear_unused_tail_bits(&mut self) {
        let used_bits = u64::from(self.width_px) * u64::from(self.height_px);
        let remainder = (used_bits % 8) as u8;
        if remainder != 0
            && let Some(last) = self.bytes.last_mut()
        {
            *last &= (1_u8 << remainder) - 1;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedOperationMask {
    pub mapping_id: MappingId,
    pub source_layer_id: LayerId,
    pub combine: CombineMode,
    pub mask_sha256: String,
    pub mask: BitMask,
}

impl ResolvedOperationMask {
    pub fn new(
        mapping_id: MappingId,
        source_layer_id: LayerId,
        combine: CombineMode,
        mask: BitMask,
    ) -> Self {
        let mask_sha256 = mask.sha256();
        Self {
            mapping_id,
            source_layer_id,
            combine,
            mask_sha256,
            mask,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedFabricationLayer {
    pub target: ProductionTarget,
    pub polarity: LayerPolarity,
    pub composite: BitMask,
    pub composite_sha256: String,
    pub operations: Vec<ResolvedOperationMask>,
}

impl ResolvedFabricationLayer {
    pub fn empty(
        target: ProductionTarget,
        polarity: LayerPolarity,
        width_px: u32,
        height_px: u32,
    ) -> Result<Self, FabricationResolveError> {
        let composite = BitMask::new(width_px, height_px)?;
        let composite_sha256 = composite.sha256();
        Ok(Self {
            target,
            polarity,
            composite,
            composite_sha256,
            operations: Vec::new(),
        })
    }

    pub fn rebuild_composite(&mut self) -> Result<(), FabricationResolveError> {
        let mut composite = BitMask::new(self.composite.width_px, self.composite.height_px)?;
        for operation in &self.operations {
            if operation.mask_sha256 != operation.mask.sha256() {
                return Err(FabricationResolveError::OperationMaskHashMismatch(
                    operation.mapping_id,
                ));
            }
            composite.combine(&operation.mask, operation.combine)?;
        }
        self.composite_sha256 = composite.sha256();
        self.composite = composite;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricationBuildManifest {
    pub compiler_version: String,
    pub input_sha256: String,
    pub output_sha256: String,
    pub font_fingerprint: String,
    pub pixel_pitch_um: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedFabricationBoard {
    pub outline: BoardOutline,
    pub stackup: StackupPreset,
    pub grid: RasterGrid,
    pub layers: Vec<ResolvedFabricationLayer>,
    pub mechanical_features: Vec<MechanicalFeature>,
    pub build: FabricationBuildManifest,
}

pub trait FabricationRasterizer {
    fn fingerprint(&self) -> String;

    fn rasterize(
        &mut self,
        operation: &FabricationOperation,
        grid: &RasterGrid,
    ) -> Result<BitMask, String>;
}

pub fn compile_fabrication_plan(
    document: &AtelierDocument,
) -> Result<FabricationPlan, FabricationError> {
    document.validate()?;

    let layer_index = document
        .front
        .layers
        .iter()
        .chain(&document.back.layers)
        .map(|layer| (layer.id, layer))
        .collect::<HashMap<_, _>>();
    let mut layers = canonical_layers();

    for mapping in &document.mappings {
        let source = layer_index
            .get(&mapping.source_layer_id)
            .copied()
            .expect("validated mapping source must exist");
        if !participates_in_export(source, &layer_index) {
            continue;
        }

        let primitive = match &source.kind {
            ContentKind::Image(image) => FabricationPrimitive::Image {
                asset_id: image.asset_id,
                crop: image.crop.clone(),
            },
            ContentKind::Text(text) => FabricationPrimitive::Text(text.clone()),
            ContentKind::BoardFill(fill) => FabricationPrimitive::BoardFill {
                outline: document.board.clone(),
                edge_clearance_um: fill.edge_clearance_um,
            },
            ContentKind::Group => {
                return Err(FabricationError::GroupCannotBeMapped(source.id));
            }
        };
        let destination = layers
            .iter_mut()
            .find(|layer| layer.target == mapping.target)
            .expect("all canonical production layers exist");
        destination.operations.push(FabricationOperation {
            mapping_id: mapping.id,
            source_layer_id: source.id,
            source_name: source.name.clone(),
            combine: mapping.combine,
            transform: source.transform,
            primitive,
            clip_to_board: true,
        });
    }

    Ok(FabricationPlan {
        outline: document.board.clone(),
        stackup: document.stackup.clone(),
        layers,
        mechanical_features: document.mechanical_features.clone(),
    })
}

pub fn resolve_fabrication_plan(
    plan: &FabricationPlan,
    pixel_pitch_um: u32,
    rasterizer: &mut impl FabricationRasterizer,
) -> Result<ResolvedFabricationBoard, FabricationResolveError> {
    let grid = RasterGrid::for_board(&plan.outline, pixel_pitch_um)?;
    let font_fingerprint = rasterizer.fingerprint();
    let input_sha256 = plan_input_hash(plan, &grid, &font_fingerprint)?;
    let mut layers = Vec::with_capacity(plan.layers.len());

    for planned_layer in &plan.layers {
        let mut resolved = ResolvedFabricationLayer::empty(
            planned_layer.target,
            planned_layer.polarity,
            grid.width_px,
            grid.height_px,
        )?;
        for operation in &planned_layer.operations {
            let mask = rasterizer.rasterize(operation, &grid).map_err(|message| {
                FabricationResolveError::Rasterizer {
                    mapping_id: operation.mapping_id,
                    message,
                }
            })?;
            if mask.width_px() != grid.width_px || mask.height_px() != grid.height_px {
                return Err(FabricationResolveError::MaskDimensionsMismatch);
            }
            resolved.operations.push(ResolvedOperationMask::new(
                operation.mapping_id,
                operation.source_layer_id,
                operation.combine,
                mask,
            ));
        }
        resolved.rebuild_composite()?;
        layers.push(resolved);
    }

    let output_sha256 = resolved_output_hash(&grid, &layers);
    Ok(ResolvedFabricationBoard {
        outline: plan.outline.clone(),
        stackup: plan.stackup.clone(),
        grid,
        layers,
        mechanical_features: plan.mechanical_features.clone(),
        build: FabricationBuildManifest {
            compiler_version: FABRICATION_COMPILER_VERSION.to_owned(),
            input_sha256,
            output_sha256,
            font_fingerprint,
            pixel_pitch_um,
        },
    })
}

fn participates_in_export(
    layer: &ContentLayer,
    layer_index: &HashMap<LayerId, &ContentLayer>,
) -> bool {
    if !layer.export_enabled {
        return false;
    }
    let mut parent_id = layer.parent_id;
    while let Some(id) = parent_id {
        let Some(parent) = layer_index.get(&id) else {
            return false;
        };
        if !parent.export_enabled {
            return false;
        }
        parent_id = parent.parent_id;
    }
    true
}

fn canonical_layers() -> Vec<FabricationLayer> {
    [CardSide::Front, CardSide::Back]
        .into_iter()
        .flat_map(|side| {
            [
                FaceProductionLayer::Copper,
                FaceProductionLayer::SolderMaskOpen,
                FaceProductionLayer::Silkscreen,
            ]
            .into_iter()
            .map(move |layer| {
                let polarity = match layer {
                    FaceProductionLayer::SolderMaskOpen => LayerPolarity::Opening,
                    FaceProductionLayer::Copper | FaceProductionLayer::Silkscreen => {
                        LayerPolarity::Positive
                    }
                };
                FabricationLayer {
                    target: ProductionTarget::new(side, layer),
                    polarity,
                    operations: Vec::new(),
                }
            })
        })
        .collect()
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FabricationError {
    #[error(transparent)]
    InvalidDocument(#[from] DocumentError),
    #[error("group content layer cannot be mapped directly to a production layer: {0}")]
    GroupCannotBeMapped(LayerId),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FabricationResolveError {
    #[error("production pixel pitch must be positive")]
    InvalidPixelPitch,
    #[error("production bit mask is too large")]
    MaskTooLarge,
    #[error("bit mask dimensions do not match")]
    MaskDimensionsMismatch,
    #[error("bit mask pixel ({x}, {y}) is outside {width} × {height}")]
    PixelOutOfBounds {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    #[error("resolved operation mask hash does not match mapping {0}")]
    OperationMaskHashMismatch(MappingId),
    #[error("failed to rasterize production mapping {mapping_id}: {message}")]
    Rasterizer {
        mapping_id: MappingId,
        message: String,
    },
    #[error("failed to serialize fabrication plan for hashing: {0}")]
    SerializePlan(String),
}

fn checked_mask_byte_len(width_px: u32, height_px: u32) -> Result<usize, FabricationResolveError> {
    let bits = u64::from(width_px)
        .checked_mul(u64::from(height_px))
        .ok_or(FabricationResolveError::MaskTooLarge)?;
    let bytes = bits.div_ceil(8);
    usize::try_from(bytes).map_err(|_| FabricationResolveError::MaskTooLarge)
}

fn plan_input_hash(
    plan: &FabricationPlan,
    grid: &RasterGrid,
    rasterizer_fingerprint: &str,
) -> Result<String, FabricationResolveError> {
    let plan_bytes = serde_json::to_vec(plan)
        .map_err(|error| FabricationResolveError::SerializePlan(error.to_string()))?;
    let mut digest = Sha256::new();
    digest.update(FABRICATION_COMPILER_VERSION.as_bytes());
    digest.update(grid.pixel_pitch_um.to_le_bytes());
    digest.update(rasterizer_fingerprint.as_bytes());
    digest.update(plan_bytes);
    Ok(format!("{:x}", digest.finalize()))
}

fn resolved_output_hash(grid: &RasterGrid, layers: &[ResolvedFabricationLayer]) -> String {
    let mut digest = Sha256::new();
    digest.update(grid.width_um.to_le_bytes());
    digest.update(grid.height_um.to_le_bytes());
    digest.update(grid.pixel_pitch_um.to_le_bytes());
    for layer in layers {
        digest.update(layer.target.canonical_name().as_bytes());
        digest.update(layer.composite_sha256.as_bytes());
    }
    format!("{:x}", digest.finalize())
}
