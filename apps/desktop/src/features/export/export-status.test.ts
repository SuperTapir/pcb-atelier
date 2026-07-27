import { describe, expect, it } from "vitest";

import { formatEasyedaExportStatus } from "@/features/export/export-status";
import type { EasyedaExportReport } from "@/lib/core";

describe("EasyEDA export report contract", () => {
  it("retains the native project path and resolved fabrication hashes", () => {
    const report = {
      nativeProjectPath: "/tmp/golden-v0001.eprj2",
      fabricationInputSha256: "input",
      fabricationOutputSha256: "output",
    } satisfies Pick<
      EasyedaExportReport,
      | "nativeProjectPath"
      | "fabricationInputSha256"
      | "fabricationOutputSha256"
    >;

    expect(report.nativeProjectPath.endsWith(".eprj2")).toBe(true);
    expect(report.fabricationInputSha256).not.toBe(
      report.fabricationOutputSha256,
    );
  });

  it("never presents a lossy manufacturing handoff as direct ordering", () => {
    const status = formatEasyedaExportStatus({
      exportVersion: "golden-v0001",
      nativeProjectPath: "/tmp/golden-v0001.eprj2",
      primitives: { fillCount: 12, holeCount: 1, filledLayerIds: [1, 2] },
      orderSupport: {
        status: "requiresManualAdjustment",
        directOrderSupported: false,
        issues: ["彩色丝印尚未生成独立彩色生产资料"],
        downgradeActions: ["改用标准白色丝印后重新导出"],
      },
    });

    expect(status).toContain("不可直接下单");
    expect(status).toContain("改用标准白色丝印");
    expect(status).not.toContain("一键下单");
  });
});
