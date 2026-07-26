export function getTextRenderFrame(
  layout: "autoWidth" | "fixedFrame",
  width: number,
  height: number,
) {
  if (layout === "autoWidth") {
    return { wrap: "none" as const };
  }

  return {
    width,
    height,
    wrap: "word" as const,
  };
}
