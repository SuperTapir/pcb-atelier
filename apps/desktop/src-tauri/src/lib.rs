use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use atelier_core::{
    AssetId, AssetReference, AtelierDocument, BoardOutline, ByteBudgetLru, CardSide, CombineMode,
    CommandHistory, CommandOutcome, CompiledImageTreatment, ContentKind, ContentLayer,
    DocumentCommand, DocumentDiagnostic, EasyedaHandoffExportReport, FaceProductionLayer,
    ImageProductionMode, ImageTreatment, LayerId, LayerTransferMode, LayerTransform,
    ManufacturerProfileSnapshot, MappingId, PreparedImage, PreviewPalette, PreviewTexture,
    ProductionLayerPreviewTexture, ProductionMapping, ProductionTarget, ProjectAssetCommand,
    ProjectAssetCommandOutcome, ProjectBundle, ProjectBundleRasterizer, ResolvedFabricationBoard,
    SamplingPurpose, StackupPreset, TextContent, TextLayout, TransformUm, TreatmentCompileRequest,
    TreatmentId, TreatmentRecipe, build_production_trace, compile_fabrication_plan,
    compile_image_treatment, compile_prepared_image_with_cancel, export_easyeda_handoff,
    prepare_image, resolve_fabrication_plan, resolve_fabrication_plan_for_purpose,
    system_font_families, treatment_cache_key,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use image::{
    ExtendedColorType, ImageEncoder as _,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;

mod processing_scheduler;

use processing_scheduler::{TreatmentJobError, TreatmentJobKey, TreatmentProcessingScheduler};

pub const WORKSPACE_CONTRACT_VERSION: &str = "pcb-atelier-workspace-v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBridgeRequest {
    pub contract_version: String,
    pub command: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBridgeResponse {
    pub contract_version: &'static str,
    pub revision: u64,
    pub payload: Value,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CoreInfo {
    project_format: &'static str,
    schema_version: u32,
}

fn core_info() -> CoreInfo {
    CoreInfo {
        project_format: atelier_core::PROJECT_FORMAT,
        schema_version: atelier_core::PROJECT_SCHEMA_VERSION,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceDocumentView {
    title: String,
    board: BoardView,
    stackup: StackupPreset,
    faces: FaceSummaryView,
    front_layers: Vec<ContentLayer>,
    back_layers: Vec<ContentLayer>,
    assets: Vec<AssetReference>,
    image_treatments: Vec<ImageTreatment>,
    mappings: Vec<ProductionMapping>,
    manufacturer_profile: ManufacturerProfileSnapshot,
    history: HistoryAvailabilityView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct BoardView {
    width_um: u32,
    height_um: u32,
    outline: BoardOutlineView,
    corner_radius_um: u32,
    diagnostics: Vec<DocumentDiagnosticView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum DocumentDiagnosticView {
    ContentOutsideBoard {
        side: CardSide,
        layer_id: LayerId,
        bounds: PhysicalBoundsView,
        board_width_um: u32,
        board_height_um: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalBoundsView {
    min_x_um: i64,
    min_y_um: i64,
    max_x_um: i64,
    max_y_um: i64,
}

impl From<DocumentDiagnostic> for DocumentDiagnosticView {
    fn from(diagnostic: DocumentDiagnostic) -> Self {
        match diagnostic {
            DocumentDiagnostic::ContentOutsideBoard {
                side,
                layer_id,
                bounds,
                board_width_um,
                board_height_um,
            } => Self::ContentOutsideBoard {
                side,
                layer_id,
                bounds: PhysicalBoundsView {
                    min_x_um: bounds.min_x_um,
                    min_y_um: bounds.min_y_um,
                    max_x_um: bounds.max_x_um,
                    max_y_um: bounds.max_y_um,
                },
                board_width_um,
                board_height_um,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct FaceSummaryView {
    front_layer_count: usize,
    back_layer_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryAvailabilityView {
    can_undo: bool,
    can_redo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct FontCatalogView {
    families: Vec<String>,
    fallback_family: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeImageFileView {
    name: String,
    media_type: String,
    bytes: Vec<u8>,
}

// Interactive previews trade manufacturing-grid detail for responsive inspection.
// EasyEDA export uses the explicit FormalProduction resolver.
const INTERACTIVE_PREVIEW_PITCH_UM: u32 = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct BoardPreviewView {
    outline: BoardOutlineView,
    thickness_um: u32,
    fabrication_input_sha256: String,
    fabrication_output_sha256: String,
    textures: PreviewTexturesView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductionPreviewView {
    source: &'static str,
    outline: BoardOutlineView,
    fabrication_input_sha256: String,
    fabrication_output_sha256: String,
    pixel_pitch_um: u32,
    textures: Vec<ProductionTextureView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewTexturesView {
    palette: PreviewPalette,
    front: PreviewTextureView,
    back: PreviewTextureView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewTextureView {
    side: CardSide,
    width_px: u32,
    height_px: u32,
    png_data_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductionTextureView {
    side: CardSide,
    layer: FaceProductionLayer,
    width_px: u32,
    height_px: u32,
    png_data_url: String,
}

impl PreviewTextureView {
    fn from_texture(texture: PreviewTexture) -> Result<Self, String> {
        Ok(Self {
            side: texture.side,
            width_px: texture.width_px,
            height_px: texture.height_px,
            png_data_url: rgba_png_data_url(texture.width_px, texture.height_px, texture.rgba)?,
        })
    }
}

impl ProductionTextureView {
    fn from_texture(texture: ProductionLayerPreviewTexture) -> Result<Self, String> {
        Ok(Self {
            side: texture.side,
            layer: texture.layer,
            width_px: texture.width_px,
            height_px: texture.height_px,
            png_data_url: rgba_png_data_url(texture.width_px, texture.height_px, texture.rgba)?,
        })
    }
}

fn rgba_png_data_url(width_px: u32, height_px: u32, rgba: Vec<u8>) -> Result<String, String> {
    let expected_len = u64::from(width_px)
        .checked_mul(u64::from(height_px))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or_else(|| format!("preview texture dimensions overflow: {width_px}x{height_px}"))?;
    if rgba.len() != expected_len {
        return Err(format!(
            "invalid RGBA texture length for {width_px}x{height_px}: expected {expected_len}, got {}",
            rgba.len()
        ));
    }
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Adaptive)
        .write_image(&rgba, width_px, height_px, ExtendedColorType::Rgba8)
        .map_err(|error| format!("failed to encode preview PNG: {error}"))?;
    Ok(format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(png)
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum BoardOutlineView {
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

impl From<&BoardOutline> for BoardOutlineView {
    fn from(outline: &BoardOutline) -> Self {
        match outline {
            BoardOutline::Rectangle {
                width_um,
                height_um,
            } => Self::Rectangle {
                width_um: *width_um,
                height_um: *height_um,
            },
            BoardOutline::RoundedRectangle {
                width_um,
                height_um,
                corner_radius_um,
            } => Self::RoundedRectangle {
                width_um: *width_um,
                height_um: *height_um,
                corner_radius_um: *corner_radius_um,
            },
        }
    }
}

impl From<BoardOutlineView> for BoardOutline {
    fn from(outline: BoardOutlineView) -> Self {
        match outline {
            BoardOutlineView::Rectangle {
                width_um,
                height_um,
            } => Self::Rectangle {
                width_um,
                height_um,
            },
            BoardOutlineView::RoundedRectangle {
                width_um,
                height_um,
                corner_radius_um,
            } => Self::RoundedRectangle {
                width_um,
                height_um,
                corner_radius_um,
            },
        }
    }
}

fn workspace_document_view(
    document: &AtelierDocument,
    history: &CommandHistory,
) -> WorkspaceDocumentView {
    WorkspaceDocumentView {
        title: document.title.clone(),
        board: BoardView {
            width_um: document.board.width_um(),
            height_um: document.board.height_um(),
            outline: BoardOutlineView::from(&document.board),
            corner_radius_um: match &document.board {
                BoardOutline::Rectangle { .. } => 0,
                BoardOutline::RoundedRectangle {
                    corner_radius_um, ..
                } => *corner_radius_um,
            },
            diagnostics: document
                .content_bounds_diagnostics()
                .into_iter()
                .map(DocumentDiagnosticView::from)
                .collect(),
        },
        stackup: document.stackup.clone(),
        faces: FaceSummaryView {
            front_layer_count: document.front.layers.len(),
            back_layer_count: document.back.layers.len(),
        },
        front_layers: document.front.layers.clone(),
        back_layers: document.back.layers.clone(),
        assets: document.assets.clone(),
        image_treatments: document.image_treatments.clone(),
        mappings: document.mappings.clone(),
        manufacturer_profile: document.manufacturer_profile.clone(),
        history: HistoryAvailabilityView {
            can_undo: history.undo_depth() > 0,
            can_redo: history.redo_depth() > 0,
        },
    }
}

fn initial_workspace_document() -> AtelierDocument {
    AtelierDocument::new_card("未命名卡片", 64_000, 100_000)
}

struct WorkspaceSession {
    bundle: ProjectBundle,
    history: CommandHistory,
}

pub struct WorkspaceService {
    session: WorkspaceSession,
    revision: u64,
    resolved_board_cache: Arc<Mutex<ResolvedBoardCache>>,
    treatment_scheduler: Arc<TreatmentProcessingScheduler>,
    image_preview_runtime: Arc<Mutex<ImagePreviewRuntime>>,
}

const PREPARED_IMAGE_BUDGET_BYTES: usize = 256 * 1024 * 1024;
const IMAGE_PREVIEW_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

struct ImagePreviewSession {
    prepared: Arc<PreparedImage>,
    last_used: Instant,
}

struct ImagePreviewRuntime {
    sessions: HashMap<TreatmentId, ImagePreviewSession>,
    prepared: ByteBudgetLru<String, PreparedImage>,
    source_bytes: u64,
    prepare_count: u64,
    proxy_compile_count: u64,
}

impl Default for ImagePreviewRuntime {
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
            prepared: ByteBudgetLru::new(PREPARED_IMAGE_BUDGET_BYTES),
            source_bytes: 0,
            prepare_count: 0,
            proxy_compile_count: 0,
        }
    }
}

#[derive(Debug, Default)]
struct ResolvedBoardCache {
    entry: Option<CachedResolvedBoard>,
    resolution_count: u64,
}

#[derive(Debug)]
struct CachedResolvedBoard {
    revision: u64,
    board: ResolvedFabricationBoard,
}

impl WorkspaceService {
    pub fn new(document: AtelierDocument) -> Self {
        Self {
            session: WorkspaceSession::new(document),
            revision: 0,
            resolved_board_cache: Arc::new(Mutex::new(ResolvedBoardCache::default())),
            treatment_scheduler: Arc::new(TreatmentProcessingScheduler::new(2)),
            image_preview_runtime: Arc::new(Mutex::new(ImagePreviewRuntime::default())),
        }
    }

    pub fn invoke(&mut self, request: WorkspaceBridgeRequest) -> WorkspaceBridgeResponse {
        if request.contract_version != WORKSPACE_CONTRACT_VERSION {
            return self.failure(format!(
                "unsupported workspace contract version: {}",
                request.contract_version
            ));
        }
        let result = self.dispatch(&request.command, request.args);
        match result {
            Ok((payload, mutated)) => {
                if mutated {
                    self.revision = self.revision.saturating_add(1);
                    self.treatment_scheduler.advance_revision(self.revision);
                }
                WorkspaceBridgeResponse {
                    contract_version: WORKSPACE_CONTRACT_VERSION,
                    revision: self.revision,
                    payload,
                    error: None,
                }
            }
            Err(error) => self.failure(error),
        }
    }

    pub fn snapshot_for_read(&self) -> Self {
        Self {
            session: WorkspaceSession {
                bundle: self.session.bundle.clone(),
                history: CommandHistory::default(),
            },
            revision: self.revision,
            resolved_board_cache: Arc::clone(&self.resolved_board_cache),
            treatment_scheduler: Arc::clone(&self.treatment_scheduler),
            image_preview_runtime: Arc::clone(&self.image_preview_runtime),
        }
    }

    pub fn should_use_read_snapshot(command: &str) -> bool {
        matches!(
            command,
            "get_board_preview"
                | "get_production_preview"
                | "compile_image_treatment"
                | "begin_image_preview_source"
                | "request_image_preview"
                | "release_image_preview_source"
                | "get_image_preview_diagnostics"
                | "validate_manufacturer_profile"
                | "get_production_trace"
        )
    }

    fn resolved_interactive_board(&self) -> Result<ResolvedFabricationBoard, String> {
        let mut cache = self
            .resolved_board_cache
            .lock()
            .map_err(|_| "resolved board cache lock is poisoned".to_owned())?;
        if let Some(entry) = &cache.entry
            && entry.revision == self.revision
        {
            return Ok(entry.board.clone());
        }
        let board = self.session.resolve_interactive_board()?;
        cache.resolution_count = cache.resolution_count.saturating_add(1);
        cache.entry = Some(CachedResolvedBoard {
            revision: self.revision,
            board: board.clone(),
        });
        Ok(board)
    }

    #[cfg(test)]
    fn preview_resolution_count(&self) -> u64 {
        self.resolved_board_cache
            .lock()
            .expect("resolved board cache")
            .resolution_count
    }

    fn dispatch(&mut self, command: &str, args: Value) -> Result<(Value, bool), String> {
        let result = match command {
            "get_core_info" => return serialize_response(core_info(), false),
            "get_workspace_document" => {
                return serialize_response(self.session.document_view(), false);
            }
            "open_project" => {
                self.clear_image_preview_runtime()?;
                self.session
                    .open_project(decode_request(&args)?)
                    .and_then(to_json)
            }
            "new_project" => {
                self.clear_image_preview_runtime()?;
                self.session
                    .new_project(decode_request(&args)?)
                    .and_then(to_json)
            }
            "save_project" => {
                self.session.save_project(decode_request(&args)?)?;
                return serialize_response(self.session.document_view(), false);
            }
            "get_system_fonts" => {
                let mut families = system_font_families();
                families.retain(|family| family != "sans-serif");
                families.insert(0, "sans-serif".to_owned());
                return serialize_response(
                    FontCatalogView {
                        families,
                        fallback_family: "sans-serif",
                    },
                    false,
                );
            }
            "get_board_preview" => {
                let resolved = self.resolved_interactive_board()?;
                return serialize_response(board_preview(&resolved)?, false);
            }
            "get_production_preview" => {
                let resolved = self.resolved_interactive_board()?;
                return serialize_response(production_preview(&resolved)?, false);
            }
            "get_asset_bytes" => {
                let asset_id = serde_json::from_value(args["assetId"].clone())
                    .map_err(|error| format!("invalid assetId: {error}"))?;
                self.session.asset_bytes(asset_id).and_then(to_json)
            }
            "import_project_asset" => {
                let (view, changed) = self.session.import_asset(decode_request(&args)?)?;
                return serialize_response(view, changed);
            }
            "move_project_asset" => {
                let (view, changed) = self.session.move_project_asset(decode_request(&args)?)?;
                return serialize_response(view, changed);
            }
            "replace_all_asset_references" => {
                let (view, changed) = self
                    .session
                    .replace_all_asset_references(decode_request(&args)?)?;
                return serialize_response(view, changed);
            }
            "delete_project_asset" => {
                let view = self.session.delete_project_asset(decode_request(&args)?)?;
                return serialize_response(view, true);
            }
            "cleanup_unused_assets" => {
                let (view, changed) = self.session.cleanup_unused_assets()?;
                return serialize_response(view, changed);
            }
            "insert_image_asset" => self
                .session
                .insert_image(decode_request(&args)?)
                .and_then(to_json),
            "insert_image_treatment" => self
                .session
                .insert_treatment(decode_request(&args)?)
                .and_then(to_json),
            "set_treatment_recipe" => self
                .session
                .set_treatment_recipe(decode_request(&args)?)
                .and_then(to_json),
            "set_image_production_mode" => self
                .session
                .set_image_production_mode(decode_request(&args)?)
                .and_then(to_json),
            "compile_image_treatment" => {
                return serialize_response(self.compile_treatment(decode_request(&args)?)?, false);
            }
            "begin_image_preview_source" => {
                return serialize_response(
                    self.begin_image_preview_source(decode_request(&args)?)?,
                    false,
                );
            }
            "request_image_preview" => {
                return serialize_response(
                    self.request_image_preview(decode_request(&args)?)?,
                    false,
                );
            }
            "release_image_preview_source" => {
                self.release_image_preview_source(decode_request(&args)?)?;
                return serialize_response((), false);
            }
            "get_image_preview_diagnostics" => {
                return serialize_response(self.image_preview_diagnostics()?, false);
            }
            "confirm_image_import" => self
                .session
                .confirm_image_import(decode_request(&args)?)
                .and_then(to_json),
            "validate_manufacturer_profile" => {
                let request: ValidateManufacturerRequest = decode_request(&args)?;
                return serialize_response(
                    ManufacturerValidationView::from_profile(request.profile),
                    false,
                );
            }
            "set_manufacturer_profile" => self
                .session
                .set_manufacturer_profile(decode_request(&args)?)
                .and_then(to_json),
            "get_production_trace" => {
                return serialize_response(self.production_trace()?, false);
            }
            "insert_text_layer" => self
                .session
                .insert_text(decode_request(&args)?)
                .and_then(to_json),
            "set_text_content" => self
                .session
                .set_text(decode_request(&args)?)
                .and_then(to_json),
            "set_text_style" => self
                .session
                .set_text_style(decode_request(&args)?)
                .and_then(to_json),
            "set_layer_name" => {
                let request: LayerNameRequest = decode_request(&args)?;
                self.session
                    .execute(DocumentCommand::SetLayerName {
                        layer_id: request.layer_id,
                        name: request.name,
                    })
                    .and_then(to_json)
            }
            "delete_layer" => {
                let request: LayerIdRequest = decode_request(&args)?;
                self.session
                    .execute(DocumentCommand::DeleteLayer {
                        layer_id: request.layer_id,
                    })
                    .and_then(to_json)
            }
            "delete_layers" => {
                let request: LayerIdsRequest = decode_request(&args)?;
                self.session
                    .execute(DocumentCommand::DeleteLayers {
                        layer_ids: request.layer_ids,
                    })
                    .and_then(to_json)
            }
            "duplicate_layer" => self
                .session
                .duplicate_layer(decode_request(&args)?)
                .and_then(to_json),
            "transfer_layers" => self
                .session
                .transfer_layers(decode_request(&args)?)
                .and_then(to_json),
            "paste_layers" => self
                .session
                .paste_layers(decode_request(&args)?)
                .and_then(to_json),
            "transform_layer" => {
                let request: TransformLayerRequest = decode_request(&args)?;
                self.session
                    .execute(DocumentCommand::TransformLayer {
                        layer_id: request.layer_id,
                        transform: request.transform,
                    })
                    .and_then(to_json)
            }
            "transform_layers" => {
                let request: TransformLayersRequest = decode_request(&args)?;
                self.session
                    .execute(DocumentCommand::TransformLayers {
                        transforms: request.transforms,
                    })
                    .and_then(to_json)
            }
            "set_layer_lock" => {
                let request: LayerFlagRequest = decode_request(&args)?;
                self.session
                    .execute(DocumentCommand::SetLayerLock {
                        layer_id: request.layer_id,
                        locked: request.value,
                    })
                    .and_then(to_json)
            }
            "set_layer_visibility" => {
                let request: LayerFlagRequest = decode_request(&args)?;
                self.session
                    .execute(DocumentCommand::SetLayerVisibility {
                        layer_id: request.layer_id,
                        visible: request.value,
                    })
                    .and_then(to_json)
            }
            "set_layer_export_enabled" => {
                let request: LayerFlagRequest = decode_request(&args)?;
                self.session
                    .set_layer_export_enabled(request.layer_id, request.value)
                    .and_then(to_json)
            }
            "reorder_layer" => {
                let request: ReorderLayerRequest = decode_request(&args)?;
                self.session
                    .execute(DocumentCommand::ReorderLayer {
                        layer_id: request.layer_id,
                        new_parent_id: request.new_parent_id,
                        new_index: request.new_index,
                    })
                    .and_then(to_json)
            }
            "move_layer" => {
                let request: MoveLayerRequest = decode_request(&args)?;
                self.session
                    .execute(DocumentCommand::MoveLayer {
                        layer_id: request.layer_id,
                        new_parent_id: request.new_parent_id,
                        new_index: request.new_index,
                        from_target: ProductionTarget::new(request.side, request.from_layer),
                        to_target: ProductionTarget::new(request.side, request.to_layer),
                    })
                    .and_then(to_json)
            }
            "group_layers" => self.session.group(decode_request(&args)?).and_then(to_json),
            "ungroup_layer" => {
                let request: LayerIdRequest = decode_request(&args)?;
                self.session
                    .execute(DocumentCommand::UngroupLayer {
                        group_id: request.layer_id,
                    })
                    .and_then(to_json)
            }
            "map_layer" => self
                .session
                .map_layer(decode_request(&args)?)
                .and_then(to_json),
            "unmap_layer" => {
                let request: UnmapLayerRequest = decode_request(&args)?;
                self.session
                    .unmap_layer(request.mapping_id)
                    .and_then(to_json)
            }
            "set_stackup" => {
                let request: SetStackupRequest = decode_request(&args)?;
                self.session.set_stackup(request.stackup).and_then(to_json)
            }
            "set_board_outline" => {
                let request: SetBoardOutlineRequest = decode_request(&args)?;
                self.session
                    .set_board_outline(request.outline.into())
                    .and_then(to_json)
            }
            "create_board_fill" => {
                let result = self.session.create_board_fill(decode_request(&args)?)?;
                let created = result.created;
                return serialize_response(result, created);
            }
            "undo_workspace" => self.session.undo().and_then(to_json),
            "redo_workspace" => self.session.redo().and_then(to_json),
            "export_easyeda" => {
                let output_directory: PathBuf =
                    serde_json::from_value(args["outputDirectory"].clone())
                        .map_err(|error| format!("invalid outputDirectory: {error}"))?;
                return serialize_response(self.session.export_easyeda(&output_directory)?, false);
            }
            other => return Err(format!("unsupported workspace command: {other}")),
        }?;
        Ok((result, true))
    }

    fn failure(&self, error: String) -> WorkspaceBridgeResponse {
        WorkspaceBridgeResponse {
            contract_version: WORKSPACE_CONTRACT_VERSION,
            revision: self.revision,
            payload: Value::Null,
            error: Some(error),
        }
    }

    fn compile_treatment(
        &self,
        request: CompileTreatmentRequest,
    ) -> Result<TreatmentCompileView, String> {
        let treatment = self
            .session
            .bundle
            .document
            .image_treatments
            .iter()
            .find(|treatment| treatment.id == request.treatment_id)
            .ok_or_else(|| format!("image treatment not found: {}", request.treatment_id))?
            .clone();
        let asset = self
            .session
            .bundle
            .document
            .assets
            .iter()
            .find(|asset| asset.id == treatment.asset_id)
            .ok_or_else(|| format!("asset not found: {}", treatment.asset_id))?
            .clone();
        let bytes = self
            .session
            .bundle
            .asset_bytes(asset.id)
            .ok_or_else(|| format!("asset bytes not found: {}", asset.id))?
            .to_vec();
        let compile_request = TreatmentCompileRequest {
            physical_width_um: request.physical_width_um,
            physical_height_um: request.physical_height_um,
            pixel_pitch_um: request
                .pixel_pitch_um
                .unwrap_or_else(|| request.purpose.default_pixel_pitch_um()),
            revision: self.revision,
            purpose: request.purpose,
        };
        let recipe_fingerprint = treatment.recipe.fingerprint();
        let key = TreatmentJobKey {
            stream_id: format!("treatment:{}", treatment.id),
            generation: self.revision,
            workspace_revision: self.revision,
            recipe_fingerprint: recipe_fingerprint.clone(),
            cache_key: treatment_cache_key(
                &asset.sha256,
                &treatment.recipe,
                request.physical_width_um,
                request.physical_height_um,
                compile_request.pixel_pitch_um,
            ),
        };
        let recipe = treatment.recipe;
        let compiled = self
            .treatment_scheduler
            .compile(key, move |_| {
                compile_image_treatment(&bytes, &recipe, compile_request)
                    .map_err(|error| error.to_string())
            })
            .map_err(|error| match error {
                TreatmentJobError::Cancelled => {
                    "image treatment compile was cancelled by a newer request".to_owned()
                }
                TreatmentJobError::Stale => {
                    "image treatment compile result is stale and was discarded".to_owned()
                }
                TreatmentJobError::Failed(message) => message,
            })?;
        TreatmentCompileView::from_compiled(&compiled)
    }

    fn begin_image_preview_source(
        &self,
        request: BeginImagePreviewSourceRequest,
    ) -> Result<BeginImagePreviewSourceView, String> {
        let (bytes, media_type, transferred_bytes) = match (request.bytes, request.asset_id) {
            (Some(bytes), None) => (
                bytes,
                request
                    .media_type
                    .ok_or_else(|| "mediaType is required with preview source bytes".to_owned())?,
                true,
            ),
            (None, Some(asset_id)) => {
                let asset = self
                    .session
                    .bundle
                    .document
                    .assets
                    .iter()
                    .find(|asset| asset.id == asset_id)
                    .ok_or_else(|| format!("asset not found: {asset_id}"))?;
                (
                    self.session
                        .bundle
                        .asset_bytes(asset_id)
                        .ok_or_else(|| format!("asset bytes not found: {asset_id}"))?
                        .to_vec(),
                    asset.media_type.clone(),
                    false,
                )
            }
            _ => {
                return Err(
                    "begin image preview requires exactly one of bytes or assetId".to_owned(),
                );
            }
        };
        let source_sha256 = format!("{:x}", sha2::Sha256::digest(&bytes));
        let mut runtime = self
            .image_preview_runtime
            .lock()
            .map_err(|_| "image preview runtime lock is poisoned".to_owned())?;
        runtime.expire_idle_sessions();
        let prepared = if let Some(prepared) = runtime.prepared.get(&source_sha256) {
            prepared
        } else {
            let prepared = Arc::new(prepare_image(&bytes).map_err(|error| error.to_string())?);
            if prepared.estimated_bytes() > runtime.prepared.budget_bytes() {
                return Err(format!(
                    "image preview source needs {} bytes but the prepared-image budget is {} bytes",
                    prepared.estimated_bytes(),
                    runtime.prepared.budget_bytes()
                ));
            }
            runtime.prepare_count = runtime.prepare_count.saturating_add(1);
            let inserted = runtime.prepared.insert(
                source_sha256.clone(),
                Arc::clone(&prepared),
                prepared.estimated_bytes(),
            );
            if !inserted {
                return Err("image preview source exceeds the prepared-image budget".to_owned());
            }
            prepared
        };
        if transferred_bytes {
            runtime.source_bytes = runtime.source_bytes.saturating_add(bytes.len() as u64);
        }
        let source_handle = TreatmentId::new();
        runtime.sessions.insert(
            source_handle,
            ImagePreviewSession {
                prepared: Arc::clone(&prepared),
                last_used: Instant::now(),
            },
        );
        Ok(BeginImagePreviewSourceView {
            source_handle,
            source_sha256,
            width_px: prepared.width_px(),
            height_px: prepared.height_px(),
            media_type,
            workspace_revision: self.revision,
        })
    }

    fn request_image_preview(
        &self,
        request: RequestImagePreviewRequest,
    ) -> Result<TreatmentCompileView, String> {
        if request.workspace_revision > self.revision {
            return Err(format!(
                "image preview workspace revision {} is ahead of current revision {}",
                request.workspace_revision, self.revision
            ));
        }
        let prepared = {
            let mut runtime = self
                .image_preview_runtime
                .lock()
                .map_err(|_| "image preview runtime lock is poisoned".to_owned())?;
            runtime.expire_idle_sessions();
            let session = runtime
                .sessions
                .get_mut(&request.source_handle)
                .ok_or_else(|| "image preview source handle is missing or expired".to_owned())?;
            session.last_used = Instant::now();
            Arc::clone(&session.prepared)
        };
        let compile_request = TreatmentCompileRequest {
            physical_width_um: request.physical_width_um,
            physical_height_um: request.physical_height_um,
            pixel_pitch_um: request.pixel_pitch_um,
            revision: self.revision,
            purpose: SamplingPurpose::InteractiveProxy,
        };
        let recipe_fingerprint = request.recipe.fingerprint();
        let key = TreatmentJobKey {
            stream_id: format!("{}:{}", request.source_handle, request.preview_stream_id),
            generation: request.generation,
            workspace_revision: self.revision,
            recipe_fingerprint: recipe_fingerprint.clone(),
            cache_key: treatment_cache_key(
                prepared.source_sha256(),
                &request.recipe,
                request.physical_width_um,
                request.physical_height_um,
                request.pixel_pitch_um,
            ),
        };
        let recipe = request.recipe;
        let compiled = self
            .treatment_scheduler
            .compile(key, move |token| {
                compile_prepared_image_with_cancel(&prepared, &recipe, compile_request, || {
                    token.is_cancelled()
                })
                .map_err(|error| error.to_string())
            })
            .map_err(|error| match error {
                TreatmentJobError::Cancelled => {
                    "image preview was cancelled by a newer generation".to_owned()
                }
                TreatmentJobError::Stale => {
                    "image preview result is stale and was discarded".to_owned()
                }
                TreatmentJobError::Failed(message) => message,
            })?;
        self.image_preview_runtime
            .lock()
            .map_err(|_| "image preview runtime lock is poisoned".to_owned())?
            .proxy_compile_count += 1;
        TreatmentCompileView::from_compiled(&compiled)
    }

    fn release_image_preview_source(
        &self,
        request: ReleaseImagePreviewSourceRequest,
    ) -> Result<(), String> {
        self.image_preview_runtime
            .lock()
            .map_err(|_| "image preview runtime lock is poisoned".to_owned())?
            .sessions
            .remove(&request.source_handle);
        Ok(())
    }

    fn image_preview_diagnostics(&self) -> Result<ImagePreviewDiagnosticsView, String> {
        let runtime = self
            .image_preview_runtime
            .lock()
            .map_err(|_| "image preview runtime lock is poisoned".to_owned())?;
        let scheduler = self.treatment_scheduler.diagnostics();
        Ok(ImagePreviewDiagnosticsView {
            source_bytes: runtime.source_bytes,
            prepare_count: runtime.prepare_count,
            proxy_compile_count: runtime.proxy_compile_count,
            active_sessions: runtime.sessions.len(),
            prepared_resident_bytes: runtime.prepared.resident_bytes(),
            coalesce_count: scheduler.coalesce_count,
            cancel_count: scheduler.cancel_count,
            active: scheduler.active,
            pending: scheduler.pending,
        })
    }

    fn clear_image_preview_runtime(&self) -> Result<(), String> {
        let mut runtime = self
            .image_preview_runtime
            .lock()
            .map_err(|_| "image preview runtime lock is poisoned".to_owned())?;
        runtime.sessions.clear();
        runtime.prepared.clear();
        Ok(())
    }

    fn production_trace(&self) -> Result<atelier_core::ProductionTraceReport, String> {
        let resolved = self.resolved_interactive_board()?;
        Ok(build_production_trace(
            self.revision,
            &self.session.bundle.document,
            &resolved,
        ))
    }
}

impl ImagePreviewRuntime {
    fn expire_idle_sessions(&mut self) {
        let now = Instant::now();
        self.sessions.retain(|_, session| {
            now.saturating_duration_since(session.last_used) < IMAGE_PREVIEW_IDLE_TIMEOUT
        });
    }
}

fn decode_request<T: for<'de> Deserialize<'de>>(args: &Value) -> Result<T, String> {
    serde_json::from_value(args["request"].clone())
        .map_err(|error| format!("invalid command request: {error}"))
}

fn to_json(value: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

fn serialize_response(value: impl Serialize, mutated: bool) -> Result<(Value, bool), String> {
    Ok((to_json(value)?, mutated))
}

impl WorkspaceSession {
    fn new(document: AtelierDocument) -> Self {
        Self {
            bundle: ProjectBundle::new(document),
            history: CommandHistory::default(),
        }
    }

    fn document_view(&self) -> WorkspaceDocumentView {
        workspace_document_view(&self.bundle.document, &self.history)
    }

    fn open_project(
        &mut self,
        request: OpenProjectRequest,
    ) -> Result<WorkspaceDocumentView, String> {
        let bundle = ProjectBundle::open(Path::new(&request.path))
            .map_err(|error| format!("无法打开 PCB Atelier 工程：{error}"))?;
        self.bundle = bundle;
        self.history = CommandHistory::default();
        Ok(self.document_view())
    }

    fn new_project(&mut self, request: NewProjectRequest) -> Result<WorkspaceDocumentView, String> {
        let document =
            AtelierDocument::new_card(request.title, request.width_um, request.height_um);
        document.validate().map_err(|error| error.to_string())?;
        self.bundle = ProjectBundle::new(document);
        self.history = CommandHistory::default();
        Ok(self.document_view())
    }

    fn save_project(&self, request: SaveProjectRequest) -> Result<(), String> {
        self.bundle
            .save(Path::new(&request.path))
            .map_err(|error| format!("无法保存 PCB Atelier 工程：{error}"))
    }

    fn import_asset(
        &mut self,
        request: ImportAssetRequest,
    ) -> Result<(AssetMutationView, bool), String> {
        let previous_count = self.bundle.document.assets.len();
        let asset_id = self
            .bundle
            .embed_asset(
                request.original_filename,
                request.media_type,
                request.pixel_width,
                request.pixel_height,
                request.bytes,
            )
            .map_err(|error| error.to_string())?;
        Ok((
            AssetMutationView {
                document: self.document_view(),
                asset_id,
                reused: self.bundle.document.assets.len() == previous_count,
            },
            self.bundle.document.assets.len() != previous_count,
        ))
    }

    fn replace_all_asset_references(
        &mut self,
        request: ReplaceAllAssetReferencesRequest,
    ) -> Result<(AssetReferencesMutationView, bool), String> {
        let outcome = ProjectAssetCommand::ReplaceAllReferences {
            original_asset_id: request.original_asset_id,
            replacement_asset_id: request.replacement_asset_id,
        }
        .apply(&mut self.bundle)
        .map_err(|error| error.to_string())?;
        let ProjectAssetCommandOutcome::ReferencesReplaced {
            original_asset_id,
            replacement_asset_id,
            instance_count,
            treatment_count,
        } = outcome
        else {
            return Err("replace all asset references returned an unexpected outcome".to_owned());
        };
        let changed = instance_count + treatment_count > 0;
        Ok((
            AssetReferencesMutationView {
                document: self.document_view(),
                original_asset_id,
                replacement_asset_id,
                replaced_instance_count: instance_count,
                replaced_treatment_count: treatment_count,
            },
            changed,
        ))
    }

    fn move_project_asset(
        &mut self,
        request: MoveProjectAssetRequest,
    ) -> Result<(AssetFolderMutationView, bool), String> {
        let previous_folder_path = self
            .bundle
            .document
            .assets
            .iter()
            .find(|asset| asset.id == request.asset_id)
            .map(|asset| asset.folder_path.clone());
        let outcome = ProjectAssetCommand::MoveToFolder {
            asset_id: request.asset_id,
            folder_path: request.folder_path,
        }
        .apply(&mut self.bundle)
        .map_err(|error| error.to_string())?;
        let ProjectAssetCommandOutcome::AssetMoved {
            asset_id,
            folder_path,
        } = outcome
        else {
            return Err("move project asset returned an unexpected outcome".to_owned());
        };
        let changed = previous_folder_path.is_some_and(|previous| previous != folder_path);
        Ok((
            AssetFolderMutationView {
                document: self.document_view(),
                asset_id,
                folder_path,
            },
            changed,
        ))
    }

    fn delete_project_asset(
        &mut self,
        request: ProjectAssetIdRequest,
    ) -> Result<AssetDeletionMutationView, String> {
        let outcome = ProjectAssetCommand::Delete {
            asset_id: request.asset_id,
        }
        .apply(&mut self.bundle)
        .map_err(|error| error.to_string())?;
        let ProjectAssetCommandOutcome::AssetDeleted { asset_id } = outcome else {
            return Err("delete project asset returned an unexpected outcome".to_owned());
        };
        Ok(AssetDeletionMutationView {
            document: self.document_view(),
            deleted_asset_id: asset_id,
        })
    }

    fn cleanup_unused_assets(&mut self) -> Result<(AssetCleanupMutationView, bool), String> {
        let outcome = ProjectAssetCommand::CleanupUnused
            .apply(&mut self.bundle)
            .map_err(|error| error.to_string())?;
        let ProjectAssetCommandOutcome::UnusedAssetsRemoved { asset_ids } = outcome else {
            return Err("cleanup unused assets returned an unexpected outcome".to_owned());
        };
        let changed = !asset_ids.is_empty();
        Ok((
            AssetCleanupMutationView {
                document: self.document_view(),
                removed_asset_ids: asset_ids,
            },
            changed,
        ))
    }

    fn insert_image(&mut self, request: ImageAssetRequest) -> Result<LayerMutationView, String> {
        let placement_center = request
            .placement_center_um
            .as_ref()
            .map(|point| (point.x_um, point.y_um));
        if request.replace_layer_id.is_none() {
            if let Some((x_um, y_um)) = placement_center {
                let board_width_um = i64::from(self.bundle.document.board.width_um());
                let board_height_um = i64::from(self.bundle.document.board.height_um());
                if !(0..=board_width_um).contains(&x_um) || !(0..=board_height_um).contains(&y_um) {
                    return Err("图片落点必须位于板框内".to_owned());
                }
            }
        }
        let previous_bundle = self.bundle.clone();
        let asset_id = self
            .bundle
            .embed_asset(
                &request.original_filename,
                &request.media_type,
                request.pixel_width,
                request.pixel_height,
                request.bytes,
            )
            .map_err(|error| error.to_string())?;

        let layer_id = if let Some(layer_id) = request.replace_layer_id {
            let command = DocumentCommand::ReplaceImageInstanceAsset { layer_id, asset_id };
            if let Err(error) = self.history.execute(&mut self.bundle.document, command) {
                self.bundle = previous_bundle;
                return Err(error.to_string());
            }
            layer_id
        } else {
            let mut transform = fit_image_transform(
                &self.bundle.document,
                request.pixel_width,
                request.pixel_height,
            );
            if let Some((x_um, y_um)) = placement_center {
                transform.x_um = x_um - i64::from(transform.width_um) / 2;
                transform.y_um = y_um - i64::from(transform.height_um) / 2;
            }
            let layer = ContentLayer::new_image(request.original_filename, asset_id, transform);
            let layer_id = layer.id;
            let index = face_layers(&self.bundle.document, request.side).len();
            let command = DocumentCommand::InsertLayer {
                side: request.side,
                layer,
                index,
            };
            if let Err(error) = self.history.execute(&mut self.bundle.document, command) {
                self.bundle = previous_bundle;
                return Err(error.to_string());
            }
            layer_id
        };

        Ok(LayerMutationView {
            document: self.document_view(),
            layer_id,
        })
    }

    fn insert_treatment(
        &mut self,
        request: InsertTreatmentRequest,
    ) -> Result<TreatmentMutationView, String> {
        validate_color_original_asset(&self.bundle, request.asset_id, request.production_mode)?;
        let mut treatment = ImageTreatment::new(request.asset_id, request.recipe);
        treatment.production_mode = request.production_mode;
        let treatment_id = treatment.id;
        self.execute(DocumentCommand::InsertImageTreatment { treatment })?;
        Ok(TreatmentMutationView {
            document: self.document_view(),
            treatment_id,
        })
    }

    fn confirm_image_import(
        &mut self,
        request: ConfirmImageImportRequest,
    ) -> Result<ConfirmedImageImportView, String> {
        if let Some(point) = &request.placement_center_um {
            let board_width_um = i64::from(self.bundle.document.board.width_um());
            let board_height_um = i64::from(self.bundle.document.board.height_um());
            if !(0..=board_width_um).contains(&point.x_um)
                || !(0..=board_height_um).contains(&point.y_um)
            {
                return Err("图片落点必须位于板框内".to_owned());
            }
        }

        let mut staged_bundle = self.bundle.clone();
        let asset_id = staged_bundle
            .embed_asset(
                &request.original_filename,
                &request.media_type,
                request.pixel_width,
                request.pixel_height,
                request.bytes,
            )
            .map_err(|error| error.to_string())?;
        let new_asset = if self
            .bundle
            .document
            .assets
            .iter()
            .any(|asset| asset.id == asset_id)
        {
            None
        } else {
            staged_bundle
                .document
                .assets
                .iter()
                .find(|asset| asset.id == asset_id)
                .cloned()
        };
        validate_color_original_asset(&staged_bundle, asset_id, request.production_mode)?;

        let mut transform = fit_image_transform(
            &self.bundle.document,
            request.pixel_width,
            request.pixel_height,
        );
        if let Some(point) = request.placement_center_um {
            transform.x_um = point.x_um - i64::from(transform.width_um) / 2;
            transform.y_um = point.y_um - i64::from(transform.height_um) / 2;
        }
        let layer = ContentLayer::new_image(request.original_filename, asset_id, transform);
        let layer_id = layer.id;
        let mut treatment = ImageTreatment::new(asset_id, request.recipe);
        treatment.production_mode = request.production_mode;
        let treatment_id = treatment.id;
        let mut mapping = ProductionMapping::new(
            layer_id,
            ProductionTarget::new(request.side, request.layer),
            CombineMode::Add,
        );
        mapping.treatment_id = Some(treatment_id);
        let index = face_layers(&self.bundle.document, request.side).len();

        self.history
            .execute(
                &mut self.bundle.document,
                DocumentCommand::InsertProcessedImage {
                    asset: new_asset,
                    side: request.side,
                    layer,
                    index,
                    treatment,
                    mapping,
                },
            )
            .map_err(|error| error.to_string())?;
        self.bundle.assets = staged_bundle.assets;

        Ok(ConfirmedImageImportView {
            document: self.document_view(),
            asset_id,
            treatment_id,
            layer_id,
        })
    }

    fn set_treatment_recipe(
        &mut self,
        request: SetTreatmentRecipeRequest,
    ) -> Result<WorkspaceDocumentView, String> {
        self.execute(DocumentCommand::SetTreatmentRecipe {
            treatment_id: request.treatment_id,
            recipe: request.recipe,
        })
    }

    fn set_image_production_mode(
        &mut self,
        request: SetImageProductionModeRequest,
    ) -> Result<WorkspaceDocumentView, String> {
        let asset_id = self
            .bundle
            .document
            .image_treatments
            .iter()
            .find(|treatment| treatment.id == request.treatment_id)
            .ok_or_else(|| format!("image treatment not found: {}", request.treatment_id))?
            .asset_id;
        validate_color_original_asset(&self.bundle, asset_id, request.production_mode)?;
        self.execute(DocumentCommand::SetImageProductionMode {
            treatment_id: request.treatment_id,
            production_mode: request.production_mode,
        })
    }

    fn set_manufacturer_profile(
        &mut self,
        request: SetManufacturerRequest,
    ) -> Result<WorkspaceDocumentView, String> {
        self.execute(DocumentCommand::SetManufacturerProfile {
            profile: request.profile,
        })
    }

    fn insert_text(&mut self, request: InsertTextRequest) -> Result<LayerMutationView, String> {
        let transform = TransformUm::rect(
            request.x_um,
            request.y_um,
            request.width_um,
            request.height_um,
        );
        let mut layer = ContentLayer::new_text("文字", "文字", transform);
        if let ContentKind::Text(content) = &mut layer.kind {
            content.layout = request.layout;
        }
        let layer_id = layer.id;
        let index = face_layers(&self.bundle.document, request.side).len();
        self.history
            .execute(
                &mut self.bundle.document,
                DocumentCommand::InsertLayer {
                    side: request.side,
                    layer,
                    index,
                },
            )
            .map_err(|error| error.to_string())?;

        Ok(LayerMutationView {
            document: self.document_view(),
            layer_id,
        })
    }

    fn set_text(&mut self, request: SetTextRequest) -> Result<WorkspaceDocumentView, String> {
        let existing = find_layer(&self.bundle.document, request.layer_id)
            .ok_or_else(|| format!("content layer not found: {}", request.layer_id))?;
        let ContentKind::Text(existing_text) = &existing.kind else {
            return Err(format!("content layer {} is not text", request.layer_id));
        };
        let text = TextContent {
            text: request.text,
            ..existing_text.clone()
        };
        self.history
            .execute(
                &mut self.bundle.document,
                DocumentCommand::SetTextContent {
                    layer_id: request.layer_id,
                    text,
                },
            )
            .map_err(|error| error.to_string())?;
        Ok(self.document_view())
    }

    fn set_text_style(
        &mut self,
        request: SetTextStyleRequest,
    ) -> Result<WorkspaceDocumentView, String> {
        if request.font_family.trim().is_empty() {
            return Err("font family must not be empty".to_owned());
        }
        if request.font_size_um == 0 {
            return Err("font size must be positive".to_owned());
        }
        let existing = find_layer(&self.bundle.document, request.layer_id)
            .ok_or_else(|| format!("content layer not found: {}", request.layer_id))?;
        let ContentKind::Text(existing_text) = &existing.kind else {
            return Err(format!("content layer {} is not text", request.layer_id));
        };
        let text = TextContent {
            font_family: request.font_family,
            font_size_um: request.font_size_um,
            ..existing_text.clone()
        };
        self.history
            .execute(
                &mut self.bundle.document,
                DocumentCommand::SetTextContent {
                    layer_id: request.layer_id,
                    text,
                },
            )
            .map_err(|error| error.to_string())?;
        Ok(self.document_view())
    }

    fn asset_bytes(&self, asset_id: AssetId) -> Result<AssetBytesView, String> {
        let reference = self
            .bundle
            .document
            .assets
            .iter()
            .find(|asset| asset.id == asset_id)
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        let bytes = self
            .bundle
            .asset_bytes(asset_id)
            .ok_or_else(|| format!("asset bytes not found: {asset_id}"))?;
        Ok(AssetBytesView {
            media_type: reference.media_type.clone(),
            bytes: bytes.to_vec(),
        })
    }

    fn resolve_interactive_board(&self) -> Result<ResolvedFabricationBoard, String> {
        let plan =
            compile_fabrication_plan(&self.bundle.document).map_err(|error| error.to_string())?;
        let mut rasterizer =
            ProjectBundleRasterizer::new(&self.bundle).map_err(|error| error.to_string())?;
        resolve_fabrication_plan(&plan, INTERACTIVE_PREVIEW_PITCH_UM, &mut rasterizer)
            .map_err(|error| error.to_string())
    }

    fn export_easyeda(
        &self,
        output_directory: &Path,
    ) -> Result<EasyedaHandoffExportReport, String> {
        let failure_prefix = format!(
            "嘉立创 EDA 导出失败（输出目录：{}）",
            output_directory.display()
        );
        let plan = compile_fabrication_plan(&self.bundle.document)
            .map_err(|error| format!("{failure_prefix}：生产编译失败：{error}"))?;
        let mut rasterizer = ProjectBundleRasterizer::new(&self.bundle)
            .map_err(|error| format!("{failure_prefix}：生产栅格器初始化失败：{error}"))?;
        let resolved = resolve_fabrication_plan_for_purpose(
            &plan,
            SamplingPurpose::FormalProduction,
            &mut rasterizer,
        )
        .map_err(|error| format!("{failure_prefix}：生产几何解析失败：{error}"))?;
        export_easyeda_handoff(output_directory, &self.bundle, &resolved)
            .map_err(|error| format!("{failure_prefix}：产物写入或校验失败：{error}"))
    }

    fn execute(&mut self, command: DocumentCommand) -> Result<WorkspaceDocumentView, String> {
        self.history
            .execute(&mut self.bundle.document, command)
            .map_err(|error| error.to_string())?;
        Ok(self.document_view())
    }

    fn group(&mut self, request: GroupLayersRequest) -> Result<LayerMutationView, String> {
        let group = ContentLayer::new_group(request.name);
        let layer_id = group.id;
        self.execute(DocumentCommand::GroupLayers {
            side: request.side,
            group,
            layer_ids: request.layer_ids,
        })?;
        Ok(LayerMutationView {
            document: self.document_view(),
            layer_id,
        })
    }

    fn map_layer(&mut self, request: MapLayerRequest) -> Result<WorkspaceDocumentView, String> {
        if self.bundle.document.mappings.iter().any(|mapping| {
            mapping.source_layer_id == request.layer_id
                && mapping.target == ProductionTarget::new(request.side, request.layer)
                && mapping.combine == request.combine
        }) {
            return Ok(self.document_view());
        }
        self.execute(DocumentCommand::MapLayer {
            mapping: ProductionMapping::new(
                request.layer_id,
                ProductionTarget::new(request.side, request.layer),
                request.combine,
            ),
        })
    }

    fn unmap_layer(&mut self, mapping_id: MappingId) -> Result<WorkspaceDocumentView, String> {
        self.execute(DocumentCommand::UnmapLayer { mapping_id })
    }

    fn set_layer_export_enabled(
        &mut self,
        layer_id: LayerId,
        export_enabled: bool,
    ) -> Result<WorkspaceDocumentView, String> {
        let layer = find_layer(&self.bundle.document, layer_id)
            .ok_or_else(|| format!("content layer not found: {layer_id}"))?;
        if layer.locked {
            return Err(format!("content layer is locked: {layer_id}"));
        }
        self.execute(DocumentCommand::SetLayerExportEnabled {
            layer_id,
            export_enabled,
        })
    }

    fn duplicate_layer(&mut self, request: LayerIdRequest) -> Result<LayerMutationView, String> {
        let duplicate_layer_id = LayerId::new();
        let duplicate_mapping_ids = self
            .bundle
            .document
            .mappings
            .iter()
            .filter(|mapping| mapping.source_layer_id == request.layer_id)
            .map(|_| MappingId::new())
            .collect();
        self.execute(DocumentCommand::DuplicateLayer {
            layer_id: request.layer_id,
            duplicate_layer_id,
            duplicate_mapping_ids,
            offset_um: 2_000,
        })?;
        Ok(LayerMutationView {
            document: self.document_view(),
            layer_id: duplicate_layer_id,
        })
    }

    fn transfer_layers(
        &mut self,
        request: TransferLayersRequest,
    ) -> Result<LayersMutationView, String> {
        let selected = request.layer_ids.iter().copied().collect::<HashSet<_>>();
        let roots = request
            .layer_ids
            .iter()
            .copied()
            .filter(|layer_id| {
                let mut parent_id =
                    find_layer(&self.bundle.document, *layer_id).and_then(|layer| layer.parent_id);
                while let Some(parent) = parent_id {
                    if selected.contains(&parent) {
                        return false;
                    }
                    parent_id =
                        find_layer(&self.bundle.document, parent).and_then(|layer| layer.parent_id);
                }
                true
            })
            .collect::<Vec<_>>();
        let mut transferring = roots.iter().copied().collect::<HashSet<_>>();
        loop {
            let before = transferring.len();
            for layer in self
                .bundle
                .document
                .front
                .layers
                .iter()
                .chain(self.bundle.document.back.layers.iter())
            {
                if layer
                    .parent_id
                    .is_some_and(|parent| transferring.contains(&parent))
                {
                    transferring.insert(layer.id);
                }
            }
            if transferring.len() == before {
                break;
            }
        }
        let ordered_ids = self
            .bundle
            .document
            .front
            .layers
            .iter()
            .chain(self.bundle.document.back.layers.iter())
            .filter(|layer| transferring.contains(&layer.id))
            .map(|layer| layer.id)
            .collect::<Vec<_>>();
        let duplicate_layer_ids = if request.mode == LayerTransferMode::Copy {
            ordered_ids
                .iter()
                .map(|_| LayerId::new())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let duplicate_mapping_ids = if request.mode == LayerTransferMode::Copy {
            self.bundle
                .document
                .mappings
                .iter()
                .filter(|mapping| transferring.contains(&mapping.source_layer_id))
                .map(|_| MappingId::new())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let id_map = ordered_ids
            .iter()
            .copied()
            .zip(duplicate_layer_ids.iter().copied())
            .collect::<HashMap<_, _>>();
        let result_layer_ids = if request.mode == LayerTransferMode::Copy {
            roots
                .iter()
                .filter_map(|root| id_map.get(root).copied())
                .collect()
        } else {
            roots
        };
        self.execute(DocumentCommand::TransferLayers {
            layer_ids: request.layer_ids,
            target: ProductionTarget::new(request.target_side, request.target_layer),
            new_parent_id: request.new_parent_id,
            new_index: request.new_index,
            mode: request.mode,
            duplicate_layer_ids,
            duplicate_mapping_ids,
            offset_um: request.offset_um,
        })?;
        Ok(LayersMutationView {
            document: self.document_view(),
            layer_ids: result_layer_ids,
        })
    }

    fn paste_layers(&mut self, request: PasteLayersRequest) -> Result<LayersMutationView, String> {
        let pasted_ids = request
            .layers
            .iter()
            .map(|layer| layer.id)
            .collect::<HashSet<_>>();
        let root_ids = request
            .layers
            .iter()
            .filter(|layer| {
                layer
                    .parent_id
                    .is_none_or(|parent| !pasted_ids.contains(&parent))
            })
            .map(|layer| layer.id)
            .collect::<Vec<_>>();
        self.execute(DocumentCommand::PasteLayers {
            layers: request.layers,
            mappings: request.mappings,
            target: ProductionTarget::new(request.target_side, request.target_layer),
            new_parent_id: request.new_parent_id,
            new_index: request.new_index,
        })?;
        Ok(LayersMutationView {
            document: self.document_view(),
            layer_ids: root_ids,
        })
    }

    fn set_stackup(&mut self, stackup: StackupPreset) -> Result<WorkspaceDocumentView, String> {
        self.execute(DocumentCommand::SetStackup { stackup })
    }

    fn set_board_outline(
        &mut self,
        outline: BoardOutline,
    ) -> Result<WorkspaceDocumentView, String> {
        self.execute(DocumentCommand::SetBoardOutline { outline })
    }

    fn create_board_fill(
        &mut self,
        request: CreateBoardFillRequest,
    ) -> Result<BoardFillMutationView, String> {
        let proposed_layer_id = LayerId::new();
        let outcome = self
            .history
            .execute(
                &mut self.bundle.document,
                DocumentCommand::CreateBoardFill {
                    side: request.side,
                    layer_id: proposed_layer_id,
                    name: "基础铺铜".to_owned(),
                    edge_clearance_um: request.edge_clearance_um,
                },
            )
            .map_err(|error| error.to_string())?;
        let CommandOutcome::BoardFillReady { layer_id, created } = outcome else {
            return Err("create board fill returned an unexpected command outcome".to_owned());
        };
        self.map_layer(MapLayerRequest {
            layer_id,
            side: request.side,
            layer: FaceProductionLayer::Copper,
            combine: CombineMode::Add,
        })?;
        Ok(BoardFillMutationView {
            document: self.document_view(),
            layer_id,
            created,
        })
    }

    fn undo(&mut self) -> Result<WorkspaceDocumentView, String> {
        self.history
            .undo(&mut self.bundle.document)
            .map_err(|error| error.to_string())?;
        Ok(self.document_view())
    }

    fn redo(&mut self) -> Result<WorkspaceDocumentView, String> {
        self.history
            .redo(&mut self.bundle.document)
            .map_err(|error| error.to_string())?;
        Ok(self.document_view())
    }
}

fn board_preview(resolved: &ResolvedFabricationBoard) -> Result<BoardPreviewView, String> {
    let textures = resolved
        .preview_textures()
        .map_err(|error| error.to_string())?;
    Ok(BoardPreviewView {
        outline: BoardOutlineView::from(&resolved.outline),
        thickness_um: resolved.stackup.thickness_um,
        fabrication_input_sha256: resolved.build.input_sha256.clone(),
        fabrication_output_sha256: resolved.build.output_sha256.clone(),
        textures: PreviewTexturesView {
            palette: textures.palette,
            front: PreviewTextureView::from_texture(textures.front)?,
            back: PreviewTextureView::from_texture(textures.back)?,
        },
    })
}

fn production_preview(
    resolved: &ResolvedFabricationBoard,
) -> Result<ProductionPreviewView, String> {
    let textures = resolved
        .production_layer_textures()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(ProductionTextureView::from_texture)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProductionPreviewView {
        source: "resolvedFabricationBoard",
        outline: BoardOutlineView::from(&resolved.outline),
        fabrication_input_sha256: resolved.build.input_sha256.clone(),
        fabrication_output_sha256: resolved.build.output_sha256.clone(),
        pixel_pitch_um: resolved.grid.pixel_pitch_um,
        textures,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenProjectRequest {
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewProjectRequest {
    title: String,
    width_um: u32,
    height_um: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveProjectRequest {
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportAssetRequest {
    original_filename: String,
    media_type: String,
    pixel_width: u32,
    pixel_height: u32,
    bytes: Vec<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveProjectAssetRequest {
    asset_id: AssetId,
    folder_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplaceAllAssetReferencesRequest {
    original_asset_id: AssetId,
    replacement_asset_id: AssetId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectAssetIdRequest {
    asset_id: AssetId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImageAssetRequest {
    side: CardSide,
    original_filename: String,
    media_type: String,
    pixel_width: u32,
    pixel_height: u32,
    bytes: Vec<u8>,
    replace_layer_id: Option<LayerId>,
    placement_center_um: Option<ImagePlacementCenterRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImagePlacementCenterRequest {
    x_um: i64,
    y_um: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InsertTreatmentRequest {
    asset_id: AssetId,
    recipe: TreatmentRecipe,
    production_mode: ImageProductionMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetTreatmentRecipeRequest {
    treatment_id: TreatmentId,
    recipe: TreatmentRecipe,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetImageProductionModeRequest {
    treatment_id: TreatmentId,
    production_mode: ImageProductionMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompileTreatmentRequest {
    treatment_id: TreatmentId,
    physical_width_um: u32,
    physical_height_um: u32,
    #[serde(default)]
    pixel_pitch_um: Option<u32>,
    purpose: SamplingPurpose,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BeginImagePreviewSourceRequest {
    #[serde(default)]
    bytes: Option<Vec<u8>>,
    #[serde(default)]
    asset_id: Option<AssetId>,
    #[serde(default)]
    media_type: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BeginImagePreviewSourceView {
    source_handle: TreatmentId,
    source_sha256: String,
    width_px: u32,
    height_px: u32,
    media_type: String,
    workspace_revision: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestImagePreviewRequest {
    source_handle: TreatmentId,
    preview_stream_id: String,
    generation: u64,
    workspace_revision: u64,
    recipe: TreatmentRecipe,
    physical_width_um: u32,
    physical_height_um: u32,
    pixel_pitch_um: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReleaseImagePreviewSourceRequest {
    source_handle: TreatmentId,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImagePreviewDiagnosticsView {
    source_bytes: u64,
    prepare_count: u64,
    proxy_compile_count: u64,
    active_sessions: usize,
    prepared_resident_bytes: usize,
    coalesce_count: u64,
    cancel_count: u64,
    active: usize,
    pending: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmImageImportRequest {
    side: CardSide,
    layer: FaceProductionLayer,
    original_filename: String,
    media_type: String,
    pixel_width: u32,
    pixel_height: u32,
    bytes: Vec<u8>,
    recipe: TreatmentRecipe,
    production_mode: ImageProductionMode,
    placement_center_um: Option<ImagePlacementCenterRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValidateManufacturerRequest {
    profile: ManufacturerProfileSnapshot,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetManufacturerRequest {
    profile: ManufacturerProfileSnapshot,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InsertTextRequest {
    side: CardSide,
    x_um: i64,
    y_um: i64,
    width_um: u32,
    height_um: u32,
    layout: TextLayout,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetTextRequest {
    layer_id: LayerId,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetTextStyleRequest {
    layer_id: LayerId,
    font_family: String,
    font_size_um: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayerIdRequest {
    layer_id: LayerId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayerIdsRequest {
    layer_ids: Vec<LayerId>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayerNameRequest {
    layer_id: LayerId,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransformLayerRequest {
    layer_id: LayerId,
    transform: TransformUm,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransformLayersRequest {
    transforms: Vec<LayerTransform>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayerFlagRequest {
    layer_id: LayerId,
    value: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReorderLayerRequest {
    layer_id: LayerId,
    new_parent_id: Option<LayerId>,
    new_index: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveLayerRequest {
    layer_id: LayerId,
    new_parent_id: Option<LayerId>,
    new_index: usize,
    side: CardSide,
    from_layer: FaceProductionLayer,
    to_layer: FaceProductionLayer,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferLayersRequest {
    layer_ids: Vec<LayerId>,
    target_side: CardSide,
    target_layer: FaceProductionLayer,
    new_parent_id: Option<LayerId>,
    new_index: usize,
    mode: LayerTransferMode,
    offset_um: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasteLayersRequest {
    layers: Vec<ContentLayer>,
    mappings: Vec<ProductionMapping>,
    target_side: CardSide,
    target_layer: FaceProductionLayer,
    new_parent_id: Option<LayerId>,
    new_index: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupLayersRequest {
    side: CardSide,
    name: String,
    layer_ids: Vec<LayerId>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MapLayerRequest {
    layer_id: LayerId,
    side: CardSide,
    layer: FaceProductionLayer,
    combine: CombineMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnmapLayerRequest {
    mapping_id: MappingId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetStackupRequest {
    stackup: StackupPreset,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetBoardOutlineRequest {
    outline: BoardOutlineView,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBoardFillRequest {
    side: CardSide,
    edge_clearance_um: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LayerMutationView {
    document: WorkspaceDocumentView,
    layer_id: LayerId,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LayersMutationView {
    document: WorkspaceDocumentView,
    layer_ids: Vec<LayerId>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetMutationView {
    document: WorkspaceDocumentView,
    asset_id: AssetId,
    reused: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetReferencesMutationView {
    document: WorkspaceDocumentView,
    original_asset_id: AssetId,
    replacement_asset_id: AssetId,
    replaced_instance_count: usize,
    replaced_treatment_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetFolderMutationView {
    document: WorkspaceDocumentView,
    asset_id: AssetId,
    folder_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetDeletionMutationView {
    document: WorkspaceDocumentView,
    deleted_asset_id: AssetId,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetCleanupMutationView {
    document: WorkspaceDocumentView,
    removed_asset_ids: Vec<AssetId>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TreatmentMutationView {
    document: WorkspaceDocumentView,
    treatment_id: TreatmentId,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmedImageImportView {
    document: WorkspaceDocumentView,
    asset_id: AssetId,
    treatment_id: TreatmentId,
    layer_id: LayerId,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BoardFillMutationView {
    document: WorkspaceDocumentView,
    layer_id: LayerId,
    created: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetBytesView {
    media_type: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TreatmentCompileView {
    width_px: u32,
    height_px: u32,
    applied_threshold: u8,
    mask_sha256: String,
    preview_png_data_url: String,
    pixel_pitch_um: u32,
    recipe_fingerprint: String,
    revision: u64,
    purpose: SamplingPurpose,
    topology: atelier_core::MaskTopology,
    diagnostics: Vec<atelier_core::TreatmentDiagnostic>,
}

impl TreatmentCompileView {
    fn from_compiled(compiled: &CompiledImageTreatment) -> Result<Self, String> {
        let mut rgba = Vec::with_capacity(
            usize::try_from(
                u64::from(compiled.mask.width_px()) * u64::from(compiled.mask.height_px()) * 4,
            )
            .map_err(|_| "image treatment preview dimensions overflow".to_owned())?,
        );
        for y in 0..compiled.mask.height_px() {
            for x in 0..compiled.mask.width_px() {
                if compiled.mask.get(x, y).map_err(|error| error.to_string())? {
                    rgba.extend_from_slice(&[255, 255, 255, 255]);
                } else {
                    rgba.extend_from_slice(&[0, 0, 0, 0]);
                }
            }
        }
        Ok(Self {
            width_px: compiled.mask.width_px(),
            height_px: compiled.mask.height_px(),
            applied_threshold: compiled.applied_threshold,
            mask_sha256: compiled.mask.sha256(),
            preview_png_data_url: rgba_png_data_url(
                compiled.mask.width_px(),
                compiled.mask.height_px(),
                rgba,
            )?,
            pixel_pitch_um: compiled.pixel_pitch_um,
            recipe_fingerprint: compiled.recipe_fingerprint.clone(),
            revision: compiled.revision,
            purpose: compiled.purpose,
            topology: compiled.topology,
            diagnostics: compiled.diagnostics.clone(),
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManufacturerValidationView {
    profile: ManufacturerProfileSnapshot,
    valid: bool,
    errors: Vec<String>,
}

impl ManufacturerValidationView {
    fn from_profile(profile: ManufacturerProfileSnapshot) -> Self {
        let errors = profile.validate().err().unwrap_or_default();
        Self {
            profile,
            valid: errors.is_empty(),
            errors,
        }
    }
}

#[tauri::command]
async fn workspace_invoke(
    request: WorkspaceBridgeRequest,
    service: tauri::State<'_, Mutex<WorkspaceService>>,
) -> Result<WorkspaceBridgeResponse, String> {
    if WorkspaceService::should_use_read_snapshot(&request.command) {
        let mut snapshot = service
            .lock()
            .map_err(|_| "workspace service lock is poisoned".to_owned())?
            .snapshot_for_read();
        return tauri::async_runtime::spawn_blocking(move || snapshot.invoke(request))
            .await
            .map_err(|error| format!("workspace preview task failed: {error}"));
    }
    invoke_workspace_service(&service, request)
}

fn invoke_workspace_service(
    service: &Mutex<WorkspaceService>,
    request: WorkspaceBridgeRequest,
) -> Result<WorkspaceBridgeResponse, String> {
    if WorkspaceService::should_use_read_snapshot(&request.command) {
        let mut snapshot = service
            .lock()
            .map_err(|_| "workspace service lock is poisoned".to_owned())?
            .snapshot_for_read();
        return Ok(snapshot.invoke(request));
    }
    let mut service = service
        .lock()
        .map_err(|_| "workspace service lock is poisoned".to_owned())?;
    Ok(service.invoke(request))
}

#[tauri::command]
fn open_easyeda_project(path: PathBuf) -> Result<(), String> {
    let path = validated_easyeda_project_path(&path)?;
    open_with_system(&path)
}

#[tauri::command]
fn reveal_exported_project(path: PathBuf) -> Result<(), String> {
    let path = path
        .canonicalize()
        .map_err(|error| format!("导出文件不存在或无法访问：{error}"))?;
    reveal_with_system(&path)
}

#[tauri::command]
fn read_image_file(path: PathBuf) -> Result<NativeImageFileView, String> {
    read_supported_image_file(&path)
}

fn read_supported_image_file(path: &Path) -> Result<NativeImageFileView, String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    let bytes = std::fs::read(path).map_err(|error| format!("无法读取图片文件：{error}"))?;
    let media_type = match (extension.as_deref(), supported_image_media_type(&bytes)) {
        (Some("png"), Some("image/png")) => "image/png",
        (Some("jpg" | "jpeg"), Some("image/jpeg")) => "image/jpeg",
        (Some("webp"), Some("image/webp")) => "image/webp",
        _ => return Err("仅支持有效的 PNG、JPEG 或 WebP 图片".to_owned()),
    };
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "图片文件名无效".to_owned())?
        .to_owned();
    Ok(NativeImageFileView {
        name,
        media_type: media_type.to_owned(),
        bytes,
    })
}

fn supported_image_media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

fn validated_easyeda_project_path(path: &Path) -> Result<PathBuf, String> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("eprj2") {
        return Err("只能使用嘉立创 EDA 打开 .eprj2 工程".to_owned());
    }
    path.canonicalize()
        .map_err(|error| format!("嘉立创 EDA 工程不存在或无法访问：{error}"))
}

fn open_with_system(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");

    command
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法使用系统关联程序打开嘉立创 EDA 工程：{error}"))
}

fn reveal_with_system(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg("-R");
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg("/select,");
        command
    };
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");

    #[cfg(target_os = "linux")]
    let target = path.parent().unwrap_or(path);
    #[cfg(not(target_os = "linux"))]
    let target = path;

    command
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法在文件管理器中显示导出工程：{error}"))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(WorkspaceService::new(
            initial_workspace_document(),
        )))
        .invoke_handler(tauri::generate_handler![
            workspace_invoke,
            open_easyeda_project,
            reveal_exported_project,
            read_image_file
        ])
        .run(tauri::generate_context!())
        .expect("failed to run PCB Atelier desktop application");
}

fn face_layers(document: &AtelierDocument, side: CardSide) -> &[ContentLayer] {
    match side {
        CardSide::Front => &document.front.layers,
        CardSide::Back => &document.back.layers,
    }
}

fn find_layer(document: &AtelierDocument, layer_id: LayerId) -> Option<&ContentLayer> {
    document
        .front
        .layers
        .iter()
        .chain(document.back.layers.iter())
        .find(|layer| layer.id == layer_id)
}

fn validate_color_original_asset(
    bundle: &ProjectBundle,
    asset_id: AssetId,
    production_mode: ImageProductionMode,
) -> Result<(), String> {
    if production_mode != ImageProductionMode::ColorOriginal {
        return Ok(());
    }
    let asset = bundle
        .document
        .assets
        .iter()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| format!("彩色丝印素材不存在：{asset_id}"))?;
    let bytes = bundle
        .asset_bytes(asset_id)
        .ok_or_else(|| format!("彩色丝印素材缺少嵌入原图字节：{asset_id}"))?;
    let supported = matches!(
        (asset.media_type.as_str(), image::guess_format(bytes)),
        ("image/png", Ok(image::ImageFormat::Png)) | ("image/jpeg", Ok(image::ImageFormat::Jpeg))
    );
    if supported {
        Ok(())
    } else {
        Err(format!(
            "彩色原图生产仅支持声明类型与实际字节一致的 PNG/JPEG 素材；请重新导入素材 {asset_id} 或改用标准单色丝印"
        ))
    }
}

fn fit_image_transform(
    document: &AtelierDocument,
    pixel_width: u32,
    pixel_height: u32,
) -> TransformUm {
    let max_width = document.board.width_um() * 4 / 5;
    let max_height = document.board.height_um() * 4 / 5;
    let (width_um, height_um) = if u64::from(max_width) * u64::from(pixel_height)
        <= u64::from(max_height) * u64::from(pixel_width)
    {
        (
            max_width,
            u32::try_from(u64::from(max_width) * u64::from(pixel_height) / u64::from(pixel_width))
                .unwrap_or(max_height)
                .max(1),
        )
    } else {
        (
            u32::try_from(u64::from(max_height) * u64::from(pixel_width) / u64::from(pixel_height))
                .unwrap_or(max_width)
                .max(1),
            max_height,
        )
    };
    TransformUm::rect(
        i64::from(document.board.width_um() - width_um) / 2,
        i64::from(document.board.height_um() - height_um) / 2,
        width_um,
        height_um,
    )
}

#[cfg(test)]
mod tests {
    use atelier_core::{
        BoardOutline, CardSide, CombineMode, ContentLayer, DocumentCommand, FaceProductionLayer,
        ProductionMapping, ProductionTarget, ProjectBundle, ProjectBundleRasterizer,
        SolderMaskColor, SurfaceFinish, TransformUm, compile_fabrication_plan,
        resolve_fabrication_plan,
    };

    #[test]
    fn core_info_comes_from_the_domain_crate() {
        let info = super::core_info();

        assert_eq!(info.project_format, atelier_core::PROJECT_FORMAT);
        assert_eq!(info.schema_version, atelier_core::PROJECT_SCHEMA_VERSION);
    }

    #[test]
    fn color_original_bridge_validation_checks_real_asset_bytes() {
        let mut bundle = ProjectBundle::new(atelier_core::AtelierDocument::new_card(
            "彩色素材",
            10_000,
            10_000,
        ));
        let asset_id = bundle
            .embed_asset("disguised.png", "image/png", 10, 10, b"<svg/>".to_vec())
            .expect("embed non-empty fixture");

        let error = super::validate_color_original_asset(
            &bundle,
            asset_id,
            atelier_core::ImageProductionMode::ColorOriginal,
        )
        .expect_err("declared PNG with SVG bytes must be rejected");

        assert!(error.contains("实际字节"));
        assert!(error.contains("PNG/JPEG"));
    }

    #[test]
    fn workspace_projection_reads_dimensions_from_the_domain_document() {
        let document = atelier_core::AtelierDocument::new_card("投影验证", 72_345, 48_210);

        let history = atelier_core::CommandHistory::default();
        let projection = super::workspace_document_view(&document, &history);

        assert_eq!(projection.title, document.title);
        assert_eq!(projection.board.width_um, document.board.width_um());
        assert_eq!(projection.board.height_um, document.board.height_um());
        assert_eq!(
            projection.faces.front_layer_count,
            document.front.layers.len()
        );
        assert_eq!(
            projection.faces.back_layer_count,
            document.back.layers.len()
        );
        assert_eq!(projection.stackup, document.stackup);
    }

    #[test]
    fn image_insertion_fits_and_centers_on_the_requested_face() {
        let mut session = super::WorkspaceSession::new(atelier_core::AtelierDocument::new_card(
            "图片", 64_000, 100_000,
        ));

        let result = session
            .insert_image(super::ImageAssetRequest {
                side: CardSide::Back,
                original_filename: "wide.png".to_owned(),
                media_type: "image/png".to_owned(),
                pixel_width: 2_000,
                pixel_height: 1_000,
                bytes: vec![1, 2, 3],
                replace_layer_id: None,
                placement_center_um: None,
            })
            .expect("image insertion should succeed");
        let layer = session
            .bundle
            .document
            .back
            .layers
            .iter()
            .find(|layer| layer.id == result.layer_id)
            .expect("inserted layer should exist");

        assert_eq!(layer.transform.width_um, 51_200);
        assert_eq!(layer.transform.height_um, 25_600);
        assert_eq!(layer.transform.x_um, 6_400);
        assert_eq!(layer.transform.y_um, 37_200);
        assert!(session.bundle.document.front.layers.is_empty());
    }

    #[test]
    fn image_replacement_preserves_layer_identity_transform_parent_and_mapping() {
        let mut session = super::WorkspaceSession::new(atelier_core::AtelierDocument::new_card(
            "替换", 64_000, 100_000,
        ));
        let inserted = session
            .insert_image(super::ImageAssetRequest {
                side: CardSide::Front,
                original_filename: "before.png".to_owned(),
                media_type: "image/png".to_owned(),
                pixel_width: 200,
                pixel_height: 400,
                bytes: vec![1],
                replace_layer_id: None,
                placement_center_um: None,
            })
            .expect("initial insert should succeed");
        let original = session.bundle.document.front.layers[0].clone();
        session
            .bundle
            .document
            .mappings
            .push(ProductionMapping::new(
                inserted.layer_id,
                ProductionTarget::new(CardSide::Front, FaceProductionLayer::Silkscreen),
                CombineMode::Add,
            ));

        session
            .insert_image(super::ImageAssetRequest {
                side: CardSide::Back,
                original_filename: "after.png".to_owned(),
                media_type: "image/png".to_owned(),
                pixel_width: 800,
                pixel_height: 200,
                bytes: vec![2],
                replace_layer_id: Some(inserted.layer_id),
                placement_center_um: None,
            })
            .expect("replacement should succeed");
        let replaced = &session.bundle.document.front.layers[0];

        assert_eq!(replaced.id, original.id);
        assert_eq!(replaced.transform, original.transform);
        assert_eq!(replaced.parent_id, original.parent_id);
        assert_eq!(
            session.bundle.document.mappings[0].source_layer_id,
            original.id
        );
    }

    #[test]
    fn text_session_uses_domain_commands_for_insert_and_content_update() {
        let mut session = super::WorkspaceSession::new(atelier_core::AtelierDocument::new_card(
            "文字", 64_000, 100_000,
        ));
        let inserted = session
            .insert_text(super::InsertTextRequest {
                side: CardSide::Front,
                x_um: 7_000,
                y_um: 9_000,
                width_um: 24_000,
                height_um: 12_000,
                layout: atelier_core::TextLayout::FixedFrame,
            })
            .expect("text insertion should succeed");

        session
            .set_text(super::SetTextRequest {
                layer_id: inserted.layer_id,
                text: "新的文字".to_owned(),
            })
            .expect("text update should succeed");
        session
            .set_text_style(super::SetTextStyleRequest {
                layer_id: inserted.layer_id,
                font_family: "PingFang SC".to_owned(),
                font_size_um: 6_500,
            })
            .expect("text style update should succeed");
        let layer = &session.bundle.document.front.layers[0];
        let atelier_core::ContentKind::Text(text) = &layer.kind else {
            panic!("inserted layer should be text");
        };

        assert_eq!(text.text, "新的文字");
        assert_eq!(text.layout, atelier_core::TextLayout::FixedFrame);
        assert_eq!(text.font_family, "PingFang SC");
        assert_eq!(text.font_size_um, 6_500);
        assert_eq!(
            layer.transform,
            atelier_core::TransformUm::rect(7_000, 9_000, 24_000, 12_000)
        );
        assert_eq!(session.history.undo_depth(), 3);
    }

    #[test]
    fn system_font_catalog_keeps_the_embedded_fallback_available() {
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "字体", 64_000, 100_000,
        ));
        let response = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "get_system_fonts".to_owned(),
            args: serde_json::json!({}),
        });

        assert!(response.error.is_none());
        assert_eq!(response.payload["fallbackFamily"], "sans-serif");
        assert_eq!(response.payload["families"][0], "sans-serif");
    }

    #[test]
    fn workspace_session_exposes_edit_commands_and_history_availability() {
        let mut session = super::WorkspaceSession::new(atelier_core::AtelierDocument::new_card(
            "交互命令",
            64_000,
            100_000,
        ));
        let first = session
            .insert_text(super::InsertTextRequest {
                side: CardSide::Front,
                x_um: 1_000,
                y_um: 2_000,
                width_um: 10_000,
                height_um: 5_000,
                layout: atelier_core::TextLayout::AutoWidth,
            })
            .expect("insert first");
        let second = session
            .insert_text(super::InsertTextRequest {
                side: CardSide::Front,
                x_um: 3_000,
                y_um: 4_000,
                width_um: 10_000,
                height_um: 5_000,
                layout: atelier_core::TextLayout::AutoWidth,
            })
            .expect("insert second");

        session
            .execute(DocumentCommand::TransformLayer {
                layer_id: first.layer_id,
                transform: TransformUm::rect(9_000, 8_000, 12_000, 6_000),
            })
            .expect("transform");
        session
            .execute(DocumentCommand::SetLayerVisibility {
                layer_id: second.layer_id,
                visible: false,
            })
            .expect("visibility");
        let grouped = session
            .group(super::GroupLayersRequest {
                side: CardSide::Front,
                name: "组合".to_owned(),
                layer_ids: vec![first.layer_id, second.layer_id],
            })
            .expect("group");
        session
            .execute(DocumentCommand::UngroupLayer {
                group_id: grouped.layer_id,
            })
            .expect("ungroup");

        let view = session.document_view();
        assert!(view.history.can_undo);
        assert!(!view.history.can_redo);
        session.undo().expect("undo");
        let view = session.document_view();
        assert!(view.history.can_undo);
        assert!(view.history.can_redo);
        session.redo().expect("redo");
        assert!(!session.document_view().history.can_redo);
    }

    #[test]
    fn board_fill_is_created_on_and_mapped_to_the_requested_copper_face() {
        let mut session = super::WorkspaceSession::new(atelier_core::AtelierDocument::new_card(
            "铺铜", 64_000, 100_000,
        ));

        let result = session
            .create_board_fill(super::CreateBoardFillRequest {
                side: CardSide::Back,
                edge_clearance_um: 500,
            })
            .expect("create board fill");

        assert!(result.created);
        assert!(session.bundle.document.front.layers.is_empty());
        let layer = &session.bundle.document.back.layers[0];
        assert_eq!(layer.id, result.layer_id);
        let atelier_core::ContentKind::BoardFill(fill) = &layer.kind else {
            panic!("expected board fill");
        };
        assert_eq!(fill.edge_clearance_um, 500);
        assert_eq!(
            session.bundle.document.mappings[0].target,
            ProductionTarget::new(CardSide::Back, FaceProductionLayer::Copper)
        );

        let undo_depth = session.history.undo_depth();
        let repeated = session
            .create_board_fill(super::CreateBoardFillRequest {
                side: CardSide::Back,
                edge_clearance_um: 500,
            })
            .expect("reuse board fill");
        assert!(!repeated.created);
        assert_eq!(repeated.layer_id, result.layer_id);
        assert_eq!(session.bundle.document.back.layers.len(), 1);
        assert_eq!(session.bundle.document.mappings.len(), 1);
        assert_eq!(session.history.undo_depth(), undo_depth);
    }

    #[test]
    fn board_outline_and_content_diagnostics_round_trip_through_the_service_view() {
        let mut session = super::WorkspaceSession::new(atelier_core::AtelierDocument::new_card(
            "板框", 20_000, 20_000,
        ));
        session
            .insert_text(super::InsertTextRequest {
                side: CardSide::Front,
                x_um: 9_000,
                y_um: 1_000,
                width_um: 5_000,
                height_um: 3_000,
                layout: atelier_core::TextLayout::FixedFrame,
            })
            .expect("insert content");

        session
            .set_board_outline(BoardOutline::RoundedRectangle {
                width_um: 10_000,
                height_um: 8_000,
                corner_radius_um: 1_250,
            })
            .expect("set board outline");
        let view = session.document_view();

        assert_eq!(view.board.width_um, 10_000);
        assert_eq!(view.board.height_um, 8_000);
        assert_eq!(view.board.corner_radius_um, 1_250);
        assert_eq!(
            view.board.outline,
            super::BoardOutlineView::RoundedRectangle {
                width_um: 10_000,
                height_um: 8_000,
                corner_radius_um: 1_250,
            }
        );
        assert_eq!(view.board.diagnostics.len(), 1);
        assert!(matches!(
            &view.board.diagnostics[0],
            super::DocumentDiagnosticView::ContentOutsideBoard {
                side: CardSide::Front,
                ..
            }
        ));

        session.undo().expect("undo outline");
        assert_eq!(session.document_view().board.width_um, 20_000);
        session.redo().expect("redo outline");
        assert_eq!(session.document_view().board.corner_radius_um, 1_250);
    }

    #[test]
    fn desktop_export_uses_the_session_bundle_and_matches_core_and_cli_geometry_hashes() {
        let session = asymmetric_export_session();
        let plan = compile_fabrication_plan(&session.bundle.document).expect("compile directly");
        let mut rasterizer = ProjectBundleRasterizer::new(&session.bundle).expect("embedded font");
        let direct =
            resolve_fabrication_plan(&plan, 25, &mut rasterizer).expect("resolve directly");
        let temp = tempfile::tempdir().expect("temporary directory");

        let report = session
            .export_easyeda(temp.path())
            .expect("desktop command path export");
        let project_path = temp.path().join("shared.pcba");
        session
            .bundle
            .save(&project_path)
            .expect("save shared fixture");
        let cli_json = atelier_cli::execute(&[
            "production-inspect".to_owned(),
            project_path.display().to_string(),
        ])
        .expect("CLI inspect");
        let cli: serde_json::Value = serde_json::from_str(&cli_json).expect("CLI JSON");

        assert_eq!(report.fabrication_input_sha256, direct.build.input_sha256);
        assert_eq!(report.fabrication_output_sha256, direct.build.output_sha256);
        assert_eq!(
            report.fabrication_output_sha256,
            cli["build"]["outputSha256"]
                .as_str()
                .expect("CLI output hash")
        );
        assert_eq!(report.primitives.filled_layer_ids, vec![1, 2, 3, 4, 5, 6]);
        assert!(report.native_validation.is_valid);
    }

    #[test]
    fn desktop_export_reports_the_failing_stage_and_destination() {
        let session = asymmetric_export_session();
        let temp = tempfile::tempdir().expect("temporary directory");
        let invalid_directory = temp.path().join("already-a-file");
        std::fs::write(&invalid_directory, b"occupied").expect("create invalid destination");

        let error = session
            .export_easyeda(&invalid_directory)
            .expect_err("a file cannot be an output directory");

        assert!(error.contains("嘉立创 EDA 导出失败"));
        assert!(error.contains(&invalid_directory.display().to_string()));
    }

    #[test]
    fn unmap_and_export_enabled_use_history_and_reject_locked_sources() {
        let mut session = super::WorkspaceSession::new(atelier_core::AtelierDocument::new_card(
            "生产设置",
            64_000,
            100_000,
        ));
        let layer = ContentLayer::new_text(
            "source",
            "F",
            TransformUm::rect(1_000, 2_000, 8_000, 10_000),
        );
        let layer_id = layer.id;
        session.bundle.document.front.layers.push(layer);
        let mapping = ProductionMapping::new(
            layer_id,
            ProductionTarget::new(CardSide::Front, FaceProductionLayer::Silkscreen),
            CombineMode::Add,
        );
        let mapping_id = mapping.id;
        session.bundle.document.mappings.push(mapping);

        session
            .set_layer_export_enabled(layer_id, false)
            .expect("disable export");
        assert!(!session.bundle.document.front.layers[0].export_enabled);
        session.undo().expect("undo export flag");
        assert!(session.bundle.document.front.layers[0].export_enabled);
        session.redo().expect("redo export flag");
        assert!(!session.bundle.document.front.layers[0].export_enabled);

        session.unmap_layer(mapping_id).expect("remove mapping");
        assert!(session.bundle.document.mappings.is_empty());
        session.undo().expect("undo mapping removal");
        assert_eq!(session.bundle.document.mappings[0].id, mapping_id);
        session.redo().expect("redo mapping removal");
        assert!(session.bundle.document.mappings.is_empty());

        session.undo().expect("restore mapping before lock test");
        session.bundle.document.front.layers[0].locked = true;
        assert!(
            session
                .unmap_layer(mapping_id)
                .unwrap_err()
                .contains("locked")
        );
        assert!(
            session
                .set_layer_export_enabled(layer_id, true)
                .unwrap_err()
                .contains("locked")
        );
        assert_eq!(session.bundle.document.mappings.len(), 1);
        assert!(!session.bundle.document.front.layers[0].export_enabled);
    }

    #[test]
    fn stackup_changes_round_trip_through_undo_and_redo() {
        let mut session = super::WorkspaceSession::new(atelier_core::AtelierDocument::new_card(
            "叠层", 64_000, 100_000,
        ));
        let original = session.bundle.document.stackup.clone();
        let mut changed = original.clone();
        changed.solder_mask_color = SolderMaskColor::Red;
        changed.surface_finish = SurfaceFinish::HaslLeadFree;

        session
            .set_stackup(changed.clone())
            .expect("change stackup");
        assert_eq!(session.document_view().stackup, changed);
        session.undo().expect("undo stackup");
        assert_eq!(session.document_view().stackup, original);
        session.redo().expect("redo stackup");
        assert_eq!(session.document_view().stackup, changed);
    }

    #[test]
    fn workspace_service_contract_is_versioned_and_preview_comes_from_current_document() {
        let document = atelier_core::AtelierDocument::new_card("Bridge contract", 8_000, 12_000);
        let bundle = ProjectBundle::new(document.clone());
        let plan = compile_fabrication_plan(&bundle.document).expect("compile current document");
        let mut rasterizer = ProjectBundleRasterizer::new(&bundle).expect("embedded font");
        let resolved =
            resolve_fabrication_plan(&plan, super::INTERACTIVE_PREVIEW_PITCH_UM, &mut rasterizer)
                .expect("resolve current document");
        let mut service = super::WorkspaceService::new(document.clone());
        let document_response = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "get_workspace_document".to_owned(),
            args: serde_json::json!({}),
        });
        let preview = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "get_board_preview".to_owned(),
            args: serde_json::json!({}),
        });

        assert_eq!(
            document_response.contract_version,
            super::WORKSPACE_CONTRACT_VERSION
        );
        assert_eq!(document_response.revision, 0);
        assert!(document_response.error.is_none());
        assert_eq!(preview.payload["textures"]["front"]["widthPx"], 40);
        assert_eq!(preview.payload["textures"]["front"]["heightPx"], 60);
        assert_ne!(preview.payload["textures"]["front"]["widthPx"], 8);
        assert_ne!(preview.payload["textures"]["front"]["heightPx"], 5);
        assert_eq!(
            preview.payload["fabricationInputSha256"],
            resolved.build.input_sha256
        );
        assert_eq!(
            preview.payload["fabricationOutputSha256"],
            resolved.build.output_sha256
        );
        let incompatible = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: "pcb-atelier-workspace-v0".to_owned(),
            command: "get_workspace_document".to_owned(),
            args: serde_json::json!({}),
        });
        assert_eq!(incompatible.revision, 0);
        assert_eq!(incompatible.payload, serde_json::Value::Null);
        assert!(
            incompatible
                .error
                .as_deref()
                .expect("version mismatch error")
                .contains("unsupported workspace contract version")
        );

        let tauri_service = std::sync::Mutex::new(super::WorkspaceService::new(document));
        let tauri_response = super::invoke_workspace_service(
            &tauri_service,
            super::WorkspaceBridgeRequest {
                contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
                command: "get_board_preview".to_owned(),
                args: serde_json::json!({}),
            },
        )
        .expect("thin Tauri adapter");
        assert_eq!(tauri_response, preview);
    }

    #[test]
    fn preview_transport_uses_png_data_urls_and_reflects_solder_mask_color_changes() {
        use base64::Engine as _;

        fn first_pixel(data_url: &str) -> image::Rgba<u8> {
            let encoded = data_url
                .strip_prefix("data:image/png;base64,")
                .expect("PNG data URL prefix");
            let png = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .expect("base64 PNG");
            image::load_from_memory_with_format(&png, image::ImageFormat::Png)
                .expect("decode PNG")
                .to_rgba8()
                .get_pixel(0, 0)
                .to_owned()
        }

        let mut document = atelier_core::AtelierDocument::new_card("Mask preview", 2_000, 2_000);
        document.stackup.solder_mask_color = SolderMaskColor::Black;
        document.manufacturer_profile.solder_mask = SolderMaskColor::Black;
        let mut white_stackup = document.stackup.clone();
        white_stackup.solder_mask_color = SolderMaskColor::White;
        let mut white_profile = document.manufacturer_profile.clone();
        white_profile.solder_mask = SolderMaskColor::White;
        let mut service = super::WorkspaceService::new(document);
        let preview_request = || super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "get_board_preview".to_owned(),
            args: serde_json::json!({}),
        };

        let black = service.invoke(preview_request());
        let black_texture = &black.payload["textures"]["front"];
        assert!(black_texture.get("rgba").is_none());
        let black_pixel = first_pixel(
            black_texture["pngDataUrl"]
                .as_str()
                .expect("front PNG data URL"),
        );

        let changed = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "set_stackup".to_owned(),
            args: serde_json::json!({ "request": { "stackup": white_stackup } }),
        });
        assert!(changed.error.is_none());
        let changed = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "set_manufacturer_profile".to_owned(),
            args: serde_json::json!({ "request": { "profile": white_profile } }),
        });
        assert!(changed.error.is_none());

        let white = service.invoke(preview_request());
        let white_pixel = first_pixel(
            white.payload["textures"]["front"]["pngDataUrl"]
                .as_str()
                .expect("front PNG data URL"),
        );
        assert_ne!(black_pixel, white_pixel);
        assert!(white_pixel.0[0] > black_pixel.0[0]);
        assert_eq!(
            service.preview_resolution_count(),
            2,
            "stackup mutation must invalidate the cached resolved board"
        );
    }

    #[test]
    fn repeated_board_fill_request_is_idempotent_for_revision_and_identity() {
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "Board fill bridge",
            8_000,
            12_000,
        ));
        let request = || super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "create_board_fill".to_owned(),
            args: serde_json::json!({
                "request": {
                    "side": "front",
                    "edgeClearanceUm": 500
                }
            }),
        };

        let first = service.invoke(request());
        let repeated = service.invoke(request());

        assert!(first.error.is_none());
        assert_eq!(first.revision, 1);
        assert_eq!(first.payload["created"], true);
        assert_eq!(repeated.error, None);
        assert_eq!(repeated.revision, 1);
        assert_eq!(repeated.payload["created"], false);
        assert_eq!(repeated.payload["layerId"], first.payload["layerId"]);
        assert_eq!(
            repeated.payload["document"]["frontLayers"]
                .as_array()
                .expect("front layers")
                .len(),
            1
        );
        assert_eq!(
            repeated.payload["document"]["mappings"]
                .as_array()
                .expect("mappings")
                .len(),
            1
        );
    }

    #[test]
    fn board_outline_transport_accepts_camel_case_fields_and_returns_diagnostics() {
        let mut document =
            atelier_core::AtelierDocument::new_card("Board outline bridge", 64_000, 100_000);
        document.front.layers.push(ContentLayer::new_text(
            "越界对象",
            "EDGE",
            TransformUm::rect(20_000, 4_000, 20_000, 5_000),
        ));
        let mut service = super::WorkspaceService::new(document);

        let response = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "set_board_outline".to_owned(),
            args: serde_json::json!({
                "request": {
                    "outline": {
                        "type": "roundedRectangle",
                        "widthUm": 30_000,
                        "heightUm": 100_000,
                        "cornerRadiusUm": 2_000
                    }
                }
            }),
        });

        assert!(response.error.is_none(), "{:?}", response.error);
        assert_eq!(response.revision, 1);
        assert_eq!(response.payload["board"]["widthUm"], 30_000);
        assert_eq!(response.payload["board"]["cornerRadiusUm"], 2_000);
        assert_eq!(
            response.payload["board"]["diagnostics"][0]["kind"],
            "contentOutsideBoard"
        );
        assert!(
            response.payload["board"]["diagnostics"][0]["layerId"]
                .as_str()
                .is_some()
        );
    }

    #[test]
    fn versioned_service_executes_image_group_ungroup_and_preview_on_one_document() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("encode test PNG");
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "Editing bridge",
            20_000,
            20_000,
        ));
        let mut invoke = |command: &str, args: serde_json::Value| {
            service.invoke(super::WorkspaceBridgeRequest {
                contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
                command: command.to_owned(),
                args,
            })
        };

        let image = invoke(
            "insert_image_asset",
            serde_json::json!({
                "request": {
                    "side": "front",
                    "originalFilename": "pixel.png",
                    "mediaType": "image/png",
                    "pixelWidth": 2,
                    "pixelHeight": 2,
                    "bytes": png.into_inner(),
                    "replaceLayerId": null
                }
            }),
        );
        assert!(image.error.is_none());
        assert_eq!(image.revision, 1);
        let image_layer_id = image.payload["layerId"].clone();

        let text = invoke(
            "insert_text_layer",
            serde_json::json!({
                "request": {
                    "side": "front",
                    "xUm": 1_000,
                    "yUm": 1_000,
                    "widthUm": 4_000,
                    "heightUm": 2_000,
                    "layout": "fixedFrame"
                }
            }),
        );
        assert!(text.error.is_none());
        assert_eq!(text.revision, 2);
        let text_layer_id = text.payload["layerId"].clone();

        let group = invoke(
            "group_layers",
            serde_json::json!({
                "request": {
                    "side": "front",
                    "name": "组合",
                    "layerIds": [image_layer_id, text_layer_id]
                }
            }),
        );
        assert!(group.error.is_none());
        assert_eq!(group.revision, 3);

        let ungroup = invoke(
            "ungroup_layer",
            serde_json::json!({
                "request": {
                    "layerId": group.payload["layerId"]
                }
            }),
        );
        assert!(ungroup.error.is_none());
        assert_eq!(ungroup.revision, 4);

        let preview = invoke("get_board_preview", serde_json::json!({}));
        assert!(preview.error.is_none());
        assert_eq!(preview.revision, 4);
        assert_eq!(preview.payload["textures"]["front"]["widthPx"], 100);
        assert_eq!(preview.payload["textures"]["front"]["heightPx"], 100);
        assert_eq!(
            preview.payload["fabricationOutputSha256"]
                .as_str()
                .expect("preview hash")
                .len(),
            64
        );
    }

    #[test]
    fn image_bridge_places_a_new_instance_at_the_requested_board_point() {
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "Drop point",
            64_000,
            100_000,
        ));
        let response = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "insert_image_asset".to_owned(),
            args: serde_json::json!({
                "request": {
                    "side": "back",
                    "originalFilename": "drop.png",
                    "mediaType": "image/png",
                    "pixelWidth": 4,
                    "pixelHeight": 2,
                    "bytes": [1, 2, 3],
                    "replaceLayerId": null,
                    "placementCenterUm": {
                        "xUm": 17_250,
                        "yUm": 63_750
                    }
                }
            }),
        });

        assert_eq!(response.error, None);
        let transform = &response.payload["document"]["backLayers"][0]["transform"];
        let center_x =
            transform["xUm"].as_i64().unwrap() + transform["widthUm"].as_i64().unwrap() / 2;
        let center_y =
            transform["yUm"].as_i64().unwrap() + transform["heightUm"].as_i64().unwrap() / 2;
        assert_eq!((center_x, center_y), (17_250, 63_750));
    }

    #[test]
    fn opening_project_replaces_the_session_only_after_bundle_validation() {
        let directory = tempfile::tempdir().expect("temporary project directory");
        let project_path = directory.path().join("opened.pcba");
        let opened_document = atelier_core::AtelierDocument::new_card("已打开工程", 48_000, 72_000);
        atelier_core::ProjectBundle::new(opened_document)
            .save(&project_path)
            .expect("save project fixture");
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "原工程",
            8_000,
            12_000,
        ));

        let opened = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "open_project".to_owned(),
            args: serde_json::json!({
                "request": { "path": project_path }
            }),
        });

        assert!(opened.error.is_none());
        assert_eq!(opened.revision, 1);
        assert_eq!(opened.payload["title"], "已打开工程");
        assert_eq!(opened.payload["board"]["widthUm"], 48_000);
        assert_eq!(opened.payload["history"]["canUndo"], false);

        let invalid_path = directory.path().join("invalid.pcba");
        std::fs::write(&invalid_path, b"not a project").expect("write invalid fixture");
        let rejected = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "open_project".to_owned(),
            args: serde_json::json!({
                "request": { "path": invalid_path }
            }),
        });
        assert!(rejected.error.is_some());
        assert_eq!(rejected.revision, 1);

        let current = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "get_workspace_document".to_owned(),
            args: serde_json::json!({}),
        });
        assert_eq!(current.payload["title"], "已打开工程");
    }

    #[test]
    fn new_project_can_be_saved_and_reopened_as_a_valid_bundle() {
        let directory = tempfile::tempdir().expect("temporary project directory");
        let project_path = directory.path().join("new-card.pcba");
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "原工程",
            8_000,
            12_000,
        ));

        let created = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "new_project".to_owned(),
            args: serde_json::json!({
                "request": {
                    "title": "新卡片",
                    "widthUm": 64_000,
                    "heightUm": 100_000
                }
            }),
        });
        assert!(created.error.is_none());
        assert_eq!(created.payload["title"], "新卡片");
        assert_eq!(created.payload["board"]["heightUm"], 100_000);

        let saved = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "save_project".to_owned(),
            args: serde_json::json!({
                "request": { "path": project_path }
            }),
        });
        assert!(saved.error.is_none());
        assert_eq!(
            saved.revision, created.revision,
            "saving must not change document revision"
        );
        let reopened =
            atelier_core::ProjectBundle::open(&project_path).expect("reopen saved project");
        assert_eq!(reopened.document.title, "新卡片");
        assert_eq!(reopened.document.board.height_um(), 100_000);
    }

    #[test]
    fn external_easyeda_open_only_accepts_an_existing_native_project() {
        let directory = tempfile::tempdir().expect("temporary export directory");
        let wrong_extension = directory.path().join("project.txt");
        std::fs::write(&wrong_extension, b"fixture").expect("write fixture");
        assert!(super::validated_easyeda_project_path(&wrong_extension).is_err());

        let missing = directory.path().join("missing.eprj2");
        assert!(super::validated_easyeda_project_path(&missing).is_err());

        let project = directory.path().join("artwork.eprj2");
        std::fs::write(&project, b"fixture").expect("write native fixture");
        assert_eq!(
            super::validated_easyeda_project_path(&project).expect("valid project path"),
            project.canonicalize().expect("canonical project path")
        );
    }

    #[test]
    fn production_preview_bridge_returns_six_layers_from_the_resolved_board() {
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "Production preview",
            8_000,
            12_000,
        ));

        let board_preview = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "get_board_preview".to_owned(),
            args: serde_json::json!({}),
        });
        let preview = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "get_production_preview".to_owned(),
            args: serde_json::json!({}),
        });

        assert!(preview.error.is_none());
        assert_eq!(
            preview.revision, 0,
            "preview reads must not advance revision"
        );
        assert_eq!(preview.payload["source"], "resolvedFabricationBoard");
        assert_eq!(
            preview.payload["fabricationInputSha256"],
            board_preview.payload["fabricationInputSha256"]
        );
        assert_eq!(
            preview.payload["fabricationOutputSha256"],
            board_preview.payload["fabricationOutputSha256"]
        );
        assert_eq!(service.preview_resolution_count(), 1);
        assert_eq!(preview.payload["textures"].as_array().unwrap().len(), 6);
        assert!(
            preview.payload["textures"][0]["pngDataUrl"]
                .as_str()
                .expect("production PNG data URL")
                .starts_with("data:image/png;base64,")
        );
        assert!(preview.payload["textures"][0].get("rgba").is_none());
        assert_eq!(preview.payload["pixelPitchUm"], 200);
        assert_eq!(preview.payload["outline"]["widthUm"], 8_000);
        assert_eq!(
            preview.payload["fabricationInputSha256"]
                .as_str()
                .unwrap()
                .len(),
            64
        );
        assert_eq!(
            preview.payload["fabricationOutputSha256"]
                .as_str()
                .unwrap()
                .len(),
            64
        );
    }

    #[test]
    fn board_and_production_pngs_share_inserted_silkscreen_pixels() {
        use base64::Engine as _;

        fn decode_rgba(data_url: &str) -> image::RgbaImage {
            let encoded = data_url
                .strip_prefix("data:image/png;base64,")
                .expect("PNG data URL prefix");
            let png = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .expect("base64 PNG");
            image::load_from_memory_with_format(&png, image::ImageFormat::Png)
                .expect("decode PNG")
                .to_rgba8()
        }

        let mut document = atelier_core::AtelierDocument::new_card("Mapped mark", 8_000, 12_000);
        let mark =
            ContentLayer::new_text("标记", "F", TransformUm::rect(1_000, 1_000, 4_000, 4_000));
        let mark_id = mark.id;
        document.front.layers.push(mark);
        document.mappings.push(ProductionMapping::new(
            mark_id,
            ProductionTarget::new(CardSide::Front, FaceProductionLayer::Silkscreen),
            CombineMode::Add,
        ));
        let mut service = super::WorkspaceService::new(document);
        let request = |command: &str| super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: command.to_owned(),
            args: serde_json::json!({}),
        };
        let board = service.invoke(request("get_board_preview"));
        let production = service.invoke(request("get_production_preview"));
        let board_face = decode_rgba(
            board.payload["textures"]["front"]["pngDataUrl"]
                .as_str()
                .expect("board face PNG"),
        );
        let silk = production.payload["textures"]
            .as_array()
            .expect("production textures")
            .iter()
            .find(|texture| texture["side"] == "front" && texture["layer"] == "silkscreen")
            .expect("front silkscreen texture");
        let silk_face = decode_rgba(silk["pngDataUrl"].as_str().expect("front silkscreen PNG"));
        let mark_pixel = silk_face
            .as_raw()
            .chunks_exact(4)
            .position(|pixel| pixel[3] != 0)
            .expect("inserted text must produce silkscreen pixels");
        assert_eq!(
            &board_face.as_raw()[mark_pixel * 4..mark_pixel * 4 + 4],
            &silk_face.as_raw()[mark_pixel * 4..mark_pixel * 4 + 4],
            "3D board texture must show the exact compiled silkscreen pixel"
        );
        assert_eq!(
            board.payload["fabricationOutputSha256"],
            production.payload["fabricationOutputSha256"]
        );
    }

    #[test]
    fn workspace_service_renames_a_layer_through_the_shared_contract() {
        let mut document = atelier_core::AtelierDocument::new_card("重命名", 64_000, 100_000);
        let layer = ContentLayer::new_text(
            "旧名称",
            "内容",
            TransformUm::rect(1_000, 1_000, 10_000, 4_000),
        );
        let layer_id = layer.id;
        document.front.layers.push(layer);
        let mut service = super::WorkspaceService::new(document);

        let response = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "set_layer_name".to_owned(),
            args: serde_json::json!({
                "request": {
                    "layerId": layer_id,
                    "name": "新名称"
                }
            }),
        });

        assert_eq!(response.error, None);
        assert_eq!(response.payload["frontLayers"][0]["name"], "新名称");
        assert_eq!(response.revision, 1);
    }

    #[test]
    fn shared_bridge_imports_compiles_validates_and_traces_domain_data() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("encode PNG");
        let bytes = png.into_inner();
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "统一契约",
            10_000,
            10_000,
        ));
        let invoke =
            |service: &mut super::WorkspaceService, command: &str, args: serde_json::Value| {
                service.invoke(super::WorkspaceBridgeRequest {
                    contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
                    command: command.to_owned(),
                    args,
                })
            };

        let imported = invoke(
            &mut service,
            "import_project_asset",
            serde_json::json!({
                "request": {
                    "originalFilename": "source.png",
                    "mediaType": "image/png",
                    "pixelWidth": 2,
                    "pixelHeight": 2,
                    "bytes": bytes.clone()
                }
            }),
        );
        assert_eq!(imported.error, None);
        assert_eq!(imported.revision, 1);
        assert_eq!(imported.payload["reused"], false);
        let asset_id = imported.payload["assetId"].clone();
        let embedded_preview = invoke(
            &mut service,
            "begin_image_preview_source",
            serde_json::json!({ "request": { "assetId": asset_id.clone() } }),
        );
        assert_eq!(embedded_preview.error, None);
        assert_eq!(embedded_preview.payload["mediaType"], "image/png");
        let preview_diagnostics = invoke(
            &mut service,
            "get_image_preview_diagnostics",
            serde_json::json!({}),
        );
        assert_eq!(
            preview_diagnostics.payload["sourceBytes"], 0,
            "embedded assets must not cross the frontend bridge again"
        );

        let inserted_treatment = invoke(
            &mut service,
            "insert_image_treatment",
            serde_json::json!({
                "request": {
                    "assetId": asset_id,
                    "productionMode": "monochromeMask",
                    "recipe": atelier_core::TreatmentRecipe::default()
                }
            }),
        );
        assert_eq!(inserted_treatment.error, None);
        assert_eq!(inserted_treatment.revision, 2);
        let treatment_id = inserted_treatment.payload["treatmentId"].clone();

        let compiled = invoke(
            &mut service,
            "compile_image_treatment",
            serde_json::json!({
                "request": {
                    "treatmentId": treatment_id,
                    "physicalWidthUm": 4_000,
                    "physicalHeightUm": 4_000,
                    "purpose": "interactiveProxy"
                }
            }),
        );
        assert_eq!(compiled.error, None);
        assert_eq!(compiled.revision, 2);
        assert_eq!(compiled.payload["revision"], 2);
        assert_eq!(
            compiled.payload["recipeFingerprint"].as_str().map(str::len),
            Some(64)
        );
        assert_eq!(
            compiled.payload["maskSha256"].as_str().map(str::len),
            Some(64)
        );
        assert!(
            compiled.payload["previewPngDataUrl"]
                .as_str()
                .is_some_and(|value| value.starts_with("data:image/png;base64,"))
        );

        let unsupported = atelier_core::ManufacturerProfileSnapshot {
            surface_finish: atelier_core::SurfaceFinish::Osp,
            ..Default::default()
        };
        let validation = invoke(
            &mut service,
            "validate_manufacturer_profile",
            serde_json::json!({ "request": { "profile": unsupported } }),
        );
        assert_eq!(validation.error, None);
        assert_eq!(validation.revision, 2);
        assert_eq!(validation.payload["valid"], false);
        assert!(
            validation.payload["errors"]
                .as_array()
                .is_some_and(|errors| !errors.is_empty())
        );

        let placed = invoke(
            &mut service,
            "insert_image_asset",
            serde_json::json!({
                "request": {
                    "side": "front",
                    "originalFilename": "source.png",
                    "mediaType": "image/png",
                    "pixelWidth": 2,
                    "pixelHeight": 2,
                    "bytes": bytes,
                    "replaceLayerId": null
                }
            }),
        );
        assert_eq!(placed.error, None);
        let mapped = invoke(
            &mut service,
            "map_layer",
            serde_json::json!({
                "request": {
                    "layerId": placed.payload["layerId"],
                    "side": "front",
                    "layer": "silkscreen",
                    "combine": "add"
                }
            }),
        );
        assert_eq!(mapped.error, None);
        let trace = invoke(&mut service, "get_production_trace", serde_json::json!({}));
        assert_eq!(trace.error, None);
        assert_eq!(trace.payload["format"], "atelier-production-trace-v1");
        assert_eq!(
            trace.payload["operations"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(trace.payload["operations"][0]["assetId"], asset_id);
        assert_eq!(
            trace.payload["operations"][0]["recipeFingerprint"]
                .as_str()
                .map(str::len),
            Some(64)
        );
    }

    #[test]
    fn image_import_preview_is_read_only_and_confirmation_inserts_all_entities_once() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("encode PNG");
        let bytes = png.into_inner();
        let recipe = atelier_core::TreatmentRecipe::default();
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "Atomic import",
            20_000,
            20_000,
        ));
        let before = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "get_workspace_document".to_owned(),
            args: serde_json::json!({}),
        });

        let begun = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "begin_image_preview_source".to_owned(),
            args: serde_json::json!({
                "request": {
                    "bytes": bytes.clone(),
                    "mediaType": "image/png"
                }
            }),
        });
        let preview = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "request_image_preview".to_owned(),
            args: serde_json::json!({
                "request": {
                    "sourceHandle": begun.payload["sourceHandle"],
                    "previewStreamId": "atomic-import",
                    "generation": 1,
                    "workspaceRevision": 0,
                    "recipe": recipe.clone(),
                    "physicalWidthUm": 10_000,
                    "physicalHeightUm": 10_000,
                    "pixelPitchUm": 250
                }
            }),
        });
        assert_eq!(preview.error, None);
        assert_eq!(preview.revision, 0);
        assert_eq!(preview.payload["purpose"], "interactiveProxy");
        assert_eq!(
            preview.payload["recipeFingerprint"],
            "5abe3f2d53ee2523d2b34586760ca4e97cdccc2f1ec267534ecea05972c318cf"
        );
        let after_preview = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "get_workspace_document".to_owned(),
            args: serde_json::json!({}),
        });
        assert_eq!(after_preview.payload, before.payload);

        let confirmed = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "confirm_image_import".to_owned(),
            args: serde_json::json!({
                "request": {
                    "side": "front",
                    "layer": "silkscreen",
                    "originalFilename": "portrait.png",
                    "mediaType": "image/png",
                    "pixelWidth": 2,
                    "pixelHeight": 2,
                    "bytes": bytes,
                    "recipe": recipe,
                    "productionMode": "monochromeMask",
                    "placementCenterUm": { "xUm": 10_000, "yUm": 10_000 }
                }
            }),
        });
        assert_eq!(confirmed.error, None);
        assert_eq!(confirmed.revision, 1);
        assert_eq!(
            confirmed.payload["document"]["assets"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            confirmed.payload["document"]["imageTreatments"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            confirmed.payload["document"]["frontLayers"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            confirmed.payload["document"]["mappings"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            confirmed.payload["document"]["mappings"][0]["treatmentId"],
            confirmed.payload["treatmentId"]
        );

        let undone = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "undo_workspace".to_owned(),
            args: serde_json::json!({}),
        });
        assert_eq!(undone.error, None);
        for field in ["assets", "imageTreatments", "frontLayers", "mappings"] {
            assert_eq!(undone.payload[field].as_array().map(Vec::len), Some(0));
        }
    }

    #[test]
    fn preview_source_is_registered_once_requested_by_handle_and_released_without_document_changes()
    {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(8, 8)
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("encode PNG");
        let bytes = png.into_inner();
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "Preview session",
            20_000,
            20_000,
        ));
        let invoke =
            |service: &mut super::WorkspaceService, command: &str, args: serde_json::Value| {
                service.invoke(super::WorkspaceBridgeRequest {
                    contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
                    command: command.to_owned(),
                    args,
                })
            };
        let before = invoke(
            &mut service,
            "get_workspace_document",
            serde_json::json!({}),
        );
        let begun = invoke(
            &mut service,
            "begin_image_preview_source",
            serde_json::json!({
                "request": {
                    "bytes": bytes,
                    "mediaType": "image/png"
                }
            }),
        );
        assert_eq!(begun.error, None);
        assert_eq!(begun.revision, 0);
        assert_eq!(begun.payload["widthPx"], 8);
        let source_handle = begun.payload["sourceHandle"].clone();

        for generation in 1..=2 {
            let preview = invoke(
                &mut service,
                "request_image_preview",
                serde_json::json!({
                    "request": {
                        "sourceHandle": source_handle,
                        "previewStreamId": "inspector",
                        "generation": generation,
                        "workspaceRevision": 0,
                        "recipe": atelier_core::TreatmentRecipe::default(),
                        "physicalWidthUm": 10_000,
                        "physicalHeightUm": 10_000,
                        "pixelPitchUm": 250
                    }
                }),
            );
            assert_eq!(preview.error, None);
            assert_eq!(preview.payload["purpose"], "interactiveProxy");
        }
        let diagnostics = invoke(
            &mut service,
            "get_image_preview_diagnostics",
            serde_json::json!({}),
        );
        assert_eq!(diagnostics.payload["prepareCount"], 1);
        assert_eq!(diagnostics.payload["proxyCompileCount"], 2);
        assert_eq!(diagnostics.payload["activeSessions"], 1);

        let released = invoke(
            &mut service,
            "release_image_preview_source",
            serde_json::json!({ "request": { "sourceHandle": source_handle } }),
        );
        assert_eq!(released.error, None);
        let after = invoke(
            &mut service,
            "get_workspace_document",
            serde_json::json!({}),
        );
        assert_eq!(after.revision, before.revision);
        assert_eq!(after.payload, before.payload);
        let diagnostics = invoke(
            &mut service,
            "get_image_preview_diagnostics",
            serde_json::json!({}),
        );
        assert_eq!(diagnostics.payload["activeSessions"], 0);
    }

    #[test]
    fn failed_image_import_confirmation_leaves_no_partial_entities() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("encode PNG");
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "Atomic rejection",
            20_000,
            20_000,
        ));
        let rejected = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "confirm_image_import".to_owned(),
            args: serde_json::json!({
                "request": {
                    "side": "front",
                    "layer": "silkscreen",
                    "originalFilename": "portrait.png",
                    "mediaType": "image/png",
                    "pixelWidth": 2,
                    "pixelHeight": 2,
                    "bytes": png.into_inner(),
                    "recipe": atelier_core::TreatmentRecipe::default(),
                    "placementCenterUm": { "xUm": 30_000, "yUm": 10_000 }
                }
            }),
        });
        assert!(rejected.error.is_some());
        assert_eq!(rejected.revision, 0);

        let document = service.invoke(super::WorkspaceBridgeRequest {
            contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: "get_workspace_document".to_owned(),
            args: serde_json::json!({}),
        });
        for field in ["assets", "imageTreatments", "frontLayers", "mappings"] {
            assert_eq!(document.payload[field].as_array().map(Vec::len), Some(0));
        }
    }

    #[test]
    fn shared_bridge_moves_project_assets_between_folder_paths_atomically() {
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "素材文件夹",
            10_000,
            10_000,
        ));
        let invoke =
            |service: &mut super::WorkspaceService, command: &str, args: serde_json::Value| {
                service.invoke(super::WorkspaceBridgeRequest {
                    contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
                    command: command.to_owned(),
                    args,
                })
            };
        let imported = invoke(
            &mut service,
            "import_project_asset",
            serde_json::json!({
                "request": {
                    "originalFilename": "logo.png",
                    "mediaType": "image/png",
                    "pixelWidth": 2,
                    "pixelHeight": 2,
                    "bytes": b"folder-logo"
                }
            }),
        );
        let asset_id = imported.payload["assetId"].clone();
        let sha256 = imported.payload["document"]["assets"][0]["sha256"].clone();

        let moved = invoke(
            &mut service,
            "move_project_asset",
            serde_json::json!({
                "request": {
                    "assetId": asset_id,
                    "folderPath": " 品牌 / 标志 "
                }
            }),
        );
        assert_eq!(moved.error, None);
        assert_eq!(moved.revision, 2);
        assert_eq!(moved.payload["assetId"], asset_id);
        assert_eq!(moved.payload["folderPath"], "品牌/标志");
        assert_eq!(
            moved.payload["document"]["assets"][0]["folderPath"],
            "品牌/标志"
        );
        assert_eq!(moved.payload["document"]["assets"][0]["sha256"], sha256);

        let repeated = invoke(
            &mut service,
            "move_project_asset",
            serde_json::json!({
                "request": {
                    "assetId": asset_id,
                    "folderPath": "品牌/标志"
                }
            }),
        );
        assert_eq!(repeated.error, None);
        assert_eq!(repeated.revision, 2);

        let rejected = invoke(
            &mut service,
            "move_project_asset",
            serde_json::json!({
                "request": {
                    "assetId": asset_id,
                    "folderPath": "../外部"
                }
            }),
        );
        assert!(
            rejected
                .error
                .as_deref()
                .is_some_and(|error| error.contains("folder path"))
        );
        assert_eq!(rejected.revision, 2);
        assert_eq!(
            service.session.document_view().assets[0]
                .folder_path
                .as_deref(),
            Some("品牌/标志")
        );
    }

    #[test]
    fn shared_bridge_replaces_protects_and_cleans_project_assets() {
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "素材领域操作",
            10_000,
            10_000,
        ));
        let invoke =
            |service: &mut super::WorkspaceService, command: &str, args: serde_json::Value| {
                service.invoke(super::WorkspaceBridgeRequest {
                    contract_version: super::WORKSPACE_CONTRACT_VERSION.to_owned(),
                    command: command.to_owned(),
                    args,
                })
            };
        let import = |service: &mut super::WorkspaceService, name: &str, bytes: &[u8]| {
            invoke(
                service,
                "import_project_asset",
                serde_json::json!({
                    "request": {
                        "originalFilename": name,
                        "mediaType": "image/png",
                        "pixelWidth": 2,
                        "pixelHeight": 2,
                        "bytes": bytes
                    }
                }),
            )
        };

        let original = import(&mut service, "original.png", b"original");
        let replacement = import(&mut service, "replacement.png", b"replacement");
        let unused = import(&mut service, "unused.png", b"unused");
        let original_id = original.payload["assetId"].clone();
        let replacement_id = replacement.payload["assetId"].clone();
        let unused_id = unused.payload["assetId"].clone();

        let placed = invoke(
            &mut service,
            "insert_image_asset",
            serde_json::json!({
                "request": {
                    "side": "front",
                    "originalFilename": "original.png",
                    "mediaType": "image/png",
                    "pixelWidth": 2,
                    "pixelHeight": 2,
                    "bytes": b"original",
                    "replaceLayerId": null
                }
            }),
        );
        let mapped = invoke(
            &mut service,
            "map_layer",
            serde_json::json!({
                "request": {
                    "layerId": placed.payload["layerId"],
                    "side": "front",
                    "layer": "silkscreen",
                    "combine": "add"
                }
            }),
        );
        assert_eq!(mapped.error, None);
        let treatment_id = mapped.payload["imageTreatments"][0]["id"].clone();
        let mapping_id = mapped.payload["mappings"][0]["id"].clone();

        let replaced = invoke(
            &mut service,
            "replace_all_asset_references",
            serde_json::json!({
                "request": {
                    "originalAssetId": original_id,
                    "replacementAssetId": replacement_id
                }
            }),
        );
        assert_eq!(replaced.error, None);
        assert_eq!(replaced.revision, 6);
        assert_eq!(replaced.payload["replacedInstanceCount"], 1);
        assert_eq!(replaced.payload["replacedTreatmentCount"], 1);
        assert_eq!(
            replaced.payload["document"]["frontLayers"][0]["kind"]["assetId"],
            replacement_id
        );
        assert_eq!(
            replaced.payload["document"]["imageTreatments"][0]["assetId"],
            replacement_id
        );
        assert_eq!(
            replaced.payload["document"]["imageTreatments"][0]["id"],
            treatment_id
        );
        assert_eq!(
            replaced.payload["document"]["mappings"][0]["id"],
            mapping_id
        );

        let deleted_original = invoke(
            &mut service,
            "delete_project_asset",
            serde_json::json!({ "request": { "assetId": original_id } }),
        );
        assert_eq!(deleted_original.error, None);
        assert_eq!(deleted_original.revision, 7);
        assert_eq!(deleted_original.payload["deletedAssetId"], original_id);

        let protected = invoke(
            &mut service,
            "delete_project_asset",
            serde_json::json!({ "request": { "assetId": replacement_id } }),
        );
        assert!(
            protected
                .error
                .as_deref()
                .is_some_and(|error| error.contains("still used"))
        );
        assert_eq!(protected.revision, 7);

        let cleaned = invoke(&mut service, "cleanup_unused_assets", serde_json::json!({}));
        assert_eq!(cleaned.error, None);
        assert_eq!(cleaned.revision, 8);
        assert_eq!(
            cleaned.payload["removedAssetIds"],
            serde_json::json!([unused_id])
        );
        assert_eq!(
            cleaned.payload["document"]["assets"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );

        let repeated_cleanup = invoke(&mut service, "cleanup_unused_assets", serde_json::json!({}));
        assert_eq!(repeated_cleanup.error, None);
        assert_eq!(repeated_cleanup.revision, 8);
        assert_eq!(
            repeated_cleanup.payload["removedAssetIds"],
            serde_json::json!([])
        );
    }

    #[test]
    fn native_image_reader_accepts_supported_signatures_and_rejects_other_files() {
        let temp = tempfile::tempdir().expect("temp directory");
        let png = temp.path().join("art.png");
        std::fs::write(&png, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]).expect("write png");

        let selected = super::read_supported_image_file(&png).expect("read png");
        assert_eq!(selected.name, "art.png");
        assert_eq!(selected.media_type, "image/png");

        let json = temp.path().join("not-an-image.json");
        std::fs::write(&json, br#"{"image":false}"#).expect("write json");
        assert_eq!(
            super::read_supported_image_file(&json),
            Err("仅支持有效的 PNG、JPEG 或 WebP 图片".to_owned())
        );
    }

    fn asymmetric_export_session() -> super::WorkspaceSession {
        let mut session = super::WorkspaceSession::new(atelier_core::AtelierDocument::new_card(
            "Desktop export",
            8_000,
            12_000,
        ));
        let front =
            ContentLayer::new_text("Front F", "F", TransformUm::rect(500, 800, 3_000, 4_000));
        let back =
            ContentLayer::new_text("Back B", "B", TransformUm::rect(4_500, 7_000, 3_000, 4_000));
        let front_id = front.id;
        let back_id = back.id;
        session.bundle.document.front.layers.push(front);
        session.bundle.document.back.layers.push(back);
        for (layer_id, side) in [(front_id, CardSide::Front), (back_id, CardSide::Back)] {
            for layer in [
                FaceProductionLayer::Copper,
                FaceProductionLayer::SolderMaskOpen,
                FaceProductionLayer::Silkscreen,
            ] {
                session
                    .bundle
                    .document
                    .mappings
                    .push(ProductionMapping::new(
                        layer_id,
                        ProductionTarget::new(side, layer),
                        CombineMode::Add,
                    ));
            }
        }
        session
    }
}
