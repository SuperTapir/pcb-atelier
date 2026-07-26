interface PointMm {
  x: number;
  y: number;
}

export interface TextDraft {
  layout: "autoWidth" | "fixedFrame";
  xUm: number;
  yUm: number;
  widthUm: number;
  heightUm: number;
}

const CLICK_THRESHOLD_MM = 2;
const DEFAULT_POINT_TEXT_WIDTH_UM = 20_000;
const DEFAULT_POINT_TEXT_HEIGHT_UM = 6_000;

export function createTextDraft(start: PointMm, end: PointMm): TextDraft {
  const deltaX = end.x - start.x;
  const deltaY = end.y - start.y;
  if (
    Math.abs(deltaX) < CLICK_THRESHOLD_MM &&
    Math.abs(deltaY) < CLICK_THRESHOLD_MM
  ) {
    return {
      layout: "autoWidth",
      xUm: mmToUm(start.x),
      yUm: mmToUm(start.y),
      widthUm: DEFAULT_POINT_TEXT_WIDTH_UM,
      heightUm: DEFAULT_POINT_TEXT_HEIGHT_UM,
    };
  }

  const left = Math.min(start.x, end.x);
  const top = Math.min(start.y, end.y);
  return {
    layout: "fixedFrame",
    xUm: mmToUm(left),
    yUm: mmToUm(top),
    widthUm: Math.max(1_000, mmToUm(Math.abs(deltaX))),
    heightUm: Math.max(1_000, mmToUm(Math.abs(deltaY))),
  };
}

function mmToUm(value: number) {
  return Math.round(value * 1_000);
}
