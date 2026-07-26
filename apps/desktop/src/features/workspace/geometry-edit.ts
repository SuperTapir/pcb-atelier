import type { TransformUm } from "@/lib/core";

export type GeometryEditErrorCode =
  | "invalidNumber"
  | "subMicrometrePrecision"
  | "subMilliDegreePrecision"
  | "nonPositiveSize"
  | "unsafeInteger"
  | "invalidTransform"
  | "layerLocked"
  | "ancestorLocked"
  | "invalidLayerHierarchy";

export class GeometryEditError extends Error {
  constructor(
    public readonly code: GeometryEditErrorCode,
    message: string,
  ) {
    super(message);
    this.name = "GeometryEditError";
  }
}

export interface GeometryLayer {
  id: string;
  name?: string;
  parentId: string | null;
  locked: boolean;
  visible?: boolean;
  transform: TransformUm;
}

export interface BoundsUm {
  leftUm: number;
  topUm: number;
  rightUm: number;
  bottomUm: number;
  centerXUm: number;
  centerYUm: number;
  widthUm: number;
  heightUm: number;
}

export type NudgeDirection = "left" | "right" | "up" | "down";
export type SnapAxis = "x" | "y";
export type SnapGuideKind =
  "grid" | "boardEdge" | "objectEdge" | "objectCenter";

export interface SnapGuide {
  axis: SnapAxis;
  kind: SnapGuideKind;
  positionUm: number;
  description: string;
  targetLayerId?: string;
}

export interface SnapTransformInput {
  layer: GeometryLayer;
  layers: readonly GeometryLayer[];
  proposedTransform: TransformUm;
  board: {
    widthUm: number;
    heightUm: number;
  };
  gridStepUm: number;
  thresholdUm: number;
  altKey: boolean;
}

export interface SnapTransformResult {
  transform: TransformUm;
  guides: SnapGuide[];
  bypassed: boolean;
}

export type TransformPatch = Partial<TransformUm>;
type AnchorName = "left" | "centerX" | "right" | "top" | "centerY" | "bottom";

interface SnapCandidate {
  axis: SnapAxis;
  deltaUm: number;
  guide: SnapGuide;
  priority: number;
}

/**
 * Parse a user-entered millimetre value without floating-point conversion.
 * Positions may be negative; sizes must be greater than zero.
 */
export function parseMillimetresToUm(
  input: string,
  field: "position" | "size",
): number {
  const value = parseDecimalThousandths(
    input,
    "subMicrometrePrecision",
    "1 µm",
  );
  if (field === "size" && value <= 0) {
    throw new GeometryEditError("nonPositiveSize", "尺寸必须大于 0 mm");
  }
  return value;
}

export function parseDegreesToMdeg(input: string): number {
  return parseDecimalThousandths(input, "subMilliDegreePrecision", "0.001°");
}

function parseDecimalThousandths(
  input: string,
  precisionCode: "subMicrometrePrecision" | "subMilliDegreePrecision",
  precisionLabel: string,
): number {
  const trimmed = input.trim();
  const match = /^([+-]?)(?:(\d+)(?:\.(\d*))?|\.(\d+))$/.exec(trimmed);
  if (!match) {
    throw new GeometryEditError(
      "invalidNumber",
      `“${input}”不是有效的毫米数值`,
    );
  }

  const sign = match[1] === "-" ? -1n : 1n;
  const whole = match[2] ?? "0";
  const fraction = match[3] ?? match[4] ?? "";
  if (fraction.length > 3 && /[1-9]/.test(fraction.slice(3))) {
    throw new GeometryEditError(
      precisionCode,
      `“${input}”包含小于 ${precisionLabel} 的精度`,
    );
  }
  const fractionUm = (fraction.slice(0, 3) + "000").slice(0, 3);
  const value = sign * (BigInt(whole) * 1_000n + BigInt(fractionUm));
  if (
    value < BigInt(Number.MIN_SAFE_INTEGER) ||
    value > BigInt(Number.MAX_SAFE_INTEGER)
  ) {
    throw new GeometryEditError(
      "unsafeInteger",
      `“${input}”超出可精确表示的微米范围`,
    );
  }
  return Number(value);
}

