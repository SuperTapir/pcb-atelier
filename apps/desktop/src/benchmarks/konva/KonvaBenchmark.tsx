import Konva from "konva";
import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";
import {
  Image as KonvaImage,
  Layer,
  Rect,
  Stage,
  Text,
  Transformer,
} from "react-konva";

import {
  createDualBoardObjects,
  evaluateInactiveBoardDraws,
  MAXIMUM_INACTIVE_DRAWS_PER_ACTIVE_CYCLE,
  MAXIMUM_INACTIVE_DRAW_RATIO,
  OBJECTS_PER_FACE,
  TOTAL_EDITABLE_OBJECTS,
  type BenchmarkFace,
  type BoardDrawSample,
  type EditableObject,
  type InactiveBoardDrawResult,
} from "./dual-board-scene";

const BOARD = { width: 410, height: 540 };
const MULTI_SELECTION_COUNT = 12;
const FACE_VISIBILITY_CYCLES = 24;
const FACE_ORDER: BenchmarkFace[] = ["front", "back"];
const TEXTURES_BY_FACE: Record<BenchmarkFace, number[]> = {
  front: [0, 1, 2],
  back: [3, 4, 5],
};
const THRESHOLDS = {
  minimumFps: 45,
  maximumP95FrameMs: 1000 / 30,
  slowFrameBoundaryMs: 1000 / 45,
  maximumSlowFrameRatio: 0.2,
  maximumInitialToFinalHeapGrowthBytes: 16 * 1024 * 1024,
  maximumCycleHeapGrowthBytes: 4 * 1024 * 1024,
  maximumCycleHeapGrowthRatio: 0.1,
  maximumInactiveDrawsPerActiveCycle:
    MAXIMUM_INACTIVE_DRAWS_PER_ACTIVE_CYCLE,
  maximumInactiveDrawRatio: MAXIMUM_INACTIVE_DRAW_RATIO,
} as const;

type Phase =
  | "idle"
  | "warmup"
  | "front-viewport"
  | "back-viewport"
  | "selection-transform"
  | "face-visibility"
  | "complete";

type HeapSample = { cycle: number; usedBytes: number };
type MemoryResult =
  | {
      available: true;
      initialBytes: number;
      finalBytes: number;
      peakBytes: number;
      growthBytes: number;
      growthRatio: number;
      cycleGrowthBytes: number;
      cycleGrowthRatio: number;
      samples: HeapSample[];
      passed: boolean;
    }
  | { available: false; reason: string; passed: null };

export type KonvaBenchmarkResult = {
  version: 2;
  userAgent: string;
  viewport: { width: number; height: number; devicePixelRatio: number };
  scene: {
    boardCount: 2;
    objectsPerFace: number;
    editableObjects: number;
    proxyImages: number;
    productionTextures: number;
    multiSelectionPerFace: number;
    viewportCycles: number;
    faceVisibilityCycles: number;
  };
  frames: {
    measured: number;
    durationMs: number;
    fps: number;
    averageMs: number;
    p95Ms: number;
    maximumMs: number;
    slowFrameRatio: number;
  };
  renderIsolation: InactiveBoardDrawResult;
  memory: MemoryResult;
  thresholds: typeof THRESHOLDS;
  checks: {
    fps: boolean;
    p95Frame: boolean;
    slowFrameRatio: boolean;
    inactiveBoard: boolean;
    heap: boolean | null;
  };
  passed: boolean;
};

declare global {
  interface Window {
    __KONVA_BENCHMARK__?: KonvaBenchmarkResult;
    gc?: () => void;
  }

  interface Performance {
    memory?: { usedJSHeapSize: number };
  }
}

function makeCanvases(
  count: number,
  width: number,
  height: number,
  colors: string[],
) {
  return Array.from({ length: count }, (_, canvasIndex) => {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext("2d")!;
    context.clearRect(0, 0, width, height);
    context.globalAlpha = 0.65;
    context.fillStyle = colors[canvasIndex % colors.length];

    for (let index = 0; index < 80; index += 1) {
      const x = (index * 83 + canvasIndex * 61) % width;
      const y = (index * 47 + canvasIndex * 37) % height;
      context.fillRect(
        x,
        y,
        18 + ((index * 13) % 70),
        10 + ((index * 7) % 50),
      );
    }
    return canvas;
  });
}

const nextFrame = () =>
  new Promise<number>((resolve) => requestAnimationFrame(resolve));

async function gcAndSettle() {
  window.gc?.();
  await nextFrame();
  await nextFrame();
}

function percentile(values: number[], fraction: number) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.ceil(sorted.length * fraction) - 1] ?? 0;
}

