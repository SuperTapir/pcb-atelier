import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  BoardPreviewInput,
  BoardPreviewOutline,
} from "@/features/preview/board-preview-renderer";
import type { ProductionPreviewInput } from "@/features/preview/production-renderer";

export interface CoreInfo {
  projectFormat: string;
  schemaVersion: number;
}

export interface WorkspaceDocument {
  title: string;
  board: {
    widthUm: number;
    heightUm: number;
    outline: BoardPreviewOutline;
    cornerRadiusUm: number;
    diagnostics: DocumentDiagnostic[];
  };
  stackup: StackupPreset;
  faces: {
    frontLayerCount: number;
    backLayerCount: number;
  };
  frontLayers: ContentLayer[];
  backLayers: ContentLayer[];
  assets: AssetReference[];
  mappings: ProductionMapping[];
  history: {
    canUndo: boolean;
    canRedo: boolean;
  };
}

export type SolderMaskColor =
  | "black"
  | "white"
  | "green"
  | "red"
  | "blue"
  | "purple"
  | "yellow";

export type SurfaceFinish = "enig" | "haslLeadFree";

export interface StackupPreset {
  substrate: "fr4";
  thicknessUm: number;
  solderMaskColor: SolderMaskColor;
  surfaceFinish: SurfaceFinish;
}

export type ProductionLayer = "copper" | "solderMaskOpen" | "silkscreen";

export interface ProductionMapping {
  id: string;
  sourceLayerId: string;
  target: {
    side: "front" | "back";
    layer: ProductionLayer;
  };
  combine: "add" | "subtract";
}

export interface TransformUm {
  xUm: number;
  yUm: number;
  widthUm: number;
  heightUm: number;
  rotationMdeg: number;
  flipX: boolean;
  flipY: boolean;
}

export interface ContentLayer {
  id: string;
  name: string;
  visible: boolean;
  locked: boolean;
  exportEnabled: boolean;
  parentId: string | null;
  transform: TransformUm;
  kind:
    | { type: "image"; assetId: string; crop: unknown | null }
    | {
        type: "text";
        text: string;
        fontFamily: string;
        fontSizeUm: number;
        layout: "autoWidth" | "fixedFrame";
      }
    | { type: "group" }
    | { type: "boardFill"; edgeClearanceUm: number };
}

export interface AssetReference {
  id: string;
  embeddedPath: string;
  originalFilename: string;
  mediaType: string;
  sha256: string;
  pixelWidth: number;
  pixelHeight: number;
}

export interface LayerMutation {
  document: WorkspaceDocument;
  layerId: string;
}

export interface BoardFillMutation extends LayerMutation {
  created: boolean;
}

export interface DocumentDiagnostic {
  kind: "contentOutsideBoard";
  side: "front" | "back";
  layerId: string;
  bounds: {
    minXUm: number;
    minYUm: number;
    maxXUm: number;
    maxYUm: number;
  };
  boardWidthUm: number;
  boardHeightUm: number;
}

export interface EasyedaExportReport {
  exportFormatVersion: string;
  exportVersion: string;
  manifestPath: string;
  publicArchivePath: string;
  nativeProjectPath: string;
  fabricationInputSha256: string;
  fabricationOutputSha256: string;
  publicArchiveSha256: string;
  nativeProjectSha256: string;
  primitives: {
    fillCount: number;
    holeCount: number;
    filledLayerIds: number[];
  };
  publicValidation: EasyedaPublicValidation;
  nativeValidation: EasyedaNativeValidation;
}

export interface EasyedaPublicValidation {
  isValid: boolean;
  errors: string[];
  title: string;
  boardUuid: string | null;
  pcbUuid: string | null;
  boardWidthUm: number;
  boardHeightUm: number;
  fillCount: number;
  holeCount: number;
  filledLayerIds: number[];
}

export interface EasyedaNativeValidation {
  isValid: boolean;
  errors: string[];
  projectUuid: string | null;
  branchUuid: string | null;
  historyUuid: string | null;
  boardUuids: string[];
  pcbUuids: string[];
  payloadRecords: number;
  tableCount: number;
  indexCount: number;
  boardWidthUm: number;
  boardHeightUm: number;
  fillCount: number;
  holeCount: number;
  filledLayerIds: number[];
  solderMaskOpeningLayerIds: number[];
  layerXExtents: Array<{
    layerId: number;
    minXNanoMil: number;
    maxXNanoMil: number;
  }>;
}

export interface WorkspaceBoardPreview extends BoardPreviewInput {
  fabricationInputSha256: string;
  fabricationOutputSha256: string;
}

export interface ImageAssetInput {
  side: "front" | "back";
  originalFilename: string;
  mediaType: string;
  pixelWidth: number;
  pixelHeight: number;
  bytes: number[];
  replaceLayerId: string | null;
}

export interface InsertTextInput {
  side: "front" | "back";
  xUm: number;
  yUm: number;
  widthUm: number;
  heightUm: number;
  layout: "autoWidth" | "fixedFrame";
}

export interface FontCatalog {
  families: string[];
  fallbackFamily: string;
}

async function invokeCore<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const request: WorkspaceBridgeRequest = {
    contractVersion: WORKSPACE_CONTRACT_VERSION,
    command,
    args: args ?? {},
  };
  const response = isTauriRuntime()
    ? await invoke<WorkspaceBridgeResponse<T>>("workspace_invoke", { request })
    : await invokeLocalBridge<T>(request);
  if (response.contractVersion !== WORKSPACE_CONTRACT_VERSION) {
    throw new Error(
      `工作区契约版本不匹配：${response.contractVersion}`,
    );
  }
  if (response.error) throw new Error(response.error);
  return response.payload;
}

