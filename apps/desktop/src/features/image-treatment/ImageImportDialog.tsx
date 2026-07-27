import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { ImageTreatmentEditor } from "@/features/image-treatment/ImageTreatmentEditor";
import {
  beginImagePreviewSource,
  confirmImageImport,
  releaseImagePreviewSource,
  requestImagePreview,
  type ConfirmedImageImport,
  type ImageProductionMode,
  type ProductionLayer,
  type TreatmentCompileReport,
  type TreatmentRecipe,
} from "@/lib/core";

export interface ImageImportDraft {
  draftId: string;
  side: "front" | "back";
  productionLayer: ProductionLayer;
  originalFilename: string;
  mediaType: string;
  pixelWidth: number;
  pixelHeight: number;
  bytes: number[];
  physicalWidthUm: number;
  physicalHeightUm: number;
  placementCenterUm?: { xUm: number; yUm: number };
}

export interface ImageImportDialogProps {
  draft: ImageImportDraft;
  colorOriginalAvailable: boolean;
  colorOriginalUnavailableReason?: string;
  onCancel: () => void;
  onConfirmed: (result: ConfirmedImageImport) => void;
  onEnableColorOriginal?: () => Promise<void>;
  onError: (error: unknown) => void;
}

export const DEFAULT_IMAGE_TREATMENT_RECIPE: TreatmentRecipe = {
  algorithmVersion: "atelier-image-treatment-v2",
  alphaMode: "compositeOnWhite",
  threshold: { mode: "otsu" },
  invert: false,
  smoothingRadiusUm: 0,
  despeckleRadiusUm: 0,
  removeIslandsBelowUm2: 0,
  minimumLineWidthUm: 0,
  thinFeaturePolicy: "preserve",
  minimumGapUm: 0,
  crop: null,
};

