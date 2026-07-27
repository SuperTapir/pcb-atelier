export function getTextRenderFrame(
  layout: "autoWidth" | "fixedFrame",
  width: number,
  height: number,
) {
  if (layout === "autoWidth") {
    return {
      align: "center" as const,
      width,
      height,
      verticalAlign: "middle" as const,
      wrap: "none" as const,
    };
  }

  return {
    align: "center" as const,
    width,
    height,
    verticalAlign: "middle" as const,
    wrap: "word" as const,
  };
}
