import {
  lazy,
  startTransition,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import brandIconUrl from "../../../../../assets/branding/pcb-atelier-logo.png";
import {
  Box,
  ChevronDown,
  Columns2,
  FolderOpen,
  FilePlus2,
  Focus,
  Gauge,
  ImagePlus,
  Layers3,
  Lock,
  Maximize2,
  Monitor,
  Moon,
  MousePointer2,
  Pencil,
  Rabbit,
  RotateCcw,
  RotateCw,
  Rows2,
  Save,
  Settings2,
  Sun,
  Turtle,
  Type,
  Ungroup,
  Unlock,
  X,
  type LucideIcon,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { ExportEasyedaButton } from "@/features/export/ExportEasyedaButton";
import {
  fitImportedImageSize,
  ImageImportDialog,
  type ImageImportDraft,
} from "@/features/image-treatment/ImageImportDialog";
import { ImageTreatmentEditor } from "@/features/image-treatment/ImageTreatmentEditor";
import { ManufacturerInspector } from "@/features/manufacturer/ManufacturerInspector";
import {
  isNativeImagePickerAvailable,
  selectSupportedImageFile,
} from "@/features/media/native-image-picker";
import { ProjectMediaDock } from "@/features/media/ProjectMediaLibrary";
import {
  isSupportedImageFileMetadata,
  readSupportedImageFile,
  SUPPORTED_IMAGE_FILE_ACCEPT,
} from "@/features/media/supported-image-file";
import type { BoardPreviewInput } from "@/features/preview/board-preview-renderer";
import {
  loadAppSettings,
  saveAppSettings,
  type AppSettings,
  type CanvasView,
  type LaunchWindowMode,
  type WheelZoomDamping,
} from "@/features/settings/app-settings";
import { useTheme } from "@/features/theme/ThemeProvider";
import type { ThemePreference } from "@/features/theme/theme-state";
import {
  createCommandRegistry,
  isEditorShortcutSuppressed,
  type EditorCommandScope,
} from "@/features/workspace/command-registry";
import {
  applyGroupTransform,
  applyTransformPatch,
  isLayerTransformEditable,
  nudgeTransform,
  parseDegreesToMdeg,
  parseMillimetresToUm,
  resizeWithLockedAspectRatio,
  type TransformPatch,
} from "@/features/workspace/geometry-edit";
import { ProductionLayerTree } from "@/features/workspace/ProductionLayerTree";
import {
  createProductionInspectionState,
  isProductionLayerRendered,
  toggleProductionIsolation,
  toggleProductionVisibility,
  type ProductionInspectionState,
} from "@/features/workspace/production-inspection";
import {
  getFitViewport,
  WorkspaceCanvas,
} from "@/features/workspace/WorkspaceCanvas";
import {
  cycleOverlappingSelection,
  resolveLayerSelection,
  resolveTreeLayerSelection,
} from "@/features/workspace/layer-selection";
import {
  createInitialWorkspaceState,
  workspaceReducer,
  type CardFace,
  type WorkContext,
  type WorkspaceMode,
  type WorkspaceTool,
} from "@/features/workspace/workspace-state";
import {
  resizeWorkspacePanelWidth,
  type WorkspacePanelSide,
} from "@/features/workspace/workspace-panel-layout";
import type { TextDraft } from "@/features/workspace/text-gesture";
import {
  insertImageAsset,
  insertTextLayer,
  createBoardFill,
  createNewAtelierProject,
  deleteLayers,
  duplicateLayer,
  getAssetBytes,
  getBoardPreview,
  getSystemFonts,
  getWorkspaceDocument,
  groupLayers,
  importProjectAsset,
  mapLayer,
  moveProjectAsset,
  moveLayer,
  isDesktopRuntime,
  openAtelierProject,
  pasteLayers,
  redoWorkspace,
  reorderLayer,
  selectAtelierProjectFile,
  selectAtelierSaveFile,
  setLayerLock,
  setLayerName,
  setLayerExportEnabled,
  setLayerVisibility,
  setManufacturerProfile,
  setImageProductionMode,
  setBoardOutline,
  setTextContent,
  setTextStyle,
  setTreatmentRecipe,
  saveAtelierProject,
  transformLayer,
  transformLayers,
  transferLayers,
  ungroupLayer,
  unmapLayer,
  undoWorkspace,
  type ContentLayer,
  type CoreInfo,
  type ProductionMapping,
  type TreatmentCompileReport,
  type WorkspaceDocument,
} from "@/lib/core";
import { cn } from "@/lib/utils";

const Board3DPreview = lazy(() =>
  import("@/features/preview/Board3DPreview").then((module) => ({
    default: module.Board3DPreview,
  })),
);

interface WorkspaceShellProps {
  core: CoreInfo;
  document: WorkspaceDocument;
}

interface WorkspaceCommandContext {
  face?: CardFace;
  layerId?: string;
  layerIds?: string[];
  largeStep?: boolean;
  mappingId?: string;
  name?: string;
  productionLayer?: WorkContext;
  transform?: ContentLayer["transform"];
  value?: boolean;
}

const TOOLS: Array<{
  id: WorkspaceTool;
  label: string;
  shortcut: string;
  icon: typeof MousePointer2;
}> = [
  { id: "select", label: "选择", shortcut: "V", icon: MousePointer2 },
  { id: "text", label: "文字", shortcut: "T", icon: Type },
  { id: "image", label: "图片", shortcut: "P", icon: ImagePlus },
];

const WORKSPACE_MODES: Array<{
  id: WorkspaceMode;
  label: string;
  icon: typeof Layers3;
}> = [
  { id: "edit", label: "编辑", icon: Pencil },
  { id: "preview", label: "预览", icon: Box },
];

export function WorkspaceShell({
  core,
  document: initialDocument,
}: WorkspaceShellProps) {
  const { preference: themePreference, setPreference: setThemePreference } =
    useTheme();
  const [sessionDocument, setSessionDocument] =
    useState<WorkspaceDocument>(initialDocument);
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [projectMenuOpen, setProjectMenuOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [canvasContextMenu, setCanvasContextMenu] = useState<{
    face: CardFace;
    layerId: string | null;
    x: number;
    y: number;
  } | null>(null);
  const [contextRenameValue, setContextRenameValue] = useState("");
  const [showProjectHome, setShowProjectHome] = useState(isDesktopRuntime);
  const [appSettings, setAppSettings] = useState(loadAppSettings);
  const [workspace, dispatch] = useReducer(
    workspaceReducer,
    appSettings,
    (settings) =>
      createInitialWorkspaceState(
        settings.canvasView === "vertical"
          ? "vertical"
          : "horizontal",
        settings.canvasView === "focus-active" ? "focus" : "both",
      ),
  );
  const [editingLayerId, setEditingLayerId] = useState<string | null>(null);
  const [selectedAspectRatioLocked, setSelectedAspectRatioLocked] =
    useState(true);
  const [treatmentReports, setTreatmentReports] = useState<
    Record<string, TreatmentCompileReport>
  >({});
  const [originalPreviewLayerIds, setOriginalPreviewLayerIds] = useState<
    Set<string>
  >(new Set());
  const [imageImportDraft, setImageImportDraft] =
    useState<ImageImportDraft | null>(null);
  const [pendingDelete, setPendingDelete] = useState<{
    layerIds: string[];
    mappingCount: number;
  } | null>(null);
  const [replaceLayerId, setReplaceLayerId] = useState<string | null>(null);
  const [status, setStatus] = useState("就绪");
  const [boardPreview, setBoardPreview] = useState<BoardPreviewInput | null>(
    null,
  );
  const [boardPreviewError, setBoardPreviewError] = useState<string | null>(
    null,
  );
  const [boardPreviewPending, setBoardPreviewPending] = useState(false);
  const [fontFamilies, setFontFamilies] = useState<string[]>(["sans-serif"]);
  const [drillGroupIds, setDrillGroupIds] = useState<
    Record<CardFace, string | null>
  >({ front: null, back: null });
  const [productionInspection, setProductionInspection] = useState(
    createProductionInspectionState,
  );
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [layerClipboard, setLayerClipboard] = useState<
    | {
        mode: "copy";
        sourceFace: CardFace;
        layerIds: string[];
      }
    | {
        mode: "cut";
        sourceFace: CardFace;
        layers: ContentLayer[];
        mappings: ProductionMapping[];
      }
    | null
  >(null);

  const setWheelZoomDamping = (wheelZoomDamping: WheelZoomDamping) => {
    const next = { ...appSettings, wheelZoomDamping };
    setAppSettings(next);
    saveAppSettings(next);
  };

  const setLaunchWindowMode = (launchWindowMode: LaunchWindowMode) => {
    const next = { ...appSettings, launchWindowMode };
    setAppSettings(next);
    saveAppSettings(next);
  };

  const setWorkspacePanelWidth = (
    side: WorkspacePanelSide,
    width: number,
    persist: boolean,
  ) => {
    setAppSettings((current) => {
      const next = {
        ...current,
        [side === "left"
          ? "workspaceLeftPanelWidth"
          : "workspaceRightPanelWidth"]: width,
      };
      if (persist) saveAppSettings(next);
      return next;
    });
  };

  const setBoardView = (
    value: CanvasView,
  ) => {
    const editLayout: "focus" | "both" =
      value === "focus-active" ? "focus" : "both";
    const boardArrangement =
      value === "vertical" ? "vertical" : workspace.boardArrangement;
    const resolvedArrangement =
      value === "horizontal" ? "horizontal" : boardArrangement;
    dispatch({ type: "setEditLayout", editLayout });
    dispatch({
      type: "setBoardArrangement",
      boardArrangement: resolvedArrangement,
    });
    const next = {
      ...appSettings,
      canvasView: value,
    };
    setAppSettings(next);
    saveAppSettings(next);
  };

  useEffect(() => {
    if (!settingsOpen) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      setSettingsOpen(false);
    };
    globalThis.addEventListener("keydown", closeOnEscape);
    return () => globalThis.removeEventListener("keydown", closeOnEscape);
  }, [settingsOpen]);

  useEffect(() => {
    if (!pendingDelete) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      setPendingDelete(null);
    };
    globalThis.addEventListener("keydown", closeOnEscape);
    return () => globalThis.removeEventListener("keydown", closeOnEscape);
  }, [pendingDelete]);

  useEffect(() => {
    if (!canvasContextMenu) return;
    const close = (event: PointerEvent) => {
      if (
        event.target instanceof Element &&
        event.target.closest("[data-canvas-context-menu]")
      ) {
        return;
      }
      setCanvasContextMenu(null);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setCanvasContextMenu(null);
    };
    globalThis.addEventListener("pointerdown", close);
    globalThis.addEventListener("keydown", closeOnEscape);
    return () => {
      globalThis.removeEventListener("pointerdown", close);
      globalThis.removeEventListener("keydown", closeOnEscape);
    };
  }, [canvasContextMenu]);
  useEffect(() => {
    setCanvasContextMenu(null);
  }, [
    sessionDocument,
    workspace.editLayout,
    workspace.viewports.back,
    workspace.viewports.front,
    workspace.workspaceMode,
  ]);

  useEffect(() => {
    let active = true;
    void getSystemFonts()
      .then((catalog) => {
        if (active) setFontFamilies(catalog.families);
      })
      .catch(() => {
        // The embedded font remains available when system enumeration fails.
      });
    return () => {
      active = false;
    };
  }, []);
  const pendingTransformsRef = useRef(
    new Map<string, ContentLayer["transform"]>(),
  );
  const transformQueueRef = useRef<Promise<void>>(Promise.resolve());
  const treeSelectionAnchorRef = useRef<Record<CardFace, string | null>>({
    front: null,
    back: null,
  });
  const viewport = workspace.viewports[workspace.activeFace];
  const currentLayers =
    workspace.activeFace === "front"
      ? sessionDocument.frontLayers
      : sessionDocument.backLayers;
  const selectedIds = workspace.selections[workspace.activeFace];
  const selectedLayer =
    workspace.inspectorTarget === "face"
      ? currentLayers.find((layer) => selectedIds.includes(layer.id))
      : undefined;
  const selectedImageLayer =
    selectedLayer?.kind.type === "image" ? selectedLayer : undefined;
  const selectedImageAssetId =
    selectedImageLayer?.kind.type === "image"
      ? selectedImageLayer.kind.assetId
      : undefined;
  const selectedImageAsset =
    selectedImageAssetId
      ? sessionDocument.assets.find(
          (asset) => asset.id === selectedImageAssetId,
        )
      : undefined;
  const selectedImageTreatment =
    selectedImageLayer && selectedImageAssetId
      ? (() => {
          const treatmentId = sessionDocument.mappings.find(
            (mapping) =>
              mapping.sourceLayerId === selectedImageLayer.id &&
              mapping.treatmentId,
          )?.treatmentId;
          return sessionDocument.imageTreatments.find(
            (treatment) =>
              treatment.id === treatmentId ||
              (!treatmentId &&
                treatment.assetId === selectedImageAssetId),
          );
        })()
      : undefined;
  const selectedImageMappings = selectedImageLayer
    ? sessionDocument.mappings.filter(
        (mapping) => mapping.sourceLayerId === selectedImageLayer.id,
      )
    : [];
  const selectedColorOriginalAvailability = selectedImageAsset
    ? getColorOriginalAvailability({
        mediaType: selectedImageAsset.mediaType,
        profile: sessionDocument.manufacturerProfile,
        productionLayers: selectedImageMappings.map(
          (mapping) => mapping.target.layer,
        ),
      })
    : { available: false, reason: "请先选择图片对象" };
  useEffect(() => {
    setSelectedAspectRatioLocked(true);
    setOriginalPreviewLayerIds(new Set());
  }, [selectedLayer?.id]);
  const contextMenuLayer = canvasContextMenu?.layerId
    ? (canvasContextMenu.face === "front"
        ? sessionDocument.frontLayers
        : sessionDocument.backLayers
      ).find((layer) => layer.id === canvasContextMenu.layerId)
    : undefined;
  const contextMenuCurrentMapping =
    canvasContextMenu && contextMenuLayer
      ? sessionDocument.mappings.find(
          (mapping) =>
            mapping.sourceLayerId === contextMenuLayer.id &&
            mapping.target.side === canvasContextMenu.face &&
            mapping.target.layer ===
              workspace.workContexts[canvasContextMenu.face],
        )
      : undefined;
  const widthMm = sessionDocument.board.widthUm / 1_000;
  const heightMm = sessionDocument.board.heightUm / 1_000;
  const fitCanvas = useCallback(
    (face: CardFace) => {
      const canvas = globalThis.document.querySelector<HTMLElement>(
        `[data-testid="workspace-canvas-${face}"]`,
      );
      if (!canvas) return;
      dispatch({
        type: "setViewport",
        face,
        viewport: getFitViewport({
          boardHeightMm: heightMm,
          boardWidthMm: widthMm,
          canvasHeightPx: canvas.clientHeight,
          canvasWidthPx: canvas.clientWidth,
        }),
      });
    },
    [heightMm, widthMm],
  );
  useEffect(() => {
    let fitFrame = 0;
    const frame = globalThis.requestAnimationFrame(() => {
      fitFrame = globalThis.requestAnimationFrame(() => {
        fitCanvas("front");
        fitCanvas("back");
      });
    });
    return () => {
      globalThis.cancelAnimationFrame(frame);
      globalThis.cancelAnimationFrame(fitFrame);
    };
  }, [fitCanvas, workspace.boardArrangement, workspace.editLayout]);
  const selectOnly = useCallback((face: CardFace, layerId: string) => {
    dispatch({
      type: "setSelection",
      face,
      layerIds: [layerId],
    });
  }, []);

  const selectLayer = useCallback(
    (face: CardFace, layerId: string, shiftKey = false) => {
      const faceLayers =
        face === "front"
          ? sessionDocument.frontLayers
          : sessionDocument.backLayers;
      const faceSelection = workspace.selections[face];
      const clicked = faceLayers.find((layer) => layer.id === layerId);
      const drillIntoGroup =
        clicked?.parentId !== null &&
        clicked?.parentId !== undefined &&
        drillGroupIds[face] === clicked.parentId;
      const next = resolveLayerSelection({
        current: faceSelection,
        drillIntoGroup,
        layerId,
        layers: faceLayers,
        shiftKey,
      });
      if (clicked?.parentId && next.includes(clicked.parentId)) {
        setDrillGroupIds((current) => ({
          ...current,
          [face]: clicked.parentId,
        }));
      }
      dispatch({
        type: "setSelection",
        face,
        layerIds: next,
      });
    },
    [
      drillGroupIds,
      sessionDocument.backLayers,
      sessionDocument.frontLayers,
      workspace.selections,
    ],
  );

  const applyDocumentMutation = useCallback(
    async (operation: () => Promise<WorkspaceDocument>, success: string) => {
      try {
        setStatus("正在更新工程…");
        setSessionDocument(await operation());
        setStatus(success);
      } catch (error) {
        setStatus(`操作失败：${errorMessage(error)}`);
      }
    },
    [],
  );

  const applyLayerTransform = useCallback(
    (layerId: string, transform: ContentLayer["transform"]) => {
      pendingTransformsRef.current.set(layerId, transform);
      setSessionDocument((current) => {
        const updateLayers = (layers: ContentLayer[]) => {
          const target = layers.find((layer) => layer.id === layerId);
          if (target?.kind.type === "group") {
            return applyGroupTransform(layers, layerId, transform);
          }
          return layers.map((layer) =>
            layer.id === layerId ? { ...layer, transform } : layer,
          );
        };
        return {
          ...current,
          frontLayers: updateLayers(current.frontLayers),
          backLayers: updateLayers(current.backLayers),
          history: { ...current.history, canUndo: true, canRedo: false },
        };
      });
      transformQueueRef.current = transformQueueRef.current
        .then(async () => {
          const document = await transformLayer(layerId, transform);
          if (pendingTransformsRef.current.get(layerId) === transform) {
            pendingTransformsRef.current.delete(layerId);
            setSessionDocument(document);
            setStatus("变换已更新");
          }
        })
        .catch(async (error: unknown) => {
          if (pendingTransformsRef.current.get(layerId) === transform) {
            pendingTransformsRef.current.delete(layerId);
          }
          setStatus(`操作失败：${errorMessage(error)}`);
          try {
            setSessionDocument(await getWorkspaceDocument());
          } catch {
            // Keep the optimistic document visible when Core cannot be read.
          }
        });
    },
    [],
  );

  const applyLayerTransforms = useCallback(
    (
      transforms: Array<{
        layerId: string;
        transform: ContentLayer["transform"];
      }>,
    ) => {
      for (const update of transforms) {
        pendingTransformsRef.current.set(update.layerId, update.transform);
      }
      const byId = new Map(
        transforms.map((update) => [update.layerId, update.transform]),
      );
      setSessionDocument((current) => ({
        ...current,
        frontLayers: current.frontLayers.map((layer) => {
          const transform = byId.get(layer.id);
          return transform ? { ...layer, transform } : layer;
        }),
        backLayers: current.backLayers.map((layer) => {
          const transform = byId.get(layer.id);
          return transform ? { ...layer, transform } : layer;
        }),
        history: { ...current.history, canUndo: true, canRedo: false },
      }));
      transformQueueRef.current = transformQueueRef.current
        .then(async () => {
          const document = await transformLayers(transforms);
          const isLatest = transforms.every(
            (update) =>
              pendingTransformsRef.current.get(update.layerId) ===
              update.transform,
          );
          if (!isLatest) return;
          for (const update of transforms) {
            pendingTransformsRef.current.delete(update.layerId);
          }
          setSessionDocument(document);
          setStatus(`已移动 ${transforms.length} 个对象`);
        })
        .catch(async (error: unknown) => {
          for (const update of transforms) {
            if (
              pendingTransformsRef.current.get(update.layerId) ===
              update.transform
            ) {
              pendingTransformsRef.current.delete(update.layerId);
            }
          }
          setStatus(`操作失败：${errorMessage(error)}`);
          try {
            setSessionDocument(await getWorkspaceDocument());
          } catch {
            // Keep the optimistic document visible when Core cannot be read.
          }
        });
    },
    [],
  );

  const importImage = useCallback(
    async (
      file: File,
      replacementId: string | null = null,
      target?: {
        face: CardFace;
        productionLayer: WorkContext;
        placementCenterUm?: { xUm: number; yUm: number };
      },
    ) => {
      if (!isSupportedImageFileMetadata(file)) {
        setStatus("仅支持 PNG、JPEG 或 WebP 图片");
        return;
      }
      setStatus(replacementId ? "正在替换图片…" : "正在导入图片…");
      try {
        const face = target?.face ?? workspace.activeFace;
        const validated = await readSupportedImageFile(file);
        const dimensions = await readImageDimensions(file);
        const bytes = validated.bytes;
        if (replacementId === null) {
          const physical = fitImportedImageSize(
            sessionDocument.board,
            dimensions,
          );
          setImageImportDraft({
            draftId: globalThis.crypto.randomUUID(),
            side: face,
            productionLayer:
              target?.productionLayer ?? workspace.workContexts[face],
            originalFilename: file.name || "clipboard-image.png",
            mediaType: validated.mediaType,
            pixelWidth: dimensions.width,
            pixelHeight: dimensions.height,
            bytes,
            physicalWidthUm: physical.widthUm,
            physicalHeightUm: physical.heightUm,
            placementCenterUm: target?.placementCenterUm,
          });
          setStatus("请先确认图片处理结果");
          return;
        }
        const result = await insertImageAsset({
          side: face,
          originalFilename: file.name || "clipboard-image.png",
          mediaType: validated.mediaType,
          pixelWidth: dimensions.width,
          pixelHeight: dimensions.height,
          bytes,
          replaceLayerId: replacementId,
          placementCenterUm: target?.placementCenterUm,
        });
        const productionLayer =
          replacementId === null
            ? (target?.productionLayer ?? workspace.workContexts[face])
            : null;
        const document = productionLayer
          ? await mapLayer(result.layerId, face, productionLayer)
          : result.document;
        setSessionDocument(document);
        selectOnly(face, result.layerId);
        setEditingLayerId(null);
        dispatch({ type: "setTool", tool: "select" });
        setStatus(
          replacementId
            ? "图片已替换"
            : target?.placementCenterUm
              ? "图片已放置到拖放位置"
              : "图片已插入并居中",
        );
      } catch (error) {
        setStatus(`图片导入失败：${errorMessage(error)}`);
      }
    },
    [
      selectOnly,
      sessionDocument.board,
      workspace.activeFace,
      workspace.workContexts,
    ],
  );

  const requestImageFile = useCallback(
    async (replacementId: string | null) => {
      setReplaceLayerId(replacementId);
      if (!isNativeImagePickerAvailable()) {
        fileInputRef.current?.click();
        setStatus(replacementId ? "请选择替换图片" : "请选择要插入的图片");
        return;
      }

      setStatus(replacementId ? "请选择替换图片" : "请选择要插入的图片");
      try {
        const file = await selectSupportedImageFile();
        if (file) await importImage(file, replacementId);
      } catch (error) {
        setStatus(`图片选择失败：${errorMessage(error)}`);
      } finally {
        setReplaceLayerId(null);
      }
    },
    [importImage],
  );

  const importFilesToProjectMedia = useCallback(async (files: File[]) => {
    const images = files.filter(isSupportedImageFileMetadata);
    if (images.length === 0) {
      setStatus("仅支持 PNG、JPEG 或 WebP 图片");
      return;
    }
    setStatus(`正在导入 ${images.length} 个项目素材…`);
    try {
      let latestDocument: WorkspaceDocument | null = null;
      for (const file of images) {
        const validated = await readSupportedImageFile(file);
        const dimensions = await readImageDimensions(file);
        const result = await importProjectAsset({
          originalFilename: file.name || "untitled-image.png",
          mediaType: validated.mediaType,
          pixelWidth: dimensions.width,
          pixelHeight: dimensions.height,
          bytes: validated.bytes,
        });
        latestDocument = result.document;
      }
      if (latestDocument) setSessionDocument(latestDocument);
      setStatus(`${images.length} 个素材已导入媒体库`);
    } catch (error) {
      setStatus(`素材导入失败：${errorMessage(error)}`);
    }
  }, []);

  const moveAssetToFolder = useCallback(
    async (assetId: string, folderPath: string | null) => {
      try {
        const result = await moveProjectAsset(assetId, folderPath);
        setSessionDocument(result.document);
        setStatus(
          result.folderPath
            ? `素材已移至 ${result.folderPath}`
            : "素材已移至未分类",
        );
      } catch (error) {
        setStatus(`无法整理素材：${errorMessage(error)}`);
      }
    },
    [],
  );

  const placeProjectAsset = useCallback(
    async ({
      assetId,
      face,
      productionLayer,
      placementCenterUm,
    }: {
      assetId: string;
      face: CardFace;
      productionLayer: WorkContext;
      placementCenterUm?: { xUm: number; yUm: number };
    }) => {
      const asset = sessionDocument.assets.find((item) => item.id === assetId);
      if (!asset) {
        setStatus("素材已不存在，请刷新媒体库");
        return;
      }
      setStatus(`正在放置 ${asset.originalFilename}…`);
      try {
        const embedded = await getAssetBytes(assetId);
        const physical = fitImportedImageSize(sessionDocument.board, {
          width: asset.pixelWidth,
          height: asset.pixelHeight,
        });
        setImageImportDraft({
          draftId: globalThis.crypto.randomUUID(),
          side: face,
          originalFilename: asset.originalFilename,
          mediaType: embedded.mediaType,
          pixelWidth: asset.pixelWidth,
          pixelHeight: asset.pixelHeight,
          bytes: embedded.bytes,
          productionLayer,
          physicalWidthUm: physical.widthUm,
          physicalHeightUm: physical.heightUm,
          placementCenterUm,
        });
        dispatch({ type: "setFace", face });
        dispatch({
          type: "setWorkContext",
          face,
          workContext: productionLayer,
        });
        setStatus("请先确认图片处理结果");
      } catch (error) {
        setStatus(`素材放置失败：${errorMessage(error)}`);
      }
    },
    [sessionDocument.assets, sessionDocument.board],
  );

  useEffect(() => {
    const handlePaste = (event: ClipboardEvent) => {
      const image = [...(event.clipboardData?.files ?? [])].find(
        isSupportedImageFileMetadata,
      );
      if (!image) return;
      event.preventDefault();
      void importImage(image);
    };
    globalThis.document.addEventListener("paste", handlePaste);
    return () => globalThis.document.removeEventListener("paste", handlePaste);
  }, [importImage]);

  useEffect(() => {
    if (workspace.workspaceMode !== "preview") return;
    let cancelled = false;
    setBoardPreviewError(null);
    setBoardPreviewPending(true);
    void getBoardPreview()
      .then((preview) => {
        if (!cancelled) {
          startTransition(() => setBoardPreview(preview));
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) setBoardPreviewError(errorMessage(error));
      })
      .finally(() => {
        if (!cancelled) setBoardPreviewPending(false);
      });
    return () => {
      cancelled = true;
    };
  }, [sessionDocument, workspace.workspaceMode]);

  const groupSelection = useCallback(async () => {
    try {
      const result = await groupLayers(workspace.activeFace, selectedIds);
      setSessionDocument(result.document);
      selectOnly(workspace.activeFace, result.layerId);
      setStatus("已分组");
    } catch (error) {
      setStatus(`分组失败：${errorMessage(error)}`);
    }
  }, [selectOnly, selectedIds, workspace.activeFace]);

  const ungroupSelection = useCallback(async () => {
    if (!selectedLayer || selectedLayer.kind.type !== "group") return;
    await applyDocumentMutation(
      () => ungroupLayer(selectedLayer.id),
      "已解组",
    );
  }, [applyDocumentMutation, selectedLayer]);

  const performDelete = useCallback(async (layerIds: string[]) => {
    if (layerIds.length === 0) return;
    try {
      const document = await deleteLayers(layerIds);
      setSessionDocument(document);
      dispatch({
        type: "setSelection",
        face: workspace.activeFace,
        layerIds: [],
      });
      setEditingLayerId(null);
      setStatus(
        layerIds.length === 1
          ? "对象及其全部生产层映射已删除"
          : `${layerIds.length} 个对象及其映射已删除`,
      );
    } catch (error) {
      setStatus(`删除失败：${errorMessage(error)}`);
    }
  }, [workspace.activeFace]);

  const deleteSelection = useCallback(() => {
    if (selectedIds.length === 0) return;
    const mappingCount = sessionDocument.mappings.filter((mapping) =>
      selectedIds.includes(mapping.sourceLayerId),
    ).length;
    if (selectedIds.length > 1 || mappingCount > 1) {
      setPendingDelete({ layerIds: [...selectedIds], mappingCount });
      return;
    }
    void performDelete(selectedIds);
  }, [performDelete, selectedIds, sessionDocument.mappings]);

  const duplicateSourceLayer = useCallback(
    async (layerId: string) => {
      try {
        const result = await duplicateLayer(layerId);
        setSessionDocument(result.document);
        selectOnly(workspace.activeFace, result.layerId);
        setEditingLayerId(null);
        setStatus("已创建副本，素材、处理版本与合法映射保持复用");
      } catch (error) {
        setStatus(`创建副本失败：${errorMessage(error)}`);
      }
    },
    [selectOnly, workspace.activeFace],
  );

  const cutSelectionToClipboard = useCallback(async () => {
    if (selectedIds.length === 0) return;
    const cutIds = new Set(selectedIds);
    let changed = true;
    while (changed) {
      changed = false;
      for (const layer of currentLayers) {
        if (
          layer.parentId &&
          cutIds.has(layer.parentId) &&
          !cutIds.has(layer.id)
        ) {
          cutIds.add(layer.id);
          changed = true;
        }
      }
    }
    const layers = currentLayers.filter((layer) => cutIds.has(layer.id));
    const mappings = sessionDocument.mappings.filter((mapping) =>
      cutIds.has(mapping.sourceLayerId),
    );
    try {
      const document = await deleteLayers(selectedIds);
      setSessionDocument(document);
      setLayerClipboard({
        mode: "cut",
        sourceFace: workspace.activeFace,
        layers,
        mappings,
      });
      dispatch({
        type: "setSelection",
        face: workspace.activeFace,
        layerIds: [],
      });
      setEditingLayerId(null);
      setStatus("对象已剪切");
    } catch (error) {
      setStatus(`剪切失败：${errorMessage(error)}`);
    }
  }, [
    currentLayers,
    selectedIds,
    sessionDocument.mappings,
    workspace.activeFace,
  ]);

  const pasteLayerClipboard = useCallback(async () => {
    if (!layerClipboard) return;
    const targetFace = workspace.activeFace;
    const targetLayers =
      targetFace === "front"
        ? sessionDocument.frontLayers
        : sessionDocument.backLayers;
    try {
      const result =
        layerClipboard.mode === "copy"
          ? await transferLayers({
              layerIds: layerClipboard.layerIds,
              targetSide: targetFace,
              targetLayer: workspace.workContexts[targetFace],
              newParentId: null,
              newIndex: targetLayers.length,
              mode: "copy",
              offsetUm:
                layerClipboard.sourceFace === targetFace ? 2_000 : 0,
            })
          : await (() => {
              const cutIds = new Set(
                layerClipboard.layers.map((layer) => layer.id),
              );
              const rootIds = layerClipboard.layers
                .filter(
                  (layer) =>
                    !layer.parentId || !cutIds.has(layer.parentId),
                )
                .map((layer) => layer.id);
              const currentIds = new Set(
                sessionDocument.frontLayers
                  .concat(sessionDocument.backLayers)
                  .map((layer) => layer.id),
              );
              if (rootIds.every((layerId) => currentIds.has(layerId))) {
                return transferLayers({
                  layerIds: rootIds,
                  targetSide: targetFace,
                  targetLayer: workspace.workContexts[targetFace],
                  newParentId: null,
                  newIndex: targetLayers.length,
                  mode: "copy",
                  offsetUm:
                    layerClipboard.sourceFace === targetFace ? 2_000 : 0,
                });
              }
              return pasteLayers({
                layers: layerClipboard.layers,
                mappings: layerClipboard.mappings,
                targetSide: targetFace,
                targetLayer: workspace.workContexts[targetFace],
                newParentId: null,
                newIndex: targetLayers.length,
              });
            })();
      setSessionDocument(result.document);
      dispatch({ type: "setFace", face: targetFace });
      dispatch({
        type: "setSelection",
        face: targetFace,
        layerIds: result.layerIds,
      });
      if (layerClipboard.mode === "cut") setLayerClipboard(null);
      setEditingLayerId(null);
      setStatus(
        layerClipboard.mode === "copy"
          ? `已粘贴到${targetFace === "front" ? "正面" : "背面"}`
          : `已移动到${targetFace === "front" ? "正面" : "背面"}`,
      );
    } catch (error) {
      setStatus(`粘贴失败：${errorMessage(error)}`);
    }
  }, [
    layerClipboard,
    sessionDocument.backLayers,
    sessionDocument.frontLayers,
    workspace.activeFace,
    workspace.workContexts,
  ]);

  const focusProjectMedia = useCallback(() => {
    const media = globalThis.document.querySelector<HTMLElement>(
      '[aria-label="项目媒体"]',
    );
    media
      ?.querySelector<HTMLButtonElement>('[aria-label="展开项目媒体"]')
      ?.click();
    globalThis.requestAnimationFrame(() => {
      globalThis.document
        .querySelector<HTMLInputElement>('[aria-label="搜索项目媒体"]')
        ?.focus();
    });
  }, []);

  const nudgeSelection = useCallback(
    (
      direction: "left" | "right" | "up" | "down",
      largeStep: boolean,
    ) => {
      if (
        !selectedLayer ||
        selectedIds.length !== 1 ||
        selectedLayer.kind.type === "boardFill" ||
        selectedLayer.locked
      ) {
        return;
      }
      try {
        const next = nudgeTransform(
          {
            ...selectedLayer,
            transform:
              pendingTransformsRef.current.get(selectedLayer.id) ??
              selectedLayer.transform,
          },
          currentLayers,
          direction,
          largeStep,
        );
        applyLayerTransform(selectedLayer.id, next);
      } catch (error) {
        setStatus(`无法变换：${errorMessage(error)}`);
      }
    },
    [applyLayerTransform, currentLayers, selectedIds.length, selectedLayer],
  );

  const commandRegistry = useMemo(
    () =>
      createCommandRegistry<WorkspaceCommandContext>([
        {
          id: "tool.select",
          title: "选择工具",
          scope: "canvas",
          shortcuts: ["V"],
          execute: () => dispatch({ type: "setTool", tool: "select" }),
        },
        {
          id: "tool.text",
          title: "文字工具",
          scope: "canvas",
          shortcuts: ["T"],
          execute: () => dispatch({ type: "setTool", tool: "text" }),
        },
        {
          id: "tool.image",
          title: "图片与素材",
          scope: "canvas",
          shortcuts: ["P"],
          execute: (_context, invocation) => {
            dispatch({ type: "setTool", tool: "image" });
            if (invocation.source === "toolbar") {
              void requestImageFile(null);
              return;
            }
            focusProjectMedia();
            setStatus("图片工具已激活，请从项目素材库选择或导入素材");
          },
        },
        {
          id: "selection.delete",
          title: "删除对象",
          scope: "selection",
          shortcuts: ["Delete", "Backspace"],
          isEnabled: () =>
            selectedIds.length > 0 &&
            selectedIds.every(
              (id) => !currentLayers.find((layer) => layer.id === id)?.locked,
            ),
          execute: deleteSelection,
        },
        {
          id: "selection.copy",
          title: "复制",
          scope: "selection",
          shortcuts: ["Mod+C"],
          isEnabled: () =>
            selectedIds.length > 0 &&
            selectedIds.every((id) => {
              const layer = currentLayers.find((candidate) => candidate.id === id);
              return layer && !layer.locked && layer.kind.type !== "boardFill";
            }),
          execute: () => {
            setLayerClipboard({
              mode: "copy",
              sourceFace: workspace.activeFace,
              layerIds: [...selectedIds],
            });
            setStatus("对象已复制");
          },
        },
        {
          id: "selection.cut",
          title: "剪切",
          scope: "selection",
          shortcuts: ["Mod+X"],
          isEnabled: () =>
            selectedIds.length > 0 &&
            selectedIds.every((id) => {
              const layer = currentLayers.find((candidate) => candidate.id === id);
              return layer && !layer.locked && layer.kind.type !== "boardFill";
            }),
          execute: cutSelectionToClipboard,
        },
        {
          id: "selection.paste",
          title: "粘贴",
          scope: "canvas",
          shortcuts: ["Mod+V"],
          isEnabled: () => layerClipboard !== null,
          execute: pasteLayerClipboard,
        },
        {
          id: "selection.duplicate",
          title: "创建副本",
          scope: "selection",
          shortcuts: ["Mod+D"],
          isEnabled: () =>
            selectedIds.length === 1 &&
            selectedLayer?.kind.type !== "group" &&
            selectedLayer?.kind.type !== "boardFill" &&
            !selectedLayer?.locked,
          execute: () =>
            selectedLayer ? duplicateSourceLayer(selectedLayer.id) : undefined,
        },
        {
          id: "selection.group",
          title: "分组",
          scope: "selection",
          shortcuts: ["Mod+G"],
          isEnabled: () =>
            selectedIds.length >= 2 &&
            selectedIds.every((id) => {
              const layer = currentLayers.find((candidate) => candidate.id === id);
              return layer && !layer.locked && layer.kind.type !== "boardFill";
            }),
          execute: groupSelection,
        },
        {
          id: "selection.ungroup",
          title: "解组",
          scope: "selection",
          shortcuts: ["Mod+Shift+G"],
          isEnabled: () =>
            selectedIds.length === 1 &&
            selectedLayer?.kind.type === "group" &&
            !selectedLayer.locked,
          execute: ungroupSelection,
        },
        ...(
          [
            ["left", "向左微调", "ArrowLeft"],
            ["right", "向右微调", "ArrowRight"],
            ["up", "向上微调", "ArrowUp"],
            ["down", "向下微调", "ArrowDown"],
          ] as const
        ).map(([direction, title, shortcut]) => ({
          id: `selection.nudge-${direction}`,
          title,
          scope: "selection" as const,
          shortcuts: [shortcut, `Shift+${shortcut}`],
          isEnabled: () =>
            selectedIds.length === 1 &&
            selectedLayer?.kind.type !== "boardFill" &&
            !selectedLayer?.locked,
          execute: (context: WorkspaceCommandContext) =>
            nudgeSelection(direction, context.largeStep ?? false),
        })),
        {
          id: "layer.edit",
          title: "编辑对象",
          scope: "selection",
          isEnabled: (context) => {
            const layer = [...sessionDocument.frontLayers, ...sessionDocument.backLayers]
              .find((candidate) => candidate.id === context.layerId);
            return Boolean(
              layer &&
              !layer.locked &&
              (layer.kind.type === "text" || layer.kind.type === "image"),
            );
          },
          execute: (context) => {
            const layer = [...sessionDocument.frontLayers, ...sessionDocument.backLayers]
              .find((candidate) => candidate.id === context.layerId);
            if (layer?.kind.type === "text") setEditingLayerId(layer.id);
            if (layer?.kind.type === "image") {
              void requestImageFile(layer.id);
            }
          },
        },
        {
          id: "layer.rename",
          title: "重命名",
          scope: "selection",
          isEnabled: (context) => {
            const layer = [...sessionDocument.frontLayers, ...sessionDocument.backLayers]
              .find((candidate) => candidate.id === context.layerId);
            return Boolean(layer && !layer.locked && context.name?.trim());
          },
          execute: (context) => {
            const name = context.name?.trim();
            if (!context.layerId || !name) return;
            return applyDocumentMutation(
              () => setLayerName(context.layerId!, name),
              `图层已重命名为“${name}”`,
            );
          },
        },
        {
          id: "layer.toggle-lock",
          title: "切换锁定",
          scope: "selection",
          isEnabled: (context) => Boolean(context.layerId),
          execute: (context) => {
            const layer = [...sessionDocument.frontLayers, ...sessionDocument.backLayers]
              .find((candidate) => candidate.id === context.layerId);
            if (!layer) return;
            return applyDocumentMutation(
              () => setLayerLock(layer.id, !layer.locked),
              layer.locked ? "对象已解锁" : "对象已锁定",
            );
          },
        },
        {
          id: "layer.toggle-visibility",
          title: "切换显隐",
          scope: "selection",
          isEnabled: (context) => Boolean(context.layerId),
          execute: (context) => {
            const layer = [...sessionDocument.frontLayers, ...sessionDocument.backLayers]
              .find((candidate) => candidate.id === context.layerId);
            if (!layer) return;
            return applyDocumentMutation(
              () => setLayerVisibility(layer.id, !layer.visible),
              layer.visible ? "对象已隐藏" : "对象已显示",
            );
          },
        },
        {
          id: "layer.transform",
          title: "变换对象",
          scope: "selection",
          isEnabled: (context) => Boolean(context.layerId && context.transform),
          execute: (context) => {
            if (!context.layerId || !context.transform) return;
            applyLayerTransform(context.layerId, context.transform);
          },
        },
        {
          id: "layer.set-export-enabled",
          title: "切换生产导出",
          scope: "selection",
          isEnabled: (context) =>
            Boolean(context.layerId && context.value !== undefined),
          execute: (context) => {
            if (!context.layerId || context.value === undefined) return;
            return applyDocumentMutation(
              () => setLayerExportEnabled(context.layerId!, context.value!),
              context.value ? "对象已参与生产导出" : "对象已排除生产导出",
            );
          },
        },
        {
          id: "mapping.add",
          title: "关联生产层",
          scope: "selection",
          isEnabled: (context) =>
            Boolean(context.layerId && context.face && context.productionLayer),
          execute: (context) => {
            if (!context.layerId || !context.face || !context.productionLayer) return;
            return applyDocumentMutation(
              () => mapLayer(context.layerId!, context.face!, context.productionLayer!),
              `对象已关联到${getWorkContextLabel(context.productionLayer)}`,
            );
          },
        },
        {
          id: "mapping.remove",
          title: "移出当前生产层",
          scope: "selection",
          isEnabled: (context) => Boolean(context.mappingId),
          execute: (context) => {
            if (!context.mappingId) return;
            return applyDocumentMutation(
              () => unmapLayer(context.mappingId!),
              "对象已移出当前生产层，其他关联保持不变",
            );
          },
        },
        {
          id: "history.undo",
          title: "撤销",
          scope: "application",
          shortcuts: ["Mod+Z"],
          isEnabled: () => sessionDocument.history.canUndo,
          execute: () => applyDocumentMutation(undoWorkspace, "已撤销"),
        },
        {
          id: "history.redo",
          title: "重做",
          scope: "application",
          shortcuts: ["Mod+Shift+Z"],
          isEnabled: () => sessionDocument.history.canRedo,
          execute: () => applyDocumentMutation(redoWorkspace, "已重做"),
        },
      ]),
    [
      applyDocumentMutation,
      applyLayerTransform,
      currentLayers,
      cutSelectionToClipboard,
      deleteSelection,
      duplicateSourceLayer,
      focusProjectMedia,
      groupSelection,
      layerClipboard,
      nudgeSelection,
      pasteLayerClipboard,
      requestImageFile,
      selectedIds,
      selectedLayer,
      sessionDocument.backLayers,
      sessionDocument.frontLayers,
      sessionDocument.history.canRedo,
      sessionDocument.history.canUndo,
      ungroupSelection,
      workspace.activeFace,
    ],
  );
  // Keyboard input can arrive after React has rendered fresh command state but
  // before the window listener effect has been rebound. Always dispatch through
  // the registry from the latest render so rapid undo/redo sequences are not
  // dropped in that gap.
  const commandRegistryRef = useRef(commandRegistry);
  commandRegistryRef.current = commandRegistry;

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (
        isEditorShortcutSuppressed(
          event.target,
          settingsOpen ||
            pendingDelete !== null ||
            Boolean(
              globalThis.document.querySelector(
                '[role="dialog"][aria-modal="true"], dialog[open]',
              ),
            ),
        )
      ) {
        return;
      }
      const activeScopes: EditorCommandScope[] = ["application"];
      if (workspace.workspaceMode === "edit") activeScopes.push("canvas");
      if (selectedIds.length > 0) activeScopes.push("selection");
      const registry = commandRegistryRef.current;
      const commandId = registry.resolveShortcut(event, activeScopes);
      if (commandId) {
        event.preventDefault();
        if (!event.repeat || commandId.startsWith("selection.nudge-")) {
          void registry.execute(
            commandId,
            { largeStep: event.shiftKey },
            "shortcut",
          );
        }
        return;
      }
      if (event.key === "Enter" && selectedLayer?.kind.type === "text") {
        event.preventDefault();
        setEditingLayerId(selectedLayer.id);
      }
      if (event.key === "Enter" && selectedLayer?.kind.type === "group") {
        event.preventDefault();
        setDrillGroupIds((current) => ({
          ...current,
          [workspace.activeFace]: selectedLayer.id,
        }));
        setStatus("已进入组合，再次点击可选择组内对象");
      }
      if (
        event.key === "Escape" &&
        drillGroupIds[workspace.activeFace] !== null
      ) {
        event.preventDefault();
        const groupId = drillGroupIds[workspace.activeFace];
        setDrillGroupIds((current) => ({
          ...current,
          [workspace.activeFace]: null,
        }));
        if (groupId) selectOnly(workspace.activeFace, groupId);
        setStatus("已退出组合");
      }
    };
    globalThis.addEventListener("keydown", handleKeyDown);
    return () => globalThis.removeEventListener("keydown", handleKeyDown);
  }, [
    drillGroupIds,
    pendingDelete,
    selectOnly,
    selectedIds.length,
    selectedLayer,
    settingsOpen,
    workspace.activeFace,
    workspace.workspaceMode,
  ]);

  const handleCreateText = async (face: CardFace, draft: TextDraft) => {
    setStatus("正在创建文字…");
    try {
      const result = await insertTextLayer({
        side: face,
        ...draft,
      });
      const document = await mapLayer(
        result.layerId,
        face,
        workspace.workContexts[face],
      );
      setSessionDocument(document);
      selectOnly(face, result.layerId);
      setEditingLayerId(result.layerId);
      dispatch({ type: "setTool", tool: "select" });
      setStatus("输入文字，Escape 完成编辑");
    } catch (error) {
      setStatus(`文字创建失败：${errorMessage(error)}`);
    }
  };

  const handleCommitText = async (layerId: string, text: string) => {
    setEditingLayerId(null);
    try {
      setSessionDocument(await setTextContent(layerId, text));
      setStatus("文字已更新");
    } catch (error) {
      setStatus(`文字更新失败：${errorMessage(error)}`);
    }
  };

  const activateProject = (
    document: WorkspaceDocument,
    path: string | null,
  ) => {
    setSessionDocument(document);
    setProjectPath(path);
    setEditingLayerId(null);
    setReplaceLayerId(null);
    setDrillGroupIds({ front: null, back: null });
    setProductionInspection(createProductionInspectionState());
    setBoardPreview(null);
    dispatch({ type: "setSelection", face: "front", layerIds: [] });
    dispatch({ type: "setSelection", face: "back", layerIds: [] });
    dispatch({ type: "resetViewport", face: "front" });
    dispatch({ type: "resetViewport", face: "back" });
    dispatch({ type: "setFace", face: "front" });
    setProjectMenuOpen(false);
    setShowProjectHome(false);
  };

  const confirmDiscardIfNeeded = () =>
    !sessionDocument.history.canUndo ||
    globalThis.confirm("当前工程有未保存修改，是否丢弃并继续？");

  const handleOpenProject = async () => {
    if (!confirmDiscardIfNeeded()) return;
    setProjectMenuOpen(false);
    try {
      const path = await selectAtelierProjectFile();
      if (!path) return;
      setStatus("正在打开工程…");
      const document = await openAtelierProject(path);
      activateProject(document, path);
      setStatus(`已打开工程：${document.title}`);
    } catch (error) {
      setStatus(`打开工程失败：${errorMessage(error)}`);
    }
  };

  const handleNewProject = async ({
    title = "未命名卡片",
    widthMm = 64,
    heightMm = 100,
  }: NewProjectOptions = {}) => {
    if (!confirmDiscardIfNeeded()) return;
    setStatus("正在新建工程…");
    try {
      const document = await createNewAtelierProject(
        title,
        Math.round(widthMm * 1_000),
        Math.round(heightMm * 1_000),
      );
      activateProject(document, null);
      setStatus(`已新建 ${widthMm} × ${heightMm} mm 工程`);
    } catch (error) {
      setStatus(`新建工程失败：${errorMessage(error)}`);
    }
  };

  const handleSaveProject = async (saveAs = false) => {
    setProjectMenuOpen(false);
    try {
      let path = saveAs ? null : projectPath;
      if (!path) {
        path = await selectAtelierSaveFile(sessionDocument.title);
      }
      if (!path) return;
      setStatus("正在保存工程…");
      const document = await saveAtelierProject(path);
      setSessionDocument(document);
      setProjectPath(path);
      setStatus(`已保存工程：${path}`);
    } catch (error) {
      setStatus(`保存工程失败：${errorMessage(error)}`);
    }
  };

  useEffect(() => {
    const saveShortcut = (event: KeyboardEvent) => {
      if (
        event.key.toLowerCase() === "s" &&
        (event.metaKey || event.ctrlKey)
      ) {
        event.preventDefault();
        void handleSaveProject(event.shiftKey);
      }
    };
    globalThis.addEventListener("keydown", saveShortcut);
    return () => globalThis.removeEventListener("keydown", saveShortcut);
  });

  if (showProjectHome) {
    return (
      <>
        <ProjectHome
          onNew={(options) => void handleNewProject(options)}
          onOpen={() => void handleOpenProject()}
          onOpenSettings={() => setSettingsOpen(true)}
        />
        {settingsOpen && (
          <AppSettingsDialog
            settings={appSettings}
            themePreference={themePreference}
            onCanvasViewChange={setBoardView}
            onClose={() => setSettingsOpen(false)}
            onLaunchWindowModeChange={setLaunchWindowMode}
            onThemePreferenceChange={setThemePreference}
            onWheelZoomDampingChange={setWheelZoomDamping}
          />
        )}
      </>
    );
  }

  return (
    <div
      className="grid h-screen min-h-[640px] grid-rows-[52px_minmax(0,1fr)_28px] overflow-hidden bg-background text-foreground"
    >
      <input
        accept={SUPPORTED_IMAGE_FILE_ACCEPT}
        className="sr-only"
        data-testid="image-file-input"
        onChange={(event) => {
          const file = event.currentTarget.files?.[0];
          if (file) void importImage(file, replaceLayerId);
          event.currentTarget.value = "";
          setReplaceLayerId(null);
        }}
        ref={fileInputRef}
        type="file"
      />
      {imageImportDraft && (
        <ImageImportDialog
          colorOriginalAvailable={
            getColorOriginalAvailability({
              mediaType: imageImportDraft.mediaType,
              profile: sessionDocument.manufacturerProfile,
              productionLayers: [imageImportDraft.productionLayer],
            }).available
          }
          colorOriginalUnavailableReason={
            getColorOriginalAvailability({
              mediaType: imageImportDraft.mediaType,
              profile: sessionDocument.manufacturerProfile,
              productionLayers: [imageImportDraft.productionLayer],
            }).reason
          }
          draft={imageImportDraft}
          onCancel={() => {
            setImageImportDraft(null);
            setStatus("已取消图片导入");
          }}
          onConfirmed={(result) => {
            setSessionDocument(result.document);
            selectOnly(imageImportDraft.side, result.layerId);
            setEditingLayerId(null);
            dispatch({ type: "setTool", tool: "select" });
            setImageImportDraft(null);
            setStatus(
              imageImportDraft.placementCenterUm
                ? "图片已处理并放置到拖放位置"
                : "图片已处理并插入",
            );
          }}
          onEnableColorOriginal={async () => {
            const document = await setManufacturerProfile({
              ...sessionDocument.manufacturerProfile,
              layerCount:
                sessionDocument.manufacturerProfile.layerCount === 4 ? 4 : 2,
              outerCopper: "oz1",
              solderMask: "white",
              characterProcess: "multicolor",
              surfaceFinish: "enig",
            });
            setSessionDocument(document);
            setStatus("已启用彩色丝印所需制造工艺");
          }}
          onError={(error) =>
            setStatus(`图片处理失败：${errorMessage(error)}`)
          }
        />
      )}

      <header className="relative grid grid-cols-[210px_minmax(0,1fr)_300px] border-b bg-card">
        <div className="relative border-r">
          <button
            aria-expanded={projectMenuOpen}
            aria-label="工程菜单"
            className="flex h-full w-full items-center gap-2.5 px-3 text-left hover:bg-accent/50"
            onClick={() => setProjectMenuOpen((open) => !open)}
            type="button"
          >
            <BrandIcon className="size-7 rounded-md" />
            <div className="min-w-0">
              <p className="truncate text-sm font-semibold">
                {sessionDocument.title}
              </p>
              <p className="text-[10px] text-muted-foreground">
                {projectPath ? "已保存到本地" : "尚未保存"}
              </p>
            </div>
            <ChevronDown className="ml-auto size-3.5 text-muted-foreground" />
          </button>
          {projectMenuOpen && (
            <div
              aria-label="工程操作"
              className="absolute left-2 top-[46px] z-50 w-56 rounded-lg border bg-popover p-1.5 text-popover-foreground shadow-lg"
              role="menu"
            >
              <ProjectMenuItem
                icon={FilePlus2}
                label="新建工程"
                onClick={() => void handleNewProject()}
              />
              <ProjectMenuItem
                icon={FolderOpen}
                label="打开工程…"
                onClick={() => void handleOpenProject()}
              />
              <div className="my-1 border-t" />
              <ProjectMenuItem
                icon={Save}
                label="保存"
                onClick={() => void handleSaveProject()}
                shortcut="⌘S"
              />
              <ProjectMenuItem
                icon={Save}
                label="另存为…"
                onClick={() => void handleSaveProject(true)}
              />
              <div className="my-1 border-t" />
              <ProjectMenuItem
                disabled={!commandRegistry.isEnabled("history.undo", {})}
                icon={RotateCcw}
                label="撤销"
                onClick={() => {
                  setProjectMenuOpen(false);
                  void commandRegistry.execute("history.undo", {}, "menu");
                }}
                shortcut="⌘Z"
              />
              <ProjectMenuItem
                disabled={!commandRegistry.isEnabled("history.redo", {})}
                icon={RotateCw}
                label="重做"
                onClick={() => {
                  setProjectMenuOpen(false);
                  void commandRegistry.execute("history.redo", {}, "menu");
                }}
                shortcut="⇧⌘Z"
              />
              <div className="my-1 border-t" />
              <ProjectMenuItem
                icon={Settings2}
                label="设置…"
                onClick={() => {
                  setProjectMenuOpen(false);
                  setSettingsOpen(true);
                }}
              />
            </div>
          )}
        </div>

        <div className="flex min-w-0 items-center justify-between gap-3 px-3">
          <div
            aria-label="编辑工具"
            className="flex items-center gap-1"
            role="toolbar"
          >
            {TOOLS.map((tool) => (
              <ToolButton
                active={workspace.tool === tool.id}
                icon={tool.icon}
                key={tool.id}
                label={tool.label}
                onClick={() => {
                  void commandRegistry.execute(
                    `tool.${tool.id}`,
                    {},
                    "toolbar",
                  );
                }}
                shortcut={tool.shortcut}
              />
            ))}
          </div>

          <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
            {workspace.workspaceMode === "edit" && (
              <label className="pointer-events-auto relative">
                <span className="sr-only">画板视图</span>
                <select
                  aria-label="画板视图"
                  className="h-8 appearance-none rounded-lg border-0 bg-muted py-0 pl-3 pr-7 text-[11px] font-medium text-foreground outline-none transition-transform duration-150 ease-out active:scale-[0.97] focus-visible:ring-2 focus-visible:ring-ring"
                  onChange={(event) =>
                    setBoardView(event.target.value as CanvasView)
                  }
                  value={
                    workspace.editLayout === "focus"
                      ? "focus-active"
                      : workspace.boardArrangement
                  }
                >
                  <option value="horizontal">双面 · 左右</option>
                  <option value="vertical">双面 · 上下</option>
                  <option value="focus-active">当前面</option>
                </select>
                <ChevronDown
                  aria-hidden="true"
                  className="pointer-events-none absolute right-2 top-1/2 size-3 -translate-y-1/2 text-muted-foreground"
                />
              </label>
            )}
          </div>

          <div className="flex items-center">
            <SegmentedControl
              ariaLabel="工作模式"
              items={WORKSPACE_MODES.map((mode) => ({
                id: mode.id,
                label: mode.label,
                icon: mode.icon,
              }))}
              onChange={(workspaceMode) =>
                dispatch({
                  type: "setWorkspaceMode",
                  workspaceMode: workspaceMode as WorkspaceMode,
                })
              }
              value={workspace.workspaceMode}
            />
          </div>
        </div>

        <div className="flex items-center justify-end gap-2 border-l px-3">
          <ExportEasyedaButton onStatus={setStatus} />
        </div>
      </header>

      {settingsOpen && (
        <AppSettingsDialog
          settings={appSettings}
          themePreference={themePreference}
          onCanvasViewChange={setBoardView}
          onClose={() => setSettingsOpen(false)}
          onLaunchWindowModeChange={setLaunchWindowMode}
          onThemePreferenceChange={setThemePreference}
          onWheelZoomDampingChange={setWheelZoomDamping}
        />
      )}
      {pendingDelete && (
        <div className="fixed inset-0 z-[110] grid place-items-center bg-black/55 p-6">
          <section
            aria-label="确认删除对象"
            aria-modal="true"
            className="w-full max-w-sm rounded-xl border bg-popover p-5 text-popover-foreground shadow-2xl"
            role="dialog"
          >
            <h2 className="text-sm font-semibold">删除所选对象？</h2>
            <p className="mt-2 text-[11px] leading-5 text-muted-foreground">
              将删除 {pendingDelete.layerIds.length} 个对象及{" "}
              {pendingDelete.mappingCount} 条生产层映射。项目媒体中的素材与处理版本会保留。
            </p>
            <div className="mt-5 flex justify-end gap-2">
              <Button
                onClick={() => setPendingDelete(null)}
                type="button"
                variant="ghost"
              >
                取消
              </Button>
              <Button
                onClick={() => {
                  const layerIds = pendingDelete.layerIds;
                  setPendingDelete(null);
                  void performDelete(layerIds);
                }}
                type="button"
                variant="destructive"
              >
                删除对象
              </Button>
            </div>
          </section>
        </div>
      )}
      {canvasContextMenu && (
        <div
          aria-label={contextMenuLayer ? "对象菜单" : "画布菜单"}
          className="fixed z-[70] w-52 rounded-lg border bg-popover p-1.5 text-popover-foreground shadow-xl"
          data-canvas-context-menu
          role="menu"
          style={{ left: canvasContextMenu.x, top: canvasContextMenu.y }}
        >
          {contextMenuLayer ? (
            <>
              <form
                className="p-1"
                onSubmit={(event) => {
                  event.preventDefault();
                  const name = contextRenameValue.trim();
                  if (!name) return;
                  void commandRegistry.execute(
                    "layer.rename",
                    { layerId: contextMenuLayer.id, name },
                    "context-menu",
                  );
                  setCanvasContextMenu(null);
                }}
              >
                <input
                  aria-label="重命名对象"
                  className="h-8 w-full rounded-md border bg-background px-2 text-[11px] outline-none focus:border-primary"
                  onChange={(event) =>
                    setContextRenameValue(event.currentTarget.value)
                  }
                  value={contextRenameValue}
                />
              </form>
              <ContextMenuButton
                label={
                  contextMenuLayer.kind.type === "text"
                    ? "编辑文字"
                    : contextMenuLayer.kind.type === "image"
                      ? "替换图片…"
                      : "编辑"
                }
                disabled={
                  !commandRegistry.isEnabled("layer.edit", {
                    layerId: contextMenuLayer.id,
                  })
                }
                onClick={() => {
                  void commandRegistry.execute(
                    "layer.edit",
                    { layerId: contextMenuLayer.id },
                    "context-menu",
                  );
                  setCanvasContextMenu(null);
                }}
              />
              <ContextMenuButton
                label="复制"
                onClick={() => {
                  void commandRegistry.execute(
                    "selection.copy",
                    {},
                    "context-menu",
                  );
                  setCanvasContextMenu(null);
                }}
              />
              <ContextMenuButton
                disabled={
                  !commandRegistry.isEnabled("selection.duplicate", {})
                }
                label="创建副本"
                onClick={() => {
                  void commandRegistry.execute(
                    "selection.duplicate",
                    {},
                    "context-menu",
                  );
                  setCanvasContextMenu(null);
                }}
              />
              <ContextMenuButton
                label={contextMenuLayer.locked ? "解锁" : "锁定"}
                onClick={() => {
                  void commandRegistry.execute(
                    "layer.toggle-lock",
                    { layerId: contextMenuLayer.id },
                    "context-menu",
                  );
                  setCanvasContextMenu(null);
                }}
              />
              <ContextMenuButton
                label={contextMenuLayer.visible ? "隐藏" : "显示"}
                onClick={() => {
                  void commandRegistry.execute(
                    "layer.toggle-visibility",
                    { layerId: contextMenuLayer.id },
                    "context-menu",
                  );
                  setCanvasContextMenu(null);
                }}
              />
              {commandRegistry.isEnabled("selection.group", {}) && (
                <ContextMenuButton
                  label="分组"
                  onClick={() => {
                    void commandRegistry.execute(
                      "selection.group",
                      {},
                      "context-menu",
                    );
                    setCanvasContextMenu(null);
                  }}
                />
              )}
              {commandRegistry.isEnabled("selection.ungroup", {}) && (
                <ContextMenuButton
                  label="解组"
                  onClick={() => {
                    void commandRegistry.execute(
                      "selection.ungroup",
                      {},
                      "context-menu",
                    );
                    setCanvasContextMenu(null);
                  }}
                />
              )}
              <div className="my-1 border-t" />
              {(["copper", "solderMaskOpen", "silkscreen"] as const).map(
                (productionLayer) => {
                  const exists = sessionDocument.mappings.some(
                    (mapping) =>
                      mapping.sourceLayerId === contextMenuLayer.id &&
                      mapping.target.side === canvasContextMenu.face &&
                      mapping.target.layer === productionLayer,
                  );
                  return (
                    <ContextMenuButton
                      disabled={exists}
                      key={productionLayer}
                      label={`关联到${getWorkContextLabel(productionLayer)}`}
                      onClick={() => {
                        void commandRegistry.execute(
                          "mapping.add",
                          {
                            face: canvasContextMenu.face,
                            layerId: contextMenuLayer.id,
                            productionLayer,
                          },
                          "context-menu",
                        );
                        setCanvasContextMenu(null);
                      }}
                    />
                  );
                },
              )}
              {contextMenuCurrentMapping && (
                <ContextMenuButton
                  label="移出当前生产层"
                  onClick={() => {
                    void commandRegistry.execute(
                      "mapping.remove",
                      { mappingId: contextMenuCurrentMapping.id },
                      "context-menu",
                    );
                    setCanvasContextMenu(null);
                  }}
                />
              )}
              <div className="my-1 border-t" />
              <ContextMenuButton
                destructive
                disabled={contextMenuLayer.locked}
                label="删除对象"
                onClick={() => {
                  void commandRegistry.execute(
                    "selection.delete",
                    {},
                    "context-menu",
                  );
                  setCanvasContextMenu(null);
                }}
              />
            </>
          ) : (
            <>
              <ContextMenuButton
                disabled={layerClipboard === null}
                label="粘贴"
                onClick={() => {
                  void commandRegistry.execute(
                    "selection.paste",
                    {},
                    "context-menu",
                  );
                  setCanvasContextMenu(null);
                }}
              />
              <ContextMenuButton
                label="适配画布"
                onClick={() => {
                  fitCanvas(canvasContextMenu.face);
                  setCanvasContextMenu(null);
                }}
              />
            </>
          )}
        </div>
      )}

      {workspace.workspaceMode === "edit" ? (
        <div
          className="grid min-h-0"
          style={{
            gridTemplateColumns: `${appSettings.workspaceLeftPanelWidth}px 6px minmax(0, 1fr) 6px ${appSettings.workspaceRightPanelWidth}px`,
          }}
        >
          <aside className="min-h-0 border-r bg-panel">
            <ProjectMediaDock
              activeFace={workspace.activeFace}
              activeProductionLayer={
                workspace.workContexts[workspace.activeFace]
              }
              assets={sessionDocument.assets}
              layers={{
                front: sessionDocument.frontLayers,
                back: sessionDocument.backLayers,
              }}
              onImportFiles={(files) => void importFilesToProjectMedia(files)}
              onInvalidPlacement={setStatus}
              onMoveAsset={moveAssetToFolder}
              onPlaceAsset={(request) => void placeProjectAsset(request)}
              productionPanel={
                <>
                  <div className="p-2">
              <ProductionLayerTree
                activeFace={workspace.activeFace}
                boardSelected={workspace.inspectorTarget === "board"}
                contexts={workspace.workContexts}
                inspection={productionInspection}
                layers={{
                  front: sessionDocument.frontLayers,
                  back: sessionDocument.backLayers,
                }}
                mappings={sessionDocument.mappings}
                onCopy={() =>
                  void commandRegistry.execute(
                    "selection.copy",
                    {},
                    "layer-menu",
                  )
                }
                onDropProjectAsset={(assetId, face, productionLayer) =>
                  void placeProjectAsset({
                    assetId,
                    face,
                    productionLayer,
                  })
                }
                onCreateBoardFill={(face) =>
                  void (async () => {
                    try {
                      const result = await createBoardFill(face, 500);
                      setSessionDocument(result.document);
                      selectOnly(face, result.layerId);
                      setStatus(
                        result.document[
                          face === "front" ? "frontLayers" : "backLayers"
                        ].filter((layer) => layer.kind.type === "boardFill")
                          .length === 1
                          ? "基础铺铜已选中，板边间距 0.50 mm"
                          : "基础铺铜已选中",
                      );
                    } catch (error) {
                      setStatus(`创建铺铜失败：${errorMessage(error)}`);
                    }
                  })()
                }
                onCut={() =>
                  void commandRegistry.execute(
                    "selection.cut",
                    {},
                    "layer-menu",
                  )
                }
                onDelete={() =>
                  void commandRegistry.execute(
                    "selection.delete",
                    {},
                    "layer-menu",
                  )
                }
                onDuplicate={() =>
                  void commandRegistry.execute(
                    "selection.duplicate",
                    {},
                    "layer-menu",
                  )
                }
                onReorder={(
                  sourceFace,
                  targetFace,
                  layerId,
                  newParentId,
                  newIndex,
                  sourceContext,
                  targetContext,
                ) => {
                  void applyDocumentMutation(
                    () =>
                      sourceFace === targetFace &&
                      sourceContext === targetContext
                        ? reorderLayer(layerId, newParentId, newIndex)
                        : sourceFace === targetFace
                          ? moveLayer(
                            layerId,
                            newParentId,
                            newIndex,
                            sourceFace,
                            sourceContext,
                            targetContext,
                          )
                          : transferLayers({
                              layerIds: [layerId],
                              targetSide: targetFace,
                              targetLayer: targetContext,
                              newParentId,
                              newIndex,
                              mode: "move",
                              offsetUm: 0,
                            }).then((result) => {
                              dispatch({ type: "setFace", face: targetFace });
                              dispatch({
                                type: "setSelection",
                                face: targetFace,
                                layerIds: result.layerIds,
                              });
                              return result.document;
                            }),
                    sourceFace === targetFace &&
                    sourceContext === targetContext
                      ? "图层层级与顺序已更新"
                      : sourceFace === targetFace
                        ? "对象已移动到新的生产层"
                        : `对象已移动到${targetFace === "front" ? "正面" : "背面"}`,
                  );
                }}
                onRename={(layer, name) =>
                  void commandRegistry.execute(
                    "layer.rename",
                    { layerId: layer.id, name },
                    "layer-menu",
                  )
                }
                onSelectBoard={() => dispatch({ type: "selectBoard" })}
                onSelectContext={(face, workContext) => {
                  dispatch({ type: "setFace", face });
                  dispatch({
                    type: "setWorkContext",
                    face,
                    workContext,
                  });
                }}
                onSelectSource={(
                  face,
                  layerId,
                  event,
                  orderedLayerIds,
                ) => {
                  dispatch({ type: "setFace", face });
                  const selection = resolveTreeLayerSelection({
                    anchorId: treeSelectionAnchorRef.current[face],
                    current: workspace.selections[face],
                    layerId,
                    orderedLayerIds,
                    rangeKey: event.shiftKey,
                    toggleKey: event.metaKey || event.ctrlKey,
                  });
                  treeSelectionAnchorRef.current[face] =
                    selection.anchorId;
                  dispatch({
                    type: "setSelection",
                    face,
                    layerIds: selection.selectedIds,
                  });
                  setEditingLayerId(null);
                }}
                onToggleLock={(layer) =>
                  void commandRegistry.execute(
                    "layer.toggle-lock",
                    { layerId: layer.id },
                    "layer-menu",
                  )
                }
                onToggleIsolation={(face, context) =>
                  setProductionInspection((current) =>
                    toggleProductionIsolation(current, face, context),
                  )
                }
                onToggleProductionVisibility={(face, context) =>
                  setProductionInspection((current) =>
                    toggleProductionVisibility(current, face, context),
                  )
                }
                selectedIds={workspace.selections}
                onToggleVisibility={(layer) =>
                  void commandRegistry.execute(
                    "layer.toggle-visibility",
                    { layerId: layer.id },
                    "layer-menu",
                  )
                }
              />
                  </div>
            {(commandRegistry.isEnabled("selection.group", {}) ||
              commandRegistry.isEnabled("selection.ungroup", {})) && (
              <div
                aria-label="选择操作"
                className="flex items-center justify-between gap-2 border-t bg-muted/45 px-3 py-1.5"
              >
                <span className="text-[10px] text-muted-foreground">
                  {commandRegistry.isEnabled("selection.group", {})
                    ? `已选择 ${selectedIds.length} 项`
                    : "已选择组合"}
                </span>
                {commandRegistry.isEnabled("selection.group", {}) ? (
                <Button
                  className="h-7 px-2 text-[11px]"
                  onClick={() =>
                    void commandRegistry.execute(
                      "selection.group",
                      {},
                      "toolbar",
                    )
                  }
                  variant="ghost"
                >
                  <Layers3 className="size-3.5" /> 分组
                </Button>
                ) : (
                <Button
                  className="h-7 px-2 text-[11px]"
                  onClick={() =>
                    void commandRegistry.execute(
                      "selection.ungroup",
                      {},
                      "toolbar",
                    )
                  }
                  variant="ghost"
                >
                  <Ungroup className="size-3.5" /> 解组
                </Button>
                )}
              </div>
            )}
                </>
              }
              treatments={sessionDocument.imageTreatments}
            />
          </aside>

          <WorkspacePanelResizeHandle
            otherPanelWidth={appSettings.workspaceRightPanelWidth}
            panelWidth={appSettings.workspaceLeftPanelWidth}
            side="left"
            onResize={(width, persist) =>
              setWorkspacePanelWidth("left", width, persist)
            }
          />

          <section
            className={cn(
              "relative grid min-h-0 min-w-0 gap-3 overflow-x-hidden overflow-y-auto bg-workspace p-3",
              workspace.editLayout === "focus" && "grid-cols-1",
              workspace.editLayout === "both" &&
                workspace.boardArrangement === "horizontal" &&
                "grid-cols-1 min-[1200px]:grid-cols-[repeat(2,minmax(400px,1fr))]",
              workspace.editLayout === "both" &&
                workspace.boardArrangement === "vertical" &&
                "grid-cols-1",
            )}
            data-arrangement={workspace.boardArrangement}
            data-layout={workspace.editLayout}
            data-testid="edit-board-layout"
          >
            {workspace.editLayout === "both" && (
              <div
                aria-hidden="true"
                className={cn(
                  "pointer-events-none absolute z-10 bg-primary/55 shadow-[0_0_8px_color-mix(in_oklab,var(--primary)_18%,transparent)]",
                  workspace.boardArrangement === "horizontal"
                    ? "inset-y-3 left-1/2 hidden w-px -translate-x-1/2 min-[1200px]:block"
                    : "inset-x-3 top-1/2 h-px -translate-y-1/2",
                )}
                data-active-face={workspace.activeFace}
                data-orientation={
                  workspace.boardArrangement === "horizontal"
                    ? "vertical"
                    : "horizontal"
                }
                data-testid="active-board-divider"
              />
            )}
            {(["front", "back"] as const)
              .filter(
                (face) =>
                  workspace.editLayout === "both" ||
                  face === workspace.activeFace,
              )
              .map((face) => {
                const layers =
                  face === "front"
                    ? sessionDocument.frontLayers
                    : sessionDocument.backLayers;
                const faceSelectedIds = workspace.selections[face];
                const faceEditingLayer =
                  face === workspace.activeFace
                    ? (layers.find((layer) => layer.id === editingLayerId) ??
                      null)
                    : null;

                return (
                  <div className="min-h-[320px] min-w-0" key={face}>
                    <WorkspaceCanvas
                      active={workspace.activeFace === face}
                      activeGroupId={drillGroupIds[face]}
                      aspectRatioLocked={
                        workspace.activeFace === face &&
                        faceSelectedIds.length === 1
                          ? selectedAspectRatioLocked
                          : true
                      }
                      document={sessionDocument}
                      editingLayer={faceEditingLayer}
                      face={face}
                      layers={layers}
                      onActivate={() => {
                        if (workspace.activeFace !== face) {
                          setEditingLayerId(null);
                          dispatch({ type: "setFace", face });
                        }
                      }}
                      onBeginTextEdit={(layerId) => {
                        dispatch({ type: "setFace", face });
                        setEditingLayerId(layerId);
                      }}
                      onEnterGroup={(layerId) => {
                        dispatch({
                          type: "setSelection",
                          face,
                          layerIds: [layerId],
                        });
                        setDrillGroupIds((current) => ({
                          ...current,
                          [face]: layerId,
                        }));
                        setStatus("已进入组合，Esc 返回上一级");
                      }}
                      onCommitText={(layerId, text) =>
                        void handleCommitText(layerId, text)
                      }
                      onClearSelection={() => {
                        setEditingLayerId(null);
                        dispatch({
                          type: "setSelection",
                          face,
                          layerIds: [],
                        });
                      }}
                      onCreateText={(draft) =>
                        void handleCreateText(face, draft)
                      }
                      onDropFiles={(files, point) => {
                        for (const file of files) {
                          void importImage(file, null, {
                            face,
                            productionLayer: workspace.workContexts[face],
                            placementCenterUm: point,
                          });
                        }
                      }}
                      onDropProjectAsset={(assetId, point) =>
                        void placeProjectAsset({
                          assetId,
                          face,
                          productionLayer: workspace.workContexts[face],
                          placementCenterUm: point,
                        })
                      }
                      onInvalidDrop={setStatus}
                      onOpenContextMenu={(request) => {
                        const targetLayer = request.layerId
                          ? layers.find(
                              (layer) => layer.id === request.layerId,
                            )
                          : null;
                        setContextRenameValue(targetLayer?.name ?? "");
                        setCanvasContextMenu({
                          face,
                          layerId: request.layerId,
                          x: Math.min(
                            request.clientX,
                            globalThis.innerWidth - 224,
                          ),
                          y: Math.min(
                            request.clientY,
                            globalThis.innerHeight - 360,
                          ),
                        });
                      }}
                      onSelect={(layerId, modifiers) => {
                        dispatch({ type: "setFace", face });
                        if (modifiers.altKey) {
                          dispatch({
                            type: "setSelection",
                            face,
                            layerIds: cycleOverlappingSelection(
                              modifiers.candidates,
                              faceSelectedIds,
                            ),
                          });
                          return;
                        }
                        selectLayer(face, layerId, modifiers.shiftKey);
                      }}
                      onSelectMany={(layerIds) => {
                        dispatch({ type: "setFace", face });
                        dispatch({
                          type: "setSelection",
                          face,
                          layerIds,
                        });
                        setEditingLayerId(null);
                      }}
                      onTransformLayer={(layerId, transform) => {
                        void commandRegistry.execute(
                          "layer.transform",
                          { layerId, transform },
                          "canvas",
                        );
                      }}
                      onTransformLayers={applyLayerTransforms}
                      onViewportChange={(nextViewport) =>
                        dispatch({
                          type: "setViewport",
                          face,
                          viewport: nextViewport,
                        })
                      }
                      productionVisibility={effectiveProductionVisibility(
                        productionInspection,
                        face,
                      )}
                      selectedIds={faceSelectedIds}
                      showOriginalLayerIds={originalPreviewLayerIds}
                      tool={workspace.tool}
                      viewport={workspace.viewports[face]}
                      wheelZoomDamping={appSettings.wheelZoomDamping}
                      workContext={workspace.workContexts[face]}
                    />
                  </div>
                );
              })}
          </section>

          <WorkspacePanelResizeHandle
            otherPanelWidth={appSettings.workspaceLeftPanelWidth}
            panelWidth={appSettings.workspaceRightPanelWidth}
            side="right"
            onResize={(width, persist) =>
              setWorkspacePanelWidth("right", width, persist)
            }
          />

          <aside
            className="min-h-0 min-w-0 overflow-x-hidden overflow-y-auto border-l bg-panel"
            data-testid="workspace-inspector"
          >
            <PanelHeading
              detail={selectedLayer?.name ?? "未选择对象"}
              title="检查器"
            />
            {workspace.inspectorTarget === "board" ? (
              <BoardInspector
                document={sessionDocument}
                onError={(error) => setStatus(errorMessage(error))}
                onSetOutline={(outline) =>
                  void applyDocumentMutation(
                    () => setBoardOutline(outline),
                    "板体尺寸已更新；对象物理尺寸保持不变",
                  )
                }
                onSetManufacturerProfile={(profile) =>
                  void applyDocumentMutation(
                    () => setManufacturerProfile(profile),
                    "制造参数与工艺近似已更新",
                  )
                }
              />
            ) : selectedLayer ? (
              <>
                <SelectedLayerInspector
                  aspectRatioLocked={selectedAspectRatioLocked}
                  fontFamilies={fontFamilies}
                  layer={selectedLayer}
                  layers={currentLayers}
                  onEditText={() =>
                    void commandRegistry.execute(
                      "layer.edit",
                      { layerId: selectedLayer.id },
                      "inspector",
                    )
                  }
                  onError={(error) => setStatus(errorMessage(error))}
                  onSetExportEnabled={(value) =>
                    void commandRegistry.execute(
                      "layer.set-export-enabled",
                      { layerId: selectedLayer.id, value },
                      "inspector",
                    )
                  }
                  onSetTextStyle={(fontFamily, fontSizeUm) =>
                    void applyDocumentMutation(
                      () =>
                        setTextStyle(
                          selectedLayer.id,
                          fontFamily,
                          fontSizeUm,
                        ),
                      "文字样式已更新",
                    )
                  }
                  onReplaceImage={() => {
                    void commandRegistry.execute(
                      "layer.edit",
                      { layerId: selectedLayer.id },
                      "inspector",
                    );
                  }}
                  onAspectRatioLockedChange={setSelectedAspectRatioLocked}
                  onTransform={(transform) =>
                    void commandRegistry.execute(
                      "layer.transform",
                      { layerId: selectedLayer.id, transform },
                      "inspector",
                    )
                  }
                />
                {selectedImageLayer &&
                  selectedImageAsset &&
                  selectedImageTreatment && (
                  <div className="border-t p-4">
                    <ImageTreatmentEditor
                      asset={selectedImageAsset}
                      colorOriginalAvailable={
                        selectedColorOriginalAvailability.available
                      }
                      colorOriginalUnavailableReason={
                        selectedColorOriginalAvailability.reason
                      }
                      compileReport={
                        treatmentReports[selectedImageTreatment.id] ?? null
                      }
                      onCompileAccepted={({ report }) => {
                        setTreatmentReports((current) => ({
                          ...current,
                          [selectedImageTreatment.id]: report,
                        }));
                        setStatus("图片处理预览已更新");
                      }}
                      onError={(error) =>
                        setStatus(`图片处理失败：${errorMessage(error)}`)
                      }
                      onProductionModeChange={(productionMode) => {
                        void setImageProductionMode(
                          selectedImageTreatment.id,
                          productionMode,
                        )
                          .then((document) => {
                            setSessionDocument(document);
                            setStatus(
                              productionMode === "colorOriginal"
                                ? "已启用彩色原图丝印生产"
                                : "已切换为单色生产掩膜",
                            );
                          })
                          .catch((error) =>
                            setStatus(
                              `生产方式更新失败：${errorMessage(error)}`,
                            ),
                          );
                      }}
                      onTemporaryOriginalChange={(visible) =>
                        setOriginalPreviewLayerIds((current) => {
                          if (current.has(selectedImageLayer.id) === visible) {
                            return current;
                          }
                          const next = new Set(current);
                          if (visible) next.add(selectedImageLayer.id);
                          else next.delete(selectedImageLayer.id);
                          return next;
                        })
                      }
                      persistRecipe={async (recipe) => {
                        const document = await setTreatmentRecipe(
                          selectedImageTreatment.id,
                          recipe,
                        );
                        setSessionDocument(document);
                        return document;
                      }}
                      physicalHeightUm={selectedLayer.transform.heightUm}
                      physicalWidthUm={selectedLayer.transform.widthUm}
                      resultPreviewUrl={
                        treatmentReports[selectedImageTreatment.id]
                          ?.previewPngDataUrl
                      }
                      treatment={selectedImageTreatment}
                    />
                  </div>
                )}
              </>
            ) : (
              <InspectorSection title="当前生产层">
                <InspectorRow
                  label="卡面"
                  value={workspace.activeFace === "front" ? "正面" : "背面"}
                />
                <InspectorRow
                  label="工作层"
                  value={getWorkContextLabel(
                    workspace.workContexts[workspace.activeFace],
                  )}
                />
              </InspectorSection>
            )}
            <InspectorSection title="当前状态">
              <InspectorRow
                label="工具"
                value={
                  TOOLS.find((tool) => tool.id === workspace.tool)?.label ?? ""
                }
              />
              <InspectorRow
                label="工作层"
                value={getWorkContextLabel(
                  workspace.workContexts[workspace.activeFace],
                )}
              />
              <InspectorRow
                label="缩放"
                mono
                value={`${Math.round(viewport.zoom * 100)}%`}
              />
              <Button
                className="mt-2 w-full"
                onClick={() => fitCanvas(workspace.activeFace)}
                variant="ghost"
              >
                <RotateCcw className="size-3.5" />
                适配画布
              </Button>
            </InspectorSection>
            <InspectorSection title="生产说明">
              <p className="text-[11px] leading-5 text-muted-foreground">
                选择左侧铜层、阻焊开窗或丝印层即可进入对应工作上下文；生产结果仍由源对象与显式映射统一编译。
              </p>
            </InspectorSection>
          </aside>
        </div>
      ) : (
        <section className="relative min-h-0 min-w-0 bg-workspace p-4">
          {boardPreview ? (
            <div className="relative size-full">
              <Suspense fallback={<PreviewLoading />}>
                <Board3DPreview
                  className="size-full rounded-xl border"
                  preview={boardPreview}
                />
              </Suspense>
              {boardPreviewPending && <PreviewUpdating />}
            </div>
          ) : boardPreviewError ? (
            <div
              className="grid size-full place-items-center rounded-xl border bg-card/30"
              role="alert"
            >
              <div className="max-w-sm text-center">
                <Box className="mx-auto size-8 text-destructive" />
                <p className="mt-3 text-sm font-medium">3D 预览生成失败</p>
                <p className="mt-1.5 text-[11px] leading-5 text-muted-foreground">
                  {boardPreviewError}
                </p>
              </div>
            </div>
          ) : (
            <PreviewLoading />
          )}
        </section>
      )}

      <footer className="flex items-center justify-between border-t bg-card px-3 text-[10px] text-muted-foreground">
        <span role="status">{status}</span>
        <span>
          {workspace.activeFace === "front" ? "正面" : "背面"} ·{" "}
          {
            WORKSPACE_MODES.find((mode) => mode.id === workspace.workspaceMode)
              ?.label
          }
        </span>
        <span className="font-mono">
          {core.projectFormat} · schema v{core.schemaVersion}
        </span>
      </footer>
    </div>
  );
}

