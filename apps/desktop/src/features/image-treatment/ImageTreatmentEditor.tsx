import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";

import {
  TreatmentPreviewCoordinator,
  type TreatmentPreviewAccepted,
} from "@/features/image-treatment/treatment-preview-coordinator";
import {
  compileImageTreatment,
  getAssetBytes,
  setTreatmentRecipe,
  type AssetReference,
  type ImageTreatment,
  type TreatmentCompileReport,
  type TreatmentRecipe,
} from "@/lib/core";

interface TreatmentCrop {
  xMillionths: number;
  yMillionths: number;
  widthMillionths: number;
  heightMillionths: number;
}

export interface ImageTreatmentEditorProps {
  asset: AssetReference;
  treatment: ImageTreatment;
  colorOriginalAvailable?: boolean;
  colorOriginalUnavailableReason?: string;
  showAssetHeader?: boolean;
  physicalWidthUm: number;
  physicalHeightUm: number;
  originalPreviewUrl?: string;
  resultPreviewUrl?: string;
  compileReport?: TreatmentCompileReport | null;
  debounceMs?: number;
  persistRecipe?: (recipe: TreatmentRecipe) => Promise<unknown>;
  compileInteractiveProxy?: () => Promise<TreatmentCompileReport>;
  onRecipeChange?: (recipe: TreatmentRecipe) => void;
  onProductionModeChange?: (
    mode: ImageTreatment["productionMode"],
  ) => void;
  onEnableColorOriginal?: () => void;
  onCompileAccepted: (result: TreatmentPreviewAccepted) => void;
  onCancel?: () => void;
  onConfirm?: (
    recipe: TreatmentRecipe,
    report: TreatmentCompileReport,
  ) => void;
  onTemporaryOriginalChange?: (visible: boolean) => void;
  onError?: (error: unknown) => void;
}

