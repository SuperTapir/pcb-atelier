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
