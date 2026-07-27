import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ProjectHome } from "@/features/workspace/WorkspaceShell";

describe("ProjectHome", () => {
  it("offers working creation presets without inventing unsupported recents", () => {
    const markup = renderToStaticMarkup(
      <ProjectHome
        onNew={() => undefined}
        onOpen={() => undefined}
        onOpenSettings={() => undefined}
      />,
    );

    expect(markup).toContain("新建工程");
    expect(markup).toContain("打开工程");
    expect(markup).toContain("标准艺术卡");
    expect(markup).toContain("64 × 100 mm");
    expect(markup).toContain("标准扑克牌");
    expect(markup).toContain("63 × 88 mm");
    expect(markup).toContain("自定义尺寸");
    expect(markup).toContain('name="widthMm"');
    expect(markup).toContain('name="heightMm"');
    expect(markup).toContain("设置");
    expect(markup).toContain(".pcba");
    expect(markup.match(/data-brand-icon="true"/g)).toHaveLength(1);
    expect(markup).not.toMatch(/>PA</);
    expect(markup).not.toContain("最近工程");
    expect(markup).not.toContain("导出 EDA");
    expect(markup).not.toContain("跟随系统");
  });

  it("keeps the primary actions identifiable to assistive technology", () => {
    const markup = renderToStaticMarkup(
      <ProjectHome
        onNew={() => undefined}
        onOpen={() => undefined}
        onOpenSettings={() => undefined}
      />,
    );

    expect(markup).toContain('aria-label="新建标准艺术卡工程"');
    expect(markup).toContain('aria-label="打开 PCB Atelier 工程"');
    expect(markup).toContain('aria-label="打开设置"');
  });

  it("draws every preset on one physical scale so their proportions differ", () => {
    const markup = renderToStaticMarkup(
      <ProjectHome
        onNew={() => undefined}
        onOpen={() => undefined}
        onOpenSettings={() => undefined}
      />,
    );

    expect(markup.match(/viewBox="0 0 120 120"/g)).toHaveLength(3);
    expect(markup).toContain('data-preview-size="64x100"');
    expect(markup).toContain('data-preview-size="63x88"');
    expect(markup).toContain('data-preview-size="80x80"');
    expect(markup).toMatch(/<rect[^>]*height="100"[^>]*width="64"/);
    expect(markup).toMatch(/<rect[^>]*height="80"[^>]*width="80"/);
  });
});
