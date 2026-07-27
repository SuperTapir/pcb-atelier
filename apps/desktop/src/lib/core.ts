import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
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
  imageTreatments: ImageTreatment[];
  mappings: ProductionMapping[];
  manufacturerProfile: ManufacturerProfileSnapshot;
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

export type SurfaceFinish = "enig" | "haslLead" | "haslLeadFree" | "osp";

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
  treatmentId?: string | null;
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
  folderPath: string | null;
  tags: string[];
  hasAlpha: boolean;
}

export interface ImageTreatment {
  id: string;
  assetId: string;
  productionMode: ImageProductionMode;
  recipe: TreatmentRecipe;
}

export type ImageProductionMode = "monochromeMask" | "colorOriginal";

export interface TreatmentRecipe {
  algorithmVersion: string;
  alphaMode: "compositeOnWhite" | "alphaAsCoverage" | "ignoreAlpha";
  threshold:
    | { mode: "otsu" }
    | { mode: "manual"; value: number };
  invert: boolean;
  smoothingRadiusUm: number;
  despeckleRadiusUm: number;
  removeIslandsBelowUm2: number;
  minimumLineWidthUm: number;
  thinFeaturePolicy: "preserve" | "thicken" | "remove";
  minimumGapUm: number;
  crop: unknown | null;
}

export interface ManufacturerProfileSnapshot {
  manufacturerId: string;
  profileVersion: string;
  sourceUpdatedAt: string;
  sourceUrls: string[];
  substrate: "fr4";
  layerCount: number;
  thicknessUm: number;
  outerCopper: "oz0_5" | "oz1" | "oz2";
  solderMask: SolderMaskColor;
  characterProcess: "standardWhite" | "standardBlack" | "multicolor";
  surfaceFinish: SurfaceFinish;
}

export interface LayerMutation {
  document: WorkspaceDocument;
  layerId: string;
}

export interface LayersMutation {
  document: WorkspaceDocument;
  layerIds: string[];
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
  productionSource: "formalProduction";
  imageGraphics: EasyedaImageGraphicTrace[];
  manufacturing: EasyedaManufacturingSummary;
  orderSupport: EasyedaOrderSupport;
  primitives: {
    fillCount: number;
    holeCount: number;
    filledLayerIds: number[];
  };
  publicValidation: EasyedaPublicValidation;
  nativeValidation: EasyedaNativeValidation;
}

export interface EasyedaImageGraphicTrace {
  mappingId: string;
  sourceInstanceId: string;
  target: {
    side: "front" | "back";
    layer: ProductionLayer;
  };
  treatmentId: string | null;
  algorithmVersion: string | null;
  recipeFingerprint: string | null;
  assetId: string;
  assetSha256: string;
  maskSha256: string;
}

export interface EasyedaManufacturingSummary {
  validated: boolean;
  manufacturerId: string;
  profileVersion: string;
  substrate: "fr4";
  layerCount: number;
  thicknessUm: number;
  outerCopper: "oz0_5" | "oz1" | "oz2";
  solderMask: SolderMaskColor;
  characterProcess: "standardWhite" | "standardBlack" | "multicolor";
  surfaceFinish: SurfaceFinish;
}

export interface EasyedaOrderSupport {
  status: "directOrderSupported" | "requiresManualAdjustment";
  directOrderSupported: boolean;
  issues: string[];
  downgradeActions: string[];
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
  placementCenterUm?: {
    xUm: number;
    yUm: number;
  };
}

export interface ProjectAssetInput {
  originalFilename: string;
  mediaType: string;
  pixelWidth: number;
  pixelHeight: number;
  bytes: number[];
}

export interface AssetMutation {
  document: WorkspaceDocument;
  assetId: string;
  reused: boolean;
}

export interface AssetFolderMutation {
  document: WorkspaceDocument;
  assetId: string;
  folderPath: string | null;
}

export interface AssetReferencesMutation {
  document: WorkspaceDocument;
  originalAssetId: string;
  replacementAssetId: string;
  replacedInstanceCount: number;
  replacedTreatmentCount: number;
}

export interface AssetDeletionMutation {
  document: WorkspaceDocument;
  deletedAssetId: string;
}

export interface AssetCleanupMutation {
  document: WorkspaceDocument;
  removedAssetIds: string[];
}

export interface TreatmentMutation {
  document: WorkspaceDocument;
  treatmentId: string;
}

export interface ConfirmImageImportInput {
  side: "front" | "back";
  layer: ProductionLayer;
  originalFilename: string;
  mediaType: string;
  pixelWidth: number;
  pixelHeight: number;
  bytes: number[];
  recipe: TreatmentRecipe;
  productionMode: ImageProductionMode;
  placementCenterUm?: {
    xUm: number;
    yUm: number;
  };
}

export interface ConfirmedImageImport {
  document: WorkspaceDocument;
  assetId: string;
  treatmentId: string;
  layerId: string;
}

export type SamplingPurpose =
  | "interactiveProxy"
  | "boardPreview"
  | "formalProduction";

