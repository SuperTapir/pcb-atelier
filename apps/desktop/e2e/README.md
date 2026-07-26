# 桌面工作区浏览器 E2E

Playwright 使用正式 `App`、`WorkspaceShell` 和 `WorkspaceCanvas`。Web 运行面通过
本地 `workspace-bridge` 调用与 Tauri 相同的 Rust `WorkspaceService`，不存在独立的
JavaScript 测试 IPC 或硬编码预览 fixture。

E2E 会同时启动 Vite 与 Rust bridge：

```bash
npm run dev:e2e
```

普通 `npm run dev`、`npm run build` 和 Tauri build 都不会 fallback 到 fixture。
`npm run test:e2e` 会先构建 production bundle，并扫描产物，发现 fixture 标记、fixture
标题或测试 IPC 函数名即失败，然后才运行 Playwright：

```bash
npm run test:e2e
```

当前 E2E 覆盖：

- 正反双画板同时显示、活动卡面、独立选择与聚焦恢复；
- 编辑/预览正交、真实 `ResolvedFabricationBoard` 3D 预览；
- 当前生产层插入文字与图片、跨生产层关联、唯一基础铺铜；
- Web 共享服务的分组/解组；
- 精确变换、键盘微调、吸附、锁定和板框越界诊断。