function BoardInspector({
  document,
  onError,
  onSetOutline,
  onSetManufacturerProfile,
}: {
  document: WorkspaceDocument;
  onError: (error: unknown) => void;
  onSetOutline: (
    outline: WorkspaceDocument["board"]["outline"],
  ) => void;
  onSetManufacturerProfile: (
    profile: WorkspaceDocument["manufacturerProfile"],
  ) => void;
}) {
  const [aspectRatioLocked, setAspectRatioLocked] = useState(true);
  const updateOutline = (
    patch: Partial<{
      widthUm: number;
      heightUm: number;
      cornerRadiusUm: number;
    }>,
  ) => {
    const widthUm = patch.widthUm ?? document.board.widthUm;
    const heightUm = patch.heightUm ?? document.board.heightUm;
    const cornerRadiusUm =
      patch.cornerRadiusUm ?? document.board.cornerRadiusUm;
    if (cornerRadiusUm > Math.min(widthUm, heightUm) / 2) {
      onError(new Error("圆角半径不能超过板体短边的一半"));
      return;
    }
    onSetOutline({
      type: "roundedRectangle",
      widthUm,
      heightUm,
      cornerRadiusUm,
    });
  };
  const resizeBoard = (
    changedAxis: "width" | "height",
    nextValueUm: number,
  ) => {
    updateOutline(
      aspectRatioLocked
        ? resizeWithLockedAspectRatio(
            document.board.widthUm,
            document.board.heightUm,
            changedAxis,
            nextValueUm,
          )
        : { [`${changedAxis}Um`]: nextValueUm },
    );
  };

  return (
    <>
      <InspectorSection title="板体">
        <InspectorRow label="名称" value={document.title} />
        <div className="space-y-2 pt-1">
          <div className="grid grid-cols-[minmax(0,1fr)_1.75rem_minmax(0,1fr)] items-end gap-1">
            <BoardNumberInput
              label="宽"
              onCommit={(widthUm) => resizeBoard("width", widthUm)}
              onError={onError}
              value={document.board.widthUm}
            />
            <AspectRatioToggle
              locked={aspectRatioLocked}
              onToggle={() => setAspectRatioLocked((locked) => !locked)}
            />
            <BoardNumberInput
              label="高"
              onCommit={(heightUm) => resizeBoard("height", heightUm)}
              onError={onError}
              value={document.board.heightUm}
            />
          </div>
          <div className="grid grid-cols-2 gap-2">
            <BoardNumberInput
              allowZero
              label="圆角"
              onCommit={(cornerRadiusUm) =>
                updateOutline({ cornerRadiusUm })
              }
              onError={onError}
              value={document.board.cornerRadiusUm}
            />
          </div>
        </div>
      </InspectorSection>
      <InspectorSection title="制造">
        <ManufacturerInspector
          onChange={onSetManufacturerProfile}
          onRejected={onError}
          profile={document.manufacturerProfile}
        />
      </InspectorSection>
      {document.board.diagnostics.length > 0 && (
        <InspectorSection title="越界警告">
          <p className="text-[10px] leading-4 text-destructive">
            板框已更新，以下对象保持原物理坐标和实际尺寸，但超出新板框：
          </p>
          <ul className="space-y-1 text-[10px] text-destructive">
            {document.board.diagnostics.map((diagnostic) => {
              const layers =
                diagnostic.side === "front"
                  ? document.frontLayers
                  : document.backLayers;
              const layer = layers.find(
                (candidate) => candidate.id === diagnostic.layerId,
              );
              return (
                <li key={`${diagnostic.side}-${diagnostic.layerId}`}>
                  {diagnostic.side === "front" ? "正面" : "背面"} ·{" "}
                  {layer?.name ?? diagnostic.layerId}
                </li>
              );
            })}
          </ul>
        </InspectorSection>
      )}
    </>
  );
}

