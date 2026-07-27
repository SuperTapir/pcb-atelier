import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ManufacturerInspector } from "@/features/manufacturer/ManufacturerInspector";
import type { ManufacturerProfileSnapshot } from "@/lib/core";

const profile: ManufacturerProfileSnapshot = {
  manufacturerId: "jlcpcb",
  profileVersion: "jlcpcb-fr4-art-v2026.04",
  sourceUpdatedAt: "2026-04-14",
  sourceUrls: [],
  substrate: "fr4",
  layerCount: 2,
  thicknessUm: 1_600,
  outerCopper: "oz1",
  solderMask: "white",
  characterProcess: "standardBlack",
  surfaceFinish: "enig",
};

describe("ManufacturerInspector", () => {
  it("exposes only the appearance choices that belong in the card inspector", () => {
    const markup = renderToStaticMarkup(
      <ManufacturerInspector onChange={() => undefined} profile={profile} />,
    );

    expect(markup).toContain("板面工艺");
    expect(markup).toContain('aria-label="阻焊油墨"');
    expect(markup).toContain('aria-label="露铜表面处理"');
    expect(markup).not.toContain("制造参数");
    expect(markup).not.toContain("FR-4（当前固定）");
    expect(markup).not.toContain('aria-label="铜层数"');
    expect(markup).not.toContain('aria-label="板厚"');
    expect(markup).not.toContain('aria-label="外层铜厚"');
    expect(markup).not.toContain('aria-label="字符工艺"');
  });

  it("keeps manufacturing caveats next to the two visible appearance controls", () => {
    const markup = renderToStaticMarkup(
      <ManufacturerInspector onChange={() => undefined} profile={profile} />,
    );

    expect(markup).toContain("OSP（仅铜基板，FR-4 不支持）");
    expect(markup).toMatch(/<option[^>]*disabled=""[^>]*value="osp"/);
    expect(markup).toContain("屏幕近似色不代表批次实物色差保证");
  });
});