export function ImageTreatmentEditor({
  asset,
  colorOriginalAvailable = false,
  colorOriginalUnavailableReason,
  compileReport,
  debounceMs = 100,
  onCompileAccepted,
  onCancel,
  onConfirm,
  onError,
  onEnableColorOriginal,
  onRecipeChange,
  onProductionModeChange,
  onTemporaryOriginalChange,
  physicalHeightUm,
  physicalWidthUm,
  resultPreviewUrl,
  showAssetHeader = true,
  treatment,
  ...props
}: ImageTreatmentEditorProps) {
  const [draftRecipe, setDraftRecipe] = useState(treatment.recipe);
  const [originalPinned, setOriginalPinned] = useState(false);
  const [originalHeld, setOriginalHeld] = useState(false);
  const [previewPending, setPreviewPending] = useState(false);
  const [cropDialogOpen, setCropDialogOpen] = useState(false);
  const pointerStartedAt = useRef<number | null>(null);
  const onCompileAcceptedRef = useRef(onCompileAccepted);
  const onErrorRef = useRef(onError);
  onCompileAcceptedRef.current = onCompileAccepted;
  onErrorRef.current = onError;
  const generatedOriginalUrl = useAssetPreviewUrl(
    asset.id,
    props.originalPreviewUrl !== undefined,
  );
  const originalPreviewUrl = props.originalPreviewUrl ?? generatedOriginalUrl;
  const showOriginalOnCanvas = originalPinned || originalHeld;
  const usesColorOriginal = treatment.productionMode === "colorOriginal";
  const previewUpdating =
    previewPending || (!compileReport && resultPreviewUrl === undefined);

  const coordinator = useMemo(
    () =>
      new TreatmentPreviewCoordinator({
        debounceMs,
        persistRecipe:
          props.persistRecipe ??
          ((recipe) => setTreatmentRecipe(treatment.id, recipe)),
        compileInteractiveProxy:
          props.compileInteractiveProxy ??
          (() =>
            compileImageTreatment(
              treatment.id,
              physicalWidthUm,
              physicalHeightUm,
              "interactiveProxy",
            )),
        onAccepted: (result) => {
          setPreviewPending(false);
          onCompileAcceptedRef.current(result);
        },
        onError: (error) => {
          setPreviewPending(false);
          onErrorRef.current?.(error);
        },
      }),
    [
      debounceMs,
      physicalHeightUm,
      physicalWidthUm,
      props.compileInteractiveProxy,
      props.persistRecipe,
      treatment.id,
    ],
  );

  useEffect(() => {
    coordinator.activate();
    return () => coordinator.dispose();
  }, [coordinator]);
  useEffect(() => {
    setDraftRecipe(treatment.recipe);
  }, [treatment.id, treatment.recipe]);
  useEffect(() => {
    onTemporaryOriginalChange?.(showOriginalOnCanvas);
  }, [onTemporaryOriginalChange, showOriginalOnCanvas]);

  const updateRecipe = (recipe: TreatmentRecipe) => {
    setDraftRecipe(recipe);
    setPreviewPending(true);
    onRecipeChange?.(recipe);
    coordinator.update(recipe);
  };
  const patchRecipe = (patch: Partial<TreatmentRecipe>) =>
    updateRecipe({ ...draftRecipe, ...patch });
  const crop = asTreatmentCrop(draftRecipe.crop);

  useEffect(() => {
    if (
      draftRecipe.threshold.mode !== "otsu" ||
      previewPending ||
      !compileReport
    ) {
      return;
    }
    const recipe: TreatmentRecipe = {
      ...draftRecipe,
      threshold: {
        mode: "manual",
        value: compileReport.appliedThreshold,
      },
    };
    setDraftRecipe(recipe);
    setPreviewPending(true);
    onRecipeChange?.(recipe);
    coordinator.update(recipe);
  }, [
    compileReport,
    coordinator,
    draftRecipe,
    onRecipeChange,
    previewPending,
  ]);

  return (
    <section
      aria-label="图片处理"
      className={`treatment-editor min-w-0 ${
        showAssetHeader ? "space-y-4" : "space-y-3"
      }`}
    >
      {showAssetHeader && (
        <header className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <h3 className="truncate text-xs font-semibold">
              {asset.originalFilename}
            </h3>
          </div>
        <button
          aria-label="临时查看原图"
          aria-pressed={showOriginalOnCanvas}
          className="shrink-0 rounded-md border bg-background px-2.5 py-1.5 text-[10px] hover:bg-accent"
          onClick={() => {
            const startedAt = pointerStartedAt.current;
            pointerStartedAt.current = null;
            if (startedAt === null || Date.now() - startedAt < 300) {
              setOriginalPinned((value) => !value);
            }
          }}
          onPointerCancel={() => setOriginalHeld(false)}
          onPointerDown={() => {
            pointerStartedAt.current = Date.now();
            setOriginalHeld(true);
          }}
          onPointerLeave={() => setOriginalHeld(false)}
          onPointerUp={() => setOriginalHeld(false)}
          type="button"
        >
          {showOriginalOnCanvas ? "恢复处理结果" : "临时查看原图"}
        </button>
        </header>
      )}

      {showAssetHeader && (
        <p className="text-[9px] leading-4 text-muted-foreground">
          按住可临时对照，点击可固定或取消；只切换显示，不修改配方、变换或导出。
        </p>
      )}

      {onProductionModeChange && (
        <section className="rounded-lg bg-muted/35 p-1">
          <div
            aria-label="生产方式"
            className="grid grid-cols-2 gap-1"
            role="radiogroup"
          >
            <button
              aria-checked={!usesColorOriginal}
              className={`rounded px-2 py-1.5 text-[10px] font-medium ${
                !usesColorOriginal
                  ? "bg-background shadow-sm"
                  : "text-muted-foreground"
              }`}
              onClick={() => onProductionModeChange("monochromeMask")}
              role="radio"
              type="button"
            >
              单色生产
            </button>
            <button
              aria-checked={usesColorOriginal}
              className={`rounded px-2 py-1.5 text-[10px] font-medium disabled:cursor-not-allowed disabled:opacity-45 ${
                usesColorOriginal
                  ? "bg-background shadow-sm"
                  : "text-muted-foreground"
              }`}
              disabled={!colorOriginalAvailable && !onEnableColorOriginal}
              onClick={() => {
                if (colorOriginalAvailable) {
                  onProductionModeChange("colorOriginal");
                } else {
                  onEnableColorOriginal?.();
                }
              }}
              role="radio"
              title={
                !colorOriginalAvailable
                  ? colorOriginalUnavailableReason
                  : undefined
              }
              type="button"
            >
              彩色原图
            </button>
          </div>
          {!onEnableColorOriginal &&
            !colorOriginalAvailable &&
            colorOriginalUnavailableReason && (
            <p className="px-2 pb-1 pt-2 text-[9px] leading-4 text-muted-foreground">
              {colorOriginalUnavailableReason}
            </p>
            )}
        </section>
      )}

      <div className="treatment-preview-grid grid gap-3">
        <TreatmentPreview
          alt={`${asset.originalFilename} 原图`}
          label="原图"
          url={originalPreviewUrl}
        />
        <TreatmentPreview
          alt={`${asset.originalFilename} ${
            usesColorOriginal ? "彩色生产预览" : "处理结果"
          }`}
          colorOriginal={usesColorOriginal}
          label={usesColorOriginal ? "彩色生产预览" : "处理结果"}
          productionMask={!usesColorOriginal}
          report={compileReport}
          url={
            showOriginalOnCanvas || usesColorOriginal
              ? originalPreviewUrl
              : resultPreviewUrl
          }
        />
      </div>
      <div
        aria-label="实时预览状态"
        aria-live="polite"
        className={
          showAssetHeader
            ? "flex min-h-4 items-center gap-1.5 text-[9px] text-muted-foreground"
            : "sr-only"
        }
      >
        <span
          aria-hidden="true"
          className={`size-1.5 rounded-full ${
            previewUpdating ? "bg-amber-500" : "bg-emerald-500"
          }`}
        />
        {usesColorOriginal
          ? "彩色原图将作为丝印生产资料"
          : previewUpdating
            ? "正在实时更新预览…"
            : "实时预览已更新"}
      </div>

      <div className="space-y-3 border-t pt-3">
        {!usesColorOriginal && (
          <>
        <div className="treatment-basic-grid grid gap-2">
          <EditorSelect
            label="Alpha 处理"
            onChange={(value) =>
              patchRecipe({
                alphaMode: value as TreatmentRecipe["alphaMode"],
              })
            }
            options={[
              ["compositeOnWhite", "合成到白底"],
              ["alphaAsCoverage", "Alpha 作为覆盖率"],
              ["ignoreAlpha", "忽略 Alpha"],
            ]}
            value={draftRecipe.alphaMode}
          />
          <div className="flex min-w-0 items-end gap-2">
            <RecipeRangeInput
              label="阈值"
              max={255}
              min={0}
              onChange={(value) =>
                patchRecipe({
                  threshold: { mode: "manual", value: Math.round(value) },
                })
              }
              step={1}
              unit=""
              value={
                draftRecipe.threshold.mode === "manual"
                  ? draftRecipe.threshold.value
                  : (compileReport?.appliedThreshold ?? 128)
              }
            />
            <button
              aria-label="重新自动估算阈值"
              className="h-9 shrink-0 rounded-md border bg-background px-3 text-[10px] hover:bg-accent disabled:cursor-wait disabled:opacity-50"
              disabled={
                draftRecipe.threshold.mode === "otsu" && previewPending
              }
              onClick={() => patchRecipe({ threshold: { mode: "otsu" } })}
              type="button"
            >
              自动估算
            </button>
          </div>
          <label className="flex min-h-9 items-center justify-between gap-3 rounded-md bg-muted/35 px-3 text-[10px]">
            <span className="text-muted-foreground">反相</span>
            <input
              aria-label="反相"
              checked={draftRecipe.invert}
              onChange={(event) =>
                patchRecipe({ invert: event.currentTarget.checked })
              }
              type="checkbox"
            />
          </label>
        </div>

        <section
          aria-label="清理与制造约束"
          className="rounded-lg bg-muted/30 p-3"
        >
          <div className="mb-2 flex items-end justify-between gap-4">
            <div>
              <h4 className="text-[10px] font-semibold">清理与制造约束</h4>
            <p className="mt-1 text-[9px] leading-4 text-muted-foreground">
              拖动滑杆即可实时比较处理结果。
            </p>
            </div>
            <label className="flex shrink-0 items-center gap-2 text-[9px] text-muted-foreground">
              <span>细线</span>
              <select
                aria-label="细线处理"
                className="h-7 rounded-md border bg-background px-2 text-[10px]"
                onChange={(event) =>
                  patchRecipe({
                    thinFeaturePolicy: event.currentTarget
                      .value as TreatmentRecipe["thinFeaturePolicy"],
                  })
                }
                value={draftRecipe.thinFeaturePolicy}
              >
                <option value="preserve">保留并警告</option>
                <option value="thicken">加粗</option>
                <option value="remove">移除</option>
              </select>
            </label>
          </div>
          <div className="treatment-parameter-grid grid gap-x-5 gap-y-2">
            <RecipeRangeInput
              label="平滑半径"
              max={1}
              min={0}
              onChange={(value) =>
                patchRecipe({ smoothingRadiusUm: millimetresToUm(value) })
              }
              step={0.01}
              unit="mm"
              value={draftRecipe.smoothingRadiusUm / 1_000}
            />
            <RecipeRangeInput
              label="去斑半径"
              max={1}
              min={0}
              onChange={(value) =>
                patchRecipe({ despeckleRadiusUm: millimetresToUm(value) })
              }
              step={0.01}
              unit="mm"
              value={draftRecipe.despeckleRadiusUm / 1_000}
            />
            <RecipeRangeInput
              label="去除孤岛"
              max={10}
              min={0}
              onChange={(value) =>
                patchRecipe({
                  removeIslandsBelowUm2: Math.round(value * 1_000_000),
                })
              }
              step={0.01}
              unit="mm²"
              value={draftRecipe.removeIslandsBelowUm2 / 1_000_000}
            />
            <RecipeRangeInput
              label="最小线宽"
              max={1}
              min={0}
              onChange={(value) =>
                patchRecipe({ minimumLineWidthUm: millimetresToUm(value) })
              }
              step={0.01}
              unit="mm"
              value={draftRecipe.minimumLineWidthUm / 1_000}
            />
            <RecipeRangeInput
              label="最小间距"
              max={1}
              min={0}
              onChange={(value) =>
                patchRecipe({ minimumGapUm: millimetresToUm(value) })
              }
              step={0.01}
              unit="mm"
              value={draftRecipe.minimumGapUm / 1_000}
            />
          </div>
        </section>
          </>
        )}

        <section className="flex items-center gap-3 rounded-lg bg-muted/30 px-3 py-2">
          <div className="min-w-0 flex-1">
            <h4 className="text-[10px] font-semibold">裁切</h4>
            <p className="mt-1 truncate text-[9px] text-muted-foreground">
              {crop
                ? `当前保留 ${formatPercent(crop.widthMillionths)} × ${formatPercent(crop.heightMillionths)}`
                : "使用完整原图"}
            </p>
          </div>
          {crop && (
            <button
              aria-label="移除裁切"
              className="shrink-0 rounded-md px-2 py-1.5 text-[9px] text-muted-foreground hover:bg-accent"
              onClick={() => patchRecipe({ crop: null })}
              type="button"
            >
              移除
            </button>
          )}
          <button
            aria-label={crop ? "调整裁切" : "开始裁切"}
            className="shrink-0 rounded-md border bg-background px-2.5 py-1.5 text-[9px] font-medium hover:bg-accent"
            onClick={() => setCropDialogOpen(true)}
            type="button"
          >
            {crop ? "调整…" : "裁切…"}
          </button>
        </section>
      </div>

      {cropDialogOpen && (
        <CropDialog
          crop={
            crop ?? {
              xMillionths: 0,
              yMillionths: 0,
              widthMillionths: 1_000_000,
              heightMillionths: 1_000_000,
            }
          }
          imageUrl={originalPreviewUrl}
          onCancel={() => setCropDialogOpen(false)}
          onConfirm={(nextCrop) => {
            patchRecipe({ crop: nextCrop });
            setCropDialogOpen(false);
          }}
          pixelHeight={asset.pixelHeight}
          pixelWidth={asset.pixelWidth}
        />
      )}

      {showAssetHeader && compileReport && !usesColorOriginal && (
        <details className="rounded-lg border bg-background">
          <summary className="cursor-pointer px-3 py-2 text-[10px] font-medium">
            生产检查
          </summary>
          <div className="border-t p-2">
            <TreatmentDiagnostics report={compileReport} />
          </div>
        </details>
      )}
      {onConfirm && (
        <div className="flex justify-end gap-2">
          {onCancel && (
            <button
              className="h-9 rounded-md border bg-background px-4 text-xs font-medium hover:bg-accent"
              onClick={onCancel}
              type="button"
            >
              取消
            </button>
          )}
          <button
            className="h-9 rounded-md bg-primary px-4 text-xs font-medium text-primary-foreground disabled:opacity-50"
            disabled={!compileReport || previewPending}
            onClick={() => {
              if (compileReport && !previewPending) {
                onConfirm(draftRecipe, compileReport);
              }
            }}
            type="button"
          >
            确认处理并插入
          </button>
        </div>
      )}
    </section>
  );
}