export function ImageImportDialog({
  colorOriginalAvailable,
  colorOriginalUnavailableReason,
  draft,
  onCancel,
  onConfirmed,
  onEnableColorOriginal,
  onError,
}: ImageImportDialogProps) {
  const [recipe, setRecipe] = useState(DEFAULT_IMAGE_TREATMENT_RECIPE);
  const [productionMode, setProductionMode] =
    useState<ImageProductionMode>("monochromeMask");
  const [report, setReport] = useState<TreatmentCompileReport | null>(null);
  const [confirming, setConfirming] = useState(false);
  const compileRecipe = useRef(recipe);
  const previewGeneration = useRef(0);
  const previewSourceRef = useRef<ReturnType<
    typeof beginImagePreviewSource
  > | null>(null);
  const previewSourceUsers = useRef(0);
  const [previewSource, setPreviewSource] = useState<ReturnType<
    typeof beginImagePreviewSource
  > | null>(null);
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  const originalPreviewUrl = useMemo(
    () =>
      URL.createObjectURL(
        new Blob([new Uint8Array(draft.bytes)], { type: draft.mediaType }),
      ),
    [draft.bytes, draft.mediaType],
  );

  useEffect(
    () => () => URL.revokeObjectURL(originalPreviewUrl),
    [originalPreviewUrl],
  );
  useEffect(() => {
    previewSourceUsers.current += 1;
    const source =
      previewSourceRef.current ??
      beginImagePreviewSource({
        bytes: draft.bytes,
        mediaType: draft.mediaType,
      });
    previewSourceRef.current = source;
    setPreviewSource(source);
    return () => {
      previewSourceUsers.current -= 1;
      queueMicrotask(() => {
        if (
          previewSourceUsers.current === 0 &&
          previewSourceRef.current === source
        ) {
          previewSourceRef.current = null;
          void source
            .then(({ sourceHandle }) =>
              releaseImagePreviewSource(sourceHandle),
            )
            .catch(() => undefined);
        }
      });
    };
  }, [draft.bytes, draft.mediaType]);

  const compileDraft = useCallback(
    (nextRecipe = compileRecipe.current) =>
      previewSource
        ? previewSource.then((source) =>
        requestImagePreview({
        sourceHandle: source.sourceHandle,
        previewStreamId: `import:${draft.draftId}`,
        generation: ++previewGeneration.current,
        workspaceRevision: source.workspaceRevision,
        recipe: nextRecipe,
        physicalWidthUm: draft.physicalWidthUm,
        physicalHeightUm: draft.physicalHeightUm,
        pixelPitchUm: 250,
      }))
        : Promise.reject(new Error("图片预览源尚未注册")),
    [draft.draftId, draft.physicalHeightUm, draft.physicalWidthUm, previewSource],
  );

  useEffect(() => {
    if (!previewSource) return;
    let active = true;
    const generation = previewGeneration.current + 1;
    void compileDraft()
      .then((nextReport) => {
        if (active && previewGeneration.current === generation) {
          setReport(nextReport);
        }
      })
      .catch((error) => {
        if (active && previewGeneration.current === generation) {
          onErrorRef.current(error);
        }
      });
    return () => {
      active = false;
    };
  }, [compileDraft, draft.draftId, previewSource]);

  const persistDraftRecipe = useCallback(async (nextRecipe: TreatmentRecipe) => {
    compileRecipe.current = nextRecipe;
  }, []);
  const acceptPreview = useCallback(
    ({
      recipe: acceptedRecipe,
      report: nextReport,
    }: {
      recipe: TreatmentRecipe;
      report: TreatmentCompileReport;
    }) => {
      setRecipe(acceptedRecipe);
      setReport(nextReport);
    },
    [],
  );
  const changeRecipe = useCallback((nextRecipe: TreatmentRecipe) => {
    previewGeneration.current += 1;
    compileRecipe.current = nextRecipe;
    setRecipe(nextRecipe);
  }, []);

  return (
    <div
      aria-label="图片导入处理器"
      aria-modal="true"
      className="fixed inset-0 z-50 grid place-items-center bg-black/55 p-4 sm:p-6"
      role="dialog"
    >
      <div
        className="max-h-[92vh] w-full max-w-6xl overflow-hidden rounded-xl border bg-card p-4 shadow-2xl sm:p-5"
        data-scroll-policy="dialog-no-scroll"
      >
        <h2 className="mb-4 text-sm font-semibold">导入前处理图片</h2>
        <ImageTreatmentEditor
          asset={{
            id: draft.draftId,
            embeddedPath: "",
            originalFilename: draft.originalFilename,
            mediaType: draft.mediaType,
            sha256: "",
            pixelWidth: draft.pixelWidth,
            pixelHeight: draft.pixelHeight,
            folderPath: null,
            tags: [],
            hasAlpha: draft.mediaType === "image/png",
          }}
          colorOriginalAvailable={colorOriginalAvailable}
          colorOriginalUnavailableReason={colorOriginalUnavailableReason}
          compileInteractiveProxy={compileDraft}
          compileReport={report}
          onCancel={onCancel}
          onCompileAccepted={acceptPreview}
          onConfirm={(confirmedRecipe) => {
            if (confirming || !report) return;
            setConfirming(true);
            void confirmImageImport({
              side: draft.side,
              layer: draft.productionLayer,
              originalFilename: draft.originalFilename,
              mediaType: draft.mediaType,
              pixelWidth: draft.pixelWidth,
              pixelHeight: draft.pixelHeight,
              bytes: draft.bytes,
              recipe: confirmedRecipe,
              productionMode,
              placementCenterUm: draft.placementCenterUm,
            })
              .then(onConfirmed)
              .catch(onError)
              .finally(() => setConfirming(false));
          }}
          onError={onError}
          onEnableColorOriginal={
            onEnableColorOriginal
              ? () => {
                  void onEnableColorOriginal()
                    .then(() => setProductionMode("colorOriginal"))
                    .catch(onError);
                }
              : undefined
          }
          onRecipeChange={changeRecipe}
          onProductionModeChange={setProductionMode}
          originalPreviewUrl={originalPreviewUrl}
          persistRecipe={persistDraftRecipe}
          physicalHeightUm={draft.physicalHeightUm}
          physicalWidthUm={draft.physicalWidthUm}
          resultPreviewUrl={report?.previewPngDataUrl}
          showAssetHeader={false}
          treatment={{
            id: draft.draftId,
            assetId: draft.draftId,
            productionMode,
            recipe,
          }}
        />
      </div>
    </div>
  );
}

export function fitImportedImageSize(
  board: { widthUm: number; heightUm: number },
  image: { width: number; height: number },
) {
  const maxWidth = Math.floor((board.widthUm * 4) / 5);
  const maxHeight = Math.floor((board.heightUm * 4) / 5);
  if (maxWidth * image.height <= maxHeight * image.width) {
    return {
      widthUm: maxWidth,
      heightUm: Math.max(1, Math.floor((maxWidth * image.height) / image.width)),
    };
  }
  return {
    widthUm: Math.max(1, Math.floor((maxHeight * image.width) / image.height)),
    heightUm: maxHeight,
  };
}
