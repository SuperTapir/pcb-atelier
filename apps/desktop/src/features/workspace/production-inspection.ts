import type {
  CardFace,
  WorkContext,
} from "@/features/workspace/workspace-state";

export type InspectableProductionLayer = WorkContext;

export interface ProductionInspectionLayerState {
  visible: boolean;
  isolated: boolean;
}

export type ProductionInspectionState = Record<
  CardFace,
  Record<InspectableProductionLayer, ProductionInspectionLayerState>
>;

const LAYERS: InspectableProductionLayer[] = [
  "copper",
  "solderMaskOpen",
  "silkscreen",
];

export function createProductionInspectionState(): ProductionInspectionState {
  return {
    front: createFaceState(),
    back: createFaceState(),
  };
}

export function toggleProductionVisibility(
  state: ProductionInspectionState,
  face: CardFace,
  layer: InspectableProductionLayer,
): ProductionInspectionState {
  return {
    ...state,
    [face]: {
      ...state[face],
      [layer]: {
        ...state[face][layer],
        visible: !state[face][layer].visible,
      },
    },
  };
}

export function toggleProductionIsolation(
  state: ProductionInspectionState,
  face: CardFace,
  layer: InspectableProductionLayer,
): ProductionInspectionState {
  const nextIsolated = !state[face][layer].isolated;
  return {
    ...state,
    [face]: Object.fromEntries(
      LAYERS.map((candidate) => [
        candidate,
        {
          ...state[face][candidate],
          isolated: nextIsolated && candidate === layer,
        },
      ]),
    ) as ProductionInspectionState[CardFace],
  };
}

export function isProductionLayerRendered(
  faceState: ProductionInspectionState[CardFace],
  layer: InspectableProductionLayer,
) {
  const isolated = LAYERS.find((candidate) => faceState[candidate].isolated);
  return (
    faceState[layer].visible && (isolated === undefined || isolated === layer)
  );
}

function createFaceState(): ProductionInspectionState[CardFace] {
  return {
    copper: { visible: true, isolated: false },
    solderMaskOpen: { visible: true, isolated: false },
    silkscreen: { visible: true, isolated: false },
  };
}
