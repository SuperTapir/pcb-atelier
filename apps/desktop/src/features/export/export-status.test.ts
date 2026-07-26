import { describe, expect, it } from "vitest";

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
});
