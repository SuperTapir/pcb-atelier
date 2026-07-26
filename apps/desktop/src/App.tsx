import { useCallback, useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { WorkspaceShell } from "@/features/workspace/WorkspaceShell";
import {
  getCoreInfo,
  getWorkspaceDocument,
  type CoreInfo,
  type WorkspaceDocument,
} from "@/lib/core";

type BootstrapState =
  | { state: "loading" }
  | {
      state: "ready";
      core: CoreInfo;
      document: WorkspaceDocument;
    }
  | { state: "unavailable" };

export function App() {
  const [bootstrap, setBootstrap] = useState<BootstrapState>({
    state: "loading",
  });

  const loadWorkspace = useCallback(async () => {
    setBootstrap({ state: "loading" });
    try {
      const [core, document] = await Promise.all([
        getCoreInfo(),
        getWorkspaceDocument(),
      ]);
      setBootstrap({ state: "ready", core, document });
    } catch {
      setBootstrap({ state: "unavailable" });
    }
  }, []);

  useEffect(() => {
    void loadWorkspace();
  }, [loadWorkspace]);

  if (bootstrap.state === "ready") {
    return (
      <WorkspaceShell core={bootstrap.core} document={bootstrap.document} />
    );
  }

  return (
    <main className="flex min-h-screen items-center justify-center bg-background p-8 text-foreground">
      <section className="w-full max-w-sm rounded-xl border bg-card p-7 text-center shadow-sm">
        <div className="mx-auto flex size-10 items-center justify-center rounded-lg border bg-muted">
          <RefreshCw
            className={`size-4 ${bootstrap.state === "loading" ? "animate-spin" : ""}`}
          />
        </div>
        <h1 className="mt-4 text-lg font-semibold">PCB Atelier</h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          {bootstrap.state === "loading"
            ? "正在读取 Rust Core 工程…"
            : "无法连接 Tauri Core。请从桌面应用运行工作区。"}
        </p>
        {bootstrap.state === "unavailable" && (
          <Button className="mt-5" onClick={() => void loadWorkspace()}>
            重试
          </Button>
        )}
      </section>
    </main>
  );
}