const readHeap = () => performance.memory?.usedJSHeapSize ?? null;
const number = (value: number) => value.toFixed(1);
const bytes = (value: number) => `${number(value / 1024 / 1024)} MB`;
const otherFace = (face: BenchmarkFace): BenchmarkFace =>
  face === "front" ? "back" : "front";

async function invokeTauri(command: string, args: Record<string, unknown>) {
  if (!("__TAURI_INTERNALS__" in window)) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke(command, args);
}

export function KonvaBenchmark() {
  const frontStageRef = useRef<Konva.Stage>(null);
  const backStageRef = useRef<Konva.Stage>(null);
  const frontTransformerRef = useRef<Konva.Transformer>(null);
  const backTransformerRef = useRef<Konva.Transformer>(null);
  const frontTextureLayerRef = useRef<Konva.Layer>(null);
  const backTextureLayerRef = useRef<Konva.Layer>(null);
  const stages = {
    front: frontStageRef,
    back: backStageRef,
  };
  const transformers = {
    front: frontTransformerRef,
    back: backTransformerRef,
  };
  const textureLayers = {
    front: frontTextureLayerRef,
    back: backTextureLayerRef,
  };
  const nodes = useRef<Record<BenchmarkFace, Map<string, Konva.Node>>>({
    front: new Map(),
    back: new Map(),
  });
  const drawCounts = useRef<Record<BenchmarkFace, number>>({
    front: 0,
    back: 0,
  });
  const running = useRef(false);
  const proxyImages = useMemo(
    () =>
      makeCanvases(8, 256, 160, ["#f6bd60", "#84a59d", "#6d597a"]),
    [],
  );
  const productionTextures = useMemo(
    () =>
      makeCanvases(6, 512, 640, [
        "#c99732",
        "#f6f1df",
        "#0b5d3b",
        "#996515",
        "#d8cfb4",
        "#124e3b",
      ]),
    [],
  );
  const objects = useMemo(() => createDualBoardObjects(BOARD), []);
  const [phase, setPhase] = useState<Phase>("idle");
  const [activeFace, setActiveFace] = useState<BenchmarkFace>("front");
  const [result, setResult] = useState<KonvaBenchmarkResult | null>(null);

  const register = useCallback(
    (face: BenchmarkFace, id: string, node: Konva.Node | null) => {
      if (node) nodes.current[face].set(id, node);
      else nodes.current[face].delete(id);
    },
    [],
  );

  useEffect(() => {
    const cleanups: Array<() => void> = [];
    for (const face of FACE_ORDER) {
      for (const layer of stages[face].current?.getLayers() ?? []) {
        const countDraw = () => {
          drawCounts.current[face] += 1;
        };
        layer.on("draw.benchmark", countDraw);
        cleanups.push(() => layer.off("draw.benchmark", countDraw));
      }
    }
    return () => cleanups.forEach((cleanup) => cleanup());
  }, []);

  const run = useCallback(async () => {
    const frontStage = frontStageRef.current;
    const backStage = backStageRef.current;
    if (running.current || !frontStage || !backStage) return;
    running.current = true;
    window.__KONVA_BENCHMARK__ = undefined;
    setResult(null);
    setActiveFace("front");

    for (const face of FACE_ORDER) {
      const stage = stages[face].current!;
      stage.position({ x: 0, y: 0 });
      stage.scale({ x: 1, y: 1 });
      stage.batchDraw();
      transformers[face].current?.nodes([]);
      for (const texture of textureLayers[face].current?.getChildren() ?? []) {
        texture.visible(true);
      }
      textureLayers[face].current?.batchDraw();
    }

    setPhase("warmup");
    for (let frame = 0; frame < 90; frame += 1) await nextFrame();
    await gcAndSettle();
    drawCounts.current = { front: 0, back: 0 };
    await invokeTauri("record_benchmark_checkpoint", { cycle: 0 });
    const initialHeap = readHeap();
    const heapSamples: HeapSample[] = [];
    const frameDurations: number[] = [];
    const drawSamples: BoardDrawSample[] = [];
    let previous = await nextFrame();
    const recordFrame = async () => {
      const timestamp = await nextFrame();
      frameDurations.push(timestamp - previous);
      previous = timestamp;
    };

    for (const face of FACE_ORDER) {
      setActiveFace(face);
      setPhase(face === "front" ? "front-viewport" : "back-viewport");
      await nextFrame();
      const inactiveFace = otherFace(face);
      const activeBefore = drawCounts.current[face];
      const inactiveBefore = drawCounts.current[inactiveFace];
      const stage = stages[face].current!;
      for (let frame = 0; frame < 180; frame += 1) {
        const progress = frame / 180;
        const scale =
          0.82 + Math.sin(progress * Math.PI * 6) * 0.18;
        stage.scale({ x: scale, y: scale });
        stage.position({
          x: Math.sin(progress * Math.PI * 8) * 20,
          y: Math.cos(progress * Math.PI * 6) * 28,
        });
        stage.batchDraw();
        await recordFrame();
      }
      drawSamples.push({
        activeFace: face,
        activeDraws: drawCounts.current[face] - activeBefore,
        inactiveDraws:
          drawCounts.current[inactiveFace] - inactiveBefore,
      });
    }

    setPhase("selection-transform");
    for (const face of FACE_ORDER) {
      setActiveFace(face);
      const selected = objects[face]
        .filter((_, index) => index % 8 === 0)
        .slice(0, MULTI_SELECTION_COUNT)
        .map((object) => object.id);
      const transformer = transformers[face].current!;
      transformer.nodes(
        selected
          .map((id) => nodes.current[face].get(id))
          .filter((node): node is Konva.Node => node !== undefined),
      );
      for (let frame = 0; frame < 90; frame += 1) {
        const progress = frame / 90;
        selected.forEach((id, index) => {
          nodes.current[face]
            .get(id)
            ?.rotation(Math.sin(progress * Math.PI * 4 + index) * 4);
        });
        transformer.forceUpdate();
        transformer.getLayer()?.batchDraw();
        await recordFrame();
      }
    }

    setPhase("face-visibility");
    for (let cycle = 0; cycle < FACE_VISIBILITY_CYCLES; cycle += 1) {
      const face = FACE_ORDER[cycle % 2]!;
      const inactiveFace = otherFace(face);
      setActiveFace(face);
      const activeBefore = drawCounts.current[face];
      const inactiveBefore = drawCounts.current[inactiveFace];
      const visibleLocalIndices =
        cycle % 3 === 0
          ? new Set([0])
          : cycle % 3 === 1
            ? new Set([1])
            : new Set([0, 1, 2]);
      textureLayers[face].current
        ?.getChildren()
        .forEach((texture, index) =>
          texture.visible(visibleLocalIndices.has(index)),
        );
      textureLayers[face].current?.batchDraw();
      for (let frame = 0; frame < 8; frame += 1) await recordFrame();
      drawSamples.push({
        activeFace: face,
        activeDraws: drawCounts.current[face] - activeBefore,
        inactiveDraws:
          drawCounts.current[inactiveFace] - inactiveBefore,
      });
      if (cycle % 4 === 3) {
        await gcAndSettle();
        await invokeTauri("record_benchmark_checkpoint", {
          cycle: cycle + 1,
        });
        const usedBytes = readHeap();
        if (usedBytes !== null)
          heapSamples.push({ cycle: cycle + 1, usedBytes });
        previous = await nextFrame();
      }
    }

    for (const face of FACE_ORDER) {
      stages[face].current!.position({ x: 0, y: 0 });
      stages[face].current!.scale({ x: 1, y: 1 });
      stages[face].current!.batchDraw();
      transformers[face].current?.nodes([]);
    }
    setActiveFace("front");
    await gcAndSettle();
    await invokeTauri("record_benchmark_checkpoint", {
      cycle: FACE_VISIBILITY_CYCLES + 1,
    });
    const finalHeap = readHeap();

    const durationMs = frameDurations.reduce(
      (sum, value) => sum + value,
      0,
    );
    const averageMs = durationMs / frameDurations.length;
    const p95Ms = percentile(frameDurations, 0.95);
    const slowFrameRatio =
      frameDurations.filter(
        (value) => value > THRESHOLDS.slowFrameBoundaryMs,
      ).length / frameDurations.length;
    const fps = 1000 / averageMs;
    const renderIsolation = evaluateInactiveBoardDraws(drawSamples);
    let memory: MemoryResult;
    if (initialHeap === null || finalHeap === null) {
      memory = {
        available: false,
        reason:
          "performance.memory 不可用；需以 Chromium 精确内存参数或目标 WebView 复验。",
        passed: null,
      };
    } else {
      const growthBytes = finalHeap - initialHeap;
      const growthRatio = growthBytes / Math.max(initialHeap, 1);
      const firstCycleHeap =
        heapSamples[0]?.usedBytes ?? initialHeap;
      const lastCycleHeap =
        heapSamples.at(-1)?.usedBytes ?? finalHeap;
      const cycleGrowthBytes = lastCycleHeap - firstCycleHeap;
      const cycleGrowthRatio =
        cycleGrowthBytes / Math.max(firstCycleHeap, 1);
      memory = {
        available: true,
        initialBytes: initialHeap,
        finalBytes: finalHeap,
        peakBytes: Math.max(
          initialHeap,
          finalHeap,
          ...heapSamples.map((sample) => sample.usedBytes),
        ),
        growthBytes,
        growthRatio,
        cycleGrowthBytes,
        cycleGrowthRatio,
        samples: heapSamples,
        passed:
          growthBytes <=
            THRESHOLDS.maximumInitialToFinalHeapGrowthBytes &&
          cycleGrowthBytes <=
            THRESHOLDS.maximumCycleHeapGrowthBytes &&
          cycleGrowthRatio <=
            THRESHOLDS.maximumCycleHeapGrowthRatio,
      };
    }
    const checks = {
      fps: fps >= THRESHOLDS.minimumFps,
      p95Frame: p95Ms <= THRESHOLDS.maximumP95FrameMs,
      slowFrameRatio:
        slowFrameRatio <= THRESHOLDS.maximumSlowFrameRatio,
      inactiveBoard: renderIsolation.passed,
      heap: memory.passed,
    };
    const benchmarkResult: KonvaBenchmarkResult = {
      version: 2,
      userAgent: navigator.userAgent,
      viewport: {
        width: window.innerWidth,
        height: window.innerHeight,
        devicePixelRatio: window.devicePixelRatio,
      },
      scene: {
        boardCount: 2,
        objectsPerFace: OBJECTS_PER_FACE,
        editableObjects: TOTAL_EDITABLE_OBJECTS,
        proxyImages: FACE_ORDER.flatMap((face) => objects[face]).filter(
          (object) => object.kind === "image",
        ).length,
        productionTextures: productionTextures.length,
        multiSelectionPerFace: MULTI_SELECTION_COUNT,
        viewportCycles: 2,
        faceVisibilityCycles: FACE_VISIBILITY_CYCLES,
      },
      frames: {
        measured: frameDurations.length,
        durationMs,
        fps,
        averageMs,
        p95Ms,
        maximumMs: Math.max(...frameDurations),
        slowFrameRatio,
      },
      renderIsolation,
      memory,
      thresholds: THRESHOLDS,
      checks,
      passed:
        checks.fps &&
        checks.p95Frame &&
        checks.slowFrameRatio &&
        checks.inactiveBoard &&
        checks.heap !== false,
    };
    window.__KONVA_BENCHMARK__ = benchmarkResult;
    setResult(benchmarkResult);
    setPhase("complete");
    running.current = false;
    await invokeTauri("report_benchmark", { result: benchmarkResult });
  }, [objects, productionTextures.length]);

  useEffect(() => {
    void run();
  }, [run]);

  return (
    <main className="min-h-screen bg-zinc-950 p-5 text-zinc-100">
      <section className="mx-auto max-w-[1240px]">
        <header className="mb-4 flex items-start justify-between gap-6">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.22em] text-amber-400">
              React + Konva benchmark
            </p>
            <h1 className="mt-1 text-xl font-semibold">
              PCB Atelier 双画板性能基准
            </h1>
            <p className="mt-1 text-sm text-zinc-400">
              正背面同时存在 · 各 100 对象 · 独立视口 · 非活动面重绘隔离
            </p>
          </div>
          <div className="flex items-center gap-3">
            <span className="rounded-full bg-zinc-800 px-3 py-1.5 text-xs">
              {phase}
            </span>
            <button
              className="rounded-md bg-amber-400 px-3 py-1.5 text-sm font-semibold text-zinc-950 disabled:opacity-40"
              disabled={phase !== "idle" && phase !== "complete"}
              onClick={() => void run()}
              type="button"
            >
              重新运行
            </button>
          </div>
        </header>
        <div className="grid gap-4 xl:grid-cols-[836px_1fr]">
          <div className="grid grid-cols-2 gap-4">
            {FACE_ORDER.map((face) => (
              <div
                className={`overflow-hidden rounded-xl border bg-[#101714] ${
                  activeFace === face
                    ? "border-amber-400"
                    : "border-zinc-800"
                }`}
                data-testid={`benchmark-board-${face}`}
                key={face}
              >
                <p className="px-3 py-2 text-xs font-semibold text-zinc-400">
                  {face === "front" ? "正面" : "背面"}
                </p>
                <BenchmarkBoard
                  face={face}
                  objects={objects[face]}
                  productionTextures={productionTextures}
                  proxyImages={proxyImages}
                  register={register}
                  stageRef={stages[face]}
                  textureLayerRef={textureLayers[face]}
                  transformerRef={transformers[face]}
                />
              </div>
            ))}
          </div>
          <aside className="rounded-xl border border-zinc-800 bg-zinc-900 p-4">
            <h2 className="text-sm font-semibold">结果</h2>
            {!result && (
              <p className="mt-3 text-sm text-zinc-400">正在执行…</p>
            )}
            {result && (
              <div className="mt-3 space-y-2 text-sm">
                <Metric label="平均 FPS" value={number(result.frames.fps)} />
                <Metric
                  label="平均 / P95"
                  value={`${number(result.frames.averageMs)} / ${number(result.frames.p95Ms)} ms`}
                />
                <Metric
                  label="最大帧"
                  value={`${number(result.frames.maximumMs)} ms`}
                />
                <Metric
                  label="慢帧"
                  value={`${number(result.frames.slowFrameRatio * 100)}%`}
                />
                <Metric
                  label="非活动面重绘"
                  value={`${result.renderIsolation.totalInactiveDraws} / ${result.renderIsolation.totalActiveDraws}`}
                />
                <Metric
                  label="JS heap"
                  value={
                    result.memory.available
                      ? `${bytes(result.memory.initialBytes)} → ${bytes(result.memory.finalBytes)}`
                      : "不可用"
                  }
                />
                <p
                  className={`rounded-md p-2 font-semibold ${
                    result.passed
                      ? "bg-emerald-500/15 text-emerald-300"
                      : "bg-red-500/15 text-red-300"
                  }`}
                >
                  双画板基准：{result.passed ? "通过" : "未通过"}
                </p>
                <details className="text-xs text-zinc-400">
                  <summary>完整 JSON</summary>
                  <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap bg-zinc-950 p-2">
                    {JSON.stringify(result, null, 2)}
                  </pre>
                </details>
              </div>
            )}
          </aside>
        </div>
      </section>
    </main>
  );
}