function TreatmentPreview({
  alt,
  colorOriginal = false,
  label,
  productionMask = false,
  report,
  url,
}: {
  alt: string;
  colorOriginal?: boolean;
  label: string;
  productionMask?: boolean;
  report?: TreatmentCompileReport | null;
  url?: string;
}) {
  return (
    <figure
      className="overflow-hidden rounded-lg border bg-background"
      data-preview-kind={
        productionMask
          ? "production-mask"
          : colorOriginal
            ? "color-original"
            : "source"
      }
      data-preview-size="compact"
    >
      <figcaption className="flex items-center justify-between gap-2 border-b px-3 py-2 text-[10px] font-medium">
        <span>{label}</span>
        {productionMask && (
          <span className="font-normal text-muted-foreground">
            黑色 = 生产区域
          </span>
        )}
      </figcaption>
      <div className="treatment-preview-viewport relative grid place-items-center bg-[linear-gradient(45deg,var(--muted)_25%,transparent_25%,transparent_75%,var(--muted)_75%),linear-gradient(45deg,var(--muted)_25%,transparent_25%,transparent_75%,var(--muted)_75%)] bg-[length:16px_16px] bg-[position:0_0,8px_8px]">
        {url ? (
          <div
            aria-label={alt}
            className={`absolute inset-0 bg-center bg-contain bg-no-repeat ${
              productionMask ? "brightness-0" : ""
            }`}
            data-fit-mode="contain"
            role="img"
            style={{ backgroundImage: `url("${url}")` }}
          />
        ) : report ? (
          <div className="px-4 text-center text-[10px] leading-4 text-muted-foreground">
            <p>{report.widthPx} × {report.heightPx} px 交互代理</p>
            <p className="mt-1">
              {report.topology.islandCount} 个区域 · {report.topology.holeCount} 个孔
            </p>
          </div>
        ) : (
          <span className="text-[10px] text-muted-foreground">正在生成代理…</span>
        )}
      </div>
    </figure>
  );
}

