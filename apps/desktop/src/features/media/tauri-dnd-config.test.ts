import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("Tauri DOM drag-and-drop contract", () => {
  it("disables the native file-drop handler so WKWebView receives HTML5 drag events", () => {
    const config = JSON.parse(
      readFileSync(
        new URL("../../../src-tauri/tauri.conf.json", import.meta.url),
        "utf8",
      ),
    ) as {
      app?: {
        windows?: Array<{
          dragDropEnabled?: boolean;
        }>;
      };
    };

    expect(config.app?.windows?.[0]?.dragDropEnabled).toBe(false);
  });
});
