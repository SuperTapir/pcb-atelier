import {
  ArrowDown,
  ArrowUp,
  BoxSelect,
  CircuitBoard,
  Eye,
  EyeOff,
  Focus,
  Layers3,
  Link2,
  Lock,
  Paintbrush,
  Scan,
  Trash2,
  Unlock,
} from "lucide-react";
import { useState, type MouseEvent, type ReactNode } from "react";

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
  onMove?: (face: CardFace, layerId: string, direction: -1 | 1) => void;
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
  onMove,
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
          "flex w-full items-center gap-2 rounded-lg border bg-card/40 px-2.5 py-2 text-left text-[11px]",
          boardSelected && "border-primary/45 bg-primary/5",
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
          className={cn(
            "rounded-lg border bg-card/25",
            activeFace === face.id && "border-primary/30",
          )}
          key={face.id}
          role="group"
        >
          <div className="flex items-center justify-between border-b px-2.5 py-1.5">
            <span className="text-[11px] font-semibold">{face.label}</span>
            <span className="text-[9px] text-muted-foreground">
              {face.physicalLabel}
            </span>
          </div>

          <div className="space-y-0.5 p-1">
            {PRODUCTION_NODES.map((node) => {
              const active =
                activeFace === face.id && contexts[face.id] === node.context;
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
                    "rounded-md",
                    active && "bg-primary/5 ring-1 ring-primary/30",
                  )}
                  key={node.context}
                >
                  <button
                    aria-pressed={active}
                    className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left"
                    data-testid={`production-context-${face.id}-${node.context}`}
                    onClick={() => onSelectContext(face.id, node.context)}
                    role="treeitem"
                    type="button"
                  >
                    <node.icon className="size-3.5 text-muted-foreground" />
                    <span className="min-w-0 flex-1">
                      <span className="block text-[11px] font-medium">
                        {node.label}
                      </span>
                      <span className="block truncate text-[9px] text-muted-foreground">
                        {node.hint}
                      </span>
                    </span>
                    {inspectionState.visible ? (
                      <Eye className="size-3 text-muted-foreground/60" />
                    ) : (
                      <EyeOff className="size-3 text-muted-foreground/60" />
                    )}
                  </button>

                  {active && (
                    <div className="border-t border-border/60 px-1.5 py-1.5">
                      <div className="mb-1 grid grid-cols-2 gap-1">
                        <SmallAction
                          active={inspectionState.visible}
                          label={
                            inspectionState.visible ? "显示中" : "已隐藏"
                          }
                          onClick={() =>
                            onToggleProductionVisibility?.(
                              face.id,
                              node.context,
                            )
                          }
                        >
                          {inspectionState.visible ? <Eye /> : <EyeOff />}
                        </SmallAction>
                        <SmallAction
                          active={inspectionState.isolated}
                          label={
                            inspectionState.isolated ? "取消隔离" : "隔离"
                          }
                          onClick={() =>
                            onToggleIsolation?.(face.id, node.context)
                          }
                        >
                          <Scan />
                        </SmallAction>
                      </div>

                      {entries.length === 0 ? (
                        <p className="px-1 py-1 text-[10px] text-muted-foreground">
                          当前生产层暂无对象
                        </p>
                      ) : (
                        entries.map(({ layer, mapping, inherited }) => (
                          <div
                            aria-selected={selectedIds[face.id].includes(
                              layer.id,
                            )}
                            className={cn(
                              "group flex min-h-7 items-center rounded text-[10px] hover:bg-accent",
                              selectedIds[face.id].includes(layer.id) &&
                                "bg-accent",
                            )}
                            key={`${node.context}-${layer.id}`}
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
                              label={layer.visible ? "隐藏对象" : "显示对象"}
                              onClick={() => onToggleVisibility?.(layer)}
                            >
                              {layer.visible ? <Eye /> : <EyeOff />}
                            </TreeAction>
                            <TreeAction
                              label={layer.locked ? "解锁对象" : "锁定对象"}
                              onClick={() => onToggleLock?.(layer)}
                            >
                              {layer.locked ? <Lock /> : <Unlock />}
                            </TreeAction>
                            <TreeAction
                              label="上移对象"
                              onClick={() =>
                                onMove?.(face.id, layer.id, 1)
                              }
                            >
                              <ArrowUp />
                            </TreeAction>
                            <TreeAction
                              label="下移对象"
                              onClick={() =>
                                onMove?.(face.id, layer.id, -1)
                              }
                            >
                              <ArrowDown />
                            </TreeAction>
                            {!inherited && mapping && onRemoveMapping && (
                              <TreeAction
                                label={`移除 ${layer.name} 生产层关联`}
                                onClick={() => onRemoveMapping(mapping.id)}
                              >
                                <Trash2 />
                              </TreeAction>
                            )}
                          </div>
                        ))
                      )}

                      {node.context === "copper" && (
                        <button
                          className="mt-1 flex w-full items-center justify-center gap-1.5 rounded-md border border-dashed px-2 py-1.5 text-[10px] text-muted-foreground hover:border-primary/50 hover:text-foreground"
                          onClick={() => onCreateBoardFill(face.id)}
                          type="button"
                        >
                          <Focus className="size-3" />
                          添加基础铺铜
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

function SmallAction({
  active,
  children,
  label,
  onClick,
}: {
  active: boolean;
  children: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-pressed={active}
      className="flex items-center justify-center gap-1 rounded px-1 py-1 text-[9px] text-muted-foreground hover:bg-accent [&_svg]:size-2.5"
      onClick={onClick}
      type="button"
    >
      {children}
      {label}
    </button>
  );
}

function TreeAction({
  children,
  label,
  onClick,
}: {
  children: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={label}
      className="grid size-6 shrink-0 place-items-center text-muted-foreground opacity-50 hover:text-foreground hover:opacity-100 [&_svg]:size-2.5"
      onClick={onClick}
      type="button"
    >
      {children}
    </button>
  );
}
