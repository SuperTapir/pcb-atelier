import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import {
  Group,
  Image as KonvaImage,
  Layer,
  Line,
  Rect,
  Stage,
  Text,
  Transformer,
} from "react-konva";
import type { KonvaEventObject } from "konva/lib/Node";
import type Konva from "konva";

import {
  createTextDraft,
  type TextDraft,
} from "@/features/workspace/text-gesture";
import {
  hasProjectAssetDragPayload,
  readProjectAssetDragPayload,
} from "@/features/media/ProjectMediaLibrary";
import { isSupportedImageFileMetadata } from "@/features/media/supported-image-file";
import type { WheelZoomDamping } from "@/features/settings/app-settings";
import { getManufacturerPalette } from "@/features/manufacturer/manufacturer-profile";
import {
  applyGroupTransform,
  isLayerTransformEditable,
  snapTransform,
  type SnapGuide,
} from "@/features/workspace/geometry-edit";
import { getTextRenderFrame } from "@/features/workspace/text-render-frame";
import {
  MAX_ZOOM,
  MIN_ZOOM,
  type CanvasViewport,
  type CardFace,
  type WorkContext,
  type WorkspaceTool,
} from "@/features/workspace/workspace-state";
import {
  compileImageTreatment,
  getAssetBytes,
  type ContentLayer,
  type ImageTreatment,
  type TreatmentCompileReport,
  type WorkspaceDocument,
} from "@/lib/core";

const PIXELS_PER_MM = 5.5;
const GRID_STEP_MM = 5;
const SNAP_THRESHOLD_PX = 6;
const SECONDARY_GESTURE_THRESHOLD_PX = 5;
const FIT_PADDING_PX = 48;
const WHEEL_ZOOM_SENSITIVITY: Record<WheelZoomDamping, number> = {
  high: 0.00045,
  medium: 0.0007,
  low: 0.001,
};
const interactiveProxyReportCache = new Map<
  string,
  Promise<TreatmentCompileReport>
>();
const MAX_INTERACTIVE_PROXY_CACHE_ENTRIES = 128;

interface WorkspaceCanvasProps {
  aspectRatioLocked: boolean;
  document: WorkspaceDocument;
  editingLayer: ContentLayer | null;
  face: CardFace;
  layers: ContentLayer[];
  productionVisibility: Record<WorkContext, boolean>;
  selectedIds: string[];
  showOriginalLayerIds: ReadonlySet<string>;
  tool: WorkspaceTool;
  workContext: WorkContext;
  viewport: CanvasViewport;
  wheelZoomDamping: WheelZoomDamping;
  active: boolean;
  activeGroupId: string | null;
  onActivate: () => void;
  onBeginTextEdit: (layerId: string) => void;
  onEnterGroup: (layerId: string) => void;
  onCommitText: (layerId: string, text: string) => void;
  onCreateText: (draft: TextDraft) => void;
  onClearSelection: () => void;
  onDropFiles?: (files: File[], point: BoardDropPoint) => void;
  onDropProjectAsset?: (assetId: string, point: BoardDropPoint) => void;
  onInvalidDrop?: (reason: string) => void;
  onSelectMany?: (layerIds: string[]) => void;
  onOpenContextMenu?: (request: {
    layerId: string | null;
    clientX: number;
    clientY: number;
  }) => void;
  onSelect: (
    layerId: string,
    modifiers: {
      shiftKey: boolean;
      altKey: boolean;
      candidates: string[];
    },
  ) => void;
  onTransformLayer: (
    layerId: string,
    transform: ContentLayer["transform"],
  ) => void;
  onTransformLayers: (
    transforms: Array<{
      layerId: string;
      transform: ContentLayer["transform"];
    }>,
  ) => void;
  onViewportChange: (viewport: CanvasViewport) => void;
}

interface CanvasSize {
  width: number;
  height: number;
}

interface Point {
  x: number;
  y: number;
}

export interface MarqueeBoundsUm {
  minXUm: number;
  minYUm: number;
  maxXUm: number;
  maxYUm: number;
}

export function classifySecondaryPointerGesture(
  start: Point,
  end: Point,
): "context-menu" | "pan" {
  return Math.hypot(end.x - start.x, end.y - start.y) >
    SECONDARY_GESTURE_THRESHOLD_PX
    ? "pan"
    : "context-menu";
}

export function getMarqueeLayerIds(
  layers: ContentLayer[],
  bounds: MarqueeBoundsUm,
): string[] {
  return layers
    .filter((layer) => {
      if (layer.locked || !layer.visible || layer.kind.type === "boardFill") {
        return false;
      }
      const transform = layer.transform;
      return (
        transform.xUm < bounds.maxXUm &&
        transform.xUm + transform.widthUm > bounds.minXUm &&
        transform.yUm < bounds.maxYUm &&
        transform.yUm + transform.heightUm > bounds.minYUm
      );
    })
    .map((layer) => layer.id);
}

export function getTranslatedSelectionTransforms(
  layers: ContentLayer[],
  selectedIds: string[],
  deltaXUm: number,
  deltaYUm: number,
): Array<{ layerId: string; transform: ContentLayer["transform"] }> {
  const selected = new Set(selectedIds);
  const roots = layers.filter((layer) => {
    if (!selected.has(layer.id) || !isLayerTransformEditable(layer, layers)) {
      return false;
    }
    let parentId = layer.parentId;
    while (parentId) {
      if (selected.has(parentId)) return false;
      parentId = layers.find((candidate) => candidate.id === parentId)?.parentId ?? null;
    }
    return true;
  });
  let translated = layers;
  for (const layer of roots) {
    const current = translated.find((candidate) => candidate.id === layer.id);
    if (!current) continue;
    const next = {
      ...current.transform,
      xUm: current.transform.xUm + deltaXUm,
      yUm: current.transform.yUm + deltaYUm,
    };
    translated =
      current.kind.type === "group"
        ? applyGroupTransform(translated, current.id, next)
        : translated.map((candidate) =>
            candidate.id === current.id
              ? { ...candidate, transform: next }
              : candidate,
          );
  }
  return translated.flatMap((layer, index) =>
    layer.transform === layers[index]?.transform
      ? []
      : [{ layerId: layer.id, transform: layer.transform }],
  );
}

