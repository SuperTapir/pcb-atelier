use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use atelier_core::{
    AssetId, AssetReference, AtelierDocument, BoardOutline, CardSide, CombineMode, CommandHistory,
    CommandOutcome, ContentKind, ContentLayer, DocumentCommand, DocumentDiagnostic,
    EasyedaHandoffExportReport, FaceProductionLayer, ImageContent, LayerId, MappingId,
    ProductionLayerPreviewTexture, ProductionMapping, ProductionTarget, ProjectBundle,
    ProjectBundleRasterizer, ResolvedFabricationBoard, ResolvedPreviewTextures, StackupPreset,
    TextContent, TextLayout, TransformUm, compile_fabrication_plan, export_easyeda_handoff,
    resolve_fabrication_plan,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    mappings: Vec<ProductionMapping>,
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

const INTERACTIVE_PREVIEW_PITCH_UM: u32 = 100;
const EASYEDA_EXPORT_PITCH_UM: u32 = 25;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct BoardPreviewView {
    outline: BoardOutlineView,
    thickness_um: u32,
    fabrication_input_sha256: String,
    fabrication_output_sha256: String,
    textures: ResolvedPreviewTextures,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductionPreviewView {
    source: &'static str,
    outline: BoardOutlineView,
    fabrication_input_sha256: String,
    fabrication_output_sha256: String,
    pixel_pitch_um: u32,
    textures: Vec<ProductionLayerPreviewTexture>,
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
        mappings: document.mappings.clone(),
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
}

impl WorkspaceService {
    pub fn new(document: AtelierDocument) -> Self {
        Self {
            session: WorkspaceSession::new(document),
            revision: 0,
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
        }
    }

    pub fn should_use_read_snapshot(command: &str) -> bool {
        matches!(command, "get_board_preview" | "get_production_preview")
    }

    fn dispatch(&mut self, command: &str, args: Value) -> Result<(Value, bool), String> {
        let result = match command {
            "get_core_info" => return serialize_response(core_info(), false),
            "get_workspace_document" => {
                return serialize_response(self.session.document_view(), false);
            }
            "get_board_preview" => {
                return serialize_response(self.session.board_preview()?, false);
            }
            "get_production_preview" => {
                return serialize_response(self.session.production_preview()?, false);
            }
            "get_asset_bytes" => {
                let asset_id = serde_json::from_value(args["assetId"].clone())
                    .map_err(|error| format!("invalid assetId: {error}"))?;
                self.session.asset_bytes(asset_id).and_then(to_json)
            }
            "insert_image_asset" => self
                .session
                .insert_image(decode_request(&args)?)
                .and_then(to_json),
            "insert_text_layer" => self
                .session
                .insert_text(decode_request(&args)?)
                .and_then(to_json),
            "set_text_content" => self
                .session
                .set_text(decode_request(&args)?)
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

    fn insert_image(&mut self, request: ImageAssetRequest) -> Result<LayerMutationView, String> {
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
            let command = DocumentCommand::SetImageContent {
                layer_id,
                image: ImageContent {
                    asset_id,
                    crop: None,
                },
            };
            if let Err(error) = self.history.execute(&mut self.bundle.document, command) {
                self.bundle = previous_bundle;
                return Err(error.to_string());
            }
            layer_id
        } else {
            let transform = fit_image_transform(
                &self.bundle.document,
                request.pixel_width,
                request.pixel_height,
            );
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

    fn board_preview(&self) -> Result<BoardPreviewView, String> {
        let resolved = self.resolve_interactive_board()?;
        let textures = resolved
            .preview_textures()
            .map_err(|error| error.to_string())?;
        Ok(BoardPreviewView {
            outline: BoardOutlineView::from(&resolved.outline),
            thickness_um: resolved.stackup.thickness_um,
            fabrication_input_sha256: resolved.build.input_sha256,
            fabrication_output_sha256: resolved.build.output_sha256,
            textures,
        })
    }

    fn production_preview(&self) -> Result<ProductionPreviewView, String> {
        let resolved = self.resolve_interactive_board()?;
        let textures = resolved
            .production_layer_textures()
            .map_err(|error| error.to_string())?;
        Ok(ProductionPreviewView {
            source: "resolvedFabricationBoard",
            outline: BoardOutlineView::from(&resolved.outline),
            fabrication_input_sha256: resolved.build.input_sha256,
            fabrication_output_sha256: resolved.build.output_sha256,
            pixel_pitch_um: resolved.grid.pixel_pitch_um,
            textures,
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
        let resolved = resolve_fabrication_plan(&plan, EASYEDA_EXPORT_PITCH_UM, &mut rasterizer)
            .map_err(|error| format!("{failure_prefix}：生产几何解析失败：{error}"))?;
        export_easyeda_handoff(output_directory, &self.bundle.document.title, &resolved)
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
struct LayerIdRequest {
    layer_id: LayerId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransformLayerRequest {
    layer_id: LayerId,
    transform: TransformUm,
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

#[tauri::command]
fn workspace_invoke(
    request: WorkspaceBridgeRequest,
    service: tauri::State<'_, Mutex<WorkspaceService>>,
) -> Result<WorkspaceBridgeResponse, String> {
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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(WorkspaceService::new(
            initial_workspace_document(),
        )))
        .invoke_handler(tauri::generate_handler![workspace_invoke])
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
        let layer = &session.bundle.document.front.layers[0];
        let atelier_core::ContentKind::Text(text) = &layer.kind else {
            panic!("inserted layer should be text");
        };

        assert_eq!(text.text, "新的文字");
        assert_eq!(text.layout, atelier_core::TextLayout::FixedFrame);
        assert_eq!(
            layer.transform,
            atelier_core::TransformUm::rect(7_000, 9_000, 24_000, 12_000)
        );
        assert_eq!(session.history.undo_depth(), 2);
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
        assert_eq!(preview.payload["textures"]["front"]["widthPx"], 80);
        assert_eq!(preview.payload["textures"]["front"]["heightPx"], 120);
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
        assert_eq!(preview.payload["textures"]["front"]["widthPx"], 200);
        assert_eq!(preview.payload["textures"]["front"]["heightPx"], 200);
        assert_eq!(
            preview.payload["fabricationOutputSha256"]
                .as_str()
                .expect("preview hash")
                .len(),
            64
        );
    }

    #[test]
    fn production_preview_bridge_returns_six_layers_from_the_resolved_board() {
        let mut service = super::WorkspaceService::new(atelier_core::AtelierDocument::new_card(
            "Production preview",
            8_000,
            12_000,
        ));

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
        assert_eq!(preview.payload["textures"].as_array().unwrap().len(), 6);
        assert_eq!(preview.payload["pixelPitchUm"], 100);
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