function AspectRatioToggle({
  disabled = false,
  locked,
  onToggle,
}: {
  disabled?: boolean;
  locked: boolean;
  onToggle: () => void;
}) {
  const Icon = locked ? Lock : Unlock;
  const label = locked ? "解锁宽高比" : "锁定宽高比";
  return (
    <Button
      aria-label={label}
      className="size-7 self-end p-0 text-muted-foreground"
      disabled={disabled}
      onClick={onToggle}
      title={label}
      type="button"
      variant="ghost"
    >
      <Icon className="size-3.5" />
    </Button>
  );
}

function BoardNumberInput({
  allowZero = false,
  label,
  onCommit,
  onError,
  value,
}: {
  allowZero?: boolean;
  label: string;
  onCommit: (value: number) => void;
  onError: (error: unknown) => void;
  value: number;
}) {
  const formatted = formatThousandths(value);
  const [draft, setDraft] = useState(formatted);
  useEffect(() => setDraft(formatted), [formatted]);
  const commit = () => {
    try {
      const parsed = parseMillimetresToUm(
        draft,
        allowZero ? "position" : "size",
      );
      if (allowZero && parsed < 0) {
        throw new Error(`${label}不能小于 0 mm`);
      }
      setDraft(formatThousandths(parsed));
      if (parsed !== value) onCommit(parsed);
    } catch (error) {
      setDraft(formatted);
      onError(error);
    }
  };
  return (
    <label className="space-y-1 text-[10px] text-muted-foreground">
      <span>{label}</span>
      <div className="relative">
        <input
          aria-label={`${label} (mm)`}
          className="h-7 w-full rounded-md border bg-background px-2 pr-7 font-mono text-[11px] text-foreground outline-none focus:border-primary"
          inputMode="decimal"
          onBlur={commit}
          onChange={(event) => setDraft(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              event.currentTarget.blur();
            }
            if (event.key === "Escape") {
              event.preventDefault();
              setDraft(formatted);
              event.currentTarget.blur();
            }
          }}
          value={draft}
        />
        <span className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-[9px] text-muted-foreground">
          mm
        </span>
      </div>
    </label>
  );
}

