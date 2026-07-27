import {
  BoxSelect,
  ChevronDown,
  ChevronRight,
  CircuitBoard,
  Copy,
  CopyPlus,
  Eye,
  EyeOff,
  Focus,
  Group,
  ImageIcon,
  Lock,
  PaintBucket,
  Paintbrush,
  Pencil,
  Scissors,
  Trash2,
  Type,
  Unlock,
} from "lucide-react";
import {
  useEffect,
  useRef,
  useState,
  type DragEvent,
  type MouseEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";

import {
  hasProjectAssetDragPayload,
  parseProjectAssetDragPayload,
  readProjectAssetDragPayload,
} from "@/features/media/ProjectMediaLibrary";
import type {
  CardFace,
  WorkContext,
} from "@/features/workspace/workspace-state";
import type { ContentLayer, ProductionMapping } from "@/lib/core";
import { cn } from "@/lib/utils";

type FaceLayers = Record<CardFace, ContentLayer[]>;
type FaceContexts = Record<CardFace, WorkContext>;
type FaceSelections = Record<CardFace, string[]>;
type Inspection = Record<
  CardFace,
  Record<WorkContext, { visible: boolean; isolated: boolean }>
>;

interface ProductionLayerTreeProps {
  activeFace: CardFace;
  boardSelected: boolean;
  contexts: FaceContexts;
  inspection?: Inspection;
  layers: FaceLayers;
  mappings: ProductionMapping[];
  selectedIds: FaceSelections;
  onCopy?: () => void;
  onCreateBoardFill: (face: CardFace) => void;
  onCut?: () => void;
  onDelete?: () => void;
  onDropProjectAsset?: (
    assetId: string,
    face: CardFace,
    productionLayer: WorkContext,
  ) => void;
  onReorder?: (
    sourceFace: CardFace,
    targetFace: CardFace,
    layerId: string,
    newParentId: string | null,
    newIndex: number,
    sourceContext: WorkContext,
    targetContext: WorkContext,
  ) => void;
  onDuplicate?: () => void;
  onRename?: (layer: ContentLayer, name: string) => void;
  onSelectBoard: () => void;
  onSelectContext: (face: CardFace, context: WorkContext) => void;
  onSelectSource: (
    face: CardFace,
    layerId: string,
    event: MouseEvent,
    orderedLayerIds: string[],
  ) => void;
  onToggleIsolation?: (face: CardFace, context: WorkContext) => void;
  onToggleLock?: (layer: ContentLayer) => void;
  onToggleVisibility?: (layer: ContentLayer) => void;
  onToggleProductionVisibility?: (
    face: CardFace,
    context: WorkContext,
  ) => void;
}

type LayerDropPlacement = "before" | "inside" | "after" | "rootEnd";

const PRODUCTION_NODES: Array<{
  context: WorkContext;
  icon: typeof CircuitBoard;
  label: string;
  hint: string;
}> = [
  {
    context: "copper",
    icon: CircuitBoard,
    label: "铜层",
    hint: "图片、文字、图形、铺铜与挖空",
  },
  {
    context: "solderMaskOpen",
    icon: BoxSelect,
    label: "阻焊开窗",
    hint: "添加开窗或减少开窗",
  },
  {
    context: "silkscreen",
    icon: Paintbrush,
    label: "丝印层",
    hint: "图片、文字与普通图形",
  },
];

const FACES: Array<{ id: CardFace; label: string }> = [
  { id: "front", label: "正面" },
  { id: "back", label: "背面" },
];

export function ProductionLayerTree({
  activeFace,
  boardSelected,
  contexts,
  inspection,
  layers,
  mappings,
  selectedIds,
  onCopy,
  onCreateBoardFill,
  onCut,
  onDelete,
  onDropProjectAsset,
  onDuplicate,
  onReorder,
  onRename,
  onSelectBoard,
  onSelectContext,
  onSelectSource,
  onToggleLock,
  onToggleProductionVisibility,
  onToggleVisibility,
}: ProductionLayerTreeProps) {
  const [renamingLayerId, setRenamingLayerId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [dragging, setDragging] = useState<{
    face: CardFace;
    context: WorkContext;
    layerId: string;
  } | null>(null);
  const [dropTarget, setDropTarget] = useState<{
    face: CardFace;
    context: WorkContext;
    layerId: string | null;
    placement: LayerDropPlacement;
    valid: boolean;
  } | null>(null);
  const [projectAssetDropTarget, setProjectAssetDropTarget] = useState<{
    face: CardFace;
    context: WorkContext;
  } | null>(null);
  const pointerDragRef = useRef<{
    active: boolean;
    context: WorkContext;
    face: CardFace;
    layerId: string;
    pointerId: number;
    startX: number;
    startY: number;
  } | null>(null);
  const suppressClickRef = useRef(false);
  const [expanded, setExpanded] = useState<
    Record<CardFace, Set<WorkContext>>
  >(() => ({
    front: new Set([contexts.front]),
    back: new Set([contexts.back]),
  }));

  useEffect(() => {
    setExpanded((current) => ({
      ...current,
      [activeFace]: new Set([
        ...current[activeFace],
        contexts[activeFace],
      ]),
    }));
  }, [activeFace, contexts]);

  useEffect(() => {
    const handlePointerDown = (event: PointerEvent) => {
      if (
        event.target instanceof Element &&
        event.target.closest("details[data-layer-actions]")
      ) {
        return;
      }
      closeLayerActionMenus();
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeLayerActionMenus();
    };
    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    window.addEventListener("blur", closeLayerActionMenus);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("blur", closeLayerActionMenus);
    };
  }, []);

  const toggleExpanded = (face: CardFace, context: WorkContext) => {
    setExpanded((current) => {
      const next = new Set(current[face]);
      if (next.has(context)) next.delete(context);
      else next.add(context);
      return { ...current, [face]: next };
    });
  };

  const beginRename = (layer: ContentLayer) => {
    if (layer.locked || !onRename) return;
    setRenamingLayerId(layer.id);
    setRenameDraft(layer.name);
  };

  const commitRename = (layer: ContentLayer) => {
    const name = renameDraft.trim();
    setRenamingLayerId(null);
    if (name && name !== layer.name) onRename?.(layer, name);
  };

  const pointerDropAt = (clientX: number, clientY: number) => {
    const dragging = pointerDragRef.current;
    if (!dragging?.active) return null;
    const element = document.elementFromPoint(clientX, clientY);
    const rootDrop = element?.closest<HTMLElement>(
      "[data-production-layer-drop]",
    );
    if (rootDrop?.dataset.layerDropFace) {
      const targetFace = rootDrop.dataset.layerDropFace as CardFace;
      const targetContext = rootDrop.dataset
        .layerDropContext as WorkContext;
      if (
        !canMoveLayerToContext(
          layers[dragging.face],
          dragging.layerId,
          targetContext,
        )
      ) {
        return null;
      }
      const intent =
        targetFace === dragging.face
          ? resolveLayerDrop(
              layers[dragging.face],
              dragging.layerId,
              null,
              "rootEnd",
            )
          : resolveCrossFaceLayerDrop(
              layers[dragging.face],
              layers[targetFace],
              dragging.layerId,
              null,
              "rootEnd",
            );
      return intent
        ? {
            intent,
            target: {
              face: targetFace,
              context: targetContext,
              layerId: null,
              placement: "rootEnd" as const,
              valid: true,
            },
          }
        : null;
    }

    const row = element?.closest<HTMLElement>("[data-layer-drop-id]");
    if (
      !row ||
      !row.dataset.layerDropFace
    ) {
      return null;
    }
    const targetFace = row.dataset.layerDropFace as CardFace;
    const targetContext = row.dataset.layerDropContext as WorkContext;
    if (
      !canMoveLayerToContext(
        layers[dragging.face],
        dragging.layerId,
        targetContext,
      )
    ) {
      return null;
    }
    const targetLayerId = row.dataset.layerDropId;
    const target = layers[targetFace].find(
      (layer) => layer.id === targetLayerId,
    );
    if (!target || !targetLayerId) return null;
    const placement = layerDropPlacement(clientY, row, target);
    const intent =
      targetFace === dragging.face
        ? resolveLayerDrop(
            layers[dragging.face],
            dragging.layerId,
            targetLayerId,
            placement,
          )
        : resolveCrossFaceLayerDrop(
            layers[dragging.face],
            layers[targetFace],
            dragging.layerId,
            targetLayerId,
            placement,
          );
    return intent
      ? {
          intent,
          target: {
            face: targetFace,
            context: targetContext,
            layerId: targetLayerId,
            placement,
            valid: true,
          },
        }
      : null;
  };

  const invalidPointerTargetAt = (clientX: number, clientY: number) => {
    const element = document.elementFromPoint(clientX, clientY);
    const target = element?.closest<HTMLElement>(
      "[data-layer-drop-id], [data-production-layer-drop]",
    );
    if (!target) return null;
    const context = target.dataset.layerDropContext as
      | WorkContext
      | undefined;
    if (!context) return null;
    return {
      face: (target.dataset.layerDropFace as CardFace) ?? dragging?.face ?? activeFace,
      context,
      layerId: target.dataset.layerDropId ?? null,
      placement: "rootEnd" as const,
      valid: false,
    };
  };

  const handleLayerPointerMove = (
    event: ReactPointerEvent<HTMLDivElement>,
  ) => {
    const drag = pointerDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (
      !drag.active &&
      Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) < 4
    ) {
      return;
    }
    if (!drag.active) {
      drag.active = true;
      suppressClickRef.current = true;
      event.currentTarget.setPointerCapture(event.pointerId);
      setDragging({
        face: drag.face,
        context: drag.context,
        layerId: drag.layerId,
      });
    }
    event.preventDefault();
    const candidate = pointerDropAt(event.clientX, event.clientY);
    const feedback =
      candidate?.target ??
      invalidPointerTargetAt(event.clientX, event.clientY);
    setDropTarget(feedback);
    document.documentElement.style.cursor = candidate
      ? "grabbing"
      : feedback
        ? "not-allowed"
        : "grabbing";
  };

  const finishLayerPointerDrag = (
    event: ReactPointerEvent<HTMLDivElement>,
  ) => {
    const drag = pointerDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    if (drag.active) {
      event.preventDefault();
      const candidate = pointerDropAt(event.clientX, event.clientY);
      if (candidate) {
        onReorder?.(
          drag.face,
          candidate.target.face,
          drag.layerId,
          candidate.intent.newParentId,
          candidate.intent.newIndex,
          drag.context,
          candidate.target.context,
        );
      }
      globalThis.setTimeout(() => {
        suppressClickRef.current = false;
      }, 0);
    }
    pointerDragRef.current = null;
    document.documentElement.style.removeProperty("cursor");
    setDragging(null);
    setDropTarget(null);
  };

  const acceptsProjectAsset = (event: DragEvent<HTMLElement>) =>
    hasProjectAssetDragPayload(event.dataTransfer);

  const handleProjectAssetDragOver = (
    event: DragEvent<HTMLElement>,
    face: CardFace,
    context: WorkContext,
  ) => {
    if (!acceptsProjectAsset(event)) return;
    event.preventDefault();
    event.stopPropagation();
    event.dataTransfer.dropEffect = "copy";
    setProjectAssetDropTarget({ face, context });
  };

  const handleProjectAssetDrop = (
    event: DragEvent<HTMLElement>,
    face: CardFace,
    context: WorkContext,
  ) => {
    if (!acceptsProjectAsset(event)) return;
    event.preventDefault();
    event.stopPropagation();
    const projectAsset = readProjectAssetDragPayload(event.dataTransfer);
    const request = projectAsset
      ? {
          ...projectAsset,
          face,
          productionLayer: context,
        }
      : null;
    setProjectAssetDropTarget(null);
    if (request) {
      onDropProjectAsset?.(
        request.assetId,
        request.face,
        request.productionLayer,
      );
    }
  };

  return (
    <div aria-label="板体与生产层" className="space-y-1" role="tree">
      <button
        aria-selected={boardSelected}
        className={cn(
          "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[11px] hover:bg-accent",
          boardSelected && "bg-accent text-foreground",
        )}
        data-testid="board-root"
        onClick={onSelectBoard}
        role="treeitem"
        type="button"
      >
        <CircuitBoard className="size-3.5 text-muted-foreground" />
        <span className="font-medium">板体</span>
      </button>

      {FACES.map((face) => (
        <section
          aria-label={face.label}
          className="border-t"
          key={face.id}
          role="group"
        >
          <div className="flex h-7 items-center px-2">
            <span className="text-[11px] font-semibold">{face.label}</span>
          </div>

          <div className="pb-1">
            {PRODUCTION_NODES.map((node) => {
              const active =
                activeFace === face.id && contexts[face.id] === node.context;
              const isExpanded = expanded[face.id].has(node.context);
              const inspectionState = inspection?.[face.id][node.context] ?? {
                visible: true,
                isolated: false,
              };
              const entries = entriesForProductionLayer(
                layers[face.id],
                mappings,
                face.id,
                node.context,
              );

              return (
                <div
                  className={cn(
                    "rounded-sm",
                    active && "bg-primary/8",
                    projectAssetDropTarget?.face === face.id &&
                      projectAssetDropTarget.context === node.context &&
                      "bg-primary/10 ring-1 ring-inset ring-primary/60",
                  )}
                  data-project-asset-drop-context={node.context}
                  data-project-asset-drop-face={face.id}
                  data-testid={`production-layer-${face.id}-${node.context}`}
                  key={node.context}
                  onDragLeave={(event) => {
                    if (
                      event.relatedTarget instanceof Node &&
                      event.currentTarget.contains(event.relatedTarget)
                    ) {
                      return;
                    }
                    setProjectAssetDropTarget(null);
                  }}
                  onDragOver={(event) =>
                    handleProjectAssetDragOver(event, face.id, node.context)
                  }
                  onDrop={(event) =>
                    handleProjectAssetDrop(event, face.id, node.context)
                  }
                >
                  <div
                    className={cn(
                      "group/layer flex h-8 items-center",
                      dropTarget?.layerId === null &&
                        dropTarget.face === face.id &&
                        dropTarget.context === node.context &&
                        dropTarget.valid &&
                        "bg-primary/10 ring-1 ring-inset ring-primary/60",
                      dropTarget?.layerId === null &&
                        dropTarget.face === face.id &&
                        dropTarget.context === node.context &&
                        !dropTarget.valid &&
                        "cursor-not-allowed bg-destructive/10 ring-1 ring-inset ring-destructive/70",
                    )}
                    data-layer-drop-context={node.context}
                    data-layer-drop-face={face.id}
                    data-production-layer-drop
                  >
                    <button
                      aria-expanded={isExpanded}
                      aria-label={`${isExpanded ? "收起" : "展开"}${face.label}${node.label}`}
                      className="ml-0.5 flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
                      onClick={() => toggleExpanded(face.id, node.context)}
                      type="button"
                    >
                      {isExpanded ? (
                        <ChevronDown className="size-3.5" />
                      ) : (
                        <ChevronRight className="size-3.5" />
                      )}
                    </button>
                    <button
                      aria-pressed={active}
                      className="flex min-w-0 flex-1 items-center gap-1.5 self-stretch text-left"
                      data-testid={`production-context-${face.id}-${node.context}`}
                      onClick={() => onSelectContext(face.id, node.context)}
                      role="treeitem"
                      title={node.hint}
                      type="button"
                    >
                      <node.icon className="size-3.5 text-muted-foreground" />
                      <span className="min-w-0 flex-1 truncate text-[11px] font-medium">
                        {node.label}
                      </span>
                    </button>
                    <button
                      aria-label={`${inspectionState.visible ? "隐藏" : "显示"}${face.label}${node.label}`}
                      className="flex size-7 shrink-0 items-center justify-center text-muted-foreground hover:text-foreground"
                      onClick={() =>
                        onToggleProductionVisibility?.(face.id, node.context)
                      }
                      type="button"
                    >
                      {inspectionState.visible ? (
                        <Eye className="size-3.5" />
                      ) : (
                        <EyeOff className="size-3.5" />
                      )}
                    </button>
                  </div>

                  {isExpanded && (
                    <div className="pb-1 pl-6 pr-1">
                      {entries.length > 0 &&
                        entries.map(({ layer, mapping }) => (
                          <div
                            aria-selected={selectedIds[face.id].includes(
                              layer.id,
                            )}
                            className={cn(
                              "group relative flex min-h-7 cursor-grab items-center rounded text-[10px] hover:bg-accent active:cursor-grabbing",
                              selectedIds[face.id].includes(layer.id) &&
                                "bg-accent",
                              dragging?.layerId === layer.id && "opacity-55",
                              dropTarget?.layerId === layer.id &&
                                dropTarget.face === face.id &&
                                dropTarget.context === node.context &&
                                dropTarget.valid &&
                                dropTarget.placement === "before" &&
                                "before:absolute before:inset-x-0 before:top-0 before:h-px before:bg-primary",
                              dropTarget?.layerId === layer.id &&
                                dropTarget.face === face.id &&
                                dropTarget.context === node.context &&
                                dropTarget.valid &&
                                dropTarget.placement === "after" &&
                                "after:absolute after:inset-x-0 after:bottom-0 after:h-px after:bg-primary",
                              dropTarget?.layerId === layer.id &&
                                dropTarget.face === face.id &&
                                dropTarget.context === node.context &&
                                dropTarget.valid &&
                                dropTarget.placement === "inside" &&
                                "bg-primary/10 ring-1 ring-inset ring-primary/70",
                              dropTarget?.layerId === layer.id &&
                                dropTarget.face === face.id &&
                                dropTarget.context === node.context &&
                                !dropTarget.valid &&
                                "cursor-not-allowed bg-destructive/10 ring-1 ring-inset ring-destructive/70",
                            )}
                            data-layer-drop-context={node.context}
                            data-layer-drop-face={face.id}
                            data-layer-drop-id={layer.id}
                            key={`${node.context}-${layer.id}`}
                            onClickCapture={(event) => {
                              if (!suppressClickRef.current) return;
                              event.preventDefault();
                              event.stopPropagation();
                              suppressClickRef.current = false;
                            }}
                            onContextMenu={(event) => {
                              event.preventDefault();
                              onSelectSource(
                                face.id,
                                layer.id,
                                event,
                                entries.map((entry) => entry.layer.id),
                              );
                              closeLayerActionMenus();
                              event.currentTarget
                                .querySelector<HTMLDetailsElement>(
                                  "details[data-layer-actions]",
                                )
                                ?.setAttribute("open", "");
                            }}
                            onPointerCancel={() => {
                              pointerDragRef.current = null;
                              document.documentElement.style.removeProperty(
                                "cursor",
                              );
                              setDragging(null);
                              setDropTarget(null);
                            }}
                            onPointerDown={(event) => {
                              if (
                                layer.locked ||
                                event.button !== 0 ||
                                (event.target instanceof Element &&
                                  event.target.closest(
                                    "input, details, button:not([data-layer-drag-handle])",
                                  ))
                              ) {
                                return;
                              }
                              pointerDragRef.current = {
                                active: false,
                                pointerId: event.pointerId,
                                startX: event.clientX,
                                startY: event.clientY,
                                face: face.id,
                                context: node.context,
                                layerId: layer.id,
                              };
                            }}
                            onPointerMove={handleLayerPointerMove}
                            onPointerUp={finishLayerPointerDrag}
                            role="treeitem"
                            style={{
                              paddingLeft:
                                2 +
                                layerDepth(layers[face.id], layer.id) * 14,
                              touchAction: "none",
                            }}
                          >
                            {renamingLayerId === layer.id ? (
                              <input
                                aria-label={`重命名 ${layer.name}`}
                                autoFocus
                                className="mx-1 h-6 min-w-0 flex-1 rounded border bg-background px-1.5 text-[10px] outline-none focus:border-primary"
                                onBlur={() => commitRename(layer)}
                                onChange={(event) =>
                                  setRenameDraft(event.currentTarget.value)
                                }
                                onKeyDown={(event) => {
                                  if (event.key === "Enter") {
                                    event.preventDefault();
                                    event.currentTarget.blur();
                                  } else if (event.key === "Escape") {
                                    event.preventDefault();
                                    setRenamingLayerId(null);
                                  }
                                }}
                                value={renameDraft}
                              />
                            ) : (
                              <button
                                aria-label={layer.name}
                                className="flex min-w-0 flex-1 items-center gap-1.5 px-1 text-left"
                                data-layer-drag-handle
                                onClick={(event) =>
                                  onSelectSource(
                                    face.id,
                                    layer.id,
                                    event,
                                    entries.map((entry) => entry.layer.id),
                                  )
                                }
                                onDoubleClick={() => beginRename(layer)}
                                onKeyDown={(event) => {
                                  if (
                                    (event.key === "Enter" ||
                                      event.key === "F2") &&
                                    selectedIds[face.id].includes(layer.id)
                                  ) {
                                    event.preventDefault();
                                    event.stopPropagation();
                                    beginRename(layer);
                                  }
                                }}
                                title={
                                  layer.locked
                                    ? layer.name
                                    : `${layer.name}（双击重命名）`
                                }
                                type="button"
                              >
                                <LayerKindIcon kind={layer.kind} />
                                <span className="min-w-0 flex-1 truncate">
                                  {layer.name}
                                </span>
                                {mapping?.combine === "subtract" && (
                                  <span className="text-[8px] text-muted-foreground">
                                    减少
                                  </span>
                                )}
                              </button>
                            )}
                            <TreeAction
                              persistent
                              label={layer.visible ? "隐藏对象" : "显示对象"}
                              onClick={() => onToggleVisibility?.(layer)}
                            >
                              {layer.visible ? <Eye /> : <EyeOff />}
                            </TreeAction>
                            {layer.locked && (
                              <TreeAction
                                persistent
                                label="解锁对象"
                                onClick={() => onToggleLock?.(layer)}
                              >
                                <Lock />
                              </TreeAction>
                            )}
                            <LayerActionsMenu
                              kind={layer.kind.type}
                              locked={layer.locked}
                              name={layer.name}
                              onCopy={() => onCopy?.()}
                              onCut={() => onCut?.()}
                              onDelete={() => onDelete?.()}
                              onDuplicate={() => onDuplicate?.()}
                              onRename={() => beginRename(layer)}
                              onToggleLock={() => onToggleLock?.(layer)}
                              onToggleVisibility={() =>
                                onToggleVisibility?.(layer)
                              }
                              renameEnabled={!layer.locked && Boolean(onRename)}
                              visible={layer.visible}
                            />
                          </div>
                        ))}

                      {node.context === "copper" && (
                        <button
                          aria-label="添加基础铺铜"
                          className="flex h-6 items-center gap-1 px-2 text-[9px] text-muted-foreground hover:text-foreground"
                          onClick={() => onCreateBoardFill(face.id)}
                          type="button"
                        >
                          <Focus className="size-2.5" />
                          + 基础铺铜
                        </button>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </section>
      ))}
    </div>
  );
}

function entriesForProductionLayer(
  layers: ContentLayer[],
  mappings: ProductionMapping[],
  face: CardFace,
  context: WorkContext,
) {
  const directMappings = new Map(
    mappings
      .filter(
        (mapping) =>
          mapping.target.side === face && mapping.target.layer === context,
      )
      .map((mapping) => [mapping.sourceLayerId, mapping]),
  );
  const included = new Set(directMappings.keys());
  const layerById = new Map(layers.map((layer) => [layer.id, layer]));

  for (const id of [...included]) {
    let parentId = layerById.get(id)?.parentId ?? null;
    while (parentId) {
      included.add(parentId);
      parentId = layerById.get(parentId)?.parentId ?? null;
    }
  }

  const includedLayers = layers.filter((layer) => included.has(layer.id));
  const includedById = new Map(includedLayers.map((layer) => [layer.id, layer]));
  const childrenByParent = new Map<string, ContentLayer[]>();
  for (const layer of includedLayers) {
    if (!layer.parentId || !includedById.has(layer.parentId)) continue;
    const children = childrenByParent.get(layer.parentId) ?? [];
    children.push(layer);
    childrenByParent.set(layer.parentId, children);
  }
  const ordered: ContentLayer[] = [];
  const append = (layer: ContentLayer) => {
    ordered.push(layer);
    for (const child of [...(childrenByParent.get(layer.id) ?? [])].reverse()) {
      append(child);
    }
  };
  for (const root of [...includedLayers]
    .filter((layer) => !layer.parentId || !includedById.has(layer.parentId))
    .reverse()) {
    append(root);
  }

  return ordered.map((layer) => ({
      layer,
      mapping: directMappings.get(layer.id),
      inherited: !directMappings.has(layer.id),
    }));
}

function LayerKindIcon({ kind }: { kind: ContentLayer["kind"] }) {
  const icon = {
    image: { Icon: ImageIcon, label: "图片类型" },
    text: { Icon: Type, label: "文字类型" },
    group: { Icon: Group, label: "组合类型" },
    boardFill: { Icon: PaintBucket, label: "基础铺铜类型" },
  }[kind.type];
  const Icon = icon.Icon;

  return (
    <span
      aria-label={icon.label}
      className="inline-flex shrink-0 text-muted-foreground"
      role="img"
      title={icon.label}
    >
      <Icon aria-hidden="true" className="size-3" />
    </span>
  );
}

function closeLayerActionMenus() {
  document
    .querySelectorAll<HTMLDetailsElement>(
      "details[data-layer-actions][open]",
    )
    .forEach((details) => details.removeAttribute("open"));
}

function layerDropPlacement(
  clientY: number,
  row: HTMLElement,
  target: ContentLayer,
): LayerDropPlacement {
  const bounds = row.getBoundingClientRect();
  const ratio =
    bounds.height > 0 ? (clientY - bounds.top) / bounds.height : 0.5;
  if (target.kind.type === "group") {
    if (ratio < 0.3) return "before";
    if (ratio > 0.7) return "after";
    return "inside";
  }
  return ratio < 0.5 ? "before" : "after";
}

function canMoveLayerToContext(
  layers: ContentLayer[],
  sourceLayerId: string,
  context: WorkContext,
) {
  if (context === "copper") return true;
  const movingIds = layerSubtreeIds(layers, sourceLayerId);
  return !layers.some(
    (layer) =>
      movingIds.has(layer.id) && layer.kind.type === "boardFill",
  );
}

export function resolveProjectAssetDropRequest(
  payload: string,
  face: CardFace,
  productionLayer: WorkContext,
): {
  assetId: string;
  face: CardFace;
  productionLayer: WorkContext;
} | null {
  const parsed = parseProjectAssetDragPayload(payload);
  return parsed ? { ...parsed, face, productionLayer } : null;
}

export function resolveLayerDrop(
  layers: ContentLayer[],
  sourceLayerId: string,
  targetLayerId: string | null,
  placement: LayerDropPlacement,
): { newParentId: string | null; newIndex: number } | null {
  const source = layers.find((layer) => layer.id === sourceLayerId);
  if (!source) return null;
  const movingIds = layerSubtreeIds(layers, sourceLayerId);
  if (targetLayerId && movingIds.has(targetLayerId)) return null;

  const remaining = layers.filter((layer) => !movingIds.has(layer.id));
  if (placement === "rootEnd") {
    return { newParentId: null, newIndex: 0 };
  }

  const target = remaining.find((layer) => layer.id === targetLayerId);
  if (!target) return null;
  if (placement === "inside" && target.kind.type !== "group") return null;

  if (placement === "inside") {
    return {
      newParentId: target.id,
      newIndex: subtreeEndIndex(remaining, target.id),
    };
  }

  return {
    newParentId: target.parentId,
    newIndex:
      placement === "before"
        ? subtreeEndIndex(remaining, target.id)
        : remaining.findIndex((layer) => layer.id === target.id),
  };
}

export function resolveCrossFaceLayerDrop(
  sourceLayers: ContentLayer[],
  targetLayers: ContentLayer[],
  sourceLayerId: string,
  targetLayerId: string | null,
  placement: LayerDropPlacement,
): { newParentId: string | null; newIndex: number } | null {
  if (!sourceLayers.some((layer) => layer.id === sourceLayerId)) return null;
  if (placement === "rootEnd") {
    return { newParentId: null, newIndex: 0 };
  }
  const target = targetLayers.find((layer) => layer.id === targetLayerId);
  if (!target) return null;
  if (placement === "inside" && target.kind.type !== "group") return null;
  if (placement === "inside") {
    return {
      newParentId: target.id,
      newIndex: subtreeEndIndex(targetLayers, target.id),
    };
  }
  return {
    newParentId: target.parentId,
    newIndex:
      placement === "before"
        ? subtreeEndIndex(targetLayers, target.id)
        : targetLayers.findIndex((layer) => layer.id === target.id),
  };
}

function subtreeEndIndex(layers: ContentLayer[], rootId: string) {
  const ids = layerSubtreeIds(layers, rootId);
  let end = 0;
  for (const [index, layer] of layers.entries()) {
    if (ids.has(layer.id)) end = Math.max(end, index + 1);
  }
  return end;
}

function layerSubtreeIds(layers: ContentLayer[], rootId: string) {
  const ids = new Set([rootId]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const layer of layers) {
      if (
        layer.parentId &&
        ids.has(layer.parentId) &&
        !ids.has(layer.id)
      ) {
        ids.add(layer.id);
        changed = true;
      }
    }
  }
  return ids;
}

function layerDepth(layers: ContentLayer[], layerId: string) {
  const byId = new Map(layers.map((layer) => [layer.id, layer]));
  let depth = 0;
  let parentId = byId.get(layerId)?.parentId ?? null;
  const seen = new Set<string>();
  while (parentId && !seen.has(parentId)) {
    seen.add(parentId);
    depth += 1;
    parentId = byId.get(parentId)?.parentId ?? null;
  }
  return depth;
}

function LayerActionsMenu({
  kind,
  locked,
  name,
  onCopy,
  onCut,
  onDelete,
  onDuplicate,
  onRename,
  onToggleLock,
  onToggleVisibility,
  renameEnabled,
  visible,
}: {
  kind: ContentLayer["kind"]["type"];
  locked: boolean;
  name: string;
  onCopy: () => void;
  onCut: () => void;
  onDelete: () => void;
  onDuplicate: () => void;
  onRename: () => void;
  onToggleLock: () => void;
  onToggleVisibility: () => void;
  renameEnabled: boolean;
  visible: boolean;
}) {
  const editableContent = kind !== "boardFill";
  const duplicable = editableContent && kind !== "group";
  return (
    <details className="relative" data-layer-actions>
      <summary aria-hidden="true" className="hidden" tabIndex={-1} />
      <div
        aria-label={`${name} 图层菜单`}
        className="absolute right-0 top-6 z-30 w-36 rounded-md border border-border bg-card p-1 text-card-foreground shadow-xl"
        role="menu"
      >
        <LayerMenuAction
          disabled={!renameEnabled}
          icon={<Pencil />}
          label="重命名"
          onClick={onRename}
        />
        <LayerMenuAction
          disabled={locked || !editableContent}
          icon={<Copy />}
          label="复制"
          onClick={onCopy}
        />
        <LayerMenuAction
          disabled={locked || !editableContent}
          icon={<Scissors />}
          label="剪切"
          onClick={onCut}
        />
        <LayerMenuAction
          disabled={locked || !duplicable}
          icon={<CopyPlus />}
          label="创建副本"
          onClick={onDuplicate}
        />
        <LayerMenuAction
          icon={visible ? <EyeOff /> : <Eye />}
          label={visible ? "隐藏" : "显示"}
          onClick={onToggleVisibility}
        />
        <LayerMenuAction
          icon={locked ? <Unlock /> : <Lock />}
          label={locked ? "解锁" : "锁定"}
          onClick={onToggleLock}
        />
        <div className="my-1 border-t" role="separator" />
        <LayerMenuAction
          destructive
          disabled={locked}
          icon={<Trash2 />}
          label="删除"
          onClick={onDelete}
        />
      </div>
    </details>
  );
}

function LayerMenuAction({
  destructive = false,
  disabled = false,
  icon,
  label,
  onClick,
}: {
  destructive?: boolean;
  disabled?: boolean;
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      className={cn(
        "flex h-7 w-full items-center gap-2 rounded px-2 text-left text-[10px] hover:bg-accent disabled:opacity-40 [&_svg]:size-3",
        destructive && "text-destructive hover:bg-destructive/10",
      )}
      disabled={disabled}
      onClick={(event) => {
        onClick();
        event.currentTarget.closest("details")?.removeAttribute("open");
      }}
      role="menuitem"
      type="button"
    >
      {icon}
      {label}
    </button>
  );
}

function TreeAction({
  children,
  label,
  onClick,
  persistent = false,
}: {
  children: ReactNode;
  label: string;
  onClick: () => void;
  persistent?: boolean;
}) {
  return (
    <button
      aria-label={label}
      className={cn(
        "grid size-6 shrink-0 place-items-center text-muted-foreground hover:text-foreground [&_svg]:size-2.5",
        persistent
          ? "opacity-60 hover:opacity-100"
          : "opacity-0 group-hover:opacity-60 focus:opacity-100 hover:opacity-100",
      )}
      onClick={onClick}
      type="button"
    >
      {children}
    </button>
  );
}
