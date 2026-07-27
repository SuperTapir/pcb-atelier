import { WORKSPACE_PANEL_WIDTH_LIMITS } from "@/features/settings/app-settings";

export { WORKSPACE_PANEL_WIDTH_LIMITS };

export type WorkspacePanelSide = "left" | "right";

const MIN_CANVAS_WIDTH = 520;
const RESIZE_HANDLE_BUDGET = 12;

export function resizeWorkspacePanelWidth(
  side: WorkspacePanelSide,
  startWidth: number,
  deltaX: number,
  otherPanelWidth: number,
  viewportWidth: number,
) {
  const limits = WORKSPACE_PANEL_WIDTH_LIMITS[side];
  const desiredWidth =
    side === "left" ? startWidth + deltaX : startWidth - deltaX;
  const availableMaximum =
    viewportWidth -
    otherPanelWidth -
    MIN_CANVAS_WIDTH -
    RESIZE_HANDLE_BUDGET;
  const maximum = Math.max(
    limits.min,
    Math.min(limits.max, availableMaximum),
  );
  return Math.round(Math.min(maximum, Math.max(limits.min, desiredWidth)));
}