/**
 * Return a new exact integer transform. The source layer is never mutated.
 */
export function applyTransformPatch(
  layer: GeometryLayer,
  layers: readonly GeometryLayer[],
  patch: TransformPatch,
): TransformUm {
  assertLayerEditable(layer, layers);
  const transform = { ...layer.transform, ...patch };
  validateTransform(transform);
  return transform;
}

export function nudgeTransform(
  layer: GeometryLayer,
  layers: readonly GeometryLayer[],
  direction: NudgeDirection,
  shiftKey: boolean,
): TransformUm {
  const distanceUm = shiftKey ? 1_000 : 100;
  const delta = {
    left: { xUm: -distanceUm, yUm: 0 },
    right: { xUm: distanceUm, yUm: 0 },
    up: { xUm: 0, yUm: -distanceUm },
    down: { xUm: 0, yUm: distanceUm },
  }[direction];
  return applyTransformPatch(layer, layers, {
    xUm: layer.transform.xUm + delta.xUm,
    yUm: layer.transform.yUm + delta.yUm,
  });
}

/**
 * Calculate the physical AABB after rotation around the object's centre.
 * Flips do not change the AABB.
 */
export function axisAlignedBounds(transform: TransformUm): BoundsUm {
  validateTransform(transform);
  const centerXUm = transform.xUm + transform.widthUm / 2;
  const centerYUm = transform.yUm + transform.heightUm / 2;
  const radians = (transform.rotationMdeg / 1_000) * (Math.PI / 180);
  const halfWidthUm =
    (Math.abs(Math.cos(radians)) * transform.widthUm +
      Math.abs(Math.sin(radians)) * transform.heightUm) /
    2;
  const halfHeightUm =
    (Math.abs(Math.sin(radians)) * transform.widthUm +
      Math.abs(Math.cos(radians)) * transform.heightUm) /
    2;
  const leftUm = Math.round(centerXUm - halfWidthUm);
  const rightUm = Math.round(centerXUm + halfWidthUm);
  const topUm = Math.round(centerYUm - halfHeightUm);
  const bottomUm = Math.round(centerYUm + halfHeightUm);
  return {
    leftUm,
    topUm,
    rightUm,
    bottomUm,
    centerXUm,
    centerYUm,
    widthUm: rightUm - leftUm,
    heightUm: bottomUm - topUm,
  };
}

/**
 * Snap a proposed transform in board coordinates. At most one best guide is
 * chosen per axis so a later UI can render the returned guides directly.
 */
export function snapTransform(input: SnapTransformInput): SnapTransformResult {
  const {
    layer,
    layers,
    proposedTransform,
    board,
    gridStepUm,
    thresholdUm,
    altKey,
  } = input;
  assertLayerEditable(layer, layers);
  validateTransform(proposedTransform);
  validateNonNegativeInteger(gridStepUm, "gridStepUm");
  validateNonNegativeInteger(thresholdUm, "thresholdUm");
  if (
    !Number.isSafeInteger(board.widthUm) ||
    !Number.isSafeInteger(board.heightUm) ||
    board.widthUm <= 0 ||
    board.heightUm <= 0
  ) {
    throw new GeometryEditError("invalidTransform", "板尺寸必须是正整数微米");
  }

  if (altKey) {
    return {
      transform: { ...proposedTransform },
      guides: [],
      bypassed: true,
    };
  }

  const moving = axisAlignedBounds(proposedTransform);
  const candidates = [
    ...gridCandidates(moving, gridStepUm),
    ...boardCandidates(moving, board.widthUm, board.heightUm),
    ...objectCandidates(moving, layer.id, layers),
  ].filter(
    (candidate) =>
      Number.isSafeInteger(candidate.deltaUm) &&
      Math.abs(candidate.deltaUm) <= thresholdUm,
  );
  const x = bestCandidate(candidates, "x");
  const y = bestCandidate(candidates, "y");
  const transform = {
    ...proposedTransform,
    xUm: proposedTransform.xUm + (x?.deltaUm ?? 0),
    yUm: proposedTransform.yUm + (y?.deltaUm ?? 0),
  };
  validateTransform(transform);
  return {
    transform,
    guides: [x?.guide, y?.guide].filter(
      (guide): guide is SnapGuide => guide !== undefined,
    ),
    bypassed: false,
  };
}