function SelectedLayerInspector({
  aspectRatioLocked,
  fontFamilies,
  layer,
  layers,
  onEditText,
  onError,
  onReplaceImage,
  onAspectRatioLockedChange,
  onSetExportEnabled,
  onSetTextStyle,
  onTransform,
}: {
  aspectRatioLocked: boolean;
  fontFamilies: string[];
  layer: ContentLayer;
  layers: ContentLayer[];
  onEditText: () => void;
  onError: (error: unknown) => void;
  onReplaceImage: () => void;
  onAspectRatioLockedChange: (locked: boolean) => void;
  onSetExportEnabled: (value: boolean) => void;
  onSetTextStyle: (fontFamily: string, fontSizeUm: number) => void;
  onTransform: (transform: ContentLayer["transform"]) => void;
}) {
  const hasGeometry =
    layer.kind.type === "image" ||
    layer.kind.type === "text" ||
    layer.kind.type === "group";
  const textKind = layer.kind.type === "text" ? layer.kind : null;
  const editable = hasGeometry && isLayerTransformEditable(layer, layers);
  const commitPatch = (patch: TransformPatch) => {
    try {
      onTransform(applyTransformPatch(layer, layers, patch));
    } catch (error) {
      onError(error);
    }
  };
  const resizeImage = (
    changedAxis: "width" | "height",
    nextValueUm: number,
  ) => {
    commitPatch(
      aspectRatioLocked
        ? resizeWithLockedAspectRatio(
            layer.transform.widthUm,
            layer.transform.heightUm,
            changedAxis,
            nextValueUm,
          )
        : { [`${changedAxis}Um`]: nextValueUm },
    );
  };
  return (
    <InspectorSection title="生产对象">
      <InspectorRow label="名称" value={layer.name} />
      <InspectorRow
        label="类型"
        value={
          layer.kind.type === "image"
            ? "图片"
            : layer.kind.type === "text"
              ? "文字"
              : layer.kind.type === "boardFill"
                ? "基础铺铜"
                : "组"
        }
      />
      {hasGeometry && (
        <div className="space-y-2 pt-1">
          <div className="grid grid-cols-2 gap-2">
            <TransformInput
              disabled={!editable}
              field="position"
              label="X"
              onCommit={(xUm) => commitPatch({ xUm })}
              onError={onError}
              unit="mm"
              value={layer.transform.xUm}
            />
            <TransformInput
              disabled={!editable}
              field="position"
              label="Y"
              onCommit={(yUm) => commitPatch({ yUm })}
              onError={onError}
              unit="mm"
              value={layer.transform.yUm}
            />
          </div>
          {layer.kind.type === "image" ? (
            <div className="grid grid-cols-[minmax(0,1fr)_1.75rem_minmax(0,1fr)] items-end gap-1">
              <TransformInput
                disabled={!editable}
                field="size"
                label="宽"
                onCommit={(widthUm) => resizeImage("width", widthUm)}
                onError={onError}
                unit="mm"
                value={layer.transform.widthUm}
              />
              <AspectRatioToggle
                disabled={!editable}
                locked={aspectRatioLocked}
                onToggle={() =>
                  onAspectRatioLockedChange(!aspectRatioLocked)
                }
              />
              <TransformInput
                disabled={!editable}
                field="size"
                label="高"
                onCommit={(heightUm) => resizeImage("height", heightUm)}
                onError={onError}
                unit="mm"
                value={layer.transform.heightUm}
              />
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-2">
              <TransformInput
                disabled={!editable}
                field="size"
                label="宽"
                onCommit={(widthUm) => commitPatch({ widthUm })}
                onError={onError}
                unit="mm"
                value={layer.transform.widthUm}
              />
              <TransformInput
                disabled={!editable}
                field="size"
                label="高"
                onCommit={(heightUm) => commitPatch({ heightUm })}
                onError={onError}
                unit="mm"
                value={layer.transform.heightUm}
              />
            </div>
          )}
          <div className="grid grid-cols-2 gap-2">
            <TransformInput
              disabled={!editable}
              field="rotation"
              label="旋转"
              onCommit={(rotationMdeg) => commitPatch({ rotationMdeg })}
              onError={onError}
              unit="°"
              value={layer.transform.rotationMdeg}
            />
          </div>
        </div>
      )}
      {hasGeometry && !editable && (
        <p className="text-[10px] leading-4 text-muted-foreground">
          图层自身或祖先已锁定，不能修改变换。
        </p>
      )}
      {layer.kind.type === "image" && (
        <Button
          className="mt-2 w-full"
          onClick={onReplaceImage}
          variant="ghost"
        >
          <ImagePlus className="size-3.5" />
          替换图片
        </Button>
      )}
      {textKind && (
        <div className="space-y-2 pt-2">
          <label className="block space-y-1 text-[10px] text-muted-foreground">
            <span>字体</span>
            <select
              aria-label="字体"
              className="h-8 w-full rounded-md border bg-background px-2 text-[11px] text-foreground disabled:cursor-not-allowed disabled:opacity-55"
              disabled={!editable}
              onChange={(event) =>
                onSetTextStyle(
                  event.currentTarget.value,
                  textKind.fontSizeUm,
                )
              }
              style={{ fontFamily: textKind.fontFamily }}
              value={textKind.fontFamily}
            >
              {!fontFamilies.includes(textKind.fontFamily) && (
                <option value={textKind.fontFamily}>
                  {textKind.fontFamily}（本机缺失，将回退）
                </option>
              )}
              {fontFamilies.map((family) => (
                <option key={family} value={family}>
                  {family === "sans-serif" ? "内置 Noto Sans SC" : family}
                </option>
              ))}
            </select>
          </label>
          <TransformInput
            disabled={!editable}
            field="size"
            label="字号"
            onCommit={(fontSizeUm) =>
              onSetTextStyle(textKind.fontFamily, fontSizeUm)
            }
            onError={onError}
            unit="mm"
            value={textKind.fontSizeUm}
          />
          <Button className="w-full" onClick={onEditText} variant="ghost">
            <Type className="size-3.5" />
            编辑文字
          </Button>
          <p className="text-[10px] leading-4 text-muted-foreground">
            本机字体用于当前工程；换电脑缺失时回退为内置字体。
          </p>
        </div>
      )}
      {layer.kind.type === "boardFill" && (
        <InspectorRow
          label="板边间距"
          mono
          value={`${(layer.kind.edgeClearanceUm / 1_000).toFixed(2)} mm`}
        />
      )}
      <label className="flex items-center justify-between gap-3 text-[11px]">
        <span className="text-muted-foreground">参与生产导出</span>
        <input
          checked={layer.exportEnabled}
          onChange={(event) => onSetExportEnabled(event.currentTarget.checked)}
          type="checkbox"
        />
      </label>
    </InspectorSection>
  );
}

