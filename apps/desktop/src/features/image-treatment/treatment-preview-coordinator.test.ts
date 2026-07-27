import { afterEach, describe, expect, it, vi } from "vitest";

import type {
  TreatmentCompileReport,
  TreatmentRecipe,
} from "@/lib/core";
import {
  fingerprintTreatmentRecipe,
  TreatmentPreviewCoordinator,
  type TreatmentPreviewAccepted,
} from "@/features/image-treatment/treatment-preview-coordinator";

const baseRecipe: TreatmentRecipe = {
  algorithmVersion: "atelier-image-treatment-v2",
  alphaMode: "compositeOnWhite",
  threshold: { mode: "manual", value: 128 },
  invert: false,
  smoothingRadiusUm: 0,
  despeckleRadiusUm: 0,
  removeIslandsBelowUm2: 0,
  minimumLineWidthUm: 0,
  thinFeaturePolicy: "preserve",
  minimumGapUm: 0,
  crop: null,
};

afterEach(() => vi.useRealTimers());

describe("TreatmentPreviewCoordinator", () => {
  it("matches the Rust recipe fingerprint contract", async () => {
    await expect(fingerprintTreatmentRecipe(baseRecipe)).resolves.toBe(
      "5abe3f2d53ee2523d2b34586760ca4e97cdccc2f1ec267534ecea05972c318cf",
    );
  });

  it("canonicalizes recipes received with transport-sorted keys", async () => {
    const transportRecipe = {
      algorithmVersion: "atelier-image-treatment-v2",
      alphaMode: "compositeOnWhite",
      crop: null,
      despeckleRadiusUm: 0,
      invert: false,
      minimumGapUm: 0,
      minimumLineWidthUm: 0,
      removeIslandsBelowUm2: 0,
      smoothingRadiusUm: 500,
      thinFeaturePolicy: "preserve",
      threshold: { mode: "otsu" },
    } satisfies TreatmentRecipe;
    await expect(fingerprintTreatmentRecipe(transportRecipe)).resolves.toBe(
      "125c4247000170252ef09a847fe1b53b76d80ad3ca3bdfa38a6632a919629156",
    );
  });

  it("coalesces recipe edits into one 100ms interactive compile", async () => {
    vi.useFakeTimers();
    const accepted: TreatmentPreviewAccepted[] = [];
    const coordinator = new TreatmentPreviewCoordinator({
      debounceMs: 100,
      fingerprint: async (recipe) =>
        `fp-${recipe.threshold.mode === "manual" ? recipe.threshold.value : "otsu"}`,
      compileInteractiveProxy: async () =>
        report("fp-180", 4),
      onAccepted: (result) => accepted.push(result),
    });

    coordinator.update(baseRecipe);
    await vi.advanceTimersByTimeAsync(50);
    coordinator.update({
      ...baseRecipe,
      threshold: { mode: "manual", value: 180 },
    });
    await vi.advanceTimersByTimeAsync(99);
    expect(accepted).toHaveLength(0);

    await vi.advanceTimersByTimeAsync(1);
    expect(accepted).toHaveLength(1);
    expect(accepted[0]?.report.revision).toBe(4);
  });

  it("can reactivate after a lifecycle cleanup", async () => {
    vi.useFakeTimers();
    const accepted: TreatmentPreviewAccepted[] = [];
    const coordinator = new TreatmentPreviewCoordinator({
      debounceMs: 100,
      fingerprint: async () => "fp-128",
      compileInteractiveProxy: async () => report("fp-128", 1),
      onAccepted: (result) => accepted.push(result),
    });

    coordinator.dispose();
    coordinator.activate();
    coordinator.update(baseRecipe);
    await vi.advanceTimersByTimeAsync(100);

    expect(accepted).toHaveLength(1);
  });

  it("rejects late results by request generation, revision and fingerprint", async () => {
    vi.useFakeTimers();
    const accepted: TreatmentPreviewAccepted[] = [];
    const compiles: Array<{
      resolve: (report: TreatmentCompileReport) => void;
    }> = [];
    const coordinator = new TreatmentPreviewCoordinator({
      debounceMs: 100,
      fingerprint: async (recipe) =>
        `fp-${recipe.threshold.mode === "manual" ? recipe.threshold.value : "otsu"}`,
      compileInteractiveProxy: () =>
        new Promise((resolve) => compiles.push({ resolve })),
      onAccepted: (result) => accepted.push(result),
    });

    coordinator.update(baseRecipe);
    await vi.advanceTimersByTimeAsync(100);
    coordinator.update({
      ...baseRecipe,
      threshold: { mode: "manual", value: 180 },
    });
    await vi.advanceTimersByTimeAsync(100);

    compiles[1]?.resolve(report("wrong-fingerprint", 7));
    await Promise.resolve();
    expect(accepted).toHaveLength(0);

    compiles[0]?.resolve(report("fp-128", 6));
    await Promise.resolve();
    expect(accepted).toHaveLength(0);

    coordinator.update({
      ...baseRecipe,
      threshold: { mode: "manual", value: 200 },
    });
    await vi.advanceTimersByTimeAsync(100);
    compiles[2]?.resolve(report("fp-200", 8));
    await Promise.resolve();
    expect(accepted.map(({ report: value }) => value.revision)).toEqual([8]);

    coordinator.update({
      ...baseRecipe,
      threshold: { mode: "manual", value: 220 },
    });
    await vi.advanceTimersByTimeAsync(100);
    compiles[3]?.resolve(report("fp-220", 7));
    await Promise.resolve();
    expect(accepted.map(({ report: value }) => value.revision)).toEqual([8]);
  });
});

function report(
  recipeFingerprint: string,
  revision: number,
): TreatmentCompileReport {
  return {
    widthPx: 120,
    heightPx: 80,
    appliedThreshold: 128,
    maskSha256: "a".repeat(64),
    previewPngDataUrl: "data:image/png;base64,AA==",
    pixelPitchUm: 250,
    recipeFingerprint,
    revision,
    purpose: "interactiveProxy",
    topology: { islandCount: 2, holeCount: 1 },
    diagnostics: [],
  };
}