export interface TreatmentCompileReport {
  widthPx: number;
  heightPx: number;
  appliedThreshold: number;
  maskSha256: string;
  previewPngDataUrl: string;
  pixelPitchUm: number;
  recipeFingerprint: string;
  revision: number;
  purpose: SamplingPurpose;
  topology: { islandCount: number; holeCount: number };
  diagnostics: Array<Record<string, unknown>>;
}

export interface ImagePreviewSource {
  sourceHandle: string;
  sourceSha256: string;
  widthPx: number;
  heightPx: number;
  mediaType: string;
  workspaceRevision: number;
}

export interface ImagePreviewDiagnostics {
  sourceBytes: number;
  prepareCount: number;
  proxyCompileCount: number;
  activeSessions: number;
  preparedResidentBytes: number;
  coalesceCount: number;
  cancelCount: number;
  active: number;
  pending: number;
}

export interface ManufacturerValidation {
  profile: ManufacturerProfileSnapshot;
  valid: boolean;
  errors: string[];
}

export interface ProductionTrace {
  format: "atelier-production-trace-v1";
  revision: number;
  coordinateSpace: "boardPhysicalUpright";
  manufacturerProfile: ManufacturerProfileSnapshot;
  manufacturerProfileFingerprint: string;
  fabricationInputSha256: string;
  fabricationOutputSha256: string;
  layers: Array<{
    target: { side: "front" | "back"; layer: ProductionLayer };
    polarity: "positive" | "opening";
    compositeSha256: string;
    boundsUm: {
      minXUm: number;
      minYUm: number;
      maxXUm: number;
      maxYUm: number;
    } | null;
    topology: {
      islandCount: number;
      holeCount: number;
    };
  }>;
  operations: Array<{
    mappingId: string;
    sourceLayerId: string;
    target: { side: "front" | "back"; layer: ProductionLayer };
    maskSha256: string;
    assetId: string | null;
    assetSha256: string | null;
    treatmentId: string | null;
    algorithmVersion: string | null;
    recipeFingerprint: string | null;
  }>;
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
  return (
    typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
  );
}

export function isDesktopRuntime(): boolean {
  return isTauriRuntime();
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

export async function selectAtelierProjectFile(): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  const selected = await open({
    directory: false,
    filters: [{ name: "PCB Atelier 工程", extensions: ["pcba"] }],
    multiple: false,
    title: "打开 PCB Atelier 工程",
  });
  return typeof selected === "string" ? selected : null;
}

export function openAtelierProject(path: string): Promise<WorkspaceDocument> {
  return invokeCore<WorkspaceDocument>("open_project", {
    request: { path },
  });
}

export function createNewAtelierProject(
  title = "未命名卡片",
  widthUm = 64_000,
  heightUm = 100_000,
): Promise<WorkspaceDocument> {
  return invokeCore<WorkspaceDocument>("new_project", {
    request: { title, widthUm, heightUm },
  });
}

export async function selectAtelierSaveFile(
  defaultName: string,
): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  const selected = await save({
    defaultPath: `${defaultName.replace(/[/:]/g, "-")}.pcba`,
    filters: [{ name: "PCB Atelier 工程", extensions: ["pcba"] }],
    title: "保存 PCB Atelier 工程",
  });
  return typeof selected === "string" ? selected : null;
}

export function saveAtelierProject(path: string): Promise<WorkspaceDocument> {
  return invokeCore<WorkspaceDocument>("save_project", {
    request: { path },
  });
}

export function openEasyedaProject(path: string): Promise<void> {
  if (!isTauriRuntime()) {
    return Promise.reject(new Error("仅桌面客户端支持调用嘉立创 EDA"));
  }
  return invoke<void>("open_easyeda_project", { path });
}

