import {
  ChevronDown,
  ChevronRight,
  Folder,
  Grid2X2,
  ImageIcon,
  List,
  PanelBottomClose,
  PanelBottomOpen,
  Search,
  X,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type DragEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";

import {
  getAssetBytes,
  type AssetReference,
  type ContentLayer,
  type ImageTreatment,
  type ProductionLayer,
} from "@/lib/core";
import { isSupportedImageFileMetadata } from "@/features/media/supported-image-file";

export interface ProjectMediaDockState {
  splitPercent: number;
  productionCollapsed: boolean;
  mediaCollapsed: boolean;
  productionScrollTop: number;
  mediaScrollTop: number;
  expandedProductionNodes: string[];
  expandedMediaFolders: string[];
  selectedAssetId: string | null;
  mediaViewMode: "thumbnail" | "list";
  mediaQuery: string;
}

export interface ProjectMediaDockStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export interface MediaPlacementTarget {
  face: "front" | "back";
  productionLayer: ProductionLayer;
}

export interface MediaPlacementRequest extends MediaPlacementTarget {
  assetId: string;
}

export interface ProjectMediaExternalDropEvent {
  dataTransfer: {
    files: ArrayLike<File>;
    types: ArrayLike<string>;
    dropEffect: DataTransfer["dropEffect"];
  };
  preventDefault(): void;
  stopPropagation(): void;
}

export interface DerivedProjectMediaItem extends AssetReference {
  usageCount: number;
  treatmentCount: number;
  algorithmVersions: string[];
}

export const PROJECT_MEDIA_DOCK_STORAGE_KEY =
  "pcb-atelier.project-media-dock.v1";

export const PROJECT_ASSET_DRAG_TYPE =
  "application/x-pcb-atelier-project-asset";
const PROJECT_ASSET_TEXT_PREFIX = "pcb-atelier-project-asset:";

interface ProjectAssetDataTransfer {
  types: Iterable<string> | ArrayLike<string>;
  getData(type: string): string;
}

export const DEFAULT_PROJECT_MEDIA_DOCK_STATE: ProjectMediaDockState = {
  splitPercent: 56,
  productionCollapsed: false,
  mediaCollapsed: false,
  productionScrollTop: 0,
  mediaScrollTop: 0,
  expandedProductionNodes: [],
  expandedMediaFolders: ["*"],
  selectedAssetId: null,
  mediaViewMode: "thumbnail",
  mediaQuery: "",
};

export function loadProjectMediaDockState(
  storage: ProjectMediaDockStorage | undefined = globalThis.localStorage,
): ProjectMediaDockState {
  if (!storage) return cloneDefaultDockState();
  try {
    const raw = storage.getItem(PROJECT_MEDIA_DOCK_STORAGE_KEY);
    if (!raw) return cloneDefaultDockState();
    const value = JSON.parse(raw) as Record<string, unknown>;
    return {
      splitPercent: clampNumber(value.splitPercent, 25, 75, 56),
      productionCollapsed:
        typeof value.productionCollapsed === "boolean"
          ? value.productionCollapsed
          : false,
      mediaCollapsed:
        typeof value.mediaCollapsed === "boolean"
          ? value.mediaCollapsed
          : false,
      productionScrollTop: nonNegativeNumber(value.productionScrollTop),
      mediaScrollTop: nonNegativeNumber(value.mediaScrollTop),
      expandedProductionNodes: stringArray(value.expandedProductionNodes),
      expandedMediaFolders:
        stringArray(value.expandedMediaFolders).length > 0
          ? stringArray(value.expandedMediaFolders)
          : ["*"],
      selectedAssetId:
        typeof value.selectedAssetId === "string"
          ? value.selectedAssetId
          : null,
      mediaViewMode:
        value.mediaViewMode === "list" || value.mediaViewMode === "thumbnail"
          ? value.mediaViewMode
          : "thumbnail",
      mediaQuery: typeof value.mediaQuery === "string" ? value.mediaQuery : "",
    };
  } catch {
    return cloneDefaultDockState();
  }
}

export function saveProjectMediaDockState(
  state: ProjectMediaDockState,
  storage: ProjectMediaDockStorage | undefined = globalThis.localStorage,
) {
  if (!storage) return;
  try {
    storage.setItem(PROJECT_MEDIA_DOCK_STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Dock preferences are optional; storage failures must not block editing.
  }
}

export function deriveProjectMediaItems(
  assets: AssetReference[],
  layers: Record<"front" | "back", ContentLayer[]>,
  treatments: ImageTreatment[],
  query = "",
): DerivedProjectMediaItem[] {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const usage = new Map<string, number>();
  for (const layer of [...layers.front, ...layers.back]) {
    if (layer.kind.type !== "image") continue;
    usage.set(layer.kind.assetId, (usage.get(layer.kind.assetId) ?? 0) + 1);
  }

  return assets
    .filter((asset) => {
      if (!normalizedQuery) return true;
      return [
        asset.originalFilename,
        asset.folderPath ?? "",
        asset.mediaType,
        ...asset.tags,
      ].some((value) => value.toLocaleLowerCase().includes(normalizedQuery));
    })
    .map((asset) => {
      const assetTreatments = treatments.filter(
        (treatment) => treatment.assetId === asset.id,
      );
      return {
        ...asset,
        usageCount: usage.get(asset.id) ?? 0,
        treatmentCount: assetTreatments.length,
        algorithmVersions: [
          ...new Set(
            assetTreatments.map(
              (treatment) => treatment.recipe.algorithmVersion,
            ),
          ),
        ],
      };
    })
    .sort((left, right) =>
      left.originalFilename.localeCompare(right.originalFilename, "zh-CN"),
    );
}

export function validateMediaPlacement(
  asset: AssetReference,
  target: MediaPlacementTarget,
): { allowed: true } | { allowed: false; reason: string } {
  if (
    !asset.mediaType.startsWith("image/") ||
    asset.pixelWidth <= 0 ||
    asset.pixelHeight <= 0
  ) {
    return {
      allowed: false,
      reason: "当前素材不是可放置的图片内容",
    };
  }
  if (
    target.productionLayer !== "copper" &&
    target.productionLayer !== "solderMaskOpen" &&
    target.productionLayer !== "silkscreen"
  ) {
    return {
      allowed: false,
      reason: "当前生产层不接受图片内容",
    };
  }
  return { allowed: true };
}

export function validateProjectMediaFolderPath(
  value: string,
):
  | { allowed: true; folderPath: string | null }
  | { allowed: false; reason: string } {
  const trimmed = value.trim();
  if (!trimmed) return { allowed: true, folderPath: null };
  if (
    trimmed.length > 255 ||
    trimmed.startsWith("/") ||
    trimmed.includes("\\") ||
    [...trimmed].some((character) => {
      const code = character.charCodeAt(0);
      return code <= 31 || code === 127;
    })
  ) {
    return { allowed: false, reason: "文件夹路径格式无效" };
  }
  const segments = trimmed.split("/").map((segment) => segment.trim());
  if (
    segments.some(
      (segment) => !segment || segment === "." || segment === "..",
    )
  ) {
    return { allowed: false, reason: "文件夹路径不能包含空层级、. 或 .." };
  }
  return { allowed: true, folderPath: segments.join("/") };
}

export function parseProjectAssetDragPayload(
  value: string,
): { assetId: string } | null {
  try {
    const parsed = JSON.parse(value) as { assetId?: unknown };
    return typeof parsed.assetId === "string" && parsed.assetId.length > 0
      ? { assetId: parsed.assetId }
      : null;
  } catch {
    return null;
  }
}

export function serializeProjectAssetTextPayload(assetId: string) {
  return `${PROJECT_ASSET_TEXT_PREFIX}${JSON.stringify({ assetId })}`;
}

export function hasProjectAssetDragPayload(
  dataTransfer: Pick<ProjectAssetDataTransfer, "types">,
) {
  const types = Array.from(dataTransfer.types);
  return (
    types.includes(PROJECT_ASSET_DRAG_TYPE) ||
    types.includes("text/plain")
  );
}

export function readProjectAssetDragPayload(
  dataTransfer: ProjectAssetDataTransfer,
) {
  const customPayload = dataTransfer.getData(PROJECT_ASSET_DRAG_TYPE);
  const parsedCustom = parseProjectAssetDragPayload(customPayload);
  if (parsedCustom) return parsedCustom;
  const textPayload = dataTransfer.getData("text/plain");
  if (!textPayload.startsWith(PROJECT_ASSET_TEXT_PREFIX)) return null;
  return parseProjectAssetDragPayload(
    textPayload.slice(PROJECT_ASSET_TEXT_PREFIX.length),
  );
}

export function handleProjectMediaExternalDragOver(
  event: ProjectMediaExternalDropEvent,
) {
  if (!Array.from(event.dataTransfer.types).includes("Files")) return false;
  event.preventDefault();
  event.stopPropagation();
  event.dataTransfer.dropEffect = "copy";
  return true;
}

export function handleProjectMediaExternalDrop(
  event: ProjectMediaExternalDropEvent,
  onImportFiles: (files: File[]) => void,
  onInvalid?: (reason: string) => void,
) {
  if (!Array.from(event.dataTransfer.types).includes("Files")) return false;
  event.preventDefault();
  event.stopPropagation();
  const files = Array.from(event.dataTransfer.files);
  const images = files.filter(isSupportedImageFileMetadata);
  if (images.length > 0) onImportFiles(images);
  if (images.length !== files.length || images.length === 0) {
    onInvalid?.("仅支持 PNG、JPEG 或 WebP 图片");
  }
  return true;
}

interface ProjectMediaLibraryProps {
  assets: AssetReference[];
  treatments: ImageTreatment[];
  layers: Record<"front" | "back", ContentLayer[]>;
  activeFace: "front" | "back";
  activeProductionLayer: ProductionLayer;
  thumbnailUrls?: Readonly<Record<string, string>>;
  state?: ProjectMediaDockState;
  initialState?: ProjectMediaDockState;
  onStateChange?: (state: ProjectMediaDockState) => void;
  onImportFiles?: (files: File[]) => void;
  onMoveAsset?: (
    assetId: string,
    folderPath: string | null,
  ) => void | Promise<void>;
  onPlaceAsset: (request: MediaPlacementRequest) => void;
  onInvalidPlacement?: (reason: string) => void;
}

export function ProjectMediaLibrary(props: ProjectMediaLibraryProps) {
  const {
    assets,
    treatments,
    layers,
    activeFace,
    activeProductionLayer,
    onPlaceAsset,
    onInvalidPlacement,
  } = props;
  const [localState, setLocalState] = useState(
    () => props.initialState ?? loadProjectMediaDockState(),
  );
  const [previewAssetId, setPreviewAssetId] = useState<string | null>(null);
  const state = props.state ?? localState;
  const generatedThumbnailUrls = useAssetThumbnailUrls(
    assets,
    props.thumbnailUrls !== undefined,
  );
  const thumbnailUrls = props.thumbnailUrls ?? generatedThumbnailUrls;
  const mediaItems = deriveProjectMediaItems(
    assets,
    layers,
    treatments,
    state.mediaQuery,
  );
  const folders = groupMediaItems(mediaItems);
  const flatUncategorizedItems =
    folders.length === 1 && folders[0]?.[0] === "未分类"
      ? folders[0][1]
      : null;
  const selectedItem = mediaItems.find(
    (item) => item.id === state.selectedAssetId,
  );
  const previewItem = deriveProjectMediaItems(
    assets,
    layers,
    treatments,
  ).find((item) => item.id === previewAssetId);
  const folderPaths = [
    ...new Set(
      assets
        .map((asset) => asset.folderPath?.trim())
        .filter((path): path is string => Boolean(path)),
    ),
  ].sort((left, right) => left.localeCompare(right, "zh-CN"));

  const updateState = useCallback(
    (patch: Partial<ProjectMediaDockState>) => {
      const next = { ...state, ...patch };
      if (!props.state) setLocalState(next);
      saveProjectMediaDockState(next);
      props.onStateChange?.(next);
    },
    [props, state],
  );

  const toggleFolder = (folderPath: string) => {
    const allFolderPaths = folders.map(([path]) => path);
    const expanded = state.expandedMediaFolders.includes("*")
      ? allFolderPaths
      : state.expandedMediaFolders;
    updateState({
      expandedMediaFolders: expanded.includes(folderPath)
        ? expanded.filter((path) => path !== folderPath)
        : [...expanded, folderPath],
    });
  };

  const target = {
    face: activeFace,
    productionLayer: activeProductionLayer,
  };
  const renderMediaItem = (item: DerivedProjectMediaItem) => (
    <MediaItem
      item={item}
      key={item.id}
      onDragStart={(event) => beginAssetDrag(event, item, target)}
      onSelect={() => {
        updateState({ selectedAssetId: item.id });
        setPreviewAssetId(item.id);
      }}
      selected={state.selectedAssetId === item.id}
      target={target}
      thumbnailUrl={thumbnailUrls[item.id]}
      viewMode={state.mediaViewMode}
    />
  );

  return (
    <section
      aria-label="项目媒体"
      className="flex min-h-0 min-w-0 flex-col overflow-hidden bg-panel"
      data-collapsed={state.mediaCollapsed}
      onDragOver={(event) => handleProjectMediaExternalDragOver(event)}
      onDrop={(event) =>
        handleProjectMediaExternalDrop(
          event,
          props.onImportFiles ?? (() => undefined),
          onInvalidPlacement,
        )
      }
      title="可将外部图片拖入此处，仅导入项目媒体"
    >
      <header className="flex h-9 min-w-0 shrink-0 items-center gap-1 overflow-hidden border-b px-2">
        <ImageIcon className="size-3.5 text-muted-foreground" />
        <h2 className="min-w-0 truncate text-[11px] font-semibold">
          项目媒体
        </h2>
        <span className="text-[9px] text-muted-foreground">{assets.length}</span>
        <button
          aria-label={state.mediaCollapsed ? "展开项目媒体" : "折叠项目媒体"}
          className="ml-auto grid size-7 place-items-center rounded hover:bg-accent"
          onClick={() =>
            updateState({ mediaCollapsed: !state.mediaCollapsed })
          }
          type="button"
        >
          {state.mediaCollapsed ? (
            <PanelBottomOpen className="size-3.5" />
          ) : (
            <PanelBottomClose className="size-3.5" />
          )}
        </button>
      </header>

      {!state.mediaCollapsed && (
        <>
          <div className="flex min-w-0 shrink-0 items-center gap-1.5 overflow-hidden border-b p-2">
            <label className="flex h-7 w-0 min-w-0 flex-1 items-center gap-1.5 overflow-hidden rounded-md border bg-background px-2">
              <Search className="size-3 text-muted-foreground" />
              <span className="sr-only">搜索项目媒体</span>
              <input
                aria-label="搜索项目媒体"
                className="min-w-0 flex-1 bg-transparent text-[10px] outline-none"
                onChange={(event) =>
                  updateState({ mediaQuery: event.currentTarget.value })
                }
                placeholder="搜索名称、文件夹或标签"
                type="search"
                value={state.mediaQuery}
              />
            </label>
            <button
              aria-label="缩略图视图"
              aria-pressed={state.mediaViewMode === "thumbnail"}
              className="grid size-7 shrink-0 place-items-center rounded-md border"
              onClick={() => updateState({ mediaViewMode: "thumbnail" })}
              type="button"
            >
              <Grid2X2 className="size-3" />
            </button>
            <button
              aria-label="列表视图"
              aria-pressed={state.mediaViewMode === "list"}
              className="grid size-7 shrink-0 place-items-center rounded-md border"
              onClick={() => updateState({ mediaViewMode: "list" })}
              type="button"
            >
              <List className="size-3" />
            </button>
          </div>

          <div
            className="min-h-0 flex-1 overflow-auto p-2"
            onScroll={(event) =>
              updateState({ mediaScrollTop: event.currentTarget.scrollTop })
            }
            ref={(node) => {
              if (node && node.scrollTop !== state.mediaScrollTop) {
                node.scrollTop = state.mediaScrollTop;
              }
            }}
          >
            {folders.length === 0 ? (
              <p className="p-4 text-center text-[10px] text-muted-foreground">
                {state.mediaQuery ? "没有匹配的素材" : "工程中还没有媒体素材"}
              </p>
            ) : flatUncategorizedItems ? (
              <div
                className={
                  state.mediaViewMode === "thumbnail"
                    ? "grid grid-cols-2 gap-1.5"
                    : "space-y-1"
                }
              >
                {flatUncategorizedItems.map(renderMediaItem)}
              </div>
            ) : (
              folders.map(([folderPath, items]) => {
                const expanded =
                  state.expandedMediaFolders.includes("*") ||
                  state.expandedMediaFolders.includes(folderPath);
                return (
                  <section className="mb-2" key={folderPath}>
                    <button
                      aria-expanded={expanded}
                      className="flex h-7 w-full items-center gap-1 rounded px-1 text-left text-[10px] font-medium hover:bg-accent"
                      onClick={() => toggleFolder(folderPath)}
                      type="button"
                    >
                      {expanded ? (
                        <ChevronDown className="size-3" />
                      ) : (
                        <ChevronRight className="size-3" />
                      )}
                      <Folder className="size-3 text-muted-foreground" />
                      <span className="truncate">{folderPath}</span>
                      <span className="ml-auto text-[9px] text-muted-foreground">
                        {items.length}
                      </span>
                    </button>
                    {expanded && (
                      <div
                        className={
                          state.mediaViewMode === "thumbnail"
                            ? "grid grid-cols-2 gap-1.5 pt-1"
                            : "space-y-1 pt-1"
                        }
                      >
                        {items.map(renderMediaItem)}
                      </div>
                    )}
                  </section>
                );
              })
            )}
          </div>
          {selectedItem && (
            <SelectedMediaDetails
              folderPaths={folderPaths}
              item={selectedItem}
              onMoveAsset={props.onMoveAsset}
            />
          )}
          {previewItem && (
            <MediaPreviewDialog
              item={previewItem}
              onClose={() => setPreviewAssetId(null)}
              onPlace={() => {
                const validation = validateMediaPlacement(previewItem, target);
                if (!validation.allowed) {
                  onInvalidPlacement?.(validation.reason);
                  return;
                }
                onPlaceAsset({ assetId: previewItem.id, ...target });
                setPreviewAssetId(null);
              }}
              target={target}
              thumbnailUrl={thumbnailUrls[previewItem.id]}
            />
          )}
        </>
      )}
    </section>
  );
}

interface ProjectMediaDockProps extends ProjectMediaLibraryProps {
  productionPanel: ReactNode;
}

export function ProjectMediaDock(props: ProjectMediaDockProps) {
  const [localState, setLocalState] = useState(
    () => props.initialState ?? loadProjectMediaDockState(),
  );
  const state = props.state ?? localState;
  const dockRef = useRef<HTMLDivElement>(null);

  const updateState = (next: ProjectMediaDockState) => {
    if (!props.state) setLocalState(next);
    saveProjectMediaDockState(next);
    props.onStateChange?.(next);
  };

  const setSplitPercent = (splitPercent: number) =>
    updateState({
      ...state,
      splitPercent: Math.min(75, Math.max(25, splitPercent)),
    });

  const beginResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const dock = dockRef.current;
    if (!dock || state.productionCollapsed || state.mediaCollapsed) return;
    const bounds = dock.getBoundingClientRect();
    const handleMove = (moveEvent: PointerEvent) => {
      setSplitPercent(((moveEvent.clientY - bounds.top) / bounds.height) * 100);
    };
    const handleUp = () => {
      globalThis.removeEventListener("pointermove", handleMove);
      globalThis.removeEventListener("pointerup", handleUp);
    };
    globalThis.addEventListener("pointermove", handleMove);
    globalThis.addEventListener("pointerup", handleUp);
  };

  const rows = state.productionCollapsed
    ? "36px 6px minmax(0,1fr)"
    : state.mediaCollapsed
      ? "minmax(0,1fr) 6px 36px"
      : `${state.splitPercent}% 6px minmax(0,1fr)`;

  return (
    <div
      className="grid size-full min-h-0"
      data-testid="project-media-dock"
      ref={dockRef}
      style={{ gridTemplateRows: rows }}
    >
      <section
        aria-label="生产层"
        className="flex min-h-0 flex-col overflow-hidden"
      >
        <header className="flex h-9 shrink-0 items-center border-b px-2">
          <h2 className="text-[11px] font-semibold">生产层</h2>
          <button
            aria-label={
              state.productionCollapsed ? "展开生产层" : "折叠生产层"
            }
            className="ml-auto grid size-7 place-items-center rounded hover:bg-accent"
            onClick={() =>
              updateState({
                ...state,
                productionCollapsed: !state.productionCollapsed,
              })
            }
            type="button"
          >
            {state.productionCollapsed ? (
              <PanelBottomOpen className="size-3.5" />
            ) : (
              <PanelBottomClose className="size-3.5" />
            )}
          </button>
        </header>
        {!state.productionCollapsed && (
          <div
            className="min-h-0 flex-1 overflow-auto"
            onScroll={(event) =>
              updateState({
                ...state,
                productionScrollTop: event.currentTarget.scrollTop,
              })
            }
            ref={(node) => {
              if (node && node.scrollTop !== state.productionScrollTop) {
                node.scrollTop = state.productionScrollTop;
              }
            }}
          >
            {props.productionPanel}
          </div>
        )}
      </section>

      <div
        aria-label="调整生产层与项目媒体高度"
        aria-orientation="horizontal"
        aria-valuemax={75}
        aria-valuemin={25}
        aria-valuenow={state.splitPercent}
        className="cursor-row-resize border-y bg-muted/60 hover:bg-accent"
        onKeyDown={(event) => {
          if (event.key === "ArrowUp") {
            event.preventDefault();
            setSplitPercent(state.splitPercent - 2);
          }
          if (event.key === "ArrowDown") {
            event.preventDefault();
            setSplitPercent(state.splitPercent + 2);
          }
        }}
        onPointerDown={beginResize}
        role="separator"
        tabIndex={0}
      />

      <ProjectMediaLibrary
        {...props}
        onStateChange={updateState}
        state={state}
      />
    </div>
  );
}

function MediaItem({
  item,
  thumbnailUrl,
  selected,
  target,
  viewMode,
  onSelect,
  onDragStart,
}: {
  item: DerivedProjectMediaItem;
  thumbnailUrl?: string;
  selected: boolean;
  target: MediaPlacementTarget;
  viewMode: "thumbnail" | "list";
  onSelect: () => void;
  onDragStart: (event: DragEvent<HTMLDivElement>) => void;
}) {
  const validation = validateMediaPlacement(item, target);
  const usageLabel =
    item.usageCount > 0 ? `使用 ${item.usageCount} 次` : "未使用";

  return (
    <div
      aria-disabled={!validation.allowed}
      aria-label={`${item.originalFilename}，点击预览，可拖到${faceLabel(target.face)}${productionLayerLabel(target.productionLayer)}`}
      className={`group min-w-0 max-w-full overflow-hidden rounded-md border bg-background p-1.5 ${
        selected ? "border-transparent bg-accent" : ""
      } ${viewMode === "list" ? "grid grid-cols-[40px_minmax(0,1fr)] gap-2" : ""}`}
      data-selected={selected}
      draggable={validation.allowed}
      onClick={onSelect}
      onDragStart={onDragStart}
      onKeyDown={(event) => {
        if (event.key !== "Enter") return;
        event.preventDefault();
        onSelect();
      }}
      role="button"
      tabIndex={0}
      title={
        validation.allowed
          ? "点击预览，或拖到画布"
          : validation.reason
      }
    >
      <div
        className={`relative grid overflow-hidden rounded bg-muted ${
          viewMode === "thumbnail" ? "aspect-video w-full" : "size-10"
        } place-items-center`}
      >
        {thumbnailUrl ? (
          <img
            alt=""
            className="size-full object-contain"
            draggable={false}
            src={thumbnailUrl}
          />
        ) : (
          <ImageIcon className="size-4 text-muted-foreground" />
        )}
        {viewMode === "thumbnail" && (
          <span className="absolute right-1 top-1 rounded bg-black/70 px-1 py-0.5 text-[8px] leading-none text-white">
            {usageLabel}
          </span>
        )}
      </div>
      <div
        className={
          viewMode === "thumbnail"
            ? "flex items-center gap-1 pt-1"
            : "min-w-0 self-center"
        }
      >
        <p className="truncate text-[10px] font-medium">
          {item.originalFilename}
        </p>
        {viewMode === "list" && (
          <p className="truncate text-[8px] text-muted-foreground">
            {usageLabel}
          </p>
        )}
      </div>
    </div>
  );
}

export function MediaPreviewDialog({
  item,
  thumbnailUrl,
  target,
  onClose,
  onPlace,
}: {
  item: DerivedProjectMediaItem;
  thumbnailUrl?: string;
  target: MediaPlacementTarget;
  onClose: () => void;
  onPlace: () => void;
}) {
  const validation = validateMediaPlacement(item, target);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    globalThis.addEventListener("keydown", handleKeyDown);
    return () => globalThis.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  return (
    <div
      aria-label={`预览 ${item.originalFilename}`}
      aria-modal="true"
      className="fixed inset-0 z-50 grid place-items-center bg-black/65 p-8"
      onClick={onClose}
      role="dialog"
    >
      <div
        className="flex max-h-full w-full max-w-4xl flex-col overflow-hidden rounded-xl border bg-background shadow-2xl"
        onClick={(event) => event.stopPropagation()}
      >
        <header className="flex h-12 shrink-0 items-center gap-3 border-b px-4">
          <div className="min-w-0">
            <h2 className="truncate text-sm font-medium">
              {item.originalFilename}
            </h2>
            <p className="text-[10px] text-muted-foreground">
              {item.pixelWidth} × {item.pixelHeight} ·{" "}
              {formatMediaType(item.mediaType)}
            </p>
          </div>
          <button
            aria-label="关闭素材预览"
            className="ml-auto grid size-8 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
            onClick={onClose}
            type="button"
          >
            <X className="size-4" />
          </button>
        </header>
        <div className="grid min-h-0 flex-1 place-items-center bg-muted/60 p-5">
          {thumbnailUrl ? (
            <img
              alt={item.originalFilename}
              className="max-h-[65vh] max-w-full object-contain"
              draggable={false}
              src={thumbnailUrl}
            />
          ) : (
            <div className="grid min-h-64 place-items-center text-muted-foreground">
              <ImageIcon className="size-10" />
            </div>
          )}
        </div>
        <footer className="flex shrink-0 items-center gap-3 border-t px-4 py-3">
          <p className="min-w-0 flex-1 truncate text-[10px] text-muted-foreground">
            {item.hasAlpha ? "含透明通道" : "无透明通道"} · 使用{" "}
            {item.usageCount} 次 · {item.treatmentCount} 个处理版本
          </p>
          <button
            className="h-8 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground disabled:cursor-not-allowed disabled:opacity-50"
            disabled={!validation.allowed}
            onClick={onPlace}
            title={validation.allowed ? undefined : validation.reason}
            type="button"
          >
            放置到{faceLabel(target.face)}
            {productionLayerLabel(target.productionLayer)}
          </button>
        </footer>
      </div>
    </div>
  );
}

function SelectedMediaDetails({
  item,
  folderPaths,
  onMoveAsset,
}: {
  item: DerivedProjectMediaItem;
  folderPaths: string[];
  onMoveAsset?: (
    assetId: string,
    folderPath: string | null,
  ) => void | Promise<void>;
}) {
  const [folderPath, setFolderPath] = useState(item.folderPath ?? "");
  const [folderError, setFolderError] = useState<string | null>(null);

  useEffect(() => {
    setFolderPath(item.folderPath ?? "");
    setFolderError(null);
  }, [item.folderPath, item.id]);

  return (
    <aside
      aria-label="素材详情"
      className="min-w-0 shrink-0 overflow-hidden border-t bg-background/45 px-3 py-2"
    >
      <p className="truncate text-[10px] font-medium">
        {item.originalFilename}
      </p>
      <div className="mt-1 flex flex-wrap gap-x-2 gap-y-0.5 text-[8px] leading-4 text-muted-foreground">
        <span>
          {item.pixelWidth} × {item.pixelHeight}
        </span>
        <span>{formatMediaType(item.mediaType)}</span>
        <span>{item.hasAlpha ? "含透明通道" : "无透明通道"}</span>
        <span>{item.usageCount > 0 ? `使用 ${item.usageCount} 次` : "未使用"}</span>
        <span>{item.treatmentCount} 个处理版本</span>
        {item.algorithmVersions.map((version) => (
          <span key={version}>{version}</span>
        ))}
      </div>
      {onMoveAsset && (
        <form
          className="mt-2 grid grid-cols-[minmax(0,1fr)_auto] gap-1"
          onSubmit={(event) => {
            event.preventDefault();
            const result = validateProjectMediaFolderPath(folderPath);
            if (!result.allowed) {
              setFolderError(result.reason);
              return;
            }
            setFolderError(null);
            void onMoveAsset(item.id, result.folderPath);
          }}
        >
          <input
            aria-label="素材文件夹路径"
            className="h-7 min-w-0 rounded-md border bg-background px-2 text-[9px] outline-none focus:border-primary"
            list={`project-media-folders-${item.id}`}
            onChange={(event) => setFolderPath(event.currentTarget.value)}
            placeholder="例如 品牌/Logo"
            value={folderPath}
          />
          <datalist id={`project-media-folders-${item.id}`}>
            {folderPaths.map((path) => (
              <option key={path} value={path} />
            ))}
          </datalist>
          <button
            className="h-7 rounded-md border px-2 text-[9px] hover:bg-accent"
            type="submit"
          >
            移动素材
          </button>
          <p
            className={`col-span-2 text-[8px] ${
              folderError ? "text-destructive" : "text-muted-foreground"
            }`}
            role={folderError ? "alert" : undefined}
          >
            {folderError ?? "输入已有或新文件夹路径；留空可移至未分类"}
          </p>
        </form>
      )}
    </aside>
  );
}

function beginAssetDrag(
  event: DragEvent<HTMLDivElement>,
  asset: AssetReference,
  target: MediaPlacementTarget,
) {
  const validation = validateMediaPlacement(asset, target);
  if (!validation.allowed) {
    event.preventDefault();
    return;
  }
  event.dataTransfer.effectAllowed = "copy";
  event.dataTransfer.setData(
    PROJECT_ASSET_DRAG_TYPE,
    JSON.stringify({ assetId: asset.id }),
  );
  event.dataTransfer.setData(
    "text/plain",
    serializeProjectAssetTextPayload(asset.id),
  );
}

function useAssetThumbnailUrls(
  assets: AssetReference[],
  disabled: boolean,
): Readonly<Record<string, string>> {
  const [urls, setUrls] = useState<Readonly<Record<string, string>>>({});

  useEffect(() => {
    if (disabled) return;
    let cancelled = false;
    const createdUrls: string[] = [];
    void Promise.all(
      assets.map(async (asset) => {
        const payload = await getAssetBytes(asset.id);
        const url = URL.createObjectURL(
          new Blob([new Uint8Array(payload.bytes)], {
            type: payload.mediaType,
          }),
        );
        createdUrls.push(url);
        return [asset.id, url] as const;
      }),
    )
      .then((entries) => {
        if (!cancelled) setUrls(Object.fromEntries(entries));
      })
      .catch(() => {
        if (!cancelled) setUrls({});
      });
    return () => {
      cancelled = true;
      for (const url of createdUrls) URL.revokeObjectURL(url);
    };
  }, [assets, disabled]);

  return urls;
}

function groupMediaItems(
  items: DerivedProjectMediaItem[],
): Array<[string, DerivedProjectMediaItem[]]> {
  const folders = new Map<string, DerivedProjectMediaItem[]>();
  for (const item of items) {
    const folder = item.folderPath?.trim() || "未分类";
    folders.set(folder, [...(folders.get(folder) ?? []), item]);
  }
  return [...folders.entries()].sort(([left], [right]) =>
    left.localeCompare(right, "zh-CN"),
  );
}

function faceLabel(face: "front" | "back") {
  return face === "front" ? "正面" : "背面";
}

function productionLayerLabel(layer: ProductionLayer) {
  switch (layer) {
    case "copper":
      return "铜层";
    case "solderMaskOpen":
      return "阻焊开窗";
    case "silkscreen":
      return "丝印层";
  }
}

function formatMediaType(mediaType: string) {
  const subtype = mediaType.split("/")[1]?.split("+")[0] ?? mediaType;
  return subtype === "jpeg" ? "JPEG" : subtype.toUpperCase();
}

function cloneDefaultDockState(): ProjectMediaDockState {
  return {
    ...DEFAULT_PROJECT_MEDIA_DOCK_STATE,
    expandedProductionNodes: [],
    expandedMediaFolders: ["*"],
  };
}

function clampNumber(
  value: unknown,
  minimum: number,
  maximum: number,
  fallback: number,
) {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(maximum, Math.max(minimum, value))
    : fallback;
}

function nonNegativeNumber(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0
    ? value
    : 0;
}

function stringArray(value: unknown) {
  if (!Array.isArray(value)) return [];
  return [
    ...new Set(value.filter((item): item is string => typeof item === "string")),
  ];
}
