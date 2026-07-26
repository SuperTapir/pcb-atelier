# PCB Atelier 首个端到端验收报告

验收日期：2026-07-26

## 结论

`add-card-editor-foundation` 本轮范围通过。当前应用已经形成“生产层中直接创作 → 同一
Core 编译六层生产结果 → 编辑态 2D 检查 → 只读 3D 成板预览 → 嘉立创 EDA 单向导出”
的完整链路。Web 与 Tauri 使用同一版本化 `WorkspaceService`，没有测试 IPC 或硬编码
预览旁路。

## 自动化验证

| 验证项 | 结果 |
| --- | --- |
| Rust 格式 | `cargo fmt --all -- --check` 通过 |
| Rust 工作区 | `cargo test --workspace` 全部通过 |
| Rust 静态检查 | `cargo clippy --workspace --all-targets -- -D warnings` 通过 |
| 前端单元测试 | Vitest 13 个文件、48 个测试通过 |
| 浏览器端到端 | Playwright 14 个测试通过 |
| 生产包隔离 | production bundle 不含 E2E IPC、fixture 标记或硬编码测试预览 |
| 文档契约 | `scripts/check-doc-contract.sh` 通过 |
| OpenSpec | `openspec validate add-card-editor-foundation --strict` 通过 |

Playwright 覆盖正背双画板、活动卡面、独立选择、聚焦恢复、编辑/预览正交、文字和真实
PNG 插入、生产层自动映射、多层关联、唯一基础铺铜、分组/解组、锁定、精确变换、吸附、
板框修改和越界诊断。图片插入后再次取得 3D 预览，证明预览消费当前工程的真实
`ResolvedFabricationBoard`。

## GUI、CLI 与共享服务等价性

Rust 契约测试对同一工程分别走桌面会话、Core 直接编译和 CLI
`production-inspect`，确认 `fabricationInputSha256` 与
`fabricationOutputSha256` 一致。另一个版本化服务测试在同一文档上依次执行图片插入、
分组、解组和六层预览，验证 Web bridge 与 Tauri 命令适配器共享同一语义。

## 性能基准

双 Konva Stage、两百对象、六张生产层纹理的连续平移/缩放基准结果：

| 运行面 | 平均 FPS | P95 帧时 | 慢帧 | 非活动画板绘制 | 内存 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Chrome | 71.39 | 26.1 ms | 13.25% | 0 / 1104 | heap -708 B |
| Tauri release | 52.54 | 18.0 ms | 0.68% | 6 / 1110（0.54%） | RSS +16 KiB |

两个运行面均高于 45 FPS 门槛，重复切面和显隐后没有持续内存增长。

## Tauri 实际交互

使用当前源码重新构建
`target/debug/bundle/macos/PCB Atelier.app`，而不是复用旧应用包。实际确认：

- 顶层只有“编辑 / 预览”；
- 左树为“板体 → 正面/背面 → 铜层/阻焊开窗/丝印层”，没有独立内容入口；
- 正背画板同时显示，选择背面生产层后检查器切换到背面；
- 板体检查器可见宽、高、圆角、板厚、阻焊颜色、表面处理及固定 FR-4；
- 3D 预览读取真实编译结果，拖动可旋转并观察板厚；
- 原生“导出 EDA”目录选择和导出成功，生成 `.eprj2`、`.epro2` 与 manifest。

截图：

- [Tauri 编辑器](tauri-editor.png)
- [Tauri 3D 预览](tauri-3d-preview.png)

原生导出：

- [原生工程](pcb-atelier-3abe7809d199-v0001.eprj2)
- [公开归档](pcb-atelier-3abe7809d199-v0001.epro2)
- [追溯清单](pcb-atelier-3abe7809d199-v0001.manifest.json)

该次空板导出的生产输出哈希为
`3abe7809d199f817454ba5876ad40d3e6a8f001f5ae8e32eb877d2fbaa3ed41b`，
原生结构回读有效。

## 嘉立创 EDA 黄金工程

非对称黄金卡片已在嘉立创 EDA 专业版 V3.2.166 人工打开，确认 `64 × 100 mm` 板框、
六层、阻焊开窗极性、正反 `F/B` 方向及静态可编辑生产图元。详细记录见
[嘉立创 EDA 人工验收](easyeda/MANUAL_ACCEPTANCE.md)。

## 后续体验项

当前 3D 默认斜视角较近，完整板框需要滚轮缩小；旋转、缩放、平移和生产方向均正确。
后续可单独实现基于相机投影和容器宽高的 `fit-to-board`，不影响本轮领域、预览或导出
链路的验收结论。
