export type WorkspaceTool = "select" | "text" | "image";
export type CardFace = "front" | "back";
export type WorkspaceMode = "edit" | "preview";
export type EditLayout = "both" | "focus";
export type BoardArrangement = "auto" | "horizontal" | "vertical";
export type WorkContext = "copper" | "solderMaskOpen" | "silkscreen";

export interface CanvasViewport {
  zoom: number;
  panX: number;
  panY: number;
}

export interface WorkspaceState {
  workspaceMode: WorkspaceMode;
  editLayout: EditLayout;
  boardArrangement: BoardArrangement;
  activeFace: CardFace;
  tool: WorkspaceTool;
  inspectorTarget: "board" | "face";
  workContexts: Record<CardFace, WorkContext>;
  selections: Record<CardFace, string[]>;
  viewports: Record<CardFace, CanvasViewport>;
}

export type WorkspaceAction =
  | { type: "setWorkspaceMode"; workspaceMode: WorkspaceMode }
  | { type: "setEditLayout"; editLayout: EditLayout }
  | { type: "setBoardArrangement"; boardArrangement: BoardArrangement }
  | { type: "setFace"; face: CardFace }
  | { type: "setTool"; tool: WorkspaceTool }
  | { type: "selectBoard" }
  | { type: "setWorkContext"; face: CardFace; workContext: WorkContext }
  | { type: "setSelection"; face: CardFace; layerIds: string[] }
  | {
      type: "setViewport";
      face: CardFace;
      viewport: CanvasViewport;
    }
  | { type: "resetViewport"; face: CardFace };

const DEFAULT_VIEWPORT: CanvasViewport = {
  zoom: 1,
  panX: 0,
  panY: 0,
};

export const MIN_ZOOM = 0.25;
export const MAX_ZOOM = 4;

export function createInitialWorkspaceState(): WorkspaceState {
  return {
    workspaceMode: "edit",
    editLayout: "both",
    boardArrangement: "auto",
    activeFace: "front",
    tool: "select",
    inspectorTarget: "face",
    workContexts: {
      front: "silkscreen",
      back: "silkscreen",
    },
    selections: {
      front: [],
      back: [],
    },
    viewports: {
      front: { ...DEFAULT_VIEWPORT },
      back: { ...DEFAULT_VIEWPORT },
    },
  };
}

export function workspaceReducer(
  state: WorkspaceState,
  action: WorkspaceAction,
): WorkspaceState {
  switch (action.type) {
    case "setWorkspaceMode":
      return { ...state, workspaceMode: action.workspaceMode };
    case "setEditLayout":
      return { ...state, editLayout: action.editLayout };
    case "setBoardArrangement":
      return { ...state, boardArrangement: action.boardArrangement };
    case "setFace":
      return { ...state, activeFace: action.face, inspectorTarget: "face" };
    case "setTool":
      return { ...state, tool: action.tool };
    case "selectBoard":
      return { ...state, inspectorTarget: "board" };
    case "setWorkContext":
      return {
        ...state,
        activeFace: action.face,
        inspectorTarget: "face",
        workContexts: {
          ...state.workContexts,
          [action.face]: action.workContext,
        },
      };
    case "setSelection":
      return {
        ...state,
        activeFace: action.face,
        inspectorTarget: "face",
        selections: {
          ...state.selections,
          [action.face]: [...action.layerIds],
        },
      };
    case "setViewport":
      return {
        ...state,
        viewports: {
          ...state.viewports,
          [action.face]: {
            ...action.viewport,
            zoom: clamp(action.viewport.zoom, MIN_ZOOM, MAX_ZOOM),
          },
        },
      };
    case "resetViewport":
      return {
        ...state,
        viewports: {
          ...state.viewports,
          [action.face]: { ...DEFAULT_VIEWPORT },
        },
      };
  }
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}
