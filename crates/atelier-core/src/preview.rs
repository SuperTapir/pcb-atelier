//! Lightweight 2D fabrication preview composition.
//!
//! The compositor deliberately consumes only the six final production bit
//! masks on [`ResolvedFabricationBoard`]. It never resolves assets, reads
//! source images, inspects provenance operations, or rasterizes geometry.
//! Back-face pixels remain in physical board coordinates; a renderer may
//! mirror them when presenting the board as viewed from the back.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    BitMask, CardSide, FaceProductionLayer, ProductionTarget, ResolvedFabricationBoard,
    SolderMaskColor, StackupPreset, SubstrateMaterial, SurfaceFinish,
};

const SOLDER_MASK_ALPHA: u8 = 216;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPalette {
    pub substrate: Rgba8,
    pub exposed_copper: Rgba8,
    pub solder_mask: Rgba8,
    pub silkscreen: Rgba8,
}

impl PreviewPalette {
    pub fn from_stackup(stackup: &StackupPreset) -> Self {
        let substrate = match stackup.substrate {
            SubstrateMaterial::Fr4 => Rgba8::opaque(176, 132, 79),
        };
        let exposed_copper = match stackup.surface_finish {
            SurfaceFinish::Enig => Rgba8::opaque(211, 166, 57),
            SurfaceFinish::HaslLeadFree => Rgba8::opaque(195, 199, 201),
        };
        let solder_mask = match stackup.solder_mask_color {
            SolderMaskColor::Black => Rgba8::opaque(27, 29, 28),
            SolderMaskColor::White => Rgba8::opaque(226, 228, 222),
            SolderMaskColor::Green => Rgba8::opaque(20, 105, 65),
            SolderMaskColor::Red => Rgba8::opaque(153, 36, 33),
            SolderMaskColor::Blue => Rgba8::opaque(35, 67, 135),
            SolderMaskColor::Purple => Rgba8::opaque(91, 47, 112),
            SolderMaskColor::Yellow => Rgba8::opaque(215, 171, 37),
        };
        Self {
            substrate,
            exposed_copper,
            solder_mask,
            silkscreen: Rgba8::opaque(248, 246, 224),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTexture {
    pub side: CardSide,
    pub width_px: u32,
    pub height_px: u32,
    /// Row-major, tightly packed RGBA8 pixels.
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionLayerPreviewTexture {
    pub side: CardSide,
    pub layer: FaceProductionLayer,
    pub width_px: u32,
    pub height_px: u32,
    /// Row-major RGBA8 inspection pixels. A transparent pixel means the
    /// corresponding final production mask bit is clear.
    pub rgba: Vec<u8>,
}

impl ProductionLayerPreviewTexture {
    pub fn pixel(&self, x: u32, y: u32) -> Result<Rgba8, PreviewComposeError> {
        pixel_from_rgba(&self.rgba, self.width_px, self.height_px, x, y)
    }
}

impl PreviewTexture {
    pub fn pixel(&self, x: u32, y: u32) -> Result<Rgba8, PreviewComposeError> {
        pixel_from_rgba(&self.rgba, self.width_px, self.height_px, x, y)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedPreviewTextures {
    pub palette: PreviewPalette,
    pub front: PreviewTexture,
    pub back: PreviewTexture,
}

impl ResolvedFabricationBoard {
    pub fn preview_textures(&self) -> Result<ResolvedPreviewTextures, PreviewComposeError> {
        compose_resolved_preview(self)
    }

    pub fn production_layer_textures(
        &self,
    ) -> Result<Vec<ProductionLayerPreviewTexture>, PreviewComposeError> {
        compose_production_layer_textures(self)
    }
}

pub fn compose_production_layer_textures(
    board: &ResolvedFabricationBoard,
) -> Result<Vec<ProductionLayerPreviewTexture>, PreviewComposeError> {
    let palette = PreviewPalette::from_stackup(&board.stackup);
    let mut textures = Vec::with_capacity(6);
    for side in [CardSide::Front, CardSide::Back] {
        for layer in [
            FaceProductionLayer::Copper,
            FaceProductionLayer::SolderMaskOpen,
            FaceProductionLayer::Silkscreen,
        ] {
            let target = ProductionTarget::new(side, layer);
            let mask = final_mask(board, target)?;
            validate_mask_dimensions(board, target, mask)?;
            let color = match layer {
                FaceProductionLayer::Copper => palette.exposed_copper,
                FaceProductionLayer::SolderMaskOpen => palette.solder_mask,
                FaceProductionLayer::Silkscreen => palette.silkscreen,
            };
            let mut rgba =
                Vec::with_capacity(texture_byte_len(board.grid.width_px, board.grid.height_px)?);
            let pixel_count = u64::from(board.grid.width_px) * u64::from(board.grid.height_px);
            for pixel_index in 0..pixel_count {
                if mask_bit(mask, pixel_index) {
                    rgba.extend_from_slice(&color.to_array());
                } else {
                    rgba.extend_from_slice(&[0, 0, 0, 0]);
                }
            }
            textures.push(ProductionLayerPreviewTexture {
                side,
                layer,
                width_px: board.grid.width_px,
                height_px: board.grid.height_px,
                rgba,
            });
        }
    }
    Ok(textures)
}

pub fn compose_resolved_preview(
    board: &ResolvedFabricationBoard,
) -> Result<ResolvedPreviewTextures, PreviewComposeError> {
    let palette = PreviewPalette::from_stackup(&board.stackup);
    let front = compose_face(board, CardSide::Front, palette)?;
    let back = compose_face(board, CardSide::Back, palette)?;
    Ok(ResolvedPreviewTextures {
        palette,
        front,
        back,
    })
}

fn compose_face(
    board: &ResolvedFabricationBoard,
    side: CardSide,
    palette: PreviewPalette,
) -> Result<PreviewTexture, PreviewComposeError> {
    let copper = final_mask(
        board,
        ProductionTarget::new(side, FaceProductionLayer::Copper),
    )?;
    let solder_mask_open = final_mask(
        board,
        ProductionTarget::new(side, FaceProductionLayer::SolderMaskOpen),
    )?;
    let silkscreen = final_mask(
        board,
        ProductionTarget::new(side, FaceProductionLayer::Silkscreen),
    )?;

    for (target, mask) in [
        (
            ProductionTarget::new(side, FaceProductionLayer::Copper),
            copper,
        ),
        (
            ProductionTarget::new(side, FaceProductionLayer::SolderMaskOpen),
            solder_mask_open,
        ),
        (
            ProductionTarget::new(side, FaceProductionLayer::Silkscreen),
            silkscreen,
        ),
    ] {
        validate_mask_dimensions(board, target, mask)?;
    }

    let pixel_count = u64::from(board.grid.width_px) * u64::from(board.grid.height_px);
    let byte_len = texture_byte_len(board.grid.width_px, board.grid.height_px)?;
    let mut rgba = Vec::with_capacity(byte_len);

    for pixel_index in 0..pixel_count {
        let has_copper = mask_bit(copper, pixel_index);
        let is_open = mask_bit(solder_mask_open, pixel_index);
        let has_silkscreen = mask_bit(silkscreen, pixel_index);

        let underlying = if has_copper {
            palette.exposed_copper
        } else {
            palette.substrate
        };
        let coated = if is_open {
            underlying
        } else {
            blend(underlying, palette.solder_mask, SOLDER_MASK_ALPHA)
        };
        let pixel = if has_silkscreen {
            palette.silkscreen
        } else {
            coated
        };
        rgba.extend_from_slice(&pixel.to_array());
    }

    Ok(PreviewTexture {
        side,
        width_px: board.grid.width_px,
        height_px: board.grid.height_px,
        rgba,
    })
}

fn final_mask(
    board: &ResolvedFabricationBoard,
    target: ProductionTarget,
) -> Result<&BitMask, PreviewComposeError> {
    let mut layers = board.layers.iter().filter(|layer| layer.target == target);
    let layer = layers
        .next()
        .ok_or(PreviewComposeError::MissingLayer(target))?;
    if layers.next().is_some() {
        return Err(PreviewComposeError::DuplicateLayer(target));
    }
    Ok(&layer.composite)
}

fn mask_bit(mask: &BitMask, pixel_index: u64) -> bool {
    let byte_index = (pixel_index / 8) as usize;
    let bit_index = (pixel_index % 8) as u8;
    mask.bytes()[byte_index] & (1 << bit_index) != 0
}

fn validate_mask_dimensions(
    board: &ResolvedFabricationBoard,
    target: ProductionTarget,
    mask: &BitMask,
) -> Result<(), PreviewComposeError> {
    if mask.width_px() != board.grid.width_px || mask.height_px() != board.grid.height_px {
        return Err(PreviewComposeError::MaskDimensionsMismatch {
            target,
            expected_width: board.grid.width_px,
            expected_height: board.grid.height_px,
            actual_width: mask.width_px(),
            actual_height: mask.height_px(),
        });
    }
    Ok(())
}

fn texture_byte_len(width_px: u32, height_px: u32) -> Result<usize, PreviewComposeError> {
    let pixel_count = u64::from(width_px)
        .checked_mul(u64::from(height_px))
        .ok_or(PreviewComposeError::TextureTooLarge)?;
    usize::try_from(
        pixel_count
            .checked_mul(4)
            .ok_or(PreviewComposeError::TextureTooLarge)?,
    )
    .map_err(|_| PreviewComposeError::TextureTooLarge)
}

fn pixel_from_rgba(
    rgba: &[u8],
    width_px: u32,
    height_px: u32,
    x: u32,
    y: u32,
) -> Result<Rgba8, PreviewComposeError> {
    if x >= width_px || y >= height_px {
        return Err(PreviewComposeError::PixelOutOfBounds {
            x,
            y,
            width: width_px,
            height: height_px,
        });
    }
    let pixel_index = u64::from(y) * u64::from(width_px) + u64::from(x);
    let byte_index = usize::try_from(pixel_index)
        .ok()
        .and_then(|index| index.checked_mul(4))
        .ok_or(PreviewComposeError::TextureTooLarge)?;
    Ok(Rgba8 {
        r: rgba[byte_index],
        g: rgba[byte_index + 1],
        b: rgba[byte_index + 2],
        a: rgba[byte_index + 3],
    })
}

fn blend(under: Rgba8, over: Rgba8, alpha: u8) -> Rgba8 {
    let alpha = u16::from(alpha);
    let inverse = 255 - alpha;
    let channel = |under: u8, over: u8| {
        ((u16::from(under) * inverse + u16::from(over) * alpha + 127) / 255) as u8
    };
    Rgba8::opaque(
        channel(under.r, over.r),
        channel(under.g, over.g),
        channel(under.b, over.b),
    )
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PreviewComposeError {
    #[error("resolved fabrication board is missing production layer {0:?}")]
    MissingLayer(ProductionTarget),
    #[error("resolved fabrication board contains duplicate production layer {0:?}")]
    DuplicateLayer(ProductionTarget),
    #[error(
        "production mask {target:?} has dimensions {actual_width} × {actual_height}; expected {expected_width} × {expected_height}"
    )]
    MaskDimensionsMismatch {
        target: ProductionTarget,
        expected_width: u32,
        expected_height: u32,
        actual_width: u32,
        actual_height: u32,
    },
    #[error("preview texture is too large")]
    TextureTooLarge,
    #[error("preview pixel ({x}, {y}) is outside {width} × {height}")]
    PixelOutOfBounds {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
}
