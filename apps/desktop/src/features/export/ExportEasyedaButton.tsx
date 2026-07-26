import { Download, LoaderCircle } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  exportEasyeda,
  selectEasyedaOutputDirectory,
  type EasyedaExportReport,
} from "@/lib/core";

interface ExportEasyedaButtonProps {
  onExported?: (report: EasyedaExportReport) => void;
  onStatus: (status: string) => void;
}

export function ExportEasyedaButton({
  onExported,
  onStatus,
}: ExportEasyedaButtonProps) {
  const [busy, setBusy] = useState(false);

  return (
    <Button
      data-testid="export-easyeda"
      disabled={busy}
      onClick={() =>
        void (async () => {
          setBusy(true);
          try {
            const outputDirectory = await selectEasyedaOutputDirectory();
            if (!outputDirectory) {
              onStatus("已取消导出");
              return;
            }
            onStatus("正在编译生产层并导出嘉立创 EDA…");
            const report = await exportEasyeda(outputDirectory);
            onExported?.(report);
            onStatus(
              `已导出 ${report.exportVersion} · ${report.primitives.fillCount} 个图形 · ${report.nativeProjectPath}`,
            );
          } catch (error) {
            onStatus(`导出失败：${errorMessage(error)}`);
          } finally {
            setBusy(false);
          }
        })()
      }
    >
      {busy ? (
        <LoaderCircle className="size-4 animate-spin" />
      ) : (
        <Download className="size-4" />
      )}
      {busy ? "正在导出" : "导出 EDA"}
    </Button>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