function TransformInput({
  disabled,
  field,
  label,
  onCommit,
  onError,
  unit,
  value,
}: {
  disabled: boolean;
  field: "position" | "size" | "rotation";
  label: string;
  onCommit: (value: number) => void;
  onError: (error: unknown) => void;
  unit: "mm" | "°";
  value: number;
}) {
  const formatted = formatThousandths(value);
  const [draft, setDraft] = useState(formatted);
  useEffect(() => setDraft(formatted), [formatted]);
  const commit = () => {
    try {
      const parsed =
        field === "rotation"
          ? parseDegreesToMdeg(draft)
          : parseMillimetresToUm(draft, field);
      setDraft(formatThousandths(parsed));
      if (parsed !== value) onCommit(parsed);
    } catch (error) {
      setDraft(formatted);
      onError(error);
    }
  };
  return (
    <label className="space-y-1 text-[10px] text-muted-foreground">
      <span>{label}</span>
      <div className="relative">
        <input
          aria-label={`${label} (${unit})`}
          className="h-7 w-full rounded-md border bg-background px-2 pr-7 font-mono text-[11px] text-foreground outline-none focus:border-primary disabled:cursor-not-allowed disabled:opacity-55"
          disabled={disabled}
          inputMode="decimal"
          onBlur={commit}
          onChange={(event) => setDraft(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              event.currentTarget.blur();
            }
            if (event.key === "Escape") {
              event.preventDefault();
              setDraft(formatted);
              event.currentTarget.blur();
            }
          }}
          value={draft}
        />
        <span className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-[9px] text-muted-foreground">
          {unit}
        </span>
      </div>
    </label>
  );
}

