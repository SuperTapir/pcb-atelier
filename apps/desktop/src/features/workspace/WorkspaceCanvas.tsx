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

import { KonvaProductionRenderer } from "@/features/preview/KonvaProductionRenderer";
import type {
  ProductionLayerSelection,
  ProductionPreviewInput,
} from "@/features/preview/production-renderer";
import {
  createTextDraft,
  type TextDraft,
} from "@/features/workspace/text-gesture";
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
  getAssetBytes,
  type ContentLayer,
  type WorkspaceDocument,
} from "@/lib/core";

const PIXELS_PER_MM = 5.5;
const GRID_STEP_MM = 5;
const SNAP_THRESHOLD_PX = 6;

interface WorkspaceCanvasProps {
  document: WorkspaceDocument;
  productionPreview?: ProductionPreviewInput | null;
  productionSelection?: ProductionLayerSelection;
  editingLayer: ContentLayer | null;
  face: CardFace;
  layers: ContentLayer[];
  selectedIds: string[];
  tool: WorkspaceTool;
  workContext: WorkContext;
  viewport: CanvasViewport;
  active: boolean;
  activeGroupId: string | null;
  onActivate: () => void;
  onBeginTextEdit: (layerId: string) => void;
  onEnterGroup: (layerId: string) => void;
  onCommitText: (layerId: string, text: string) => void;
  onCreateText: (draft: TextDraft) => void;
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

export function WorkspaceCanvas({
  document,
  productionPreview,
  productionSelection,
  editingLayer,
  face,
  layers,
  selectedIds,
  tool,
  workContext,
  viewport,
  active,
  activeGroupId,
  onActivate,
  onBeginTextEdit,
  onEnterGroup,
  onCommitText,
  onCreateText,
  onSelect,
  onTransformLayer,
  onViewportChange,
}: WorkspaceCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const panStartRef = useRef<{
    pointer: Point;
    viewport: CanvasViewport;
  } | null>(null);
  const textStartRef = useRef<Point | null>(null);
  const [size, setSize] = useState<CanvasSize>({ width: 0, height: 0 });
  const [pointerMm, setPointerMm] = useState<Point | null>(null);
  const [isPanning, setIsPanning] = useState(false);
  const [snapGuides, setSnapGuides] = useState<SnapGuide[]>([]);
  const assetImages = useAssetImages(document);

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
    setIsPanning(false);
  };

  const updatePointerReadout = (event: KonvaEventObject<MouseEvent>) => {
    const pointer = event.target.getStage()?.getPointerPosition();
    if (pointer) {
      setPointerMm(screenToBoard(pointer, transform, face, boardWidthMm));
    }
  };

  const handleMouseDown = (event: KonvaEventObject<MouseEvent>) => {
    const pointer = event.target.getStage()?.getPointerPosition();
    if (!pointer) return;
    if (!event.target.hasName("pan-surface")) return;
    if (tool !== "select") return;
    panStartRef.current = { pointer, viewport };
    setIsPanning(true);
  };

  const handleMouseMove = (event: KonvaEventObject<MouseEvent>) => {
    updatePointerReadout(event);
    const start = panStartRef.current;
    const pointer = event.target.getStage()?.getPointerPosition();
    if (!start || !pointer) return;
    onViewportChange({
      ...start.viewport,
      panX: start.viewport.panX + pointer.x - start.pointer.x,
      panY: start.viewport.panY + pointer.y - start.pointer.y,
    });
  };