export function assertLayerEditable(
  layer: GeometryLayer,
  layers: readonly GeometryLayer[],
): void {
  if (layer.locked) {
    throw new GeometryEditError("layerLocked", `图层“${layer.id}”已锁定`);
  }
  const byId = new Map(layers.map((candidate) => [candidate.id, candidate]));
  const visited = new Set([layer.id]);
  let parentId = layer.parentId;
  while (parentId !== null) {
    if (visited.has(parentId)) {
      throw new GeometryEditError(
        "invalidLayerHierarchy",
        `图层“${layer.id}”的祖先关系包含循环`,
      );
    }
    visited.add(parentId);
    const parent = byId.get(parentId);
    if (!parent) {
      throw new GeometryEditError(
        "invalidLayerHierarchy",
        `找不到图层“${layer.id}”的父级“${parentId}”`,
      );
    }
    if (parent.locked) {
      throw new GeometryEditError(
        "ancestorLocked",
        `图层“${layer.id}”的祖先“${parent.id}”已锁定`,
      );
    }
    parentId = parent.parentId;
  }
}

export function isLayerTransformEditable(
  layer: GeometryLayer,
  layers: readonly GeometryLayer[],
): boolean {
  try {
    assertLayerEditable(layer, layers);
    return true;
  } catch {
    return false;
  }
}

function validateTransform(transform: TransformUm): void {
  for (const [field, value] of [
    ["xUm", transform.xUm],
    ["yUm", transform.yUm],
    ["widthUm", transform.widthUm],
    ["heightUm", transform.heightUm],
    ["rotationMdeg", transform.rotationMdeg],
  ] as const) {
    if (!Number.isSafeInteger(value)) {
      throw new GeometryEditError(
        "invalidTransform",
        `${field} 必须是可精确表示的整数`,
      );
    }
  }
  if (transform.widthUm <= 0 || transform.heightUm <= 0) {
    throw new GeometryEditError("invalidTransform", "对象尺寸必须大于零");
  }
}

function validateNonNegativeInteger(value: number, field: string): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new GeometryEditError(
      "invalidTransform",
      `${field} 必须是非负整数微米`,
    );
  }
}

function gridCandidates(bounds: BoundsUm, gridStepUm: number): SnapCandidate[] {
  if (gridStepUm === 0) return [];
  return [
    ["x", "left", bounds.leftUm],
    ["x", "centerX", bounds.centerXUm],
    ["x", "right", bounds.rightUm],
    ["y", "top", bounds.topUm],
    ["y", "centerY", bounds.centerYUm],
    ["y", "bottom", bounds.bottomUm],
  ].map(([axis, _anchor, position]) => {
    const target = Math.round(Number(position) / gridStepUm) * gridStepUm;
    return {
      axis: axis as SnapAxis,
      deltaUm: target - Number(position),
      priority: 3,
      guide: {
        axis: axis as SnapAxis,
        kind: "grid" as const,
        positionUm: target,
        description: `${axis === "x" ? "X" : "Y"} 对齐到 ${formatMm(target)} mm 网格`,
      },
    };
  });
}