type CropDragMode = "move" | "northWest" | "northEast" | "southWest" | "southEast";

function CropDialog({
  crop,
  imageUrl,
  onCancel,
  onConfirm,
  pixelHeight,
  pixelWidth,
}: {
  crop: TreatmentCrop;
  imageUrl?: string;
  onCancel: () => void;
  onConfirm: (crop: TreatmentCrop) => void;
  pixelHeight: number;
  pixelWidth: number;
}) {
  const [draft, setDraft] = useState(crop);
  return (
    <div
      aria-label="裁切图片"
      aria-modal="true"
      className="fixed inset-0 z-[70] grid place-items-center bg-black/60 p-4"
      role="dialog"
    >
      <section className="max-h-[92vh] w-full max-w-2xl overflow-auto rounded-xl border bg-card p-4 shadow-2xl sm:p-5">
        <header className="flex items-start justify-between gap-4">
          <div>
            <h3 className="text-sm font-semibold">裁切图片</h3>
            <p className="mt-1 text-[10px] text-muted-foreground">
              拖动取景框或四角手柄；完成前不会修改生产配方。
            </p>
          </div>
          <button
            aria-label="关闭裁切"
            className="rounded-md px-2 py-1 text-muted-foreground hover:bg-accent"
            onClick={onCancel}
            type="button"
          >
            ×
          </button>
        </header>
        <CropEditor
          crop={draft}
          imageUrl={imageUrl}
          onChange={setDraft}
          pixelHeight={pixelHeight}
          pixelWidth={pixelWidth}
        />
        <footer className="mt-4 flex justify-end gap-2 border-t pt-4">
          <button
            className="h-9 rounded-md border bg-background px-4 text-xs font-medium hover:bg-accent"
            onClick={onCancel}
            type="button"
          >
            取消
          </button>
          <button
            className="h-9 rounded-md bg-primary px-4 text-xs font-medium text-primary-foreground"
            onClick={() => onConfirm(draft)}
            type="button"
          >
            完成裁切
          </button>
        </footer>
      </section>
    </div>
  );
}