  const handleWheel = (event: KonvaEventObject<WheelEvent>) => {
    event.evt.preventDefault();
    const pointer = event.target.getStage()?.getPointerPosition();
    if (!pointer) return;
    const boardPoint = screenToBoard(pointer, transform, face, boardWidthMm);
    const direction = event.evt.deltaY > 0 ? 1 / 1.12 : 1.12;
    const zoom = clamp(viewport.zoom * direction, MIN_ZOOM, MAX_ZOOM);
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
  const palette = getViewPalette(workContext);
  const isPointerInsideBoard =
    pointerMm !== null && isInsideBoard(pointerMm, boardWidthMm, boardHeightMm);
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
      className={`relative size-full min-h-0 overflow-hidden rounded-xl border bg-[radial-gradient(circle_at_center,var(--workspace-glow),transparent_55%)] ${cursor} ${
        active ? "border-primary/70 ring-2 ring-primary/15" : "border-border/70"
      }`}
      data-active={active}
      data-edit-orientation="upright"
      data-face={face}
      data-testid={`workspace-canvas-${face}`}
      onMouseDownCapture={(event) => {
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
              onViewportChange({ zoom: 1, panX: 0, panY: 0 });
            }
          }}
          onMouseDown={handleMouseDown}
          onMouseLeave={() => {
            finishPointerGesture();
            setPointerMm(null);
          }}
          onMouseMove={handleMouseMove}
          onMouseUp={finishPointerGesture}
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
              {productionPreview && productionSelection && (
                <KonvaProductionRenderer
                  preview={productionPreview}
                  selection={{
                    ...productionSelection,
                    mirroredForViewing: false,
                  }}
                />
              )}
              {gridLines.map((line) => (
                <Line
                  key={line.key}
                  points={line.points}
                  stroke="rgba(92, 82, 65, 0.13)"
                  strokeWidth={0.5 / transform.scale}
                />
              ))}
              <Text
                align="center"
                fill={palette.label}
                fontFamily="Inter, sans-serif"
                fontSize={3.2}
                fontStyle="600"
                text={face === "front" ? "FRONT" : "BACK"}
                width={boardWidthMm}
                y={boardHeightMm / 2 - 1.6}
              />
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
                .filter((layer) => isEffectivelyVisible(layer, layers))
                .sort(
                  (left, right) =>
                    Number(left.kind.type === "group") -
                    Number(right.kind.type === "group"),
                )
                .map((layer) => (
                  <ContentNode
                    image={
                      layer.kind.type === "image"
                        ? assetImages.get(layer.kind.assetId)
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
                    selected={selectedIds.includes(layer.id)}
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
        <CanvasBadge>
          {face === "front" ? "正面" : "背面"} · {boardWidthMm.toFixed(2)} ×{" "}
          {boardHeightMm.toFixed(2)} mm
        </CanvasBadge>
        <CanvasBadge>{workContextLabel(workContext)}</CanvasBadge>
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
      <div className="pointer-events-none absolute bottom-3 left-1/2 -translate-x-1/2 rounded-md border bg-card/92 px-3 py-1.5 text-[11px] text-muted-foreground shadow-sm backdrop-blur">
        {tool === "text"
          ? "点击创建点文字 · 拖动创建定宽文字"
          : "拖动画布平移 · 滚轮缩放 · 双击适配"}
      </div>
      <div className="pointer-events-none absolute bottom-3 right-3 min-w-32 rounded-md border bg-card/92 px-3 py-1.5 text-right font-mono text-[11px] text-muted-foreground shadow-sm backdrop-blur">
        {isPointerInsideBoard && pointerMm
          ? `X ${pointerMm.x.toFixed(2)}  Y ${pointerMm.y.toFixed(2)} mm`
          : "X —  Y — mm"}
      </div>
    </div>
  );
}

function ContentNode({
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
  selected,
  snapThresholdUm,
  strokeWidth,
  locked,
  listening,
}: {
  image?: HTMLImageElement;
  layer: ContentLayer;
  onBeginTextEdit: (layerId: string) => void;
  onEnterGroup: (layerId: string) => void;
  onSelect: WorkspaceCanvasProps["onSelect"];
  onSnapGuidesChange: (guides: SnapGuide[]) => void;
  onTransformLayer: WorkspaceCanvasProps["onTransformLayer"];
  selected: boolean;
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
        }}
        onDragEnd={commitNodeTransform}
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

  return (
    <textarea
      autoFocus
      className="absolute z-20 resize-none overflow-hidden rounded-sm border border-primary bg-card/95 p-1 text-foreground shadow-lg outline-none ring-2 ring-ring/30"
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
        fontSize: Math.max(
          12,
          (layer.kind.fontSizeUm / 1_000) * transform.scale,
        ),
        lineHeight: 1.2,
        transform: `rotate(${layer.transform.rotationMdeg / 1_000}deg)`,
        transformOrigin: "top left",
      }}
      value={value}
    />
  );
}

function useAssetImages(document: WorkspaceDocument) {
  const [images, setImages] = useState<Map<string, HTMLImageElement>>(
    new Map(),
  );
  useEffect(() => {
    let cancelled = false;
    const urls: string[] = [];
    void Promise.all(
      document.assets.map(async (asset) => {
        const payload = await getAssetBytes(asset.id);
        const url = URL.createObjectURL(
          new Blob([new Uint8Array(payload.bytes)], {
            type: payload.mediaType,
          }),
        );
        urls.push(url);
        const image = new window.Image();
        image.src = url;
        await image.decode();
        return [asset.id, image] as const;
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
  }, [document.assets]);
  return images;
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

export function displayedXToBoardX(
  displayedX: number,
  _face: CardFace,
  _boardWidthMm: number,
) {
  return displayedX;
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

function getViewPalette(workContext: WorkContext) {
  switch (workContext) {
    case "copper":
      return {
        boardFill: "#174b3b",
        boardStroke: "#0d2f26",
        gradient: undefined,
        label: "rgba(236, 245, 230, 0.5)",
      };
    case "solderMaskOpen":
      return {
        boardFill: "#1b5a45",
        boardStroke: "#0b3326",
        gradient: [0, "#286b55", 0.48, "#174a39", 1, "#0f3529"],
        label: "rgba(247, 242, 213, 0.46)",
      };
    case "silkscreen":
      return {
        boardFill: "#15492f",
        boardStroke: "#6f957f",
        gradient: [0, "#194f35", 1, "#0f3a27"],
        label: "rgba(247, 246, 236, 0.75)",
      };
  }
}

function workContextLabel(workContext: WorkContext) {
  switch (workContext) {
    case "copper":
      return "铜层工作";
    case "solderMaskOpen":
      return "阻焊开窗";
    case "silkscreen":
      return "丝印层工作";
  }
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}