interface BenchmarkBoardProps {
  face: BenchmarkFace;
  objects: EditableObject[];
  proxyImages: HTMLCanvasElement[];
  productionTextures: HTMLCanvasElement[];
  register: (
    face: BenchmarkFace,
    id: string,
    node: Konva.Node | null,
  ) => void;
  stageRef: RefObject<Konva.Stage | null>;
  textureLayerRef: RefObject<Konva.Layer | null>;
  transformerRef: RefObject<Konva.Transformer | null>;
}

const BenchmarkBoard = memo(function BenchmarkBoard({
  face,
  objects,
  proxyImages,
  productionTextures,
  register,
  stageRef,
  textureLayerRef,
  transformerRef,
}: BenchmarkBoardProps) {
  return (
    <Stage height={BOARD.height} ref={stageRef} width={BOARD.width}>
      <Layer listening={false} ref={textureLayerRef}>
        <Rect
          cornerRadius={24}
          fill={face === "front" ? "#173b2c" : "#173348"}
          height={BOARD.height}
          width={BOARD.width}
        />
        {TEXTURES_BY_FACE[face].map((textureIndex) => (
          <KonvaImage
            height={BOARD.height}
            image={productionTextures[textureIndex]}
            key={textureIndex}
            opacity={0.44}
            scaleX={face === "front" ? 1 : -1}
            width={BOARD.width}
            x={face === "front" ? 0 : BOARD.width}
          />
        ))}
      </Layer>
      <Layer>
        {objects.map((object) => {
          const common = {
            draggable: true,
            id: object.id,
            key: object.id,
            ref: (node: Konva.Node | null) =>
              register(face, object.id, node),
          };
          if (object.kind === "rect")
            return (
              <Rect
                {...common}
                cornerRadius={4}
                fill={object.color}
                height={object.height}
                opacity={0.82}
                width={object.width}
                x={object.x}
                y={object.y}
              />
            );
          if (object.kind === "text")
            return (
              <Text
                {...common}
                fill={object.color}
                fontSize={13}
                fontStyle="bold"
                text={object.text}
                x={object.x}
                y={object.y}
              />
            );
          return (
            <KonvaImage
              {...common}
              height={object.height}
              image={proxyImages[object.imageIndex]}
              opacity={0.72}
              width={object.width}
              x={object.x}
              y={object.y}
            />
          );
        })}
      </Layer>
      <Layer>
        <Transformer ref={transformerRef} />
      </Layer>
    </Stage>
  );
});

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between border-b border-zinc-800 pb-2">
      <span className="text-zinc-400">{label}</span>
      <span className="font-mono">{value}</span>
    </div>
  );
}