function CropEditor({
  crop,
  imageUrl,
  onChange,
  pixelHeight,
  pixelWidth,
}: {
  crop: TreatmentCrop;
  imageUrl?: string;
  onChange: (crop: TreatmentCrop) => void;
  pixelHeight: number;
  pixelWidth: number;
}) {
  const surfaceRef = useRef<HTMLDivElement | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const dragRef = useRef<{
    crop: TreatmentCrop;
    mode: CropDragMode;
    pointerId: number;
    x: number;
    y: number;
  } | null>(null);
  const startDrag = (
    mode: CropDragMode,
    event: ReactPointerEvent<HTMLElement>,
  ) => {
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = {
      crop,
      mode,
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY,
    };
  };
  const moveCrop = (clientX: number, clientY: number) => {
    const drag = dragRef.current;
    const bounds = surfaceRef.current?.getBoundingClientRect();
    if (!drag || !bounds || bounds.width <= 0 || bounds.height <= 0) return;
    const dx = Math.round(
      ((clientX - drag.x) / bounds.width) * 1_000_000,
    );
    const dy = Math.round(
      ((clientY - drag.y) / bounds.height) * 1_000_000,
    );
    onChangeRef.current(resizeTreatmentCrop(drag.crop, drag.mode, dx, dy));
  };
  useEffect(() => {
    const handlePointerMove = (event: PointerEvent) => {
      if (dragRef.current?.pointerId === event.pointerId) {
        moveCrop(event.clientX, event.clientY);
      }
    };
    const handlePointerEnd = (event: PointerEvent) => {
      if (dragRef.current?.pointerId === event.pointerId) {
        dragRef.current = null;
      }
    };
    globalThis.addEventListener("pointermove", handlePointerMove);
    globalThis.addEventListener("pointerup", handlePointerEnd);
    globalThis.addEventListener("pointercancel", handlePointerEnd);
    return () => {
      globalThis.removeEventListener("pointermove", handlePointerMove);
      globalThis.removeEventListener("pointerup", handlePointerEnd);
      globalThis.removeEventListener("pointercancel", handlePointerEnd);
    };
  }, []);
  const left = crop.xMillionths / 10_000;
  const top = crop.yMillionths / 10_000;
  const width = crop.widthMillionths / 10_000;
  const height = crop.heightMillionths / 10_000;
  const portraitWidth = Math.max(
    58,
    Math.min(100, (pixelWidth / Math.max(1, pixelHeight)) * 100),
  );

  return (
    <div className="mt-3 min-w-0">
      <div
        aria-label="裁切区域"
        className="relative mx-auto touch-none select-none overflow-hidden rounded-lg border bg-muted"
        ref={surfaceRef}
        style={{
          aspectRatio: `${Math.max(1, pixelWidth)} / ${Math.max(1, pixelHeight)}`,
          backgroundImage: imageUrl ? `url("${imageUrl}")` : undefined,
          backgroundPosition: "center",
          backgroundRepeat: "no-repeat",
          backgroundSize: "100% 100%",
          width: `${portraitWidth}%`,
        }}
      >
        <div
          aria-label="移动裁切区域"
          className="absolute cursor-move border-2 border-white shadow-[0_0_0_1px_rgb(0_0_0_/_0.45),0_0_0_999px_rgb(0_0_0_/_0.55)]"
          onPointerDown={(event) => startDrag("move", event)}
          role="button"
          style={{
            height: `${height}%`,
            left: `${left}%`,
            top: `${top}%`,
            width: `${width}%`,
          }}
          tabIndex={0}
        >
          {(
            [
              ["northWest", "left-0 top-0 -translate-x-1/2 -translate-y-1/2", "从左上角调整裁切区域"],
              ["northEast", "right-0 top-0 translate-x-1/2 -translate-y-1/2", "从右上角调整裁切区域"],
              ["southWest", "bottom-0 left-0 -translate-x-1/2 translate-y-1/2", "从左下角调整裁切区域"],
              ["southEast", "bottom-0 right-0 translate-x-1/2 translate-y-1/2", "从右下角调整裁切区域"],
            ] as const
          ).map(([mode, position, label]) => (
            <button
              aria-label={label}
              className={`absolute size-3 rounded-sm border border-primary bg-white shadow-sm ${position}`}
              key={mode}
              onPointerDown={(event) => {
                event.stopPropagation();
                startDrag(mode, event);
              }}
              type="button"
            />
          ))}
        </div>
      </div>

      <div className="mt-3 flex items-center justify-between gap-3">
        <p className="text-[9px] leading-4 text-muted-foreground">
          拖动框移动，拖四角调整范围。
        </p>
        <button
          aria-label="重置裁切"
          className="shrink-0 rounded-md border bg-background px-2 py-1 text-[9px] hover:bg-accent"
          onClick={() =>
            onChange({
              xMillionths: 0,
              yMillionths: 0,
              widthMillionths: 1_000_000,
              heightMillionths: 1_000_000,
            })
          }
          type="button"
        >
          重置
        </button>
      </div>

      <details className="mt-2 rounded-md border bg-background">
        <summary className="cursor-pointer select-none px-2.5 py-2 text-[9px] text-muted-foreground">
          精确数值
        </summary>
        <div
          className="treatment-crop-grid grid gap-2 border-t p-2.5"
          data-layout="crop-controls"
        >
          {(
            [
              ["裁切 X", "xMillionths"],
              ["裁切 Y", "yMillionths"],
              ["裁切宽度", "widthMillionths"],
              ["裁切高度", "heightMillionths"],
            ] as const
          ).map(([label, key]) => (
            <ExactNumberInput
              key={key}
              label={label}
              max={100}
              min={
                key === "widthMillionths" || key === "heightMillionths"
                  ? 0.1
                  : 0
              }
              onChange={(value) =>
                onChange(updateTreatmentCrop(crop, key, value))
              }
              step={0.1}
              unit="%"
              value={crop[key] / 10_000}
            />
          ))}
        </div>
      </details>
    </div>
  );
}