const WORKSPACE_CONTRACT_VERSION = "pcb-atelier-workspace-v1";

interface WorkspaceBridgeRequest {
  contractVersion: string;
  command: string;
  args: Record<string, unknown>;
}

interface WorkspaceBridgeResponse<T> {
  contractVersion: string;
  revision: number;
  payload: T;
  error: string | null;
}

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

async function invokeLocalBridge<T>(
  request: WorkspaceBridgeRequest,
): Promise<WorkspaceBridgeResponse<T>> {
  if (!import.meta.env.DEV) {
    throw new Error("Web 生产构建未启用本地 Rust Workspace bridge");
  }
  const response = await fetch("/__atelier_bridge", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
  });
  if (!response.ok) {
    throw new Error(`本地 Rust Workspace bridge 请求失败：HTTP ${response.status}`);
  }
  return (await response.json()) as WorkspaceBridgeResponse<T>;
}

export function getCoreInfo(): Promise<CoreInfo> {
  return invokeCore<CoreInfo>("get_core_info");
}

export function getWorkspaceDocument(): Promise<WorkspaceDocument> {
  return invokeCore<WorkspaceDocument>("get_workspace_document");
}

export function getSystemFonts(): Promise<FontCatalog> {
  return invokeCore<FontCatalog>("get_system_fonts");
}

export function getBoardPreview(): Promise<WorkspaceBoardPreview> {
  return invokeCore<WorkspaceBoardPreview>("get_board_preview");
}

export function getProductionPreview(): Promise<ProductionPreviewInput> {
  return invokeCore<ProductionPreviewInput>("get_production_preview");
}

export async function selectEasyedaOutputDirectory(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "选择嘉立创 EDA 导出目录",
  });
  return typeof selected === "string" ? selected : null;
}

export function exportEasyeda(
  outputDirectory: string,
): Promise<EasyedaExportReport> {
  return invokeCore<EasyedaExportReport>("export_easyeda", { outputDirectory });
}

export function insertImageAsset(
  request: ImageAssetInput,
): Promise<LayerMutation> {
  return invokeCore<LayerMutation>("insert_image_asset", { request });
}

export function insertTextLayer(
  request: InsertTextInput,
): Promise<LayerMutation> {
  return invokeCore<LayerMutation>("insert_text_layer", { request });
}

export function setTextContent(
  layerId: string,
  text: string,
): Promise<WorkspaceDocument> {
  return invokeCore<WorkspaceDocument>("set_text_content", {
    request: { layerId, text },
  });
}

export function setTextStyle(
  layerId: string,
  fontFamily: string,
  fontSizeUm: number,
): Promise<WorkspaceDocument> {
  return invokeCore<WorkspaceDocument>("set_text_style", {
    request: { layerId, fontFamily, fontSizeUm },
  });
}

export function setLayerName(
  layerId: string,
  name: string,
): Promise<WorkspaceDocument> {
  return invokeCore("set_layer_name", {
    request: { layerId, name },
  });
}

export function getAssetBytes(
  assetId: string,
): Promise<{ mediaType: string; bytes: number[] }> {
  return invokeCore("get_asset_bytes", { assetId });
}

export function transformLayer(
  layerId: string,
  transform: TransformUm,
): Promise<WorkspaceDocument> {
  return invokeCore("transform_layer", { request: { layerId, transform } });
}

export function setLayerLock(
  layerId: string,
  value: boolean,
): Promise<WorkspaceDocument> {
  return invokeCore("set_layer_lock", { request: { layerId, value } });
}

export function setLayerVisibility(
  layerId: string,
  value: boolean,
): Promise<WorkspaceDocument> {
  return invokeCore("set_layer_visibility", { request: { layerId, value } });
}

export function mapLayer(
  layerId: string,
  side: "front" | "back",
  layer: ProductionLayer,
  combine: "add" | "subtract" = "add",
): Promise<WorkspaceDocument> {
  return invokeCore("map_layer", {
    request: { layerId, side, layer, combine },
  });
}

export function unmapLayer(mappingId: string): Promise<WorkspaceDocument> {
  return invokeCore("unmap_layer", { request: { mappingId } });
}

export function setLayerExportEnabled(
  layerId: string,
  value: boolean,
): Promise<WorkspaceDocument> {
  return invokeCore("set_layer_export_enabled", {
    request: { layerId, value },
  });
}

export function setStackup(
  stackup: StackupPreset,
): Promise<WorkspaceDocument> {
  return invokeCore("set_stackup", { request: { stackup } });
}

export function setBoardOutline(
  outline: BoardPreviewOutline,
): Promise<WorkspaceDocument> {
  return invokeCore("set_board_outline", { request: { outline } });
}

export function createBoardFill(
  side: "front" | "back",
  edgeClearanceUm: number,
): Promise<BoardFillMutation> {
  return invokeCore("create_board_fill", {
    request: { side, edgeClearanceUm },
  });
}

export function reorderLayer(
  layerId: string,
  newParentId: string | null,
  newIndex: number,
): Promise<WorkspaceDocument> {
  return invokeCore("reorder_layer", {
    request: { layerId, newParentId, newIndex },
  });
}

export function groupLayers(
  side: "front" | "back",
  layerIds: string[],
): Promise<LayerMutation> {
  return invokeCore("group_layers", {
    request: { side, name: "组合", layerIds },
  });
}

export function ungroupLayer(layerId: string): Promise<WorkspaceDocument> {
  return invokeCore("ungroup_layer", { request: { layerId } });
}

export function undoWorkspace(): Promise<WorkspaceDocument> {
  return invokeCore("undo_workspace");
}

export function redoWorkspace(): Promise<WorkspaceDocument> {
  return invokeCore("redo_workspace");
}
