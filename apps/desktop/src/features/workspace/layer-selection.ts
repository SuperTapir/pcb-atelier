import type { ContentLayer } from "@/lib/core";

export function resolveLayerSelection({
  current,
  drillIntoGroup,
  layerId,
  layers,
  shiftKey,
}: {
  current: string[];
  drillIntoGroup: boolean;
  layerId: string;
  layers: ContentLayer[];
  shiftKey: boolean;
}) {
  const clicked = layers.find((layer) => layer.id === layerId);
  const target =
    !drillIntoGroup && clicked?.parentId ? clicked.parentId : layerId;
  if (!shiftKey) return [target];
  return current.includes(target)
    ? current.filter((id) => id !== target)
    : [...current, target];
}

export function cycleOverlappingSelection(
  candidates: string[],
  current: string[],
) {
  if (candidates.length === 0) return current;
  const index = candidates.indexOf(current.at(-1) ?? "");
  return [candidates[(index + 1) % candidates.length]];
}

export function resolveTreeLayerSelection({
  anchorId,
  current,
  layerId,
  orderedLayerIds,
  rangeKey,
  toggleKey,
}: {
  anchorId: string | null;
  current: string[];
  layerId: string;
  orderedLayerIds: string[];
  rangeKey: boolean;
  toggleKey: boolean;
}): { anchorId: string; selectedIds: string[] } {
  if (rangeKey && anchorId) {
    const anchorIndex = orderedLayerIds.indexOf(anchorId);
    const targetIndex = orderedLayerIds.indexOf(layerId);
    if (anchorIndex >= 0 && targetIndex >= 0) {
      const start = Math.min(anchorIndex, targetIndex);
      const end = Math.max(anchorIndex, targetIndex);
      return {
        anchorId,
        selectedIds: orderedLayerIds.slice(start, end + 1),
      };
    }
  }
  if (toggleKey) {
    return {
      anchorId: layerId,
      selectedIds: current.includes(layerId)
        ? current.filter((id) => id !== layerId)
        : [...current, layerId],
    };
  }
  return { anchorId: layerId, selectedIds: [layerId] };
}