function TreatmentDiagnostics({ report }: { report: TreatmentCompileReport }) {
  const summaries = summarizeDiagnostics(report.diagnostics);
  const unresolvedCount = summaries.filter(
    (summary) => summary.tone === "warning",
  ).length;
  return (
    <section aria-label="处理诊断" className="rounded-lg border p-3">
      <h4 className="text-[10px] font-semibold">生产检查</h4>
      {summaries.length === 0 ? (
        <p className="mt-2 rounded-md bg-emerald-500/10 px-2.5 py-2 text-[10px]">
          生产检查通过，当前参数未发现需要处理的问题。
        </p>
      ) : (
        <>
          <p className="mt-1 text-[9px] text-muted-foreground">
            {unresolvedCount > 0
              ? `${unresolvedCount} 项仍需处理；已修复项不会阻止插入。`
              : "发现的问题均已按当前参数自动修复。"}
          </p>
          <ul className="mt-2 space-y-1.5 text-[10px]">
          {summaries.map((summary) => (
            <li
              className={`rounded px-2.5 py-2 ${
                summary.tone === "warning"
                  ? "bg-amber-500/12"
                  : "bg-emerald-500/10"
              }`}
              key={summary.kind}
            >
              <span className="font-medium">
                {summary.tone === "warning" ? "需处理" : "已修复"}
              </span>
              <span className="text-muted-foreground">
                {" · "}
                {summary.label}
              </span>
            </li>
          ))}
          </ul>
        </>
      )}
    </section>
  );
}

