import { describe, expect, it } from "vitest";

import {
  detectSupportedImageType,
  isSupportedImageFileMetadata,
} from "@/features/media/supported-image-file";

describe("supported image files", () => {
  it("only advertises PNG, JPEG and WebP metadata", () => {
    expect(
      isSupportedImageFileMetadata({ name: "art.png", type: "image/png" }),
    ).toBe(true);
    expect(
      isSupportedImageFileMetadata({ name: "photo.JPG", type: "image/jpeg" }),
    ).toBe(true);
    expect(
      isSupportedImageFileMetadata({ name: "texture.webp", type: "" }),
    ).toBe(true);
    expect(
      isSupportedImageFileMetadata({
        name: "vector.svg",
        type: "image/svg+xml",
      }),
    ).toBe(false);
    expect(
      isSupportedImageFileMetadata({
        name: "document.pdf",
        type: "application/pdf",
      }),
    ).toBe(false);
  });

  it("identifies supported formats by file signature instead of trusting MIME", () => {
    expect(
      detectSupportedImageType(
        new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      ),
    ).toBe("image/png");
    expect(
      detectSupportedImageType(new Uint8Array([0xff, 0xd8, 0xff, 0xe0])),
    ).toBe("image/jpeg");
    expect(
      detectSupportedImageType(
        new Uint8Array([
          0x52, 0x49, 0x46, 0x46, 0, 0, 0, 0, 0x57, 0x45, 0x42, 0x50,
        ]),
      ),
    ).toBe("image/webp");
    expect(
      detectSupportedImageType(new TextEncoder().encode("not really a png")),
    ).toBeNull();
  });
});
