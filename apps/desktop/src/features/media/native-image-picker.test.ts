import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, open } = vi.hoisted(() => ({
  invoke: vi.fn(),
  open: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  SUPPORTED_IMAGE_DIALOG_FILTERS,
  selectSupportedImageFile,
} from "@/features/media/native-image-picker";

describe("native image picker", () => {
  beforeEach(() => {
    Object.defineProperty(globalThis, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    open.mockReset();
    invoke.mockReset();
  });

  afterEach(() => {
    Reflect.deleteProperty(globalThis, "__TAURI_INTERNALS__");
  });

  it("opens a native dialog that only advertises supported image extensions", async () => {
    open.mockResolvedValue(null);

    await selectSupportedImageFile();

    expect(open).toHaveBeenCalledWith(
      expect.objectContaining({
        directory: false,
        multiple: false,
        filters: SUPPORTED_IMAGE_DIALOG_FILTERS,
      }),
    );
    expect(SUPPORTED_IMAGE_DIALOG_FILTERS).toEqual([
      {
        name: "图片（PNG、JPEG、WebP）",
        extensions: ["png", "jpg", "jpeg", "webp"],
      },
    ]);
  });

  it("turns a backend-validated native file into the same File contract as web input", async () => {
    open.mockResolvedValue("/tmp/art.png");
    invoke.mockResolvedValue({
      bytes: [0x89, 0x50, 0x4e, 0x47],
      mediaType: "image/png",
      name: "art.png",
    });

    const file = await selectSupportedImageFile();

    expect(invoke).toHaveBeenCalledWith("read_image_file", {
      path: "/tmp/art.png",
    });
    expect(file).toBeInstanceOf(File);
    expect(file?.name).toBe("art.png");
    expect(file?.type).toBe("image/png");
    expect(Array.from(new Uint8Array(await file!.arrayBuffer()))).toEqual([
      0x89, 0x50, 0x4e, 0x47,
    ]);
  });
});
