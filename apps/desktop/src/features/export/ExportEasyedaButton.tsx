import { Download, ExternalLink, FolderOpen, LoaderCircle } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  exportEasyeda,
  openEasyedaProject,
  revealExportedProject,
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
  const [lastExport, setLastExport] = useState<EasyedaExportReport | null>(
    null,
  );

  return (
    <div className="flex items-center gap-1">
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
              setLastExport(report);
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
      {lastExport && (
        <>
          <Button
            aria-label="使用嘉立创 EDA 打开导出工程"
            onClick={() =>
              void openEasyedaProject(lastExport.nativeProjectPath)
                .then(() => onStatus("已交给系统打开嘉立创 EDA 工程"))
                .catch((error) =>
                  onStatus(`打开 EDA 失败：${errorMessage(error)}`),
                )
            }
            size="icon"
            title="使用嘉立创 EDA 打开"
            variant="ghost"
          >
            <ExternalLink className="size-4" />
          </Button>
          <Button
            aria-label="在 Finder 中显示导出工程"
            onClick={() =>
              void revealExportedProject(lastExport.nativeProjectPath)
                .then(() => onStatus("已在 Finder 中显示导出工程"))
                .catch((error) =>
                  onStatus(`显示导出文件失败：${errorMessage(error)}`),
                )
            }
            size="icon"
            title="在 Finder 中显示"
            variant="ghost"
          >
            <FolderOpen className="size-4" />
          </Button>
        </>
      )}
    </div>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