export function getWheelZoom(
  currentZoom: number,
  deltaY: number,
  damping: WheelZoomDamping = "medium",
): number {
  const boundedDelta = clamp(deltaY, -100, 100);
  return clamp(
    currentZoom * Math.exp(-boundedDelta * WHEEL_ZOOM_SENSITIVITY[damping]),
    MIN_ZOOM,
    MAX_ZOOM,
  );
}

export function getFitViewport({
  boardHeightMm,
  boardWidthMm,
  canvasHeightPx,
  canvasWidthPx,
}: {
  boardHeightMm: number;
  boardWidthMm: number;
  canvasHeightPx: number;
  canvasWidthPx: number;
}): CanvasViewport {
  const availableWidth = Math.max(1, canvasWidthPx - FIT_PADDING_PX * 2);
  const availableHeight = Math.max(1, canvasHeightPx - FIT_PADDING_PX * 2);
  const zoom = clamp(
    Math.min(
      availableWidth / (boardWidthMm * PIXELS_PER_MM),
      availableHeight / (boardHeightMm * PIXELS_PER_MM),
    ),
    MIN_ZOOM,
    MAX_ZOOM,
  );
  return {
    zoom: Math.round(zoom * 100) / 100,
    panX: 0,
    panY: 0,
  };
}

export function resolveTransformerKeepRatio(
  aspectRatioLocked: boolean,
): boolean {
  return aspectRatioLocked;
}

