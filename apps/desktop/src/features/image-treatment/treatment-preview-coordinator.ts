import type {
  TreatmentCompileReport,
  TreatmentRecipe,
} from "@/lib/core";

export interface TreatmentPreviewAccepted {
  recipe: TreatmentRecipe;
  report: TreatmentCompileReport;
}

export interface TreatmentPreviewCoordinatorOptions {
  debounceMs?: number;
  persistRecipe: (recipe: TreatmentRecipe) => Promise<unknown>;
  compileInteractiveProxy: () => Promise<TreatmentCompileReport>;
  fingerprint?: (recipe: TreatmentRecipe) => Promise<string>;
  onAccepted: (result: TreatmentPreviewAccepted) => void;
  onError?: (error: unknown) => void;
}

export class TreatmentPreviewCoordinator {
  private readonly debounceMs: number;
  private readonly options: TreatmentPreviewCoordinatorOptions;
  private generation = 0;
  private latestAcceptedRevision = -1;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private disposed = false;

  constructor(options: TreatmentPreviewCoordinatorOptions) {
    this.options = options;
    this.debounceMs = options.debounceMs ?? 100;
    if (this.debounceMs < 75 || this.debounceMs > 250) {
      throw new Error("图片处理参数防抖必须在 75–250 ms 之间");
    }
  }

  update(recipe: TreatmentRecipe) {
    const generation = ++this.generation;
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.run(generation, recipe);
    }, this.debounceMs);
  }

  activate() {
    this.disposed = false;
  }

  dispose() {
    this.disposed = true;
    this.generation += 1;
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }

  private async run(generation: number, recipe: TreatmentRecipe) {
    try {
      const expectedFingerprint = await (
        this.options.fingerprint ?? fingerprintTreatmentRecipe
      )(recipe);
      if (!this.isCurrent(generation)) return;

      await this.options.persistRecipe(recipe);
      if (!this.isCurrent(generation)) return;

      const report = await this.options.compileInteractiveProxy();
      if (!this.isCurrent(generation)) return;
      if (report.purpose !== "interactiveProxy") {
        throw new Error("图片处理预览返回了错误的采样用途");
      }
      if (report.recipeFingerprint !== expectedFingerprint) {
        throw new Error("图片处理预览配方指纹与当前参数不一致");
      }
      if (report.revision <= this.latestAcceptedRevision) {
        throw new Error("图片处理预览返回了陈旧 revision");
      }

      this.latestAcceptedRevision = report.revision;
      this.options.onAccepted({ recipe, report });
    } catch (error) {
      if (this.isCurrent(generation)) this.options.onError?.(error);
    }
  }

  private isCurrent(generation: number) {
    return !this.disposed && generation === this.generation;
  }
}

export async function fingerprintTreatmentRecipe(
  recipe: TreatmentRecipe,
): Promise<string> {
  const bytes = new TextEncoder().encode(JSON.stringify(recipe));
  const digest = await globalThis.crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}
