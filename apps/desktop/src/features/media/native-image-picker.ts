import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export const SUPPORTED_IMAGE_DIALOG_FILTERS = [
  {
    name: "图片（PNG、JPEG、WebP）",
    extensions: ["png", "jpg", "jpeg", "webp"],
  },
] as const;

interface NativeImageFile {
  bytes: number[];
  mediaType: string;
  name: string;
}

export function isNativeImagePickerAvailable() {
  return "__TAURI_INTERNALS__" in globalThis;
}

export async function selectSupportedImageFile(): Promise<File | null> {
  const path = await open({
    directory: false,
    filters: SUPPORTED_IMAGE_DIALOG_FILTERS.map((filter) => ({
      name: filter.name,
      extensions: [...filter.extensions],
    })),
    multiple: false,
    title: "选择要插入的图片",
  });
  if (typeof path !== "string") return null;

  const selected = await invoke<NativeImageFile>("read_image_file", { path });
  return new File([Uint8Array.from(selected.bytes)], selected.name, {
    type: selected.mediaType,
  });
}