export function WorkspaceCanvas({
  aspectRatioLocked,
  document,
  editingLayer,
  face,
  layers,
  productionVisibility,
  selectedIds,
  showOriginalLayerIds,
  tool,
  workContext,
  viewport,
  wheelZoomDamping,
  active,
  activeGroupId,
  onActivate,
  onBeginTextEdit,
  onEnterGroup,
  onCommitText,
  onCreateText,
  onClearSelection,
  onDropFiles,
  onDropProjectAsset,
  onInvalidDrop,
  onSelectMany,
  onOpenContextMenu,
  onSelect,
  onTransformLayer,
  onTransformLayers,
  onViewportChange,
}: WorkspaceCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const panStartRef = useRef<{
    pointer: Point;
    lastPointer: Point;
    viewport: CanvasViewport;
    layerId: string | null;
  } | null>(null);
  const marqueeStartRef = useRef<Point | null>(null);
  const activeAtPointerDownRef = useRef(active);
  const textStartRef = useRef<Point | null>(null);
  const [size, setSize] = useState<CanvasSize>({ width: 0, height: 0 });
  const [isPanning, setIsPanning] = useState(false);
  const [marquee, setMarquee] = useState<{
    start: Point;
    end: Point;
  } | null>(null);
  const [snapGuides, setSnapGuides] = useState<SnapGuide[]>([]);
  const proxyPixelPitchUm = getAdaptiveProxyPixelPitchUm(viewport.zoom);
  const assetImages = useProductionProxyImages(
    document,
    layers,
    face,
    workContext,
    showOriginalLayerIds,
    proxyPixelPitchUm,
  );

  const boardWidthMm = document.board.widthUm / 1_000;
  const boardHeightMm = document.board.heightUm / 1_000;
  const transform = useMemo(
    () => getBoardTransform(size, boardWidthMm, boardHeightMm, viewport),
    [boardHeightMm, boardWidthMm, size, viewport],
  );

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      setSize({
        width: Math.max(1, Math.floor(entry.contentRect.width)),
        height: Math.max(1, Math.floor(entry.contentRect.height)),
      });
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  const finishPointerGesture = () => {
    panStartRef.current = null;
    marqueeStartRef.current = null;
    setIsPanning(false);
    setMarquee(null);
  };

  const handleMouseDown = (event: KonvaEventObject<MouseEvent>) => {
    const pointer = event.target.getStage()?.getPointerPosition();
    if (!pointer) return;
    const isPanSurface = event.target.hasName("pan-surface");
    if (event.evt.button === 2) {
      event.evt.preventDefault();
      panStartRef.current = {
        pointer,
        lastPointer: pointer,
        viewport,
        layerId: findContentLayerId(event.target),
      };
      setIsPanning(true);
      return;
    }
    if (event.evt.button !== 0 || !isPanSurface || tool !== "select") return;
    marqueeStartRef.current = pointer;
    setMarquee({ start: pointer, end: pointer });
  };

  const updateSecondaryPan = (pointer: Point) => {
    const start = panStartRef.current;
    if (!start) return false;
    start.lastPointer = pointer;
    if (
      classifySecondaryPointerGesture(start.pointer, pointer) === "pan"
    ) {
      onViewportChange({
        ...start.viewport,
        panX: start.viewport.panX + pointer.x - start.pointer.x,
        panY: start.viewport.panY + pointer.y - start.pointer.y,
      });
    }
    return true;
  };

  const handleMouseMove = (event: KonvaEventObject<MouseEvent>) => {
    const pointer = event.target.getStage()?.getPointerPosition();
    if (!pointer) return;
    if (updateSecondaryPan(pointer)) return;
    if (marqueeStartRef.current) {
      setMarquee({ start: marqueeStartRef.current, end: pointer });
    }
  };

  const handleMouseUp = (event: KonvaEventObject<MouseEvent>) => {
    const pointer =
      event.target.getStage()?.getPointerPosition() ??
      panStartRef.current?.lastPointer;
    const panStart = panStartRef.current;
    if (panStart && pointer) {
      if (
        classifySecondaryPointerGesture(panStart.pointer, pointer) ===
        "context-menu"
      ) {
        if (
          panStart.layerId &&
          !selectedIds.includes(panStart.layerId)
        ) {
          onSelect(panStart.layerId, {
            shiftKey: false,
            altKey: false,
            candidates: [panStart.layerId],
          });
        }
        onOpenContextMenu?.({
          layerId: panStart.layerId,
          clientX: event.evt.clientX,
          clientY: event.evt.clientY,
        });
      }
      finishPointerGesture();
      return;
    }

    const marqueeStart = marqueeStartRef.current;
    if (marqueeStart && pointer) {
      if (Math.hypot(pointer.x - marqueeStart.x, pointer.y - marqueeStart.y) > 3) {
        const first = screenToBoard(
          marqueeStart,
          transform,
          face,
          boardWidthMm,
        );
        const second = screenToBoard(pointer, transform, face, boardWidthMm);
        onSelectMany?.(
          getMarqueeLayerIds(layers, {
            minXUm: Math.round(Math.min(first.x, second.x) * 1_000),
            minYUm: Math.round(Math.min(first.y, second.y) * 1_000),
            maxXUm: Math.round(Math.max(first.x, second.x) * 1_000),
            maxYUm: Math.round(Math.max(first.y, second.y) * 1_000),
          }),
        );
      } else if (
        shouldClearCanvasSelection(
          tool,
          true,
          activeAtPointerDownRef.current,
        )
      ) {
        onClearSelection();
      }
    }
    finishPointerGesture();
  };

  const handleWheel = (event: KonvaEventObject<WheelEvent>) => {
    event.evt.preventDefault();
    const pointer = event.target.getStage()?.getPointerPosition();
    if (!pointer) return;
    const boardPoint = screenToBoard(pointer, transform, face, boardWidthMm);
    const zoom = getWheelZoom(
      viewport.zoom,
      event.evt.deltaY,
      wheelZoomDamping,
    );
    const scale = PIXELS_PER_MM * zoom;
    const nextOriginX = pointer.x - boardPoint.x * scale;
    const nextOriginY = pointer.y - boardPoint.y * scale;
    onViewportChange({
      zoom,
      panX: nextOriginX - size.width / 2 + (boardWidthMm * scale) / 2,
      panY: nextOriginY - size.height / 2 + (boardHeightMm * scale) / 2,
    });
  };

  const gridLines = useMemo(
    () => createGridLines(boardWidthMm, boardHeightMm),
    [boardHeightMm, boardWidthMm],
  );
  const manufacturerPalette = getManufacturerPalette(
    document.manufacturerProfile,
  );
  const palette = getViewPalette(workContext, manufacturerPalette);
  const cursor = isPanning
    ? "cursor-grabbing"
    : tool === "text"
      ? "cursor-text"
      : "cursor-grab";
  const boardPointFromPointerEvent = (
    event: ReactMouseEvent<HTMLDivElement>,
  ) => {
    const bounds = containerRef.current?.getBoundingClientRect();
    if (!bounds) return null;
    return screenToBoard(
      { x: event.clientX - bounds.left, y: event.clientY - bounds.top },
      transform,
      face,
      boardWidthMm,
    );
  };

  return (
    <div
      ref={containerRef}
      className={`relative size-full min-h-0 select-none overflow-hidden bg-[radial-gradient(circle_at_center,var(--workspace-glow),transparent_55%)] ${cursor}`}
      data-active={active}
      data-edit-orientation="upright"
      data-exposed-copper-color={manufacturerPalette.exposedCopper}
      data-face={face}
      data-solder-mask-color={manufacturerPalette.solderMask}
      data-testid={`workspace-canvas-${face}`}
      data-viewport-pan-x={viewport.panX}
      data-viewport-pan-y={viewport.panY}
      data-viewport-zoom={viewport.zoom}
      onDragOver={(event) => {
        if (
          hasProjectAssetDragPayload(event.dataTransfer) ||
          event.dataTransfer.types.includes("Files")
        ) {
          event.preventDefault();
          event.dataTransfer.dropEffect = "copy";
        }
      }}
      onDrop={(event) => {
        const bounds = containerRef.current?.getBoundingClientRect();
        const point = bounds
          ? getBoardDropPoint({
              boardHeightMm,
              boardWidthMm,
              bounds,
              clientX: event.clientX,
              clientY: event.clientY,
              face,
              viewport,
            })
          : null;
        const projectAsset = readProjectAssetDragPayload(event.dataTransfer);
        if (projectAsset) {
          event.preventDefault();
          event.stopPropagation();
          if (point) {
            onDropProjectAsset?.(projectAsset.assetId, point);
          } else {
            onInvalidDrop?.("图片必须拖到板框内");
          }
          return;
        }
        const droppedFiles = [...event.dataTransfer.files];
        const images = droppedFiles.filter(isSupportedImageFileMetadata);
        if (images.length > 0) {
          event.preventDefault();
          event.stopPropagation();
          if (point) {
            onDropFiles?.(images, point);
          } else {
            onInvalidDrop?.("图片必须拖到板框内");
          }
        } else if (droppedFiles.length > 0) {
          event.preventDefault();
          event.stopPropagation();
          onInvalidDrop?.("仅支持 PNG、JPEG 或 WebP 图片");
        }
      }}
      onContextMenu={(event) => event.preventDefault()}
      onMouseDownCapture={(event) => {
        activeAtPointerDownRef.current = active;
        onActivate();
        if (
          tool !== "text" ||
          (event.target as HTMLElement).closest("textarea")
        ) {
          return;
        }
        const point = boardPointFromPointerEvent(event);
        if (point && isInsideBoard(point, boardWidthMm, boardHeightMm)) {
          textStartRef.current = point;
        }
      }}
      onMouseMoveCapture={(event) => {
        if (!panStartRef.current || (event.buttons & 2) === 0) return;
        const bounds = containerRef.current?.getBoundingClientRect();
        if (!bounds) return;
        updateSecondaryPan({
          x: event.clientX - bounds.left,
          y: event.clientY - bounds.top,
        });
      }}
      onMouseUpCapture={(event) => {
        const start = textStartRef.current;
        textStartRef.current = null;
        if (!start) return;
        const end = boardPointFromPointerEvent(event);
        if (end && isInsideBoard(end, boardWidthMm, boardHeightMm)) {
          onCreateText(createTextDraft(start, end));
        }
      }}
    >
      {size.width > 0 && size.height > 0 && (
        <Stage
          height={size.height}
          onDblClick={(event) => {
            if (event.target.hasName("pan-surface")) {
              onViewportChange(
                getFitViewport({
                  boardHeightMm,
                  boardWidthMm,
                  canvasHeightPx: size.height,
                  canvasWidthPx: size.width,
                }),
              );
            }
          }}
          onMouseDown={handleMouseDown}
          onMouseLeave={() => {
            finishPointerGesture();
          }}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onWheel={handleWheel}
          width={size.width}
        >
          <Layer>
            <Rect
              fill="rgba(0,0,0,0.001)"
              height={size.height}
              name="pan-surface"
              width={size.width}
            />
          </Layer>
          <Layer listening={false}>
            <Group
              scaleX={transform.scale}
              scaleY={transform.scale}
              x={transform.originX}
              y={transform.originY}
            >
              <Rect
                cornerRadius={2}
                fill={palette.boardFill}
                fillLinearGradientColorStops={palette.gradient}
                fillLinearGradientEndPoint={{
                  x: boardWidthMm,
                  y: boardHeightMm,
                }}
                height={boardHeightMm}
                shadowBlur={18 / transform.scale}
                shadowColor="rgba(20, 18, 14, 0.28)"
                shadowOffsetY={6 / transform.scale}
                stroke={palette.boardStroke}
                strokeWidth={1 / transform.scale}
                width={boardWidthMm}
              />
              {gridLines.map((line) => (
                <Line
                  key={line.key}
                  points={line.points}
                  stroke="rgba(92, 82, 65, 0.13)"
                  strokeWidth={0.5 / transform.scale}
                />
              ))}
            </Group>
          </Layer>
          <Layer>
            <Rect
              cornerRadius={2 * transform.scale}
              fill="rgba(0,0,0,0.001)"
              height={boardHeightMm * transform.scale}
              name="pan-surface"
              width={boardWidthMm * transform.scale}
              x={transform.originX}
              y={transform.originY}
            />
            <Group
              scaleX={transform.scale}
              scaleY={transform.scale}
              x={transform.originX}
              y={transform.originY}
            >
              {layers
                .filter(
                  (layer) =>
                    isEffectivelyVisible(layer, layers) &&
                    isVisibleInProductionContext(
                      layer,
                      document.mappings,
                      face,
                      productionVisibility,
                    ),
                )
                .sort(
                  (left, right) =>
                    Number(left.kind.type === "group") -
                    Number(right.kind.type === "group"),
                )
                .map((layer) => (
                  <ContentNode
                    aspectRatioLocked={aspectRatioLocked}
                    image={
                      layer.kind.type === "image"
                        ? assetImages.get(layer.id)
                        : undefined
                    }
                    key={layer.id}
                    layer={layer}
                    layers={layers}
                    boardHeightUm={document.board.heightUm}
                    boardWidthUm={document.board.widthUm}
                    onBeginTextEdit={onBeginTextEdit}
                    onEnterGroup={onEnterGroup}
                    onSelect={onSelect}
                    onSnapGuidesChange={setSnapGuides}
                    onTransformLayer={onTransformLayer}
                    onTransformLayers={onTransformLayers}
                    selected={selectedIds.includes(layer.id)}
                    selectedIds={selectedIds}
                    strokeWidth={1 / transform.scale}
                    snapThresholdUm={Math.max(
                      1,
                      Math.round((SNAP_THRESHOLD_PX / transform.scale) * 1_000),
                    )}
                    locked={
                      layer.kind.type === "boardFill" ||
                      !isLayerTransformEditable(layer, layers)
                    }
                    listening={
                      layer.kind.type !== "group" ||
                      layer.id !== activeGroupId
                    }
                  />
                ))}
              {snapGuides.map((guide) =>
                guide.axis === "x" ? (
                  <Line
                    key={`x-${guide.kind}-${guide.positionUm}`}
                    points={[
                      guide.positionUm / 1_000,
                      0,
                      guide.positionUm / 1_000,
                      boardHeightMm,
                    ]}
                    stroke="#e56b2f"
                    strokeWidth={1 / transform.scale}
                  />
                ) : (
                  <Line
                    key={`y-${guide.kind}-${guide.positionUm}`}
                    points={[
                      0,
                      guide.positionUm / 1_000,
                      boardWidthMm,
                      guide.positionUm / 1_000,
                    ]}
                    stroke="#e56b2f"
                    strokeWidth={1 / transform.scale}
                  />
                ),
              )}
            </Group>
          </Layer>
          {marquee && (
            <Layer listening={false}>
              <Rect
                dash={[4, 3]}
                fill="rgba(200,120,62,0.08)"
                height={Math.abs(marquee.end.y - marquee.start.y)}
                stroke="#c8783e"
                strokeWidth={1}
                width={Math.abs(marquee.end.x - marquee.start.x)}
                x={Math.min(marquee.start.x, marquee.end.x)}
                y={Math.min(marquee.start.y, marquee.end.y)}
              />
            </Layer>
          )}
        </Stage>
      )}

      {editingLayer?.kind.type === "text" && (
        <TextEditorOverlay
          key={editingLayer.id}
          layer={editingLayer}
          onCommit={onCommitText}
          transform={transform}
        />
      )}

      <div className="pointer-events-none absolute left-3 top-3 flex items-center gap-2">
        <CanvasBadge>{face === "front" ? "正面" : "背面"}</CanvasBadge>
        <CanvasBadge>{Math.round(viewport.zoom * 100)}%</CanvasBadge>
      </div>
      {snapGuides.length > 0 && (
        <div
          className="pointer-events-none absolute left-1/2 top-12 flex -translate-x-1/2 flex-col items-center gap-1"
          data-testid={`snap-guides-${face}`}
        >
          {snapGuides.map((guide) => (
            <span
              className="rounded-md bg-primary px-2 py-1 text-[10px] font-medium text-primary-foreground shadow-sm"
              key={`${guide.axis}-${guide.kind}-${guide.positionUm}`}
            >
              {guide.description}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

export function shouldClearCanvasSelection(
  tool: WorkspaceTool,
  isPanSurface: boolean,
  active: boolean,
) {
  return active && tool === "select" && isPanSurface;
}

function ContentNode({
  aspectRatioLocked,
  image,
  layer,
  layers,
  boardHeightUm,
  boardWidthUm,
  onBeginTextEdit,
  onEnterGroup,
  onSelect,
  onSnapGuidesChange,
  onTransformLayer,
  onTransformLayers,
  selected,
  selectedIds,
  snapThresholdUm,
  strokeWidth,
  locked,
  listening,
}: {
  aspectRatioLocked: boolean;
  image?: HTMLImageElement;
  layer: ContentLayer;
  onBeginTextEdit: (layerId: string) => void;
  onEnterGroup: (layerId: string) => void;
  onSelect: WorkspaceCanvasProps["onSelect"];
  onSnapGuidesChange: (guides: SnapGuide[]) => void;
  onTransformLayer: WorkspaceCanvasProps["onTransformLayer"];
  onTransformLayers: WorkspaceCanvasProps["onTransformLayers"];
  selected: boolean;
  selectedIds: string[];
  snapThresholdUm: number;
  strokeWidth: number;
  locked: boolean;
  listening: boolean;
  layers: ContentLayer[];
  boardWidthUm: number;
  boardHeightUm: number;
}) {
  const nodeRef = useRef<Konva.Group>(null);
  const transformerRef = useRef<Konva.Transformer>(null);
  useEffect(() => {
    if (selected && nodeRef.current && transformerRef.current) {
      transformerRef.current.nodes([nodeRef.current]);
      transformerRef.current.getLayer()?.batchDraw();
    }
  }, [selected]);
  const transform = layer.transform;
  const x = transform.xUm / 1_000;
  const y = transform.yUm / 1_000;
  const width = transform.widthUm / 1_000;
  const height = transform.heightUm / 1_000;
  const commitNodeTransform = () => {
    const node = nodeRef.current;
    if (!node || locked) return;
    const scaleX = node.scaleX();
    const scaleY = node.scaleY();
    const nextWidth = width * Math.abs(scaleX);
    const nextHeight = height * Math.abs(scaleY);
    onTransformLayer(layer.id, {
      xUm: Math.round((node.x() - nextWidth / 2) * 1_000),
      yUm: Math.round((node.y() - nextHeight / 2) * 1_000),
      widthUm: Math.max(1, Math.round(nextWidth * 1_000)),
      heightUm: Math.max(1, Math.round(nextHeight * 1_000)),
      rotationMdeg: Math.round(node.rotation() * 1_000),
      flipX: scaleX < 0,
      flipY: scaleY < 0,
    });
    node.scaleX(1);
    node.scaleY(1);
    onSnapGuidesChange([]);
  };
  const snapDraggedNode = (altKey: boolean) => {
    const node = nodeRef.current;
    if (!node || locked) return;
    const result = snapTransform({
      layer,
      layers,
      proposedTransform: {
        ...layer.transform,
        xUm: Math.round((node.x() - width / 2) * 1_000),
        yUm: Math.round((node.y() - height / 2) * 1_000),
      },
      board: { widthUm: boardWidthUm, heightUm: boardHeightUm },
      gridStepUm: GRID_STEP_MM * 1_000,
      thresholdUm: snapThresholdUm,
      altKey,
    });
    node.position({
      x: result.transform.xUm / 1_000 + width / 2,
      y: result.transform.yUm / 1_000 + height / 2,
    });
    onSnapGuidesChange(result.guides);
  };
  const previewGroupMembers = () => {
    const node = nodeRef.current;
    const stage = node?.getStage();
    if (!node || !stage || layer.kind.type !== "group" || locked) return;
    const scaleX = node.scaleX();
    const scaleY = node.scaleY();
    const widthUm = Math.max(
      1,
      Math.round(layer.transform.widthUm * Math.abs(scaleX)),
    );
    const heightUm = Math.max(
      1,
      Math.round(layer.transform.heightUm * Math.abs(scaleY)),
    );
    const previewLayers = applyGroupTransform(layers, layer.id, {
      xUm: Math.round((node.x() - widthUm / 2_000) * 1_000),
      yUm: Math.round((node.y() - heightUm / 2_000) * 1_000),
      widthUm,
      heightUm,
      rotationMdeg: Math.round(node.rotation() * 1_000),
      flipX: scaleX < 0,
      flipY: scaleY < 0,
    });
    for (const previewLayer of previewLayers) {
      const original = layers.find(
        (candidate) => candidate.id === previewLayer.id,
      );
      if (
        !original ||
        previewLayer === original ||
        previewLayer.id === layer.id
      ) {
        continue;
      }
      const sibling = stage.find(
        (candidate: Konva.Node) => candidate.id() === previewLayer.id,
      )[0];
      if (!sibling) continue;
      sibling.position({
        x:
          previewLayer.transform.xUm / 1_000 +
          previewLayer.transform.widthUm / 2_000,
        y:
          previewLayer.transform.yUm / 1_000 +
          previewLayer.transform.heightUm / 2_000,
      });
      sibling.rotation(previewLayer.transform.rotationMdeg / 1_000);
      sibling.scale({
        x:
          (previewLayer.transform.flipX ? -1 : 1) *
          (previewLayer.transform.widthUm /
            Math.max(1, original.transform.widthUm)),
        y:
          (previewLayer.transform.flipY ? -1 : 1) *
          (previewLayer.transform.heightUm /
            Math.max(1, original.transform.heightUm)),
      });
    }
    node.getLayer()?.batchDraw();
  };
  const selectionTransforms = () => {
    const node = nodeRef.current;
    if (!node) return [];
    return getTranslatedSelectionTransforms(
      layers,
      selectedIds,
      Math.round((node.x() - (x + width / 2)) * 1_000),
      Math.round((node.y() - (y + height / 2)) * 1_000),
    );
  };
  const previewSelectedLayers = () => {
    if (selectedIds.length < 2) return;
    const node = nodeRef.current;
    const stage = node?.getStage();
    if (!node || !stage) return;
    for (const update of selectionTransforms()) {
      if (update.layerId === layer.id) continue;
      const original = layers.find((candidate) => candidate.id === update.layerId);
      const sibling = stage.find(
        (candidate: Konva.Node) => candidate.id() === update.layerId,
      )[0];
      if (!original || !sibling) continue;
      sibling.position({
        x: update.transform.xUm / 1_000 + update.transform.widthUm / 2_000,
        y: update.transform.yUm / 1_000 + update.transform.heightUm / 2_000,
      });
    }
    node.getLayer()?.batchDraw();
  };
  const commitDrag = () => {
    if (selectedIds.length < 2) {
      commitNodeTransform();
      return;
    }
    const updates = selectionTransforms();
    if (updates.length > 0) onTransformLayers(updates);
    onSnapGuidesChange([]);
  };
  return (
    <>
      <Group
        draggable={selected && !locked}
        id={layer.id}
        listening={!locked && listening}
        name="content-node"
        ref={nodeRef}
        onClick={(event) => {
          event.cancelBubble = true;
          const pointer = event.target.getStage()?.getPointerPosition();
          const candidates = pointer
            ? (event.target
                .getStage()
                ?.getAllIntersections(pointer)
                .map(
                  (node) =>
                    node.findAncestor(".content-node")?.id() ?? node.id(),
                )
                .filter((id, index, all) => id && all.indexOf(id) === index) ??
              [])
            : [];
          onSelect(layer.id, {
            shiftKey: event.evt.shiftKey,
            altKey: event.evt.altKey,
            candidates,
          });
        }}
        onDblClick={(event) => {
          event.cancelBubble = true;
          if (layer.kind.type === "text") {
            onBeginTextEdit(layer.id);
          } else if (layer.kind.type === "group") {
            onEnterGroup(layer.id);
          }
        }}
        offsetX={width / 2}
        offsetY={height / 2}
        rotation={transform.rotationMdeg / 1_000}
        scaleX={transform.flipX ? -1 : 1}
        scaleY={transform.flipY ? -1 : 1}
        x={x + width / 2}
        y={y + height / 2}
        onDragStart={() => onSnapGuidesChange([])}
        onDragMove={(event) => {
          snapDraggedNode(event.evt.altKey);
          previewGroupMembers();
          previewSelectedLayers();
        }}
        onDragEnd={commitDrag}
        onTransform={previewGroupMembers}
        onTransformEnd={commitNodeTransform}
      >
        {layer.kind.type === "group" && (
          <Rect
            fill="rgba(0,0,0,0.001)"
            height={height}
            width={width}
          />
        )}
        {layer.kind.type === "image" &&
          (image ? (
            <KonvaImage height={height} image={image} width={width} />
          ) : (
            <Rect
              fill="rgba(120, 112, 96, 0.15)"
              height={height}
              stroke="rgba(90, 82, 68, 0.25)"
              strokeWidth={strokeWidth}
              width={width}
            />
          ))}
        {layer.kind.type === "text" && (
          <Text
            fill="#2b2924"
            fontFamily={layer.kind.fontFamily}
            fontSize={layer.kind.fontSizeUm / 1_000}
            text={layer.kind.text}
            {...getTextRenderFrame(layer.kind.layout, width, height)}
          />
        )}
        {selected && (
          <Rect
            dash={[3 * strokeWidth, 2 * strokeWidth]}
            height={height}
            listening={!locked}
            stroke="#c8783e"
            strokeWidth={strokeWidth}
            width={width}
          />
        )}
      </Group>
      {selected && !locked && (
        <Transformer
          flipEnabled
          keepRatio={resolveTransformerKeepRatio(aspectRatioLocked)}
          ref={transformerRef}
          rotateEnabled
          boundBoxFunc={(oldBox, nextBox) =>
            Math.abs(nextBox.width) < 2 || Math.abs(nextBox.height) < 2
              ? oldBox
              : nextBox
          }
        />
      )}
    </>
  );
}

function isEffectivelyLocked(layer: ContentLayer, layers: ContentLayer[]) {
  if (layer.locked) return true;
  let parentId = layer.parentId;
  while (parentId) {
    const parent = layers.find((candidate) => candidate.id === parentId);
    if (!parent) return false;
    if (parent.locked) return true;
    parentId = parent.parentId;
  }
  return false;
}

function isEffectivelyVisible(layer: ContentLayer, layers: ContentLayer[]) {
  if (!layer.visible) return false;
  let parentId = layer.parentId;
  while (parentId) {
    const parent = layers.find((candidate) => candidate.id === parentId);
    if (!parent || !parent.visible) return false;
    parentId = parent.parentId;
  }
  return true;
}

function TextEditorOverlay({
  layer,
  onCommit,
  transform,
}: {
  layer: ContentLayer;
  onCommit: (layerId: string, text: string) => void;
  transform: ReturnType<typeof getBoardTransform>;
}) {
  if (layer.kind.type !== "text") return null;
  const [value, setValue] = useState(layer.kind.text);
  const finishedRef = useRef(false);
  const finish = () => {
    if (finishedRef.current) return;
    finishedRef.current = true;
    onCommit(layer.id, value);
  };
  const layerX = layer.transform.xUm / 1_000;
  const left = transform.originX + layerX * transform.scale;
  const top =
    transform.originY + (layer.transform.yUm / 1_000) * transform.scale;
  const width = Math.max(
    60,
    (layer.transform.widthUm / 1_000) * transform.scale,
  );
  const height = Math.max(
    28,
    (layer.transform.heightUm / 1_000) * transform.scale,
  );
  const fontSize = Math.max(
    12,
    (layer.kind.fontSizeUm / 1_000) * transform.scale,
  );
  const lineHeight = fontSize * 1.2;
  const explicitLineCount = Math.max(1, value.split("\n").length);
  const verticalPadding = Math.max(
    4,
    (height - lineHeight * explicitLineCount) / 2,
  );

  return (
    <textarea
      autoFocus
      className="absolute z-20 resize-none overflow-hidden rounded-sm border border-primary bg-card/95 text-center text-foreground shadow-lg outline-none ring-2 ring-ring/30"
      data-testid="text-editor"
      onBlur={finish}
      onChange={(event) => setValue(event.currentTarget.value)}
      onFocus={(event) => event.currentTarget.select()}
      onKeyDown={(event) => {
        if (
          event.key === "Escape" ||
          (event.key === "Enter" && (event.metaKey || event.ctrlKey))
        ) {
          event.preventDefault();
          event.currentTarget.blur();
        }
      }}
      style={{
        left,
        top,
        width,
        height,
        fontFamily: layer.kind.fontFamily,
        fontSize,
        lineHeight: 1.2,
        padding: `${verticalPadding}px 4px 4px`,
        transform: `rotate(${layer.transform.rotationMdeg / 1_000}deg)`,
        transformOrigin: "top left",
      }}
      value={value}
    />
  );
}

function useProductionProxyImages(
  document: WorkspaceDocument,
  layers: ContentLayer[],
  face: CardFace,
  workContext: WorkContext,
  showOriginalLayerIds: ReadonlySet<string>,
  pixelPitchUm: number,
) {
  const [images, setImages] = useState<Map<string, HTMLImageElement>>(
    new Map(),
  );
  useEffect(() => {
    let cancelled = false;
    const urls: string[] = [];
    const palette = getManufacturerPalette(document.manufacturerProfile);
    void Promise.all(
      layers
        .filter((layer) => layer.kind.type === "image")
        .map(async (layer) => {
        if (layer.kind.type !== "image") {
          throw new Error("production proxy requested for a non-image layer");
        }
        if (showOriginalLayerIds.has(layer.id)) {
          const payload = await getAssetBytes(layer.kind.assetId);
          const url = URL.createObjectURL(
            new Blob([new Uint8Array(payload.bytes)], {
              type: payload.mediaType,
            }),
          );
          urls.push(url);
          const image = new window.Image();
          image.src = url;
          await image.decode();
          return [layer.id, image] as const;
        }
        const faceMappings = document.mappings.filter(
          (candidate) =>
            candidate.sourceLayerId === layer.id &&
            candidate.target.side === face &&
            candidate.treatmentId,
        );
        const mapping =
          faceMappings.find(
            (candidate) => candidate.target.layer === workContext,
          ) ?? faceMappings[0];
        const treatment = document.imageTreatments.find(
          (candidate) => candidate.id === mapping?.treatmentId,
        );
        if (treatment) {
          if (treatment.productionMode === "colorOriginal") {
            const payload = await getAssetBytes(layer.kind.assetId);
            const url = URL.createObjectURL(
              new Blob([new Uint8Array(payload.bytes)], {
                type: payload.mediaType,
              }),
            );
            urls.push(url);
            const source = new window.Image();
            source.src = url;
            await source.decode();
            return [
              layer.id,
              await cropOriginalImage(source, treatment.recipe.crop),
            ] as const;
          }
          const tint =
            mapping?.target.layer === "copper"
              ? palette.exposedCopper
              : mapping?.target.layer === "solderMaskOpen"
                ? palette.substrate
                : palette.silkscreen;
          const report = await getCachedInteractiveProxy(
            treatment,
            layer.transform.widthUm,
            layer.transform.heightUm,
            pixelPitchUm,
          );
          return [
            layer.id,
            await loadTintedMask(report.previewPngDataUrl, tint),
          ] as const;
        }
        const payload = await getAssetBytes(layer.kind.assetId);
        const url = URL.createObjectURL(
          new Blob([new Uint8Array(payload.bytes)], {
            type: payload.mediaType,
          }),
        );
        urls.push(url);
        const image = new window.Image();
        image.src = url;
        await image.decode();
        return [layer.id, image] as const;
      }),
    )
      .then((entries) => {
        if (!cancelled) setImages(new Map(entries));
      })
      .catch(() => {
        if (!cancelled) setImages(new Map());
      });
    return () => {
      cancelled = true;
      urls.forEach((url) => URL.revokeObjectURL(url));
    };
  }, [
    document.assets,
    document.imageTreatments,
    document.manufacturerProfile,
    document.mappings,
    face,
    layers,
    pixelPitchUm,
    showOriginalLayerIds,
    workContext,
  ]);
  return images;
}

export function getInteractiveProxyCacheKey(
  treatment: ImageTreatment,
  widthUm: number,
  heightUm: number,
  pixelPitchUm = 250,
) {
  return JSON.stringify([
    treatment.id,
    treatment.assetId,
    treatment.productionMode,
    treatment.recipe,
    widthUm,
    heightUm,
    pixelPitchUm,
  ]);
}

export function getAdaptiveProxyPixelPitchUm(zoom: number): number {
  const desiredPitchUm = 1_000 / (PIXELS_PER_MM * Math.max(zoom, MIN_ZOOM));
  return [250, 100, 50, 25].find((pitch) => pitch <= desiredPitchUm) ?? 25;
}

function getCachedInteractiveProxy(
  treatment: ImageTreatment,
  widthUm: number,
  heightUm: number,
  pixelPitchUm: number,
) {
  const key = getInteractiveProxyCacheKey(
    treatment,
    widthUm,
    heightUm,
    pixelPitchUm,
  );
  const cached = interactiveProxyReportCache.get(key);
  if (cached) return cached;
  const request = compileImageTreatment(
    treatment.id,
    widthUm,
    heightUm,
    "interactiveProxy",
    pixelPitchUm,
  ).catch((error) => {
    interactiveProxyReportCache.delete(key);
    throw error;
  });
  interactiveProxyReportCache.set(key, request);
  if (interactiveProxyReportCache.size > MAX_INTERACTIVE_PROXY_CACHE_ENTRIES) {
    const oldestKey = interactiveProxyReportCache.keys().next().value;
    if (oldestKey) interactiveProxyReportCache.delete(oldestKey);
  }
  return request;
}

async function cropOriginalImage(
  source: HTMLImageElement,
  cropValue: unknown,
): Promise<HTMLImageElement> {
  const crop = readNormalizedCrop(cropValue);
  if (!crop) return source;
  const sourceX = Math.round(
    (source.naturalWidth * crop.xMillionths) / 1_000_000,
  );
  const sourceY = Math.round(
    (source.naturalHeight * crop.yMillionths) / 1_000_000,
  );
  const sourceWidth = Math.max(
    1,
    Math.round((source.naturalWidth * crop.widthMillionths) / 1_000_000),
  );
  const sourceHeight = Math.max(
    1,
    Math.round((source.naturalHeight * crop.heightMillionths) / 1_000_000),
  );
  const canvas = document.createElement("canvas");
  canvas.width = sourceWidth;
  canvas.height = sourceHeight;
  const context = canvas.getContext("2d");
  if (!context) return source;
  context.drawImage(
    source,
    sourceX,
    sourceY,
    sourceWidth,
    sourceHeight,
    0,
    0,
    sourceWidth,
    sourceHeight,
  );
  const image = new window.Image();
  image.src = canvas.toDataURL("image/png");
  await image.decode();
  return image;
}

function readNormalizedCrop(value: unknown): {
  xMillionths: number;
  yMillionths: number;
  widthMillionths: number;
  heightMillionths: number;
} | null {
  if (!value || typeof value !== "object") return null;
  const crop = value as Record<string, unknown>;
  if (
    typeof crop.xMillionths !== "number" ||
    typeof crop.yMillionths !== "number" ||
    typeof crop.widthMillionths !== "number" ||
    typeof crop.heightMillionths !== "number"
  ) {
    return null;
  }
  return {
    xMillionths: crop.xMillionths,
    yMillionths: crop.yMillionths,
    widthMillionths: crop.widthMillionths,
    heightMillionths: crop.heightMillionths,
  };
}

async function loadTintedMask(
  dataUrl: string,
  fill: string,
): Promise<HTMLImageElement> {
  const source = new window.Image();
  source.src = dataUrl;
  await source.decode();
  const canvas = document.createElement("canvas");
  canvas.width = source.naturalWidth;
  canvas.height = source.naturalHeight;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("无法创建图片生产代理画布");
  context.drawImage(source, 0, 0);
  context.globalCompositeOperation = "source-in";
  context.fillStyle = fill;
  context.fillRect(0, 0, canvas.width, canvas.height);
  const image = new window.Image();
  image.src = canvas.toDataURL("image/png");
  await image.decode();
  return image;
}

function CanvasBadge({ children }: { children: React.ReactNode }) {
  return (
    <span className="rounded-md border bg-card/92 px-2.5 py-1 font-mono text-[11px] text-muted-foreground shadow-sm backdrop-blur">
      {children}
    </span>
  );
}

function getBoardTransform(
  size: CanvasSize,
  widthMm: number,
  heightMm: number,
  viewport: CanvasViewport,
) {
  const scale = PIXELS_PER_MM * viewport.zoom;
  return {
    scale,
    originX: size.width / 2 - (widthMm * scale) / 2 + viewport.panX,
    originY: size.height / 2 - (heightMm * scale) / 2 + viewport.panY,
  };
}

export interface BoardDropPoint {
  xUm: number;
  yUm: number;
}

export function getBoardDropPoint({
  boardHeightMm,
  boardWidthMm,
  bounds,
  clientX,
  clientY,
  face,
  viewport,
}: {
  boardHeightMm: number;
  boardWidthMm: number;
  bounds: { height: number; left: number; top: number; width: number };
  clientX: number;
  clientY: number;
  face: CardFace;
  viewport: CanvasViewport;
}): BoardDropPoint | null {
  const transform = getBoardTransform(
    { height: bounds.height, width: bounds.width },
    boardWidthMm,
    boardHeightMm,
    viewport,
  );
  const point = screenToBoard(
    { x: clientX - bounds.left, y: clientY - bounds.top },
    transform,
    face,
    boardWidthMm,
  );
  if (!isInsideBoard(point, boardWidthMm, boardHeightMm)) return null;
  return {
    xUm: Math.round(point.x * 1_000),
    yUm: Math.round(point.y * 1_000),
  };
}

function screenToBoard(
  point: Point,
  transform: ReturnType<typeof getBoardTransform>,
  face: CardFace,
  boardWidthMm: number,
) {
  const displayedX = (point.x - transform.originX) / transform.scale;
  return {
    x: displayedXToBoardX(displayedX, face, boardWidthMm),
    y: (point.y - transform.originY) / transform.scale,
  };
}

function findContentLayerId(node: Konva.Node): string | null {
  if (node.hasName("content-node")) return node.id() || null;
  return node.findAncestor(".content-node")?.id() || null;
}

export function displayedXToBoardX(
  displayedX: number,
  _face: CardFace,
  _boardWidthMm: number,
) {
  return displayedX;
}

export function isVisibleInProductionContext(
  layer: ContentLayer,
  mappings: WorkspaceDocument["mappings"],
  face: CardFace,
  visibility: Record<WorkContext, boolean>,
) {
  const targets = mappings.filter(
    (mapping) =>
      mapping.sourceLayerId === layer.id && mapping.target.side === face,
  );
  return (
    targets.length === 0 ||
    targets.some((mapping) => visibility[mapping.target.layer])
  );
}

function isInsideBoard(point: Point, widthMm: number, heightMm: number) {
  return (
    point.x >= 0 && point.y >= 0 && point.x <= widthMm && point.y <= heightMm
  );
}

function createGridLines(widthMm: number, heightMm: number) {
  const lines: Array<{ key: string; points: number[] }> = [];
  for (let x = GRID_STEP_MM; x < widthMm; x += GRID_STEP_MM) {
    lines.push({ key: `x-${x}`, points: [x, 0, x, heightMm] });
  }
  for (let y = GRID_STEP_MM; y < heightMm; y += GRID_STEP_MM) {
    lines.push({ key: `y-${y}`, points: [0, y, widthMm, y] });
  }
  return lines;
}

function getViewPalette(
  workContext: WorkContext,
  manufacturer: ReturnType<typeof getManufacturerPalette>,
) {
  switch (workContext) {
    case "copper":
      return {
        boardFill: manufacturer.solderMask,
        boardStroke: "#0d2f26",
        gradient: undefined,
      };
    case "solderMaskOpen":
      return {
        boardFill: manufacturer.solderMask,
        boardStroke: "#0b3326",
        gradient: undefined,
      };
    case "silkscreen":
      return {
        boardFill: manufacturer.solderMask,
        boardStroke: "#6f957f",
        gradient: undefined,
      };
  }
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}
