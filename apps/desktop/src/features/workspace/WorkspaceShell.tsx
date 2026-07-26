import {
  lazy,
  startTransition,
  Suspense,
  useCallback,
  useEffect,
  useReducer,
  useRef,
  useState,
} from "react";
import {
  Box,
  ChevronDown,
  Columns2,
  ImagePlus,
  Layers3,
  MousePointer2,
  PanelTop,
  Pencil,
  Redo2,
  RotateCcw,
  Type,
  Undo2,
  Ungroup,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { ExportEasyedaButton } from "@/features/export/ExportEasyedaButton";
import type { BoardPreviewInput } from "@/features/preview/board-preview-renderer";
import type { ProductionPreviewInput } from "@/features/preview/production-renderer";
import { useTheme } from "@/features/theme/ThemeProvider";
import type { ThemePreference } from "@/features/theme/theme-state";
import {
  applyGroupTransform,
  applyTransformPatch,
  isLayerTransformEditable,
  nudgeTransform,
  parseDegreesToMdeg,
  parseMillimetresToUm,
  type TransformPatch,
} from "@/features/workspace/geometry-edit";
import { ProductionLayerTree } from "@/features/workspace/ProductionLayerTree";
import {
  createProductionInspectionState,
  toggleProductionIsolation,
  toggleProductionVisibility,
} from "@/features/workspace/production-inspection";
import { WorkspaceCanvas } from "@/features/workspace/WorkspaceCanvas";
import {
  cycleOverlappingSelection,
  resolveLayerSelection,
} from "@/features/workspace/layer-selection";
import {
  createInitialWorkspaceState,
  workspaceReducer,
  type CardFace,
  type WorkContext,
  type WorkspaceMode,
  type WorkspaceTool,
} from "@/features/workspace/workspace-state";
import type { TextDraft } from "@/features/workspace/text-gesture";
import {
  insertImageAsset,
  insertTextLayer,
  createBoardFill,
  getBoardPreview,
  getProductionPreview,
  getSystemFonts,
  getWorkspaceDocument,
  groupLayers,
  mapLayer,
  redoWorkspace,
  reorderLayer,
  setLayerLock,
  setLayerName,
  setLayerExportEnabled,
  setLayerVisibility,
  setBoardOutline,
  setStackup,
  setTextContent,
  setTextStyle,
  transformLayer,
  unmapLayer,
  ungroupLayer,
  undoWorkspace,
  type ContentLayer,
  type CoreInfo,
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

const TOOLS: Array<{
  id: WorkspaceTool;
  label: string;
  shortcut: string;
  icon: typeof MousePointer2;
}> = [
  { id: "select", label: "选择", shortcut: "V", icon: MousePointer2 },
  { id: "text", label: "文字", shortcut: "T", icon: Type },
  { id: "image", label: "图片", shortcut: "I", icon: ImagePlus },
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
  const [workspace, dispatch] = useReducer(
    workspaceReducer,
    undefined,
    createInitialWorkspaceState,
  );
  const [editingLayerId, setEditingLayerId] = useState<string | null>(null);
  const [replaceLayerId, setReplaceLayerId] = useState<string | null>(null);
  const [status, setStatus] = useState("就绪");
  const [boardPreview, setBoardPreview] = useState<BoardPreviewInput | null>(
    null,
  );
  const [boardPreviewError, setBoardPreviewError] = useState<string | null>(
    null,
  );
  const [productionPreview, setProductionPreview] =
    useState<ProductionPreviewInput | null>(null);
  const [fontFamilies, setFontFamilies] = useState<string[]>(["sans-serif"]);
  const productionPreviewRequestRef = useRef(0);
  const [drillGroupIds, setDrillGroupIds] = useState<
    Record<CardFace, string | null>
  >({ front: null, back: null });
  const [productionInspection, setProductionInspection] = useState(
    createProductionInspectionState,
  );
  const fileInputRef = useRef<HTMLInputElement>(null);

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
  const widthMm = sessionDocument.board.widthUm / 1_000;
  const heightMm = sessionDocument.board.heightUm / 1_000;

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

  const importImage = useCallback(
    async (file: File, replacementId: string | null = null) => {
      if (!file.type.startsWith("image/")) {
        setStatus("只支持图片文件");
        return;
      }
      setStatus(replacementId ? "正在替换图片…" : "正在导入图片…");
      try {
        const dimensions = await readImageDimensions(file);
        const result = await insertImageAsset({
          side: workspace.activeFace,
          originalFilename: file.name || "clipboard-image.png",
          mediaType: file.type || "image/png",
          pixelWidth: dimensions.width,
          pixelHeight: dimensions.height,
          bytes: Array.from(new Uint8Array(await file.arrayBuffer())),
          replaceLayerId: replacementId,
        });
        const productionLayer =
          replacementId === null
            ? workspace.workContexts[workspace.activeFace]
            : null;
        const document = productionLayer
          ? await mapLayer(
              result.layerId,
              workspace.activeFace,
              productionLayer,
            )
          : result.document;
        setSessionDocument(document);
        selectOnly(workspace.activeFace, result.layerId);
        setEditingLayerId(null);
        setStatus(replacementId ? "图片已替换" : "图片已插入并居中");
      } catch (error) {
        setStatus(`图片导入失败：${errorMessage(error)}`);
      }
    },
    [selectOnly, workspace.activeFace, workspace.workContexts],
  );

  useEffect(() => {
    const handlePaste = (event: ClipboardEvent) => {
      const image = [...(event.clipboardData?.files ?? [])].find((file) =>
        file.type.startsWith("image/"),
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
    void getBoardPreview()
      .then((preview) => {
        if (!cancelled) {
          startTransition(() => setBoardPreview(preview));
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) setBoardPreviewError(errorMessage(error));
      });
    return () => {
      cancelled = true;
    };
  }, [sessionDocument, workspace.workspaceMode]);

  useEffect(() => {
    if (workspace.workspaceMode !== "edit") return;
    const request = productionPreviewRequestRef.current + 1;
    productionPreviewRequestRef.current = request;
    const timeout = window.setTimeout(() => {
      void getProductionPreview()
        .then((preview) => {
          if (productionPreviewRequestRef.current === request) {
            startTransition(() => setProductionPreview(preview));
          }
        })
        .catch((error: unknown) => {
          if (productionPreviewRequestRef.current === request) {
            setStatus(`生产层预览失败：${errorMessage(error)}`);
          }
        });
    }, 250);
    return () => window.clearTimeout(timeout);
  }, [sessionDocument, workspace.workspaceMode]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (
        event.target instanceof HTMLTextAreaElement ||
        event.target instanceof HTMLInputElement
      ) {
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
      const direction = {
        ArrowLeft: "left",
        ArrowRight: "right",
        ArrowUp: "up",
        ArrowDown: "down",
      }[event.key] as "left" | "right" | "up" | "down" | undefined;
      if (
        direction &&
        selectedLayer &&
        selectedIds.length === 1 &&
        selectedLayer.kind.type !== "boardFill" &&
        !event.altKey &&
        !event.ctrlKey &&
        !event.metaKey
      ) {
        event.preventDefault();
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
            event.shiftKey,
          );
          applyLayerTransform(selectedLayer.id, next);
        } catch (error) {
          setStatus(`无法变换：${errorMessage(error)}`);
        }
      }
    };
    globalThis.addEventListener("keydown", handleKeyDown);
    return () => globalThis.removeEventListener("keydown", handleKeyDown);
  }, [
    applyDocumentMutation,
    applyLayerTransform,
    currentLayers,
    drillGroupIds,
    selectOnly,
    selectedIds.length,
    selectedLayer,
    workspace.activeFace,
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

  return (
    <div
      className="grid h-screen min-h-[640px] grid-rows-[52px_minmax(0,1fr)_28px] overflow-hidden bg-background text-foreground"
      onDragOver={(event) => {
        if (
          [...event.dataTransfer.items].some((item) => item.kind === "file")
        ) {
          event.preventDefault();
          event.dataTransfer.dropEffect = "copy";
        }
      }}
      onDrop={(event) => {
        const image = [...event.dataTransfer.files].find((file) =>
          file.type.startsWith("image/"),
        );
        if (!image) return;
        event.preventDefault();
        void importImage(image);
      }}
    >
      <input
        accept="image/*"
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

      <header className="grid grid-cols-[240px_minmax(0,1fr)_280px] border-b bg-card">
        <div className="flex items-center gap-2.5 border-r px-3">
          <div className="grid size-7 place-items-center rounded-md bg-primary text-[11px] font-bold tracking-tight text-primary-foreground">
            PA
          </div>
          <div className="min-w-0">
            <p className="truncate text-sm font-semibold">
              {sessionDocument.title}
            </p>
            <p className="text-[10px] text-muted-foreground">PCB Atelier</p>
          </div>
          <ChevronDown className="ml-auto size-3.5 text-muted-foreground" />
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
                  dispatch({ type: "setTool", tool: tool.id });
                  if (tool.id === "image") {
                    setReplaceLayerId(null);
                    fileInputRef.current?.click();
                  }
                }}
                shortcut={tool.shortcut}
              />
            ))}
          </div>

          <div className="flex min-w-0 items-center gap-2">
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
            {workspace.workspaceMode === "edit" && (
              <>
                <SegmentedControl
                  ariaLabel="画板布局"
                  items={[
                    { id: "both", label: "同时查看", icon: Columns2 },
                    { id: "focus", label: "聚焦当前面", icon: PanelTop },
                  ]}
                  onChange={(editLayout) =>
                    dispatch({
                      type: "setEditLayout",
                      editLayout: editLayout as "both" | "focus",
                    })
                  }
                  value={workspace.editLayout}
                />
                {workspace.editLayout === "both" && (
                  <SegmentedControl
                    ariaLabel="画板排列"
                    items={[
                      { id: "auto", label: "自动" },
                      { id: "horizontal", label: "左右" },
                      { id: "vertical", label: "上下" },
                    ]}
                    onChange={(boardArrangement) =>
                      dispatch({
                        type: "setBoardArrangement",
                        boardArrangement: boardArrangement as
                          | "auto"
                          | "horizontal"
                          | "vertical",
                      })
                    }
                    value={workspace.boardArrangement}
                  />
                )}
              </>
            )}
          </div>

          <div className="flex items-center gap-1">
            <Button
              aria-label="撤销"
              disabled={!sessionDocument.history.canUndo}
              onClick={() =>
                void applyDocumentMutation(undoWorkspace, "已撤销")
              }
              size="icon"
              variant="ghost"
            >
              <Undo2 className="size-4" />
            </Button>
            <Button
              aria-label="重做"
              disabled={!sessionDocument.history.canRedo}
              onClick={() =>
                void applyDocumentMutation(redoWorkspace, "已重做")
              }
              size="icon"
              variant="ghost"
            >
              <Redo2 className="size-4" />
            </Button>
          </div>
        </div>

        <div className="flex items-center justify-end gap-2 border-l px-3">
          <select
            aria-label="界面主题"
            className="h-8 rounded-md border bg-background px-2 text-[11px] text-foreground"
            onChange={(event) =>
              setThemePreference(
                event.currentTarget.value as ThemePreference,
              )
            }
            title="界面主题"
            value={themePreference}
          >
            <option value="system">跟随系统</option>
            <option value="light">浅色</option>
            <option value="dark">深色</option>
          </select>
          <ExportEasyedaButton onStatus={setStatus} />
        </div>
      </header>

      {workspace.workspaceMode === "edit" ? (
        <div className="grid min-h-0 grid-cols-[240px_minmax(0,1fr)_280px]">
          <aside className="flex min-h-0 flex-col border-r bg-panel">
            <PanelHeading
              detail={`${sessionDocument.frontLayers.length + sessionDocument.backLayers.length} 项`}
              title="图层"
            />
            <div className="min-h-0 flex-1 overflow-auto p-2">
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
                onMove={(face, layerId, direction) => {
                  const layers =
                    face === "front"
                      ? sessionDocument.frontLayers
                      : sessionDocument.backLayers;
                  const index = layers.findIndex(
                    (layer) => layer.id === layerId,
                  );
                  const nextIndex = Math.max(
                    0,
                    Math.min(layers.length - 1, index + direction),
                  );
                  if (index !== nextIndex) {
                    void applyDocumentMutation(
                      () => reorderLayer(layerId, null, nextIndex),
                      "图层顺序已更新",
                    );
                  }
                }}
                onRemoveMapping={(mappingId) =>
                  void applyDocumentMutation(
                    () => unmapLayer(mappingId),
                    "生产层关联已移除",
                  )
                }
                onRename={(layer, name) =>
                  void applyDocumentMutation(
                    () => setLayerName(layer.id, name),
                    `图层已重命名为“${name}”`,
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
                onSelectSource={(face, layerId, event) => {
                  dispatch({ type: "setFace", face });
                  selectLayer(face, layerId, event.shiftKey);
                  setEditingLayerId(null);
                }}
                onToggleLock={(layer) =>
                  void applyDocumentMutation(
                    () => setLayerLock(layer.id, !layer.locked),
                    layer.locked ? "图层已解锁" : "图层已锁定",
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
                  void applyDocumentMutation(
                    () => setLayerVisibility(layer.id, !layer.visible),
                    layer.visible ? "对象已隐藏" : "对象已显示",
                  )
                }
              />
            </div>
            <div className="border-t p-2">
              <div className="grid grid-cols-2 gap-1">
                <Button
                  disabled={selectedIds.length < 2}
                  onClick={() =>
                    void (async () => {
                      try {
                        const result = await groupLayers(
                          workspace.activeFace,
                          selectedIds,
                        );
                        setSessionDocument(result.document);
                        selectOnly(workspace.activeFace, result.layerId);
                        setStatus("已分组");
                      } catch (error) {
                        setStatus(`分组失败：${errorMessage(error)}`);
                      }
                    })()
                  }
                  variant="ghost"
                >
                  <Layers3 className="size-3.5" /> 分组
                </Button>
                <Button
                  disabled={
                    selectedIds.length !== 1 ||
                    selectedLayer?.kind.type !== "group"
                  }
                  onClick={() =>
                    selectedLayer &&
                    void applyDocumentMutation(
                      () => ungroupLayer(selectedLayer.id),
                      "已解组",
                    )
                  }
                  variant="ghost"
                >
                  <Ungroup className="size-3.5" /> 解组
                </Button>
              </div>
            </div>
          </aside>

          <section
            className={cn(
              "grid min-h-0 min-w-0 gap-3 overflow-auto bg-workspace p-3",
              workspace.editLayout === "focus" && "grid-cols-1",
              workspace.editLayout === "both" &&
                workspace.boardArrangement === "auto" &&
                "grid-cols-[repeat(auto-fit,minmax(min(420px,100%),1fr))]",
              workspace.editLayout === "both" &&
                workspace.boardArrangement === "horizontal" &&
                "grid-cols-[repeat(2,minmax(400px,1fr))]",
              workspace.editLayout === "both" &&
                workspace.boardArrangement === "vertical" &&
                "grid-cols-1",
            )}
            data-arrangement={workspace.boardArrangement}
            data-layout={workspace.editLayout}
            data-testid="edit-board-layout"
          >
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
                      onCreateText={(draft) =>
                        void handleCreateText(face, draft)
                      }
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
                      onTransformLayer={applyLayerTransform}
                      onViewportChange={(nextViewport) =>
                        dispatch({
                          type: "setViewport",
                          face,
                          viewport: nextViewport,
                        })
                      }
                      productionPreview={productionPreview}
                      productionSelection={{
                        side: face,
                        visibility: {
                          copper:
                            productionInspection[face].copper.visible,
                          solderMaskOpen:
                            productionInspection[face].solderMaskOpen.visible,
                          silkscreen:
                            productionInspection[face].silkscreen.visible,
                        },
                        isolatedLayer:
                          (
                            [
                              "copper",
                              "solderMaskOpen",
                              "silkscreen",
                            ] as const
                          ).find(
                            (layer) =>
                              productionInspection[face][layer].isolated,
                          ) ?? null,
                      }}
                      selectedIds={faceSelectedIds}
                      tool={workspace.tool}
                      viewport={workspace.viewports[face]}
                      workContext={workspace.workContexts[face]}
                    />
                  </div>
                );
              })}
          </section>

          <aside className="min-h-0 overflow-auto border-l bg-panel">
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
                onSetStackup={(stackup) =>
                  void applyDocumentMutation(
                    () => setStackup(stackup),
                    "板体工艺参数已更新",
                  )
                }
              />
            ) : selectedLayer ? (
              <SelectedLayerInspector
                face={workspace.activeFace}
                fontFamilies={fontFamilies}
                layer={selectedLayer}
                layers={currentLayers}
                mappings={sessionDocument.mappings}
                onEditText={() => setEditingLayerId(selectedLayer.id)}
                onError={(error) => setStatus(errorMessage(error))}
                onSetExportEnabled={(value) =>
                  void applyDocumentMutation(
                    () => setLayerExportEnabled(selectedLayer.id, value),
                    value ? "对象已参与生产导出" : "对象已排除生产导出",
                  )
                }
                onSetMapping={(target, enabled, combine) =>
                  void (async () => {
                    const existing = sessionDocument.mappings.find(
                      (mapping) =>
                        mapping.sourceLayerId === selectedLayer.id &&
                        mapping.target.side === workspace.activeFace &&
                        mapping.target.layer === target,
                    );
                    try {
                      let document = sessionDocument;
                      if (existing) {
                        document = await unmapLayer(existing.id);
                      }
                      if (enabled) {
                        document = await mapLayer(
                          selectedLayer.id,
                          workspace.activeFace,
                          target,
                          combine,
                        );
                      }
                      setSessionDocument(document);
                      setStatus(
                        enabled ? "生产层关联已更新" : "生产层关联已移除",
                      );
                    } catch (error) {
                      setStatus(`关联失败：${errorMessage(error)}`);
                    }
                  })()
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
                  setReplaceLayerId(selectedLayer.id);
                  fileInputRef.current?.click();
                }}
                onTransform={(transform) =>
                  applyLayerTransform(selectedLayer.id, transform)
                }
              />
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
                onClick={() =>
                  dispatch({
                    type: "resetViewport",
                    face: workspace.activeFace,
                  })
                }
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
            <Suspense fallback={<PreviewLoading />}>
              <Board3DPreview
                className="size-full rounded-xl border"
                preview={boardPreview}
              />
            </Suspense>
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
  onSetStackup,
}: {
  document: WorkspaceDocument;
  onError: (error: unknown) => void;
  onSetOutline: (
    outline: WorkspaceDocument["board"]["outline"],
  ) => void;
  onSetStackup: (stackup: WorkspaceDocument["stackup"]) => void;
}) {
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

  return (
    <>
      <InspectorSection title="板体">
        <InspectorRow label="名称" value={document.title} />
        <div className="grid grid-cols-2 gap-2 pt-1">
          <BoardNumberInput
            label="宽"
            onCommit={(widthUm) => updateOutline({ widthUm })}
            onError={onError}
            value={document.board.widthUm}
          />
          <BoardNumberInput
            label="高"
            onCommit={(heightUm) => updateOutline({ heightUm })}
            onError={onError}
            value={document.board.heightUm}
          />
          <BoardNumberInput
            allowZero
            label="圆角"
            onCommit={(cornerRadiusUm) =>
              updateOutline({ cornerRadiusUm })
            }
            onError={onError}
            value={document.board.cornerRadiusUm}
          />
          <BoardNumberInput
            label="板厚"
            onCommit={(thicknessUm) =>
              onSetStackup({ ...document.stackup, thicknessUm })
            }
            onError={onError}
            value={document.stackup.thicknessUm}
          />
        </div>
        <InspectorRow label="基材" value="FR-4（首版固定）" />
        <label className="block space-y-1 text-[10px] text-muted-foreground">
          <span>阻焊颜色</span>
          <select
            aria-label="阻焊颜色"
            className="h-8 w-full rounded-md border bg-background px-2 text-[11px] text-foreground"
            onChange={(event) =>
              onSetStackup({
                ...document.stackup,
                solderMaskColor: event.currentTarget
                  .value as WorkspaceDocument["stackup"]["solderMaskColor"],
              })
            }
            value={document.stackup.solderMaskColor}
          >
            <option value="green">绿色</option>
            <option value="black">黑色</option>
            <option value="white">白色</option>
            <option value="red">红色</option>
            <option value="blue">蓝色</option>
            <option value="purple">紫色</option>
            <option value="yellow">黄色</option>
          </select>
        </label>
        <label className="block space-y-1 text-[10px] text-muted-foreground">
          <span>表面处理</span>
          <select
            aria-label="表面处理"
            className="h-8 w-full rounded-md border bg-background px-2 text-[11px] text-foreground"
            onChange={(event) =>
              onSetStackup({
                ...document.stackup,
                surfaceFinish: event.currentTarget
                  .value as WorkspaceDocument["stackup"]["surfaceFinish"],
              })
            }
            value={document.stackup.surfaceFinish}
          >
            <option value="enig">沉金（ENIG）</option>
            <option value="haslLeadFree">无铅喷锡</option>
          </select>
        </label>
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
  face,
  fontFamilies,
  layer,
  layers,
  mappings,
  onEditText,
  onError,
  onReplaceImage,
  onSetExportEnabled,
  onSetMapping,
  onSetTextStyle,
  onTransform,
}: {
  face: CardFace;
  fontFamilies: string[];
  layer: ContentLayer;
  layers: ContentLayer[];
  mappings: WorkspaceDocument["mappings"];
  onEditText: () => void;
  onError: (error: unknown) => void;
  onReplaceImage: () => void;
  onSetExportEnabled: (value: boolean) => void;
  onSetMapping: (
    layer: "copper" | "solderMaskOpen" | "silkscreen",
    enabled: boolean,
    combine: "add" | "subtract",
  ) => void;
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
        <div className="grid grid-cols-2 gap-2 pt-1">
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
      {layer.kind.type !== "group" && (
      <div className="space-y-1 pt-1">
        <p className="text-[10px] font-medium text-muted-foreground">
          关联到生产层
        </p>
        {(
          [
            ["copper", "铜层"],
            ["solderMaskOpen", "阻焊开窗"],
            ["silkscreen", "丝印层"],
          ] as const
        ).map(([target, label]) => {
          const mapping = mappings.find(
            (candidate) =>
              candidate.sourceLayerId === layer.id &&
              candidate.target.side === face &&
              candidate.target.layer === target,
          );
          const illegal =
            layer.kind.type === "boardFill" && target !== "copper";
          return (
            <div
              className="flex items-center gap-1 rounded-md border px-2 py-1.5"
              key={target}
            >
              <label className="flex min-w-0 flex-1 items-center gap-2 text-[10px]">
                <input
                  checked={mapping !== undefined}
                  disabled={illegal}
                  onChange={(event) =>
                    onSetMapping(
                      target,
                      event.currentTarget.checked,
                      mapping?.combine ?? "add",
                    )
                  }
                  type="checkbox"
                />
                <span>{label}</span>
              </label>
              {target === "solderMaskOpen" && mapping && (
                <select
                  aria-label="阻焊开窗操作"
                  className="h-6 rounded border bg-background px-1 text-[9px]"
                  onChange={(event) =>
                    onSetMapping(
                      target,
                      true,
                      event.currentTarget.value as "add" | "subtract",
                    )
                  }
                  value={mapping.combine}
                >
                  <option value="add">添加开窗</option>
                  <option value="subtract">减少开窗</option>
                </select>
              )}
            </div>
          );
        })}
      </div>
      )}
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

function PreviewLoading() {
  return (
    <div
      className="grid size-full place-items-center rounded-xl border bg-card/30"
      data-testid="preview-loading"
    >
      <div className="max-w-sm text-center">
        <Box className="mx-auto size-8 text-muted-foreground" />
        <p className="mt-3 text-sm font-medium">正在准备 3D 成板预览</p>
        <p className="mt-1.5 text-[11px] leading-5 text-muted-foreground">
          预览只读取编译后的铜层、阻焊开窗、丝印与板框，不会修改编辑数据。
        </p>
      </div>
    </div>
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
  items: Array<{ id: string; label: string; icon?: typeof Layers3 }>;
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
