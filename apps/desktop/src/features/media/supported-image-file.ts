export const SUPPORTED_IMAGE_FILE_ACCEPT =
  ".png,.jpg,.jpeg,.webp,image/png,image/jpeg,image/webp";

const SUPPORTED_MEDIA_TYPES = new Set([
  "image/png",
  "image/jpeg",
  "image/webp",
]);
const SUPPORTED_EXTENSIONS = new Set(["png", "jpg", "jpeg", "webp"]);

interface ImageFileMetadata {
  name: string;
  type: string;
}

export function isSupportedImageFileMetadata(file: ImageFileMetadata) {
  const mediaType = file.type.trim().toLowerCase();
  const extension = file.name.split(".").pop()?.toLowerCase();
  const hasExtension = file.name.includes(".");

  if (mediaType && mediaType !== "application/octet-stream") {
    if (!SUPPORTED_MEDIA_TYPES.has(mediaType)) return false;
  }
  if (hasExtension && !SUPPORTED_EXTENSIONS.has(extension ?? "")) return false;
  return (
    SUPPORTED_MEDIA_TYPES.has(mediaType) ||
    SUPPORTED_EXTENSIONS.has(extension ?? "")
  );
}

export function detectSupportedImageType(bytes: Uint8Array): string | null {
  if (
    bytes.length >= 8 &&
    bytes[0] === 0x89 &&
    bytes[1] === 0x50 &&
    bytes[2] === 0x4e &&
    bytes[3] === 0x47 &&
    bytes[4] === 0x0d &&
    bytes[5] === 0x0a &&
    bytes[6] === 0x1a &&
    bytes[7] === 0x0a
  ) {
    return "image/png";
  }
  if (
    bytes.length >= 3 &&
    bytes[0] === 0xff &&
    bytes[1] === 0xd8 &&
    bytes[2] === 0xff
  ) {
    return "image/jpeg";
  }
  if (
    bytes.length >= 12 &&
    ascii(bytes, 0, 4) === "RIFF" &&
    ascii(bytes, 8, 12) === "WEBP"
  ) {
    return "image/webp";
  }
  return null;
}

export async function readSupportedImageFile(file: File) {
  if (!isSupportedImageFileMetadata(file)) {
    throw new Error("仅支持 PNG、JPEG 或 WebP 图片");
  }
  const bytes = new Uint8Array(await file.arrayBuffer());
  const mediaType = detectSupportedImageType(bytes);
  if (!mediaType) {
    throw new Error("文件内容不是有效的 PNG、JPEG 或 WebP 图片");
  }
  return { bytes: Array.from(bytes), mediaType };
}

function ascii(bytes: Uint8Array, start: number, end: number) {
  return String.fromCharCode(...bytes.slice(start, end));
}
