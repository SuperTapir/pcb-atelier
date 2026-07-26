import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ProjectHome } from "@/features/workspace/WorkspaceShell";

describe("ProjectHome", () => {
  it("separates project lifecycle from the editing and export toolbar", () => {
    const markup = renderToStaticMarkup(
      <ProjectHome onNew={() => undefined} onOpen={() => undefined} />,
    );

    expect(markup).toContain("新建工程");
    expect(markup).toContain("打开工程");
    expect(markup).toContain(".pcba");
    expect(markup).not.toContain("导出 EDA");
    expect(markup).not.toContain("跟随系统");
  });
});