function formatThousandths(value: number) {
  return (value / 1_000).toFixed(3);
}

interface NewProjectOptions {
  title?: string;
  widthMm?: number;
  heightMm?: number;
}

export function ProjectHome({
  onNew,
  onOpen,
  onOpenSettings,
}: {
  onNew: (options?: NewProjectOptions) => void;
  onOpen: () => void;
  onOpenSettings: () => void;
}) {
  const createCustomProject = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const widthMm = Number(data.get("widthMm"));
    const heightMm = Number(data.get("heightMm"));
    if (
      !Number.isFinite(widthMm) ||
      !Number.isFinite(heightMm) ||
      widthMm < 10 ||
      heightMm < 10 ||
      widthMm > 500 ||
      heightMm > 500
    ) {
      return;
    }
    onNew({ title: "未命名自定义卡片", widthMm, heightMm });
  };

  return (
    <main className="min-h-screen overflow-auto bg-workspace text-foreground">
      <section className="mx-auto flex min-h-screen w-full max-w-6xl flex-col px-5 py-6 sm:px-8 sm:py-8 lg:px-12 lg:py-10">
        <header className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <BrandIcon className="size-10 rounded-xl shadow-sm" />
            <div>
              <h1 className="text-lg font-semibold tracking-tight">
                PCB Atelier
              </h1>
              <p className="text-[11px] text-muted-foreground">
                双面 PCB 艺术卡设计工作台
              </p>
            </div>
          </div>
          <Button
            aria-label="打开设置"
            className="gap-2"
            onClick={onOpenSettings}
            variant="ghost"
          >
            <Settings2 className="size-4" />
            <span className="hidden sm:inline">设置</span>
          </Button>
        </header>

        <div className="my-auto grid gap-5 py-8 lg:grid-cols-[minmax(0,1.5fr)_minmax(280px,0.8fr)]">
          <section className="rounded-2xl border bg-card p-5 shadow-sm sm:p-7">
            <div className="mb-6 max-w-xl">
              <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-primary">
                Create
              </p>
              <h2 className="mt-2 text-2xl font-semibold tracking-tight sm:text-3xl">
                新建工程
              </h2>
              <p className="mt-2 text-xs leading-5 text-muted-foreground">
                从常用双面板卡尺寸开始。进入工作区后仍可在板体检查器中精确调整。
              </p>
            </div>

            <div className="grid gap-5 sm:grid-cols-3">
              <ProjectPresetButton
                description="PCB Atelier 默认画布"
                heightMm={100}
                label="标准艺术卡"
                onNew={onNew}
                widthMm={64}
              />
              <ProjectPresetButton
                description="常用桌游与收藏卡尺寸"
                heightMm={88}
                label="标准扑克牌"
                onNew={onNew}
                widthMm={63}
              />
              <ProjectPresetButton
                description="适合徽标与对称构图"
                heightMm={80}
                label="方形卡片"
                onNew={onNew}
                widthMm={80}
              />
            </div>

            <form
              className="mt-4 grid gap-3 rounded-xl border bg-background/60 p-4 sm:grid-cols-[1fr_1fr_auto] sm:items-end"
              onSubmit={createCustomProject}
            >
              <label className="grid gap-1.5 text-[11px] font-medium">
                自定义尺寸 · 宽
                <span className="relative">
                  <input
                    className="h-9 w-full rounded-md border bg-background px-3 pr-9 text-xs outline-none focus:border-primary"
                    defaultValue="64"
                    max="500"
                    min="10"
                    name="widthMm"
                    required
                    step="0.1"
                    type="number"
                  />
                  <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[10px] text-muted-foreground">
                    mm
                  </span>
                </span>
              </label>
              <label className="grid gap-1.5 text-[11px] font-medium">
                高
                <span className="relative">
                  <input
                    className="h-9 w-full rounded-md border bg-background px-3 pr-9 text-xs outline-none focus:border-primary"
                    defaultValue="100"
                    max="500"
                    min="10"
                    name="heightMm"
                    required
                    step="0.1"
                    type="number"
                  />
                  <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[10px] text-muted-foreground">
                    mm
                  </span>
                </span>
              </label>
              <Button className="h-9 gap-2 text-xs" type="submit">
                <FilePlus2 className="size-4" />
                创建
              </Button>
            </form>
          </section>

          <aside className="flex flex-col rounded-2xl border bg-card p-5 shadow-sm sm:p-7">
            <div className="grid size-11 place-items-center rounded-xl border bg-background text-primary">
              <FolderOpen className="size-5" />
            </div>
            <h2 className="mt-5 text-lg font-semibold">继续已有工作</h2>
            <p className="mt-2 text-xs leading-5 text-muted-foreground">
              打开保存在 Mac 上的 PCB Atelier{" "}
              <code className="rounded bg-muted px-1 py-0.5">.pcba</code>{" "}
              工程。工程内容只在你选择文件后读取。
            </p>
            <Button
              aria-label="打开 PCB Atelier 工程"
              className="mt-6 w-full justify-start gap-2 border bg-background text-foreground hover:bg-accent"
              onClick={onOpen}
              type="button"
              variant="ghost"
            >
              <FolderOpen className="size-4" />
              打开工程…
            </Button>
            <div className="mt-auto pt-8">
              <p className="border-t pt-4 text-[10px] leading-5 text-muted-foreground">
                嘉立创 EDA 是下游交付格式；请在编辑器中完成生产检查后再导出。
              </p>
            </div>
          </aside>
        </div>

        <footer className="text-[10px] text-muted-foreground">
          本地优先 · 工程文件由你管理
        </footer>
      </section>
    </main>
  );
}

