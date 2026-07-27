## 1. 基线与失败契约

- [ ] 1.1 固化 `interactive-image-v1` profile 与 1003×1568 真实 PNG/JPEG fixture，记录 fixture SHA、51.17×80 mm、250 µm warm 代理、应用/硬件/系统/供电、输出尺寸、采样数和计时边界，并用脚本测量当前源字节、JSON 尺寸、解码/编译次数和预览延迟
- [ ] 1.2 先增加失败的 bridge 契约测试：注册源图后连续预览只传句柄和配方，取消会话释放临时资源且不修改 document
- [ ] 1.3 先增加失败的调度测试：60 个连续 generation 最终只接受最新结果，每对象至多一个运行任务和一个最新 pending，运行任务能在处理检查点取消
- [ ] 1.4 先增加失败的前端测试：滑块中间值不调用 `setTreatmentRecipe`，确认只提交一个命令，取消保持 revision 和 undo history 不变
- [ ] 1.5 先增加失败的画布/代理测试：检查器与画布相同键只编译一次，拖动、缩放和旋转期间处理与正式编译计数均为零

## 2. Rust 图片准备与缓存

- [ ] 2.1 在独立 `atelier-core` 中拆出 `PreparedImage` 与 `prepare_image`，缓存方向归一像素和灰度表示，并让现有正式入口复用相同内部处理语义
- [ ] 2.2 实现对 prepared source 的交互代理编译，证明与现有 bytes 正式入口在阈值、极性、裁切、物理清理、方向和配方指纹上等价
- [ ] 2.3 为灰度/缩放层和编码代理实现按估算字节预算的 LRU 与引用生命周期，覆盖回收、重建和不同物理尺寸/精度键隔离
- [ ] 2.4 在耗时处理阶段和固定行块加入与 UI 无关的协作取消探针，验证取消不改变未取消任务的确定性结果

## 3. Workspace 交互会话协议

- [ ] 3.1 为 Tauri 与 Web bridge 实现 `begin_image_preview_source`、按 `sourceHandle` 请求代理和 `release_image_preview_source`，源字节只在 begin 阶段出现
- [ ] 3.2 让已嵌入项目资产可以直接按 `assetId` 建立 preview session，不经前端重新读取或传输资产字节
- [ ] 3.3 将运行时会话接入 prepared-image 缓存、引用计数、空闲超时和工程关闭清理；活跃会话固定 prepared source 并计入预算，确认临时 handle、prepared source 和代理不会写入 `.pcba`
- [ ] 3.4 以 `(workspaceId, previewStreamId)` 扩展处理调度器为“一个运行 + 一个最新 pending”，把 generation、配方指纹与 workspace revision 一起用于取消和陈旧写回防护，并验证同源不同实例隔离、相同代理键仍可合并
- [ ] 3.5 增加只读诊断计数快照，覆盖源字节、prepare、代理编译、coalesce、cancel、active 与 pending；确保生产 bundle 不暴露可修改工程的测试 IPC

## 4. Draft 编辑与共享代理

- [ ] 4.1 重构 `TreatmentPreviewCoordinator` 为 read-only draft preview：完整 recipe 随请求传入，预览前不得执行领域持久化
- [ ] 4.2 将 `ImageImportDialog` 改为打开时注册一次源图、关闭时释放，会话中保留上一帧并只接受最新 generation
- [ ] 4.3 将已存在图片检查器改为本地 draft，滑块 `pointerup` 或数值 `change/blur` 仅提交相对本次手势开始状态的一个最终领域命令；关闭检查器保留已完成手势，导入弹窗取消仍回滚全部 draft
- [ ] 4.4 实现 `ImageProxyBroker`，以稳定缓存键合并检查器与画布订阅，复用编码结果和 `ImageBitmap`/图片纹理，并用引用计数安全释放
- [ ] 4.5 将画布自适应代理迁移到 broker：参数输入期间使用稳定旧纹理，输入空闲后才按 250/100/50/25 µm 离散档位升级，并断言 400% 下代理像素不超过一个 CSS 像素
- [ ] 4.6 删除固定 128 项无字节上限的旧代理 Promise 缓存和重复预览调用，保留失败重试与内存压力下可重建行为

## 5. 交互与生产一致性验证

- [ ] 5.1 增加浏览器 E2E，覆盖完整原图/结果实时对照、快速滑动不闪白、导入取消无副作用、常驻检查器每手势单次撤销、显式重新 Otsu 固化手动值以及临时查看原图不改生产结果
- [ ] 5.2 增加 Tauri 发布构建手势回归：后台重形态学任务运行时拖动图片，验证只变换现有纹理、零同步 IPC、零图片编译和 16.7 ms p95 帧目标
- [ ] 5.3 按 `interactive-image-v1` 增加真实图片连续 60 次输入和至少 30 个非缓存阈值/反相结果基准，验证注册后请求不含源图、最终 generation 正确、输入处理低于 16.7 ms p95 且纹理可绘制反馈达到 120 ms p95 基准目标
- [ ] 5.4 用非对称、孔洞、孤岛、细线和裁切 fixture 比较交互代理、正式 mask、3D 与 EasyEDA 输出；验证可表达结构的指纹、方向、极性、拓扑和边界误差，细于两个代理像素的结构只影响粗代理可见性，并断言 draft 不进入 3D、同 revision 的 2D/3D 只解析一次且哈希相同
- [ ] 5.5 运行 Rust workspace、前端单测、typecheck、production build、完整 Playwright、Tauri benchmark、OpenSpec strict 和 `git diff --check`
- [ ] 5.6 删除旧的完整字节重复预览接口与兼容回退，记录 PNG 编码/base64/客户端解码占比及首轮传输决策，更新性能基线和架构文档，确认当前未发布 schema 不保留迁移分支