function boardCandidates(
  bounds: BoundsUm,
  boardWidthUm: number,
  boardHeightUm: number,
): SnapCandidate[] {
  return [
    candidate("x", bounds.leftUm, 0, "boardEdge", "左边缘对齐到板左边", 0),
    candidate(
      "x",
      bounds.rightUm,
      boardWidthUm,
      "boardEdge",
      "右边缘对齐到板右边",
      0,
    ),
    candidate("y", bounds.topUm, 0, "boardEdge", "上边缘对齐到板上边", 0),
    candidate(
      "y",
      bounds.bottomUm,
      boardHeightUm,
      "boardEdge",
      "下边缘对齐到板下边",
      0,
    ),
  ];
}

function objectCandidates(
  moving: BoundsUm,
  movingLayerId: string,
  layers: readonly GeometryLayer[],
): SnapCandidate[] {
  return layers
    .filter(
      (target) =>
        target.id !== movingLayerId &&
        target.visible !== false &&
        target.transform.widthUm > 0 &&
        target.transform.heightUm > 0,
    )
    .flatMap((target) => {
      const bounds = axisAlignedBounds(target.transform);
      return [
        ...edgeCandidates(
          "x",
          [
            ["left", moving.leftUm],
            ["right", moving.rightUm],
          ],
          [
            ["left", bounds.leftUm],
            ["right", bounds.rightUm],
          ],
          target.id,
          target.name ?? target.id,
        ),
        ...edgeCandidates(
          "y",
          [
            ["top", moving.topUm],
            ["bottom", moving.bottomUm],
          ],
          [
            ["top", bounds.topUm],
            ["bottom", bounds.bottomUm],
          ],
          target.id,
          target.name ?? target.id,
        ),
        candidate(
          "x",
          moving.centerXUm,
          bounds.centerXUm,
          "objectCenter",
          `水平中心对齐到「${target.name ?? target.id}」水平中心`,
          2,
          target.id,
        ),
        candidate(
          "y",
          moving.centerYUm,
          bounds.centerYUm,
          "objectCenter",
          `垂直中心对齐到「${target.name ?? target.id}」垂直中心`,
          2,
          target.id,
        ),
      ];
    });
}

function edgeCandidates(
  axis: SnapAxis,
  moving: Array<[AnchorName, number]>,
  targets: Array<[AnchorName, number]>,
  targetLayerId: string,
  targetLabel: string,
): SnapCandidate[] {
  return moving.flatMap(([movingName, movingPosition]) =>
    targets.map(([targetName, targetPosition]) =>
      candidate(
        axis,
        movingPosition,
        targetPosition,
        "objectEdge",
        `${anchorLabel(movingName)}对齐到「${targetLabel}」${anchorLabel(targetName)}`,
        1,
        targetLayerId,
      ),
    ),
  );
}

function candidate(
  axis: SnapAxis,
  movingPosition: number,
  targetPosition: number,
  kind: SnapGuideKind,
  description: string,
  priority: number,
  targetLayerId?: string,
): SnapCandidate {
  return {
    axis,
    deltaUm: targetPosition - movingPosition,
    priority,
    guide: {
      axis,
      kind,
      positionUm: targetPosition,
      description,
      ...(targetLayerId === undefined ? {} : { targetLayerId }),
    },
  };
}

function bestCandidate(
  candidates: readonly SnapCandidate[],
  axis: SnapAxis,
): SnapCandidate | undefined {
  return candidates
    .filter((candidate) => candidate.axis === axis)
    .sort(
      (left, right) =>
        Math.abs(left.deltaUm) - Math.abs(right.deltaUm) ||
        left.priority - right.priority ||
        left.guide.positionUm - right.guide.positionUm,
    )[0];
}

function anchorLabel(anchor: AnchorName): string {
  return {
    left: "左边缘",
    right: "右边缘",
    top: "上边缘",
    bottom: "下边缘",
    centerX: "水平中心",
    centerY: "垂直中心",
  }[anchor];
}

function formatMm(valueUm: number): string {
  return (valueUm / 1_000).toFixed(3);
}