function BrandIcon({ className }: { className: string }) {
  return (
    <img
      alt=""
      aria-hidden="true"
      className={`shrink-0 object-cover ${className}`}
      data-brand-icon="true"
      draggable={false}
      src={brandIconUrl}
    />
  );
}

function ProjectPresetButton({
  description,
  heightMm,
  label,
  onNew,
  widthMm,
}: {
  description: string;
  heightMm: number;
  label: string;
  onNew: (options: NewProjectOptions) => void;
  widthMm: number;
}) {
  const previewX = (120 - widthMm) / 2;
  const previewY = (112 - heightMm) / 2;
  const cornerRadius = Math.min(5, widthMm / 10, heightMm / 10);

  return (
    <button
      aria-label={`新建${label}工程`}
      className="group rounded-xl p-3 text-left transition-[background-color,transform] hover:-translate-y-0.5 hover:bg-accent/40"
      onClick={() =>
        onNew({
          title: `未命名${label}`,
          widthMm,
          heightMm,
        })
      }
      type="button"
    >
      <svg
        aria-hidden="true"
        className="block h-24 w-full overflow-visible"
        data-preview-size={`${widthMm}x${heightMm}`}
        preserveAspectRatio="xMidYMid meet"
        viewBox="0 0 120 120"
      >
        <rect
          className="fill-muted stroke-border"
          height={heightMm}
          rx={cornerRadius}
          width={widthMm}
          x={previewX + 4}
          y={previewY + 4}
        />
        <rect
          className="fill-primary/10 stroke-primary/60"
          height={heightMm}
          rx={cornerRadius}
          width={widthMm}
          x={previewX}
          y={previewY}
        />
        <rect
          className="fill-none stroke-primary/30"
          height={Math.max(0, heightMm - 10)}
          rx={Math.max(1, cornerRadius - 1)}
          strokeDasharray="2 2"
          width={Math.max(0, widthMm - 10)}
          x={previewX + 5}
          y={previewY + 5}
        />
      </svg>
      <span className="mt-3 block text-xs font-semibold">{label}</span>
      <span className="mt-1 block text-[11px] font-medium text-primary">
        {widthMm} × {heightMm} mm
      </span>
      <span className="mt-2 block text-[10px] leading-4 text-muted-foreground">
        {description}
      </span>
    </button>
  );
}