export function revealExportedProject(path: string): Promise<void> {
  if (!isTauriRuntime()) {
    return Promise.reject(new Error("仅桌面客户端支持在文件管理器中显示"));
  }
  return invoke<void>("reveal_exported_project", { path });
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

export function importProjectAsset(
  request: ProjectAssetInput,
): Promise<AssetMutation> {
  return invokeCore<AssetMutation>("import_project_asset", { request });
}

export function moveProjectAsset(
  assetId: string,
  folderPath: string | null,
): Promise<AssetFolderMutation> {
  return invokeCore<AssetFolderMutation>("move_project_asset", {
    request: { assetId, folderPath },
  });
}

export function replaceAllAssetReferences(
  originalAssetId: string,
  replacementAssetId: string,
): Promise<AssetReferencesMutation> {
  return invokeCore<AssetReferencesMutation>("replace_all_asset_references", {
    request: { originalAssetId, replacementAssetId },
  });
}

export function deleteProjectAsset(
  assetId: string,
): Promise<AssetDeletionMutation> {
  return invokeCore<AssetDeletionMutation>("delete_project_asset", {
    request: { assetId },
  });
}

export function cleanupUnusedAssets(): Promise<AssetCleanupMutation> {
  return invokeCore<AssetCleanupMutation>("cleanup_unused_assets");
}

export function insertImageTreatment(
  assetId: string,
  recipe: TreatmentRecipe,
  productionMode: ImageProductionMode = "monochromeMask",
): Promise<TreatmentMutation> {
  return invokeCore<TreatmentMutation>("insert_image_treatment", {
    request: { assetId, productionMode, recipe },
  });
}

export function setTreatmentRecipe(
  treatmentId: string,
  recipe: TreatmentRecipe,
): Promise<WorkspaceDocument> {
  return invokeCore<WorkspaceDocument>("set_treatment_recipe", {
    request: { treatmentId, recipe },
  });
}

export function setImageProductionMode(
  treatmentId: string,
  productionMode: ImageProductionMode,
): Promise<WorkspaceDocument> {
  return invokeCore<WorkspaceDocument>("set_image_production_mode", {
    request: { treatmentId, productionMode },
  });
}

export function compileImageTreatment(
  treatmentId: string,
  physicalWidthUm: number,
  physicalHeightUm: number,
  purpose: SamplingPurpose,
  pixelPitchUm?: number,
): Promise<TreatmentCompileReport> {
  return invokeCore<TreatmentCompileReport>("compile_image_treatment", {
    request: {
      treatmentId,
      physicalWidthUm,
      physicalHeightUm,
      purpose,
      pixelPitchUm: pixelPitchUm ?? null,
    },
  });
}

export function beginImagePreviewSource(request: {
  bytes?: number[];
  assetId?: string;
  mediaType?: string;
}): Promise<ImagePreviewSource> {
  return invokeCore<ImagePreviewSource>("begin_image_preview_source", {
    request,
  });
}

export function requestImagePreview(request: {
  sourceHandle: string;
  previewStreamId: string;
  generation: number;
  workspaceRevision: number;
  recipe: TreatmentRecipe;
  physicalWidthUm: number;
  physicalHeightUm: number;
  pixelPitchUm: number;
}): Promise<TreatmentCompileReport> {
  return invokeCore<TreatmentCompileReport>("request_image_preview", {
    request,
  });
}

export function releaseImagePreviewSource(
  sourceHandle: string,
): Promise<void> {
  return invokeCore<void>("release_image_preview_source", {
    request: { sourceHandle },
  });
}

export function getImagePreviewDiagnostics(): Promise<ImagePreviewDiagnostics> {
  return invokeCore<ImagePreviewDiagnostics>("get_image_preview_diagnostics");
}

export function confirmImageImport(
  request: ConfirmImageImportInput,
): Promise<ConfirmedImageImport> {
  return invokeCore<ConfirmedImageImport>("confirm_image_import", {
    request,
  });
}

export function validateManufacturerProfile(
  profile: ManufacturerProfileSnapshot,
): Promise<ManufacturerValidation> {
  return invokeCore<ManufacturerValidation>("validate_manufacturer_profile", {
    request: { profile },
  });
}

export function setManufacturerProfile(
  profile: ManufacturerProfileSnapshot,
): Promise<WorkspaceDocument> {
  return invokeCore<WorkspaceDocument>("set_manufacturer_profile", {
    request: { profile },
  });
}

export function getProductionTrace(): Promise<ProductionTrace> {
  return invokeCore<ProductionTrace>("get_production_trace");
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

export function deleteLayer(layerId: string): Promise<WorkspaceDocument> {
  return invokeCore<WorkspaceDocument>("delete_layer", {
    request: { layerId },
  });
}

export function deleteLayers(layerIds: string[]): Promise<WorkspaceDocument> {
  return invokeCore<WorkspaceDocument>("delete_layers", {
    request: { layerIds },
  });
}

export function duplicateLayer(layerId: string): Promise<LayerMutation> {
  return invokeCore<LayerMutation>("duplicate_layer", {
    request: { layerId },
  });
}

export function transferLayers(request: {
  layerIds: string[];
  targetSide: "front" | "back";
  targetLayer: ProductionLayer;
  newParentId: string | null;
  newIndex: number;
  mode: "copy" | "move";
  offsetUm: number;
}): Promise<LayersMutation> {
  return invokeCore<LayersMutation>("transfer_layers", { request });
}

export function pasteLayers(request: {
  layers: ContentLayer[];
  mappings: ProductionMapping[];
  targetSide: "front" | "back";
  targetLayer: ProductionLayer;
  newParentId: string | null;
  newIndex: number;
}): Promise<LayersMutation> {
  return invokeCore<LayersMutation>("paste_layers", { request });
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

export interface LayerTransformUpdate {
  layerId: string;
  transform: TransformUm;
}

export function transformLayers(
  transforms: LayerTransformUpdate[],
): Promise<WorkspaceDocument> {
  return invokeCore("transform_layers", { request: { transforms } });
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

export function moveLayer(
  layerId: string,
  newParentId: string | null,
  newIndex: number,
  side: "front" | "back",
  fromLayer: ProductionLayer,
  toLayer: ProductionLayer,
): Promise<WorkspaceDocument> {
  return invokeCore("move_layer", {
    request: {
      layerId,
      newParentId,
      newIndex,
      side,
      fromLayer,
      toLayer,
    },
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
