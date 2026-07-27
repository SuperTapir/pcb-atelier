import type {
  ImageTreatment,
  TreatmentCompileReport,
  TreatmentRecipe,
} from "@/lib/core";

export interface ImageProxySubscription<T> {
  value: Promise<T>;
  release: () => void;
}

interface BrokerEntry<T> {
  value: Promise<T>;
  references: number;
  estimatedBytes: number;
  lastUsed: number;
}

export class ImageProxyBroker<T> {
  private readonly entries = new Map<string, BrokerEntry<T>>();
  private residentBytes = 0;
  private clock = 0;

  constructor(
    private readonly budgetBytes: number,
    private readonly estimateBytes: (value: T) => number,
  ) {
    if (budgetBytes <= 0) throw new Error("代理缓存字节预算必须为正数");
  }

  acquire(key: string, load: () => Promise<T>): ImageProxySubscription<T> {
    let entry = this.entries.get(key);
    if (!entry) {
      entry = {
        value: Promise.resolve().then(load),
        references: 0,
        estimatedBytes: 0,
        lastUsed: ++this.clock,
      };
      this.entries.set(key, entry);
      const ownedEntry = entry;
      entry.value = entry.value
        .then((value) => {
          if (this.entries.get(key) === ownedEntry) {
            ownedEntry.estimatedBytes = this.estimateBytes(value);
            this.residentBytes += ownedEntry.estimatedBytes;
            this.evict();
          }
          return value;
        })
        .catch((error) => {
          if (this.entries.get(key) === ownedEntry) {
            this.entries.delete(key);
          }
          throw error;
        });
    }
    entry.references += 1;
    entry.lastUsed = ++this.clock;
    let released = false;
    return {
      value: entry.value,
      release: () => {
        if (released) return;
        released = true;
        entry.references = Math.max(0, entry.references - 1);
        entry.lastUsed = ++this.clock;
        this.evict();
      },
    };
  }

  snapshot() {
    return {
      entries: this.entries.size,
      residentBytes: this.residentBytes,
      activeReferences: [...this.entries.values()].reduce(
        (total, entry) => total + entry.references,
        0,
      ),
    };
  }

  clearUnused() {
    for (const [key, entry] of this.entries) {
      if (entry.references === 0) this.remove(key, entry);
    }
  }

  private evict() {
    while (this.residentBytes > this.budgetBytes) {
      const candidate = [...this.entries.entries()]
        .filter(([, entry]) => entry.references === 0)
        .sort((left, right) => left[1].lastUsed - right[1].lastUsed)[0];
      if (!candidate) return;
      this.remove(candidate[0], candidate[1]);
    }
  }

  private remove(key: string, entry: BrokerEntry<T>) {
    if (this.entries.get(key) !== entry) return;
    this.entries.delete(key);
    this.residentBytes = Math.max(
      0,
      this.residentBytes - entry.estimatedBytes,
    );
  }
}

export const treatmentProxyBroker = new ImageProxyBroker<TreatmentCompileReport>(
  64 * 1024 * 1024,
  (report) =>
    Math.ceil(
      report.previewPngDataUrl.replace(/^data:image\/png;base64,/, "").length *
        0.75,
    ),
);

export function getImageProxyKey(
  treatment: ImageTreatment,
  widthUm: number,
  heightUm: number,
  pixelPitchUm: number,
  recipe: TreatmentRecipe = treatment.recipe,
) {
  return JSON.stringify([
    treatment.id,
    treatment.assetId,
    treatment.productionMode,
    recipe,
    widthUm,
    heightUm,
    pixelPitchUm,
  ]);
}