function EditorSelect({
  label,
  onChange,
  options,
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  options: Array<[string, string]>;
  value: string;
}) {
  return (
    <label className="grid min-w-0 gap-1.5 text-[10px]">
      <span className="text-muted-foreground">{label}</span>
      <select
        aria-label={label}
        className="h-9 w-full min-w-0 rounded-md border bg-background px-2.5 text-[11px]"
        onChange={(event) => onChange(event.currentTarget.value)}
        value={value}
      >
        {options.map(([optionValue, optionLabel]) => (
          <option key={optionValue} value={optionValue}>
            {optionLabel}
          </option>
        ))}
      </select>
    </label>
  );
}

function RecipeRangeInput({
  label,
  max,
  min,
  onChange,
  step,
  unit,
  value,
}: {
  label: string;
  max?: number;
  min: number;
  onChange: (value: number) => void;
  step: number;
  unit: string;
  value: number;
}) {
  const inputId = useId();
  const accessibleLabel = unit ? `${label} ${unit}` : label;
  const rangeMaximum = Math.max(max ?? value, value);
  return (
    <div
      className="min-w-0 py-1"
      data-layout="treatment-control"
    >
      <div className="flex min-w-0 items-center justify-between gap-2">
        <label
          className="min-w-0 truncate text-[9px] text-muted-foreground"
          htmlFor={inputId}
        >
          {label}
        </label>
        <span
          aria-label={`${accessibleLabel} 当前值`}
          className="shrink-0 text-[9px] tabular-nums text-muted-foreground"
        >
          {formatRangeValue(value, step)}
          {unit ? ` ${unit}` : ""}
        </span>
      </div>
      <input
        aria-label={`${accessibleLabel} 快速调节`}
        className="mt-1 h-3 w-full cursor-ew-resize accent-primary"
        max={rangeMaximum}
        min={min}
        onChange={(event) => onChange(Number(event.currentTarget.value))}
        step={step}
        type="range"
        value={Math.min(rangeMaximum, Math.max(min, value))}
      />
    </div>
  );
}

function ExactNumberInput({
  label,
  max,
  min,
  onChange,
  step,
  unit,
  value,
}: {
  label: string;
  max: number;
  min: number;
  onChange: (value: number) => void;
  step: number;
  unit: string;
  value: number;
}) {
  return (
    <label className="grid min-w-0 gap-1 text-[9px] text-muted-foreground">
      <span>{label}</span>
      <span className="flex h-8 min-w-0 items-center overflow-hidden rounded-md border bg-background">
        <input
          aria-label={`${label} ${unit}`}
          className="h-full min-w-0 flex-1 bg-transparent px-2 text-right text-[10px] tabular-nums outline-none"
          max={max}
          min={min}
          onChange={(event) => {
            const next = Number(event.currentTarget.value);
            if (Number.isFinite(next)) onChange(next);
          }}
          step={step}
          type="number"
          value={value}
        />
        <span aria-hidden="true" className="border-l px-1.5">
          {unit}
        </span>
      </span>
    </label>
  );
}

function useAssetPreviewUrl(assetId: string, disabled: boolean) {
  const [url, setUrl] = useState<string>();
  useEffect(() => {
    if (disabled) return;
    let active = true;
    let createdUrl: string | undefined;
    void getAssetBytes(assetId)
      .then((payload) => {
        if (!active) return;
        createdUrl = URL.createObjectURL(
          new Blob([new Uint8Array(payload.bytes)], {
            type: payload.mediaType,
          }),
        );
        setUrl(createdUrl);
      })
      .catch(() => {
        if (active) setUrl(undefined);
      });
    return () => {
      active = false;
      if (createdUrl) URL.revokeObjectURL(createdUrl);
    };
  }, [assetId, disabled]);
  return url;
}

function asTreatmentCrop(value: unknown): TreatmentCrop | null {
  if (!value || typeof value !== "object") return null;
  const crop = value as Partial<TreatmentCrop>;
  if (
    typeof crop.xMillionths !== "number" ||
    typeof crop.yMillionths !== "number" ||
    typeof crop.widthMillionths !== "number" ||
    typeof crop.heightMillionths !== "number"
  ) {
    return null;
  }
  return crop as TreatmentCrop;
}

function millimetresToUm(value: number) {
  return Math.max(0, Math.round(value * 1_000));
}

interface DiagnosticSummary {
  kind: string;
  label: string;
  tone: "fixed" | "warning";
}

