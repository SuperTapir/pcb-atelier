import {
  BoxSelect,
  ChevronDown,
  ChevronRight,
  CircuitBoard,
  Eye,
  EyeOff,
  Focus,
  Layers3,
  Link2,
  Lock,
  MoreHorizontal,
  Paintbrush,
  Pencil,
  Scan,
  Trash2,
  Unlock,
} from "lucide-react";
import {
  useEffect,
  useState,
  type DragEvent,
  type MouseEvent,
  type ReactNode,
} from "react";

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
  onCreateBoardFill: (face: CardFace) => void;
  onReorder?: (
    face: CardFace,
    layerId: string,
    targetLayerId: string,
    placement: "before" | "after",
  ) => void;
  onRename?: (layer: ContentLayer, name: string) => void;
  onRemoveMapping?: (mappingId: string) => void;
  onSelectBoard: () => void;
  onSelectContext: (face: CardFace, context: WorkContext) => void;
  onSelectSource: (
    face: CardFace,
    layerId: string,
    event: MouseEvent,
  ) => void;
  onToggleIsolation?: (face: CardFace, context: WorkContext) => void;
  onToggleLock?: (layer: ContentLayer) => void;
  onToggleVisibility?: (layer: ContentLayer) => void;
  onToggleProductionVisibility?: (
    face: CardFace,
    context: WorkContext,
  ) => void;
}

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

const FACES: Array<{ id: CardFace; label: string; physicalLabel: string }> = [
  { id: "front", label: "正面", physicalLabel: "顶面" },
  { id: "back", label: "背面", physicalLabel: "底面" },
];

