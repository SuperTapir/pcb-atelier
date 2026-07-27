import { describe, expect, it, vi } from "vitest";

import {
  createCommandRegistry,
  isEditorShortcutSuppressed,
  type EditorCommand,
} from "@/features/workspace/command-registry";

interface Context {
  canGroup: boolean;
}

describe("editor command registry", () => {
  it("shares enablement and execution across UI entry points", async () => {
    const execute = vi.fn();
    const commands: EditorCommand<Context>[] = [
      {
        id: "selection.group",
        title: "分组",
        scope: "selection",
        shortcuts: ["Mod+G"],
        isEnabled: (context) => context.canGroup,
        execute,
      },
    ];
    const registry = createCommandRegistry(commands);

    expect(registry.isEnabled("selection.group", { canGroup: false })).toBe(
      false,
    );
    expect(
      await registry.execute("selection.group", { canGroup: false }, "toolbar"),
    ).toBe(false);
    expect(execute).not.toHaveBeenCalled();

    expect(
      await registry.execute("selection.group", { canGroup: true }, "shortcut"),
    ).toBe(true);
    expect(execute).toHaveBeenCalledWith(
      { canGroup: true },
      { source: "shortcut" },
    );
  });

  it("resolves platform shortcuts without stealing modified single keys", () => {
    const registry = createCommandRegistry<Context>([
      {
        id: "tool.select",
        title: "选择",
        scope: "canvas",
        shortcuts: ["V"],
        execute: vi.fn(),
      },
      {
        id: "history.undo",
        title: "撤销",
        scope: "application",
        shortcuts: ["Mod+Z"],
        execute: vi.fn(),
      },
    ]);

    expect(
      registry.resolveShortcut({
        key: "v",
        altKey: false,
        ctrlKey: false,
        metaKey: false,
        shiftKey: false,
      }, ["canvas"]),
    ).toBe("tool.select");
    expect(
      registry.resolveShortcut(
        {
          key: "v",
          altKey: false,
          ctrlKey: false,
          metaKey: false,
          shiftKey: false,
        },
        ["application"],
      ),
    ).toBeNull();
    expect(
      registry.resolveShortcut({
        key: "v",
        altKey: false,
        ctrlKey: false,
        metaKey: true,
        shiftKey: false,
      }, ["application", "canvas"]),
    ).toBeNull();
    expect(
      registry.resolveShortcut({
        key: "z",
        altKey: false,
        ctrlKey: false,
        metaKey: true,
        shiftKey: false,
      }, ["application"]),
    ).toBe("history.undo");
  });

  it("keeps scope metadata and resolves normal and accelerated nudges", () => {
    const registry = createCommandRegistry<Context>([
      {
        id: "selection.nudge-left",
        title: "向左微调",
        scope: "selection",
        shortcuts: ["ArrowLeft", "Shift+ArrowLeft"],
        execute: vi.fn(),
      },
    ]);

    expect(registry.get("selection.nudge-left")?.scope).toBe("selection");
    expect(
      registry.resolveShortcut(
        {
          key: "ArrowLeft",
          altKey: false,
          ctrlKey: false,
          metaKey: false,
          shiftKey: false,
        },
        ["selection"],
      ),
    ).toBe("selection.nudge-left");
    expect(
      registry.resolveShortcut(
        {
          key: "ArrowLeft",
          altKey: false,
          ctrlKey: false,
          metaKey: false,
          shiftKey: true,
        },
        ["selection"],
      ),
    ).toBe("selection.nudge-left");
  });

  it("suppresses canvas shortcuts in editing controls and modal windows", () => {
    const input = { tagName: "INPUT", isContentEditable: false };
    const editable = { tagName: "DIV", isContentEditable: true };
    const canvas = { tagName: "DIV", isContentEditable: false };
    const nestedInEditor = {
      tagName: "SPAN",
      isContentEditable: false,
      closest: (selector: string) =>
        selector.includes("contenteditable") ? {} : null,
    };
    const ariaTextbox = {
      tagName: "DIV",
      isContentEditable: false,
      closest: (selector: string) =>
        selector.includes('[role="textbox"]') ? {} : null,
    };

    expect(isEditorShortcutSuppressed(input, false)).toBe(true);
    expect(isEditorShortcutSuppressed(editable, false)).toBe(true);
    expect(isEditorShortcutSuppressed(nestedInEditor, false)).toBe(true);
    expect(isEditorShortcutSuppressed(ariaTextbox, false)).toBe(true);
    expect(isEditorShortcutSuppressed(canvas, true)).toBe(true);
    expect(isEditorShortcutSuppressed(canvas, false)).toBe(false);
  });
});