function summarizeDiagnostics(
  diagnostics: Array<Record<string, unknown>>,
): DiagnosticSummary[] {
  const groups = new Map<string, Array<Record<string, unknown>>>();
  for (const diagnostic of diagnostics) {
    const kind =
      typeof diagnostic.kind === "string" ? diagnostic.kind : "unknown";
    groups.set(kind, [...(groups.get(kind) ?? []), diagnostic]);
  }
  return [...groups.entries()].map(([kind, entries]) => {
    const first = entries[0] ?? {};
    switch (kind) {
      case "removedSpeck":
        return {
          kind,
          label: `已移除 ${entries.length} 个直径小于 ${formatUm(first.diameterUm)} mm 的噪点`,
          tone: "fixed",
        };
      case "removedIsland":
        return {
          kind,
          label: `已移除 ${entries.length} 个小于规则的孤岛`,
          tone: "fixed",
        };
      case "featureBelowMinimumLineWidth":
        return {
          kind,
          label: `检测到 ${formatUm(first.measuredUm)} mm 线宽，低于 ${formatUm(first.minimumUm)} mm`,
          tone: "warning",
        };
      case "thickenedThinFeature":
        return {
          kind,
          label: `已将 ${formatUm(first.measuredUm)} mm 的细线加粗到 ${formatUm(first.minimumUm)} mm`,
          tone: "fixed",
        };
      case "removedThinFeature":
        return {
          kind,
          label: `已移除 ${formatUm(first.measuredUm)} mm 的细线（规则 ${formatUm(first.minimumUm)} mm）`,
          tone: "fixed",
        };
      case "gapBelowMinimum":
        return {
          kind,
          label: `存在小于 ${formatUm(first.minimumUm)} mm 的间距`,
          tone: "warning",
        };
      default:
        return {
          kind,
          label: "处理结果包含需要检查的问题",
          tone: "warning",
        };
    }
  });
}

function formatUm(value: unknown): string {
  return typeof value === "number" ? (value / 1_000).toFixed(3) : "—";
}

function formatPercent(millionths: number): string {
  return `${(millionths / 10_000).toFixed(1)}%`;
}

function formatRangeValue(value: number, step: number): string {
  const decimals = step >= 1 ? 0 : Math.min(3, `${step}`.split(".")[1]?.length ?? 0);
  return value.toFixed(decimals);
}

function updateTreatmentCrop(
  crop: TreatmentCrop,
  key: keyof TreatmentCrop,
  percent: number,
): TreatmentCrop {
  const value = Math.round(percent * 10_000);
  switch (key) {
    case "xMillionths":
      return {
        ...crop,
        xMillionths: Math.max(
          0,
          Math.min(value, 1_000_000 - crop.widthMillionths),
        ),
      };
    case "yMillionths":
      return {
        ...crop,
        yMillionths: Math.max(
          0,
          Math.min(value, 1_000_000 - crop.heightMillionths),
        ),
      };
    case "widthMillionths":
      return {
        ...crop,
        widthMillionths: Math.max(
          1_000,
          Math.min(value, 1_000_000 - crop.xMillionths),
        ),
      };
    case "heightMillionths":
      return {
        ...crop,
        heightMillionths: Math.max(
          1_000,
          Math.min(value, 1_000_000 - crop.yMillionths),
        ),
      };
  }
}

function resizeTreatmentCrop(
  crop: TreatmentCrop,
  mode: CropDragMode,
  dx: number,
  dy: number,
): TreatmentCrop {
  const minimumSize = 1_000;
  const left = crop.xMillionths;
  const top = crop.yMillionths;
  const right = left + crop.widthMillionths;
  const bottom = top + crop.heightMillionths;
  if (mode === "move") {
    return {
      ...crop,
      xMillionths: Math.max(
        0,
        Math.min(left + dx, 1_000_000 - crop.widthMillionths),
      ),
      yMillionths: Math.max(
        0,
        Math.min(top + dy, 1_000_000 - crop.heightMillionths),
      ),
    };
  }
  const nextLeft =
    mode === "northWest" || mode === "southWest"
      ? Math.max(0, Math.min(left + dx, right - minimumSize))
      : left;
  const nextRight =
    mode === "northEast" || mode === "southEast"
      ? Math.min(1_000_000, Math.max(right + dx, left + minimumSize))
      : right;
  const nextTop =
    mode === "northWest" || mode === "northEast"
      ? Math.max(0, Math.min(top + dy, bottom - minimumSize))
      : top;
  const nextBottom =
    mode === "southWest" || mode === "southEast"
      ? Math.min(1_000_000, Math.max(bottom + dy, top + minimumSize))
      : bottom;
  return {
    xMillionths: nextLeft,
    yMillionths: nextTop,
    widthMillionths: nextRight - nextLeft,
    heightMillionths: nextBottom - nextTop,
  };
}
