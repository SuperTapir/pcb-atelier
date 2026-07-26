use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const PROJECT_FORMAT: &str = "pcb-atelier";
pub const MIN_SUPPORTED_PROJECT_SCHEMA_VERSION: u32 = 1;
pub const PROJECT_SCHEMA_VERSION: u32 = 2;
const BOARD_FILL_SCHEMA_VERSION: u32 = 2;

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

uuid_id!(DocumentId);
uuid_id!(LayerId);
uuid_id!(MappingId);
uuid_id!(AssetId);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtelierDocument {
    pub format: String,
    pub schema_version: u32,
    pub id: DocumentId,
    pub title: String,
    pub board: BoardOutline,
    pub stackup: StackupPreset,
    pub front: CardFace,
    pub back: CardFace,
    pub assets: Vec<AssetReference>,
    pub mappings: Vec<ProductionMapping>,
    pub mechanical_features: Vec<MechanicalFeature>,
}

impl AtelierDocument {
    pub fn new_card(title: impl Into<String>, width_um: u32, height_um: u32) -> Self {
        Self {
            format: PROJECT_FORMAT.to_owned(),
            schema_version: PROJECT_SCHEMA_VERSION,
            id: DocumentId::new(),
            title: title.into(),
            board: BoardOutline::RoundedRectangle {
                width_um,
                height_um,
                corner_radius_um: 2_000.min(width_um.min(height_um) / 2),
            },
            stackup: StackupPreset::default(),
            front: CardFace::new(CardSide::Front),
            back: CardFace::new(CardSide::Back),
            assets: Vec::new(),
            mappings: Vec::new(),
            mechanical_features: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.format != PROJECT_FORMAT {
            return Err(DocumentError::InvalidFormat(self.format.clone()));
        }
        if !(MIN_SUPPORTED_PROJECT_SCHEMA_VERSION..=PROJECT_SCHEMA_VERSION)
            .contains(&self.schema_version)
        {
            return Err(DocumentError::UnsupportedSchema(self.schema_version));
        }
        self.board.validate()?;
        self.stackup.validate()?;
        if self.front.side != CardSide::Front || self.back.side != CardSide::Back {
            return Err(DocumentError::InvalidFaceAssignment);
        }

        let mut layers = HashMap::new();
        validate_face(&self.front, &mut layers)?;
        validate_face(&self.back, &mut layers)?;
        validate_board_fills(self)?;
        validate_assets(&self.assets)?;
        validate_image_asset_references(&self.front, &self.back, &self.assets)?;
        validate_mappings(&self.mappings, &layers, &self.front, &self.back)?;
        validate_mechanical_features(&self.mechanical_features, &self.board)?;
        Ok(())
    }

    /// Reports content that no longer fits the rectangular extent of the
    /// physical board without rewriting any source transform.
    ///
    /// Rounded-corner clipping remains a fabrication concern; this diagnostic
    /// intentionally answers the editor question "is any physical object
    /// outside the board width/height after a board-body change?".
    pub fn content_bounds_diagnostics(&self) -> Vec<DocumentDiagnostic> {
        let board_width_um = i64::from(self.board.width_um());
        let board_height_um = i64::from(self.board.height_um());
        [(&self.front, CardSide::Front), (&self.back, CardSide::Back)]
            .into_iter()
            .flat_map(|(face, side)| {
                face.layers.iter().filter_map(move |layer| {
                    if matches!(layer.kind, ContentKind::Group | ContentKind::BoardFill(_)) {
                        return None;
                    }
                    let bounds = layer.transform.axis_aligned_bounds();
                    (bounds.min_x_um < 0
                        || bounds.min_y_um < 0
                        || bounds.max_x_um > board_width_um
                        || bounds.max_y_um > board_height_um)
                        .then_some(DocumentDiagnostic::ContentOutsideBoard {
                            side,
                            layer_id: layer.id,
                            bounds,
                            board_width_um: self.board.width_um(),
                            board_height_um: self.board.height_um(),
                        })
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardFace {
    pub side: CardSide,
    pub layers: Vec<ContentLayer>,
}

impl CardFace {
    pub fn new(side: CardSide) -> Self {
        Self {
            side,
            layers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CardSide {
    Front,
    Back,
}

impl std::fmt::Display for CardSide {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Front => formatter.write_str("front"),
            Self::Back => formatter.write_str("back"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentLayer {
    pub id: LayerId,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub export_enabled: bool,
    pub parent_id: Option<LayerId>,
    pub transform: TransformUm,
    pub kind: ContentKind,
}

impl ContentLayer {
    pub fn new_text(
        name: impl Into<String>,
        text: impl Into<String>,
        transform: TransformUm,
    ) -> Self {
        Self {
            id: LayerId::new(),
            name: name.into(),
            visible: true,
            locked: false,
            export_enabled: true,
            parent_id: None,
            transform,
            kind: ContentKind::Text(TextContent {
                text: text.into(),
                font_family: "sans-serif".to_owned(),
                font_size_um: 4_000,
                layout: TextLayout::AutoWidth,
            }),
        }
    }

    pub fn new_image(name: impl Into<String>, asset_id: AssetId, transform: TransformUm) -> Self {
        Self {
            id: LayerId::new(),
            name: name.into(),
            visible: true,
            locked: false,
            export_enabled: true,
            parent_id: None,
            transform,
            kind: ContentKind::Image(ImageContent {
                asset_id,
                crop: None,
            }),
        }
    }

    pub fn new_group(name: impl Into<String>) -> Self {
        Self {
            id: LayerId::new(),
            name: name.into(),
            visible: true,
            locked: false,
            export_enabled: true,
            parent_id: None,
            transform: TransformUm::default(),
            kind: ContentKind::Group,
        }
    }

    pub fn new_board_fill(name: impl Into<String>, edge_clearance_um: u32) -> Self {
        Self {
            id: LayerId::new(),
            name: name.into(),
            visible: true,
            locked: false,
            export_enabled: true,
            parent_id: None,
            transform: TransformUm::default(),
            kind: ContentKind::BoardFill(BoardFillContent { edge_clearance_um }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ContentKind {
    Image(ImageContent),
    Text(TextContent),
    BoardFill(BoardFillContent),
    Group,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardFillContent {
    pub edge_clearance_um: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    pub asset_id: AssetId,
    pub crop: Option<CropRect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRect {
    pub x_millionths: u32,
    pub y_millionths: u32,
    pub width_millionths: u32,
    pub height_millionths: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContent {
    pub text: String,
    pub font_family: String,
    pub font_size_um: u32,
    pub layout: TextLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextLayout {
    AutoWidth,
    FixedFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransformUm {
    pub x_um: i64,
    pub y_um: i64,
    pub width_um: u32,
    pub height_um: u32,
    pub rotation_mdeg: i32,
    pub flip_x: bool,
    pub flip_y: bool,
}

impl TransformUm {
    pub const fn rect(x_um: i64, y_um: i64, width_um: u32, height_um: u32) -> Self {
        Self {
            x_um,
            y_um,
            width_um,
            height_um,
            rotation_mdeg: 0,
            flip_x: false,
            flip_y: false,
        }
    }

    fn axis_aligned_bounds(&self) -> PhysicalBoundsUm {
        if self.rotation_mdeg.rem_euclid(360_000) == 0 {
            return PhysicalBoundsUm {
                min_x_um: self.x_um,
                min_y_um: self.y_um,
                max_x_um: self.x_um + i64::from(self.width_um),
                max_y_um: self.y_um + i64::from(self.height_um),
            };
        }

        let center_x = self.x_um as f64 + f64::from(self.width_um) / 2.0;
        let center_y = self.y_um as f64 + f64::from(self.height_um) / 2.0;
        let half_width = f64::from(self.width_um) / 2.0;
        let half_height = f64::from(self.height_um) / 2.0;
        let radians = (f64::from(self.rotation_mdeg) / 1_000.0).to_radians();
        let (sin, cos) = radians.sin_cos();
        let corners = [
            (-half_width, -half_height),
            (half_width, -half_height),
            (half_width, half_height),
            (-half_width, half_height),
        ];
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for (x, y) in corners {
            let board_x = center_x + x * cos - y * sin;
            let board_y = center_y + x * sin + y * cos;
            min_x = min_x.min(board_x);
            min_y = min_y.min(board_y);
            max_x = max_x.max(board_x);
            max_y = max_y.max(board_y);
        }
        PhysicalBoundsUm {
            min_x_um: min_x.floor() as i64,
            min_y_um: min_y.floor() as i64,
            max_x_um: max_x.ceil() as i64,
            max_y_um: max_y.ceil() as i64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalBoundsUm {
    pub min_x_um: i64,
    pub min_y_um: i64,
    pub max_x_um: i64,
    pub max_y_um: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DocumentDiagnostic {
    ContentOutsideBoard {
        side: CardSide,
        layer_id: LayerId,
        bounds: PhysicalBoundsUm,
        board_width_um: u32,
        board_height_um: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BoardOutline {
    Rectangle {
        width_um: u32,
        height_um: u32,
    },
    RoundedRectangle {
        width_um: u32,
        height_um: u32,
        corner_radius_um: u32,
    },
}

impl BoardOutline {
    pub const fn width_um(&self) -> u32 {
        match self {
            Self::Rectangle { width_um, .. } | Self::RoundedRectangle { width_um, .. } => *width_um,
        }
    }

    pub const fn height_um(&self) -> u32 {
        match self {
            Self::Rectangle { height_um, .. } | Self::RoundedRectangle { height_um, .. } => {
                *height_um
            }
        }
    }

    pub fn mirror_x_for_back_view(&self, transform: &TransformUm) -> i64 {
        i64::from(self.width_um()) - transform.x_um - i64::from(transform.width_um)
    }

    fn validate(&self) -> Result<(), DocumentError> {
        if self.width_um() == 0 || self.height_um() == 0 {
            return Err(DocumentError::InvalidBoardDimensions);
        }
        if let Self::RoundedRectangle {
            width_um,
            height_um,
            corner_radius_um,
        } = self
        {
            let maximum = (*width_um).min(*height_um) / 2;
            if *corner_radius_um > maximum {
                return Err(DocumentError::InvalidCornerRadius {
                    radius_um: *corner_radius_um,
                    maximum_um: maximum,
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackupPreset {
    pub substrate: SubstrateMaterial,
    pub thickness_um: u32,
    pub solder_mask_color: SolderMaskColor,
    pub surface_finish: SurfaceFinish,
}

impl Default for StackupPreset {
    fn default() -> Self {
        Self {
            substrate: SubstrateMaterial::Fr4,
            thickness_um: 1_600,
            solder_mask_color: SolderMaskColor::Black,
            surface_finish: SurfaceFinish::Enig,
        }
    }
}

impl StackupPreset {
    fn validate(&self) -> Result<(), DocumentError> {
        if self.thickness_um == 0 {
            return Err(DocumentError::InvalidBoardThickness);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstrateMaterial {
    Fr4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SolderMaskColor {
    Black,
    White,
    Green,
    Red,
    Blue,
    Purple,
    Yellow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceFinish {
    Enig,
    HaslLeadFree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetReference {
    pub id: AssetId,
    pub embedded_path: String,
    pub original_filename: String,
    pub media_type: String,
    pub sha256: String,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionMapping {
    pub id: MappingId,
    pub source_layer_id: LayerId,
    pub target: ProductionTarget,
    pub combine: CombineMode,
}

impl ProductionMapping {
    pub fn new(source_layer_id: LayerId, target: ProductionTarget, combine: CombineMode) -> Self {
        Self {
            id: MappingId::new(),
            source_layer_id,
            target,
            combine,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionTarget {
    pub side: CardSide,
    pub layer: FaceProductionLayer,
}

impl ProductionTarget {
    pub const fn new(side: CardSide, layer: FaceProductionLayer) -> Self {
        Self { side, layer }
    }

    pub const fn canonical_name(self) -> &'static str {
        match (self.side, self.layer) {
            (CardSide::Front, FaceProductionLayer::Copper) => "topCopper",
            (CardSide::Front, FaceProductionLayer::SolderMaskOpen) => "topSolderMaskOpen",
            (CardSide::Front, FaceProductionLayer::Silkscreen) => "topSilkscreen",
            (CardSide::Back, FaceProductionLayer::Copper) => "bottomCopper",
            (CardSide::Back, FaceProductionLayer::SolderMaskOpen) => "bottomSolderMaskOpen",
            (CardSide::Back, FaceProductionLayer::Silkscreen) => "bottomSilkscreen",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FaceProductionLayer {
    Copper,
    SolderMaskOpen,
    Silkscreen,
}

impl FaceProductionLayer {
    pub const fn polarity_description(self) -> &'static str {
        match self {
            Self::SolderMaskOpen => "opening",
            Self::Copper | Self::Silkscreen => "positive",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CombineMode {
    Add,
    Subtract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MechanicalFeature {
    NpthRound {
        center_x_um: i64,
        center_y_um: i64,
        diameter_um: u32,
    },
    PthRound {
        center_x_um: i64,
        center_y_um: i64,
        drill_um: u32,
        pad_um: u32,
        mask_open_um: u32,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DocumentError {
    #[error("invalid project format: {0}")]
    InvalidFormat(String),
    #[error("unsupported project schema version: {0}")]
    UnsupportedSchema(u32),
    #[error("{feature} requires project schema {required}, but document declares schema {actual}")]
    FeatureRequiresSchema {
        feature: &'static str,
        required: u32,
        actual: u32,
    },
    #[error("front and back faces are assigned incorrectly")]
    InvalidFaceAssignment,
    #[error("board width and height must be positive")]
    InvalidBoardDimensions,
    #[error("board corner radius {radius_um} exceeds maximum {maximum_um}")]
    InvalidCornerRadius { radius_um: u32, maximum_um: u32 },
    #[error("board thickness must be positive")]
    InvalidBoardThickness,
    #[error("duplicate content layer id: {0}")]
    DuplicateLayerId(LayerId),
    #[error("content layer {layer_id} has empty name")]
    EmptyLayerName { layer_id: LayerId },
    #[error("content layer {layer_id} has invalid physical size")]
    InvalidLayerSize { layer_id: LayerId },
    #[error("content layer {layer_id} has an invalid image crop")]
    InvalidImageCrop { layer_id: LayerId },
    #[error(
        "board fill layer {layer_id} clearance {edge_clearance_um} does not leave a positive board interior"
    )]
    InvalidBoardFillClearance {
        layer_id: LayerId,
        edge_clearance_um: u32,
    },
    #[error(
        "{side} contains multiple board fills: first {first_layer_id}, duplicate {duplicate_layer_id}"
    )]
    MultipleBoardFills {
        side: CardSide,
        first_layer_id: LayerId,
        duplicate_layer_id: LayerId,
    },
    #[error("content layer {layer_id} refers to missing parent {parent_id}")]
    MissingParent {
        layer_id: LayerId,
        parent_id: LayerId,
    },
    #[error("content layer {layer_id} parent {parent_id} is not a group")]
    ParentIsNotGroup {
        layer_id: LayerId,
        parent_id: LayerId,
    },
    #[error("content layer hierarchy contains a cycle at {0}")]
    LayerCycle(LayerId),
    #[error("duplicate asset id: {0}")]
    DuplicateAssetId(AssetId),
    #[error("content layer {layer_id} refers to missing image asset {asset_id}")]
    MissingImageAsset {
        layer_id: LayerId,
        asset_id: AssetId,
    },
    #[error("asset {0} has unsafe embedded path")]
    UnsafeAssetPath(AssetId),
    #[error("duplicate production mapping id: {0}")]
    DuplicateMappingId(MappingId),
    #[error("production mapping refers to missing content layer: {0}")]
    MissingMappedLayer(LayerId),
    #[error("content layer {layer_id} on {source_side} cannot map to opposite face {target_side}")]
    CrossFaceMapping {
        layer_id: LayerId,
        source_side: CardSide,
        target_side: CardSide,
    },
    #[error("board fill layer {layer_id} can only map to copper, not {target}")]
    BoardFillMustMapToCopper {
        layer_id: LayerId,
        target: &'static str,
    },
    #[error("duplicate production mapping for layer {layer_id} to {target}")]
    DuplicateProductionMapping {
        layer_id: LayerId,
        target: &'static str,
    },
    #[error("mechanical feature has invalid dimensions")]
    InvalidMechanicalFeature,
    #[error("mechanical feature is outside board")]
    MechanicalFeatureOutsideBoard,
}

fn validate_face(
    face: &CardFace,
    all_layers: &mut HashMap<LayerId, CardSide>,
) -> Result<(), DocumentError> {
    let local_layers = face
        .layers
        .iter()
        .map(|layer| (layer.id, layer))
        .collect::<HashMap<_, _>>();
    if local_layers.len() != face.layers.len() {
        let mut ids = HashSet::new();
        let duplicate = face
            .layers
            .iter()
            .find(|layer| !ids.insert(layer.id))
            .expect("length mismatch guarantees duplicate");
        return Err(DocumentError::DuplicateLayerId(duplicate.id));
    }

    for layer in &face.layers {
        if all_layers.insert(layer.id, face.side).is_some() {
            return Err(DocumentError::DuplicateLayerId(layer.id));
        }
        if layer.name.trim().is_empty() {
            return Err(DocumentError::EmptyLayerName { layer_id: layer.id });
        }
        if !matches!(layer.kind, ContentKind::Group | ContentKind::BoardFill(_))
            && (layer.transform.width_um == 0 || layer.transform.height_um == 0)
        {
            return Err(DocumentError::InvalidLayerSize { layer_id: layer.id });
        }
        if let ContentKind::Text(text) = &layer.kind
            && text.font_size_um == 0
        {
            return Err(DocumentError::InvalidLayerSize { layer_id: layer.id });
        }
        if let ContentKind::Image(image) = &layer.kind
            && let Some(crop) = &image.crop
            && (crop.width_millionths == 0
                || crop.height_millionths == 0
                || crop.x_millionths.saturating_add(crop.width_millionths) > 1_000_000
                || crop.y_millionths.saturating_add(crop.height_millionths) > 1_000_000)
        {
            return Err(DocumentError::InvalidImageCrop { layer_id: layer.id });
        }
        if let Some(parent_id) = layer.parent_id {
            let parent = local_layers
                .get(&parent_id)
                .ok_or(DocumentError::MissingParent {
                    layer_id: layer.id,
                    parent_id,
                })?;
            if !matches!(parent.kind, ContentKind::Group) {
                return Err(DocumentError::ParentIsNotGroup {
                    layer_id: layer.id,
                    parent_id,
                });
            }
        }
    }

    for layer in &face.layers {
        let mut ancestors = HashSet::new();
        let mut current = layer.parent_id;
        while let Some(parent_id) = current {
            if !ancestors.insert(parent_id) || parent_id == layer.id {
                return Err(DocumentError::LayerCycle(layer.id));
            }
            current = local_layers
                .get(&parent_id)
                .and_then(|parent| parent.parent_id);
        }
    }
    Ok(())
}

fn validate_board_fills(document: &AtelierDocument) -> Result<(), DocumentError> {
    for face in [&document.front, &document.back] {
        let mut first_fill_id = None;
        for layer in &face.layers {
            let ContentKind::BoardFill(fill) = &layer.kind else {
                continue;
            };
            if let Some(first_layer_id) = first_fill_id {
                return Err(DocumentError::MultipleBoardFills {
                    side: face.side,
                    first_layer_id,
                    duplicate_layer_id: layer.id,
                });
            }
            first_fill_id = Some(layer.id);
            if document.schema_version < BOARD_FILL_SCHEMA_VERSION {
                return Err(DocumentError::FeatureRequiresSchema {
                    feature: "board fill",
                    required: BOARD_FILL_SCHEMA_VERSION,
                    actual: document.schema_version,
                });
            }
            if fill.edge_clearance_um.saturating_mul(2)
                >= document.board.width_um().min(document.board.height_um())
            {
                return Err(DocumentError::InvalidBoardFillClearance {
                    layer_id: layer.id,
                    edge_clearance_um: fill.edge_clearance_um,
                });
            }
        }
    }
    Ok(())
}

fn validate_assets(assets: &[AssetReference]) -> Result<(), DocumentError> {
    let mut ids = HashSet::new();
    for asset in assets {
        if !ids.insert(asset.id) {
            return Err(DocumentError::DuplicateAssetId(asset.id));
        }
        let path = std::path::Path::new(&asset.embedded_path);
        if path.is_absolute()
            || !asset.embedded_path.starts_with("assets/")
            || path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(DocumentError::UnsafeAssetPath(asset.id));
        }
    }
    Ok(())
}

fn validate_image_asset_references(
    front: &CardFace,
    back: &CardFace,
    assets: &[AssetReference],
) -> Result<(), DocumentError> {
    let asset_ids = assets.iter().map(|asset| asset.id).collect::<HashSet<_>>();
    for layer in front.layers.iter().chain(&back.layers) {
        if let ContentKind::Image(image) = &layer.kind
            && !asset_ids.contains(&image.asset_id)
        {
            return Err(DocumentError::MissingImageAsset {
                layer_id: layer.id,
                asset_id: image.asset_id,
            });
        }
    }
    Ok(())
}

fn validate_mappings(
    mappings: &[ProductionMapping],
    layers: &HashMap<LayerId, CardSide>,
    front: &CardFace,
    back: &CardFace,
) -> Result<(), DocumentError> {
    let mut ids = HashSet::new();
    let mut targets = HashSet::new();
    for mapping in mappings {
        if !ids.insert(mapping.id) {
            return Err(DocumentError::DuplicateMappingId(mapping.id));
        }
        let source_side = layers
            .get(&mapping.source_layer_id)
            .copied()
            .ok_or(DocumentError::MissingMappedLayer(mapping.source_layer_id))?;
        if source_side != mapping.target.side {
            return Err(DocumentError::CrossFaceMapping {
                layer_id: mapping.source_layer_id,
                source_side,
                target_side: mapping.target.side,
            });
        }
        let source = front
            .layers
            .iter()
            .chain(&back.layers)
            .find(|layer| layer.id == mapping.source_layer_id)
            .expect("mapped layer existence was checked above");
        if matches!(source.kind, ContentKind::BoardFill(_))
            && mapping.target.layer != FaceProductionLayer::Copper
        {
            return Err(DocumentError::BoardFillMustMapToCopper {
                layer_id: mapping.source_layer_id,
                target: mapping.target.canonical_name(),
            });
        }
        let key = (mapping.source_layer_id, mapping.target, mapping.combine);
        if !targets.insert(key) {
            return Err(DocumentError::DuplicateProductionMapping {
                layer_id: mapping.source_layer_id,
                target: mapping.target.canonical_name(),
            });
        }
    }
    Ok(())
}

fn validate_mechanical_features(
    features: &[MechanicalFeature],
    board: &BoardOutline,
) -> Result<(), DocumentError> {
    for feature in features {
        let (x, y, outer_diameter) = match *feature {
            MechanicalFeature::NpthRound {
                center_x_um,
                center_y_um,
                diameter_um,
            } => {
                if diameter_um == 0 {
                    return Err(DocumentError::InvalidMechanicalFeature);
                }
                (center_x_um, center_y_um, diameter_um)
            }
            MechanicalFeature::PthRound {
                center_x_um,
                center_y_um,
                drill_um,
                pad_um,
                mask_open_um,
            } => {
                if drill_um == 0 || pad_um < drill_um || mask_open_um < pad_um {
                    return Err(DocumentError::InvalidMechanicalFeature);
                }
                (center_x_um, center_y_um, mask_open_um)
            }
        };
        let radius = i64::from(outer_diameter) / 2;
        if x - radius < 0
            || y - radius < 0
            || x + radius > i64::from(board.width_um())
            || y + radius > i64::from(board.height_um())
        {
            return Err(DocumentError::MechanicalFeatureOutsideBoard);
        }
    }
    Ok(())
}
