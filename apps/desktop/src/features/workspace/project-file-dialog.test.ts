import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("PCB Atelier project file dialogs", () => {
  it("grants both open and save dialogs to the main desktop window", () => {
    const capability = JSON.parse(
      readFileSync(
        new URL("../../../src-tauri/capabilities/default.json", import.meta.url),
        "utf8",
      ),
    ) as { permissions?: string[] };

    expect(capability.permissions).toEqual(
      expect.arrayContaining(["dialog:allow-open", "dialog:allow-save"]),
    );
  });
});