function AppSettingsDialog({
  onCanvasViewChange,
  onClose,
  onLaunchWindowModeChange,
  onThemePreferenceChange,
  onWheelZoomDampingChange,
  settings,
  themePreference,
}: {
  onCanvasViewChange: (value: CanvasView) => void;
  onClose: () => void;
  onLaunchWindowModeChange: (value: LaunchWindowMode) => void;
  onThemePreferenceChange: (value: ThemePreference) => void;
  onWheelZoomDampingChange: (value: WheelZoomDamping) => void;
  settings: AppSettings;
  themePreference: ThemePreference;
}) {
  return (
    <div
      className="fixed inset-0 z-[100] grid place-items-center bg-black/55 p-6"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section
        aria-label="设置"
        aria-modal="true"
        className="flex max-h-[calc(100vh-2rem)] w-full max-w-[880px] flex-col overflow-hidden rounded-xl border bg-popover text-popover-foreground shadow-2xl"
        role="dialog"
      >
        <header className="flex shrink-0 items-center justify-between border-b px-6 py-4">
          <div>
            <h2 className="text-sm font-semibold">设置</h2>
            <p className="mt-0.5 text-[11px] text-muted-foreground">
              这些偏好保存在本机，不写入工程文件。
            </p>
          </div>
          <Button
            aria-label="关闭设置"
            onClick={onClose}
            size="icon"
            variant="ghost"
          >
            <X className="size-4" />
          </Button>
        </header>
        <div className="min-h-0 divide-y overflow-y-auto px-6 py-1">
          <SettingsSegmented
            description="启动编辑器时使用的画板组合与排列。"
            label="默认画板视图"
            onChange={(value) => onCanvasViewChange(value as CanvasView)}
            options={[
              ["horizontal", "左右", Columns2],
              ["vertical", "上下", Rows2],
              ["focus-active", "聚焦当前面", Focus],
            ]}
            value={settings.canvasView}
          />
          <SettingsSegmented
            description="控制滚轮或触控板每次滚动带来的缩放幅度。"
            label="滚轮缩放速度"
            onChange={(value) =>
              onWheelZoomDampingChange(value as WheelZoomDamping)
            }
            options={[
              ["high", "慢", Turtle],
              ["medium", "标准", Gauge],
              ["low", "快", Rabbit],
            ]}
            value={settings.wheelZoomDamping}
          />
          <SettingsSegmented
            description="下次打开桌面应用时采用的窗口状态；默认最大化但保留系统标题栏。"
            label="启动窗口"
            onChange={(value) =>
              onLaunchWindowModeChange(value as LaunchWindowMode)
            }
            options={[
              ["maximized", "窗口化全屏", Maximize2],
              ["fullscreen", "系统全屏", Focus],
              ["windowed", "普通窗口", Monitor],
            ]}
            value={settings.launchWindowMode}
          />
          <SettingsSegmented
            description="可跟随系统，或固定浅色与深色。"
            label="界面外观"
            onChange={(value) =>
              onThemePreferenceChange(value as ThemePreference)
            }
            options={[
              ["system", "跟随系统", Monitor],
              ["light", "浅色", Sun],
              ["dark", "深色", Moon],
            ]}
            value={themePreference}
          />
        </div>
      </section>
    </div>
  );
}

function SettingsSegmented({
  description,
  disabled = false,
  label,
  onChange,
  options,
  value,
}: {
  description: string;
  disabled?: boolean;
  label: string;
  onChange: (value: string) => void;
  options: Array<[string, string, LucideIcon?]>;
  value: string;
}) {
  return (
    <div className="grid gap-3 py-4 sm:grid-cols-[minmax(0,1fr)_minmax(360px,420px)] sm:items-center sm:gap-8">
      <div>
        <div className="text-xs font-medium">{label}</div>
        <div className="mt-1 text-[10px] leading-4 text-muted-foreground">
          {description}
        </div>
      </div>
      <div
        aria-disabled={disabled}
        aria-label={label}
        className={cn(
          "grid rounded-lg bg-muted p-1",
          disabled && "opacity-45",
        )}
        role="group"
        style={{
          gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))`,
        }}
      >
        {options.map(([optionValue, optionLabel, OptionIcon]) => (
          <button
            aria-pressed={value === optionValue}
            className={cn(
              "flex h-8 items-center justify-center gap-1.5 rounded-md px-2 text-[11px] font-medium text-muted-foreground transition-colors",
              value === optionValue &&
                "bg-background text-foreground shadow-sm",
              !disabled && "hover:text-foreground",
            )}
            disabled={disabled}
            key={optionValue}
            onClick={() => onChange(optionValue)}
            type="button"
          >
            {OptionIcon && <OptionIcon aria-hidden className="size-3.5" />}
            <span>{optionLabel}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

function ProjectMenuItem({
  disabled = false,
  icon: Icon,
  label,
  onClick,
  shortcut,
}: {
  disabled?: boolean;
  icon: typeof MousePointer2;
  label: string;
  onClick: () => void;
  shortcut?: string;
}) {
  return (
    <button
      className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[11px] hover:bg-accent disabled:pointer-events-none disabled:opacity-40"
      disabled={disabled}
      onClick={onClick}
      role="menuitem"
      type="button"
    >
      <Icon className="size-3.5 text-muted-foreground" />
      <span className="flex-1">{label}</span>
      {shortcut && (
        <span className="text-[9px] text-muted-foreground">{shortcut}</span>
      )}
    </button>
  );
}

function PreviewLoading() {
  return (
    <div
      className="grid size-full place-items-center rounded-xl border bg-card/30"
      data-testid="preview-loading"
    >
      <div className="max-w-sm text-center">
        <Box className="mx-auto size-8 text-muted-foreground" />
        <p className="mt-3 text-sm font-medium">正在准备 3D 成板预览</p>
        <div
          aria-label="3D 预览编译进度"
          aria-valuetext="正在编译"
          className="mx-auto mt-3 h-1 w-32 overflow-hidden rounded-full bg-muted"
          role="progressbar"
        >
          <div className="h-full w-2/3 animate-pulse rounded-full bg-primary" />
        </div>
        <p className="mt-1.5 text-[11px] leading-5 text-muted-foreground">
          预览只读取编译后的铜层、阻焊开窗、丝印与板框，不会修改编辑数据。
        </p>
      </div>
    </div>
  );
}

function PreviewUpdating() {
  return (
    <div
      aria-label="正在更新 3D 成板预览"
      className="pointer-events-none absolute right-4 top-4 flex items-center gap-2 rounded-lg border bg-popover/95 px-3 py-2 text-[10px] text-popover-foreground shadow-lg"
      role="status"
    >
      <span
        aria-hidden="true"
        className="size-2 animate-pulse rounded-full bg-primary"
      />
      正在编译当前版本，仍可旋转和缩放
      <span
        aria-label="3D 预览更新进度"
        aria-valuetext="正在编译"
        className="sr-only"
        role="progressbar"
      />
    </div>
  );
}

function ContextMenuButton({
  destructive = false,
  disabled = false,
  label,
  onClick,
}: {
  destructive?: boolean;
  disabled?: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      className={cn(
        "flex h-8 w-full items-center rounded-md px-2 text-left text-[11px] enabled:hover:bg-accent disabled:cursor-not-allowed disabled:opacity-40",
        destructive && "text-destructive",
      )}
      disabled={disabled}
      onClick={onClick}
      role="menuitem"
      type="button"
    >
      {label}
    </button>
  );
}

function ToolButton({
  active,
  icon: Icon,
  label,
  onClick,
  shortcut,
}: {
  active: boolean;
  icon: typeof MousePointer2;
  label: string;
  onClick: () => void;
  shortcut: string;
}) {
  return (
    <Button
      aria-label={`${label}工具 (${shortcut})`}
      aria-pressed={active}
      className={cn(active && "bg-accent text-accent-foreground")}
      onClick={onClick}
      size="icon"
      title={`${label} (${shortcut})`}
      variant="ghost"
    >
      <Icon className="size-4" />
    </Button>
  );
}

function SegmentedControl({
  ariaLabel,
  items,
  onChange,
  value,
}: {
  ariaLabel: string;
  items: Array<{
    id: string;
    label: string;
    ariaLabel?: string;
    icon?: typeof Layers3;
  }>;
  onChange: (value: string) => void;
  value: string;
}) {
  return (
    <div
      aria-label={ariaLabel}
      className="flex rounded-lg bg-muted p-0.5"
      role="group"
    >
      {items.map((item) => {
        const Icon = item.icon;
        const active = item.id === value;
        return (
          <button
            aria-label={item.ariaLabel}
            aria-pressed={active}
            className={cn(
              "flex h-7 items-center gap-1.5 rounded-md px-2.5 text-[11px] font-medium text-muted-foreground transition-colors",
              active &&
                "bg-card text-foreground shadow-[0_1px_2px_rgba(24,20,14,0.12)]",
            )}
            key={item.id}
            onClick={() => onChange(item.id)}
            type="button"
          >
            {Icon && <Icon className="size-3.5" />}
            {item.label}
          </button>
        );
      })}
    </div>
  );
}

function PanelHeading({ detail, title }: { detail: string; title: string }) {
  return (
    <div className="flex h-11 items-center justify-between border-b px-3">
      <h2 className="text-xs font-semibold">{title}</h2>
      <span className="text-[10px] text-muted-foreground">{detail}</span>
    </div>
  );
}

function InspectorSection({
  children,
  title,
}: {
  children: React.ReactNode;
  title: string;
}) {
  return (
    <section className="border-b px-3 py-3">
      <h3 className="mb-2.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
        {title}
      </h3>
      <div className="space-y-2">{children}</div>
    </section>
  );
}

function InspectorRow({
  label,
  mono = false,
  value,
}: {
  label: string;
  mono?: boolean;
  value: string;
}) {
  return (
    <div className="flex items-start justify-between gap-3 text-[11px]">
      <span className="text-muted-foreground">{label}</span>
      <span className={cn("min-w-0 truncate text-right", mono && "font-mono")}>
        {value}
      </span>
    </div>
  );
}

function getWorkContextLabel(workContext: WorkContext) {
  switch (workContext) {
    case "copper":
      return "铜层";
    case "solderMaskOpen":
      return "阻焊开窗";
    case "silkscreen":
      return "丝印层";
  }
}

function getColorOriginalAvailability({
  mediaType,
  productionLayers,
  profile,
}: {
  mediaType: string;
  productionLayers: WorkContext[];
  profile: WorkspaceDocument["manufacturerProfile"];
}): { available: boolean; reason?: string } {
  if (
    productionLayers.length === 0 ||
    productionLayers.some((layer) => layer !== "silkscreen")
  ) {
    return {
      available: false,
      reason: "彩色原图只能用于丝印层；请先移除铜层或阻焊开窗映射。",
    };
  }
  if (mediaType !== "image/png" && mediaType !== "image/jpeg") {
    return {
      available: false,
      reason: "彩色丝印原图仅支持 PNG 或 JPEG。",
    };
  }
  if (
    profile.characterProcess !== "multicolor" ||
    profile.solderMask !== "white" ||
    profile.surfaceFinish !== "enig" ||
    profile.outerCopper !== "oz1" ||
    (profile.layerCount !== 2 && profile.layerCount !== 4)
  ) {
    return {
      available: false,
      reason:
        "请先在板体制造参数中选择彩色丝印、白色阻焊、沉金、1 oz 与 2/4 层。",
    };
  }
  return { available: true };
}

function effectiveProductionVisibility(
  state: ProductionInspectionState,
  face: CardFace,
): Record<WorkContext, boolean> {
  return {
    copper: isProductionLayerRendered(state[face], "copper"),
    solderMaskOpen: isProductionLayerRendered(
      state[face],
      "solderMaskOpen",
    ),
    silkscreen: isProductionLayerRendered(state[face], "silkscreen"),
  };
}

function WorkspacePanelResizeHandle({
  onResize,
  otherPanelWidth,
  panelWidth,
  side,
}: {
  onResize: (width: number, persist: boolean) => void;
  otherPanelWidth: number;
  panelWidth: number;
  side: WorkspacePanelSide;
}) {
  const resizeBy = (deltaX: number, persist: boolean) => {
    onResize(
      resizeWorkspacePanelWidth(
        side,
        panelWidth,
        deltaX,
        otherPanelWidth,
        globalThis.innerWidth,
      ),
      persist,
    );
  };

  return (
    <div
      aria-label={`${side === "left" ? "左侧栏" : "右侧栏"}宽度`}
      aria-orientation="vertical"
      aria-valuemax={side === "left" ? 420 : 520}
      aria-valuemin={side === "left" ? 180 : 260}
      aria-valuenow={panelWidth}
      className="group relative z-20 cursor-col-resize bg-border/60 outline-none hover:bg-primary/45 focus-visible:bg-primary/60"
      data-testid={`workspace-${side}-panel-resizer`}
      onKeyDown={(event) => {
        if (event.key === "ArrowLeft") {
          event.preventDefault();
          resizeBy(-10, true);
        } else if (event.key === "ArrowRight") {
          event.preventDefault();
          resizeBy(10, true);
        }
      }}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        const startX = event.clientX;
        const startWidth = panelWidth;
        const viewportWidth = globalThis.innerWidth;
        const handlePointerMove = (moveEvent: PointerEvent) => {
          onResize(
            resizeWorkspacePanelWidth(
              side,
              startWidth,
              moveEvent.clientX - startX,
              otherPanelWidth,
              viewportWidth,
            ),
            false,
          );
        };
        const handlePointerUp = (upEvent: PointerEvent) => {
          const width = resizeWorkspacePanelWidth(
            side,
            startWidth,
            upEvent.clientX - startX,
            otherPanelWidth,
            viewportWidth,
          );
          globalThis.removeEventListener("pointermove", handlePointerMove);
          globalThis.removeEventListener("pointerup", handlePointerUp);
          onResize(width, true);
        };
        globalThis.addEventListener("pointermove", handlePointerMove);
        globalThis.addEventListener("pointerup", handlePointerUp);
      }}
      role="separator"
      tabIndex={0}
    >
      <span className="pointer-events-none absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-border group-hover:bg-primary" />
    </div>
  );
}

async function readImageDimensions(file: File) {
  if ("createImageBitmap" in globalThis) {
    try {
      const bitmap = await createImageBitmap(file);
      const dimensions = { width: bitmap.width, height: bitmap.height };
      bitmap.close();
      return dimensions;
    } catch {
      // WKWebView can reject formats it can still display; use an image element.
    }
  }
  const url = URL.createObjectURL(file);
  try {
    const image = new Image();
    image.src = url;
    await image.decode();
    return { width: image.naturalWidth, height: image.naturalHeight };
  } finally {
    URL.revokeObjectURL(url);
  }
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
