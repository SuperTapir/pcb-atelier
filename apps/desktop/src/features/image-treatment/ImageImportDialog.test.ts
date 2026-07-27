import { describe, expect, it } from "vitest";

import {
  DEFAULT_IMAGE_TREATMENT_RECIPE,
  fitImportedImageSize,
} from "@/features/image-treatment/ImageImportDialog";

describe("image import draft", () => {
  it("starts with the current v2 non-destructive recipe", () => {
    expect(DEFAULT_IMAGE_TREATMENT_RECIPE).toMatchObject({
      algorithmVersion: "atelier-image-treatment-v2",
      threshold: { mode: "otsu" },
      despeckleRadiusUm: 0,
      thinFeaturePolicy: "preserve",
    });
  });

  it("uses the same 80% contain sizing contract as atomic confirmation", () => {
    expect(
      fitImportedImageSize(
        { widthUm: 64_000, heightUm: 100_000 },
        { width: 1_200, height: 800 },
      ),
    ).toEqual({ widthUm: 51_200, heightUm: 34_133 });
    expect(
      fitImportedImageSize(
        { widthUm: 64_000, heightUm: 100_000 },
        { width: 400, height: 1_000 },
      ),
    ).toEqual({ widthUm: 32_000, heightUm: 80_000 });
  });
});
