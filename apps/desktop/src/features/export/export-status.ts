import type { EasyedaExportReport } from "@/lib/core";

type ExportStatusReport = Pick<
  EasyedaExportReport,
  "exportVersion" | "nativeProjectPath" | "primitives" | "orderSupport"
>;

export function formatEasyedaExportStatus(report: ExportStatusReport) {
  const artifact = `已导出 ${report.exportVersion} · ${report.primitives.fillCount} 个图形 · ${report.nativeProjectPath}`;
  if (report.orderSupport.directOrderSupported) {
    return `${artifact} · 制造配置已验证`;
  }
  const issue = report.orderSupport.issues[0] ?? "当前制造组合无法无损交付";
  const action =
    report.orderSupport.downgradeActions[0] ?? "请调整制造配置后重新导出";
  return `${artifact} · 不可直接下单：${issue}；建议：${action}`;
}