export function ProductionLayerTree({
  activeFace,
  boardSelected,
  contexts,
  inspection,
  layers,
  mappings,
  selectedIds,
  onCreateBoardFill,
  onReorder,
  onRename,
  onRemoveMapping,
  onSelectBoard,
  onSelectContext,
  onSelectSource,
  onToggleIsolation,
  onToggleLock,
  onToggleProductionVisibility,
  onToggleVisibility,
}: ProductionLayerTreeProps) {
  const associationCount = countMappingsBySource(mappings);
  const [renamingLayerId, setRenamingLayerId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [dragging, setDragging] = useState<{
    face: CardFace;
    context: WorkContext;
    layerId: string;
    parentId: string | null;
  } | null>(null);
  const [dropTarget, setDropTarget] = useState<{
    layerId: string;
    placement: "before" | "after";
  } | null>(null);
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
        <span className="ml-auto text-[9px] text-muted-foreground">
          正反面共享
        </span>
      </button>

      {FACES.map((face) => (
        <section
          aria-label={face.label}
          className="border-t"
          key={face.id}
          role="group"
        >
          <div className="flex h-7 items-center justify-between px-2">
            <span className="text-[11px] font-semibold">{face.label}</span>
            <span className="text-[9px] text-muted-foreground">
              {face.physicalLabel}
            </span>
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
                  )}
                  data-testid={`production-layer-${face.id}-${node.context}`}
                  key={node.context}
                >
                  <div className="group/layer flex h-8 items-center">
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
                      {active && (
                        <span className="text-[8px] text-primary">焦点</span>
                      )}
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
                    <button
                      aria-label={`${inspectionState.isolated ? "取消隔离" : "隔离"}${face.label}${node.label}`}
                      aria-pressed={inspectionState.isolated}
                      className={cn(
                        "mr-0.5 flex size-7 shrink-0 items-center justify-center text-muted-foreground opacity-0 hover:text-foreground group-hover/layer:opacity-100 focus:opacity-100",
                        inspectionState.isolated &&
                          "text-primary opacity-100",
                      )}
                      onClick={() =>
                        onToggleIsolation?.(face.id, node.context)
                      }
                      type="button"
                    >
                      <Scan className="size-3.5" />
                    </button>
                  </div>

                  {isExpanded && (
                    <div className="pb-1 pl-6 pr-1">
                      {entries.length === 0 ? (
                        <p className="h-6 px-2 text-[9px] leading-6 text-muted-foreground/70">
                          无对象
                        </p>
                      ) : (
                        entries.map(({ layer, mapping, inherited }) => (
                          <div
                            aria-selected={selectedIds[face.id].includes(
                              layer.id,
                            )}
                            className={cn(
                              "group relative flex min-h-7 items-center rounded text-[10px] hover:bg-accent",
                              selectedIds[face.id].includes(layer.id) &&
                                "bg-accent",
                              dropTarget?.layerId === layer.id &&
                                dropTarget.placement === "before" &&
                                "before:absolute before:inset-x-0 before:top-0 before:h-px before:bg-primary",
                              dropTarget?.layerId === layer.id &&
                                dropTarget.placement === "after" &&
                                "after:absolute after:inset-x-0 after:bottom-0 after:h-px after:bg-primary",
                            )}
                            draggable={!layer.locked}
                            key={`${node.context}-${layer.id}`}
                            onContextMenu={(event) => {
                              event.preventDefault();
                              onSelectSource(face.id, layer.id, event);
                              closeLayerActionMenus();
                              event.currentTarget
                                .querySelector<HTMLDetailsElement>(
                                  "details[data-layer-actions]",
                                )
                                ?.setAttribute("open", "");
                            }}
                            onDragEnd={() => {
                              setDragging(null);
                              setDropTarget(null);
                            }}
                            onDragOver={(event) => {
                              if (
                                !canDropLayer(
                                  dragging,
                                  face.id,
                                  node.context,
                                  layer,
                                )
                              ) {
                                return;
                              }
                              event.preventDefault();
                              event.dataTransfer.dropEffect = "move";
                              const bounds =
                                event.currentTarget.getBoundingClientRect();
                              setDropTarget({
                                layerId: layer.id,
                                placement:
                                  event.clientY < bounds.top + bounds.height / 2
                                    ? "before"
                                    : "after",
                              });
                            }}
                            onDragStart={(event) => {
                              event.dataTransfer.effectAllowed = "move";
                              event.dataTransfer.setData(
                                "text/plain",
                                layer.id,
                              );
                              setDragging({
                                face: face.id,
                                context: node.context,
                                layerId: layer.id,
                                parentId: layer.parentId,
                              });
                            }}
                            onDrop={(event) => {
                              if (
                                !dropTarget ||
                                !dragging ||
                                !canDropLayer(
                                  dragging,
                                  face.id,
                                  node.context,
                                  layer,
                                )
                              ) {
                                return;
                              }
                              event.preventDefault();
                              onReorder?.(
                                face.id,
                                dragging.layerId,
                                layer.id,
                                dropTarget.placement,
                              );
                              setDragging(null);
                              setDropTarget(null);
                            }}
                            role="treeitem"
                            style={{ paddingLeft: layer.parentId ? 14 : 2 }}
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
                                onClick={(event) =>
                                  onSelectSource(face.id, layer.id, event)
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
                                <Layers3 className="size-3 shrink-0 text-muted-foreground" />
                                <span className="min-w-0 flex-1 truncate">
                                  {layer.name}
                                </span>
                                {associationCount.get(layer.id)! > 1 && (
                                  <span
                                    className="flex items-center gap-0.5 text-[8px] text-primary"
                                    title={`同一源对象 ${layer.id}`}
                                  >
                                    <Link2 className="size-2.5" />
                                    关联
                                  </span>
                                )}
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
                              canRemoveMapping={
                                !inherited &&
                                Boolean(mapping && onRemoveMapping)
                              }
                              locked={layer.locked}
                              name={layer.name}
                              onRemoveMapping={() =>
                                mapping && onRemoveMapping?.(mapping.id)
                              }
                              onRename={() => beginRename(layer)}
                              onToggleLock={() => onToggleLock?.(layer)}
                              renameEnabled={!layer.locked && Boolean(onRename)}
                            />
                          </div>
                        ))
                      )}

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

function countMappingsBySource(mappings: ProductionMapping[]) {
  const counts = new Map<string, number>();
  for (const mapping of mappings) {
    counts.set(
      mapping.sourceLayerId,
      (counts.get(mapping.sourceLayerId) ?? 0) + 1,
    );
  }
  return counts;
}

function closeLayerActionMenus() {
  document
    .querySelectorAll<HTMLDetailsElement>(
      "details[data-layer-actions][open]",
    )
    .forEach((details) => details.removeAttribute("open"));
}

function canDropLayer(
  dragging: {
    face: CardFace;
    context: WorkContext;
    layerId: string;
    parentId: string | null;
  } | null,
  face: CardFace,
  context: WorkContext,
  target: ContentLayer,
) {
  return Boolean(
    dragging &&
      dragging.face === face &&
      dragging.context === context &&
      dragging.layerId !== target.id &&
      dragging.parentId === target.parentId,
  );
}

function LayerActionsMenu({
  canRemoveMapping,
  locked,
  name,
  onRemoveMapping,
  onRename,
  onToggleLock,
  renameEnabled,
}: {
  canRemoveMapping: boolean;
  locked: boolean;
  name: string;
  onRemoveMapping: () => void;
  onRename: () => void;
  onToggleLock: () => void;
  renameEnabled: boolean;
}) {
  return (
    <details className="relative shrink-0" data-layer-actions>
      <summary
        aria-label={`${name} 更多操作`}
        className="grid size-6 cursor-pointer list-none place-items-center text-muted-foreground opacity-0 hover:text-foreground group-hover:opacity-70 focus:opacity-100 [&::-webkit-details-marker]:hidden"
      >
        <MoreHorizontal className="size-3" />
      </summary>
      <div className="absolute right-0 top-6 z-30 w-28 rounded-md border border-border bg-card p-1 text-card-foreground shadow-xl">
        <LayerMenuAction
          disabled={!renameEnabled}
          icon={<Pencil />}
          label="重命名"
          onClick={onRename}
        />
        <LayerMenuAction
          icon={locked ? <Unlock /> : <Lock />}
          label={locked ? "解锁" : "锁定"}
          onClick={onToggleLock}
        />
        {canRemoveMapping && (
          <LayerMenuAction
            icon={<Trash2 />}
            label="移除关联"
            onClick={onRemoveMapping}
          />
        )}
      </div>
    </details>
  );
}

function LayerMenuAction({
  disabled = false,
  icon,
  label,
  onClick,
}: {
  disabled?: boolean;
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      className="flex h-7 w-full items-center gap-2 rounded px-2 text-left text-[10px] hover:bg-accent disabled:opacity-40 [&_svg]:size-3"
      disabled={disabled}
      onClick={(event) => {
        onClick();
        event.currentTarget.closest("details")?.removeAttribute("open");
      }}
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
