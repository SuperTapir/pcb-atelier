export type CommandSource =
  | "shortcut"
  | "menu"
  | "toolbar"
  | "canvas"
  | "context-menu"
  | "layer-menu"
  | "inspector";

export type EditorCommandScope = "application" | "canvas" | "selection";

export interface EditorCommand<Context> {
  id: string;
  title: string;
  scope: EditorCommandScope;
  shortcuts?: string[];
  isEnabled?: (context: Context) => boolean;
  execute: (
    context: Context,
    invocation: { source: CommandSource },
  ) => void | Promise<void>;
}

export interface ShortcutEvent {
  key: string;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
}

export interface EditorCommandRegistry<Context> {
  execute(
    id: string,
    context: Context,
    source: CommandSource,
  ): Promise<boolean>;
  get(id: string): EditorCommand<Context> | null;
  isEnabled(id: string, context: Context): boolean;
  resolveShortcut(
    event: ShortcutEvent,
    activeScopes?: readonly EditorCommandScope[],
  ): string | null;
}

export function createCommandRegistry<Context>(
  commands: EditorCommand<Context>[],
): EditorCommandRegistry<Context> {
  const byId = new Map<string, EditorCommand<Context>>();
  const byShortcut = new Map<string, string>();
  for (const command of commands) {
    if (byId.has(command.id)) {
      throw new Error(`duplicate editor command id: ${command.id}`);
    }
    byId.set(command.id, command);
    for (const shortcut of command.shortcuts ?? []) {
      const normalized = normalizeShortcut(shortcut);
      if (byShortcut.has(normalized)) {
        throw new Error(`duplicate editor shortcut: ${shortcut}`);
      }
      byShortcut.set(normalized, command.id);
    }
  }

  const get = (id: string) => byId.get(id) ?? null;
  const isEnabled = (id: string, context: Context) => {
    const command = get(id);
    return Boolean(command && (command.isEnabled?.(context) ?? true));
  };

  return {
    get,
    isEnabled,
    resolveShortcut(event, activeScopes) {
      const id = byShortcut.get(shortcutFromEvent(event));
      if (!id) return null;
      const command = get(id);
      if (
        !command ||
        (activeScopes && !activeScopes.includes(command.scope))
      ) {
        return null;
      }
      return id;
    },
    async execute(id, context, source) {
      const command = get(id);
      if (!command || !(command.isEnabled?.(context) ?? true)) return false;
      await command.execute(context, { source });
      return true;
    },
  };
}

export function isEditorShortcutSuppressed(
  target:
    | EventTarget
    | {
        tagName?: string;
        isContentEditable?: boolean;
        closest?: (selector: string) => unknown;
      }
    | null,
  modalOpen: boolean,
): boolean {
  if (modalOpen) return true;
  if (!target || typeof target !== "object") return false;
  const element = target as {
    tagName?: string;
    isContentEditable?: boolean;
    closest?: (selector: string) => unknown;
  };
  const editingAncestor = element.closest?.(
    '[contenteditable="true"], [contenteditable=""], [role="textbox"], [role="spinbutton"], [role="combobox"]',
  );
  return (
    element.isContentEditable === true ||
    Boolean(editingAncestor) ||
    ["INPUT", "TEXTAREA", "SELECT"].includes(
      element.tagName?.toUpperCase() ?? "",
    )
  );
}

function normalizeShortcut(shortcut: string): string {
  const parts = shortcut.split("+");
  const key = parts.pop()?.toLowerCase() ?? "";
  const modifiers = new Set(parts.map((part) => part.toLowerCase()));
  return [
    modifiers.has("mod") ? "mod" : "",
    modifiers.has("alt") ? "alt" : "",
    modifiers.has("shift") ? "shift" : "",
    key,
  ]
    .filter(Boolean)
    .join("+");
}

function shortcutFromEvent(event: ShortcutEvent): string {
  return [
    event.metaKey || event.ctrlKey ? "mod" : "",
    event.altKey ? "alt" : "",
    event.shiftKey ? "shift" : "",
    event.key.toLowerCase(),
  ]
    .filter(Boolean)
    .join("+");
}
