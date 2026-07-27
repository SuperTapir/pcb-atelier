## 1. 建立可重复回归基准

- [x] 1.1 为 Tauri open/save dialog capability、菜单取消和错误边界增加先失败的前端测试
- [x] 1.2 为 `.pcba` 首次保存、覆盖保存、另存为后重新打开增加领域或集成回环测试
- [x] 1.3 从用户 EasyEDA Pro 基准提取无私有素材的 `IMAGE`、`STRING`、底层锚点与非对称坐标 fixture
- [x] 1.4 为 `Kamome` 六字符完整性、底层方向、单图元聚合及复杂曲线阶梯指标增加先失败的 Rust 测试

## 2. 修复 PCB Atelier 工程文件生命周期

- [x] 2.1 补齐 Tauri 保存对话框 capability，并让生成的 capability schema 与配置一致
- [x] 2.2 将打开、首次保存和另存为的 dialog 调用纳入统一错误边界，先关闭工程菜单并正确处理取消
- [x] 2.3 验证有效 `.pcba` 打开、修改后覆盖保存、另存为、取消和损坏工程失败均保持原子状态

## 3. 统一文字编辑与预览语义

- [x] 3.1 让 Workspace 文字代理从实际目标生产映射派生物理外观，移除固定文字颜色
- [x] 3.2 自动验证背面 `Kamome` 在编辑代理、生产 mask 和 3D 纹理中的字符、位置与正向设计坐标一致

## 4. 输出 EasyEDA 原生图片和文字图元

- [x] 4.1 为 handoff 增加同时接收已校验文档和同 revision `ResolvedFabricationBoard` 的 core API，并让 GUI/CLI 共用
- [x] 4.2 对单一加法图片生成一个聚合路径的原生 `IMAGE`，保留正式 mask 哈希、来源追溯和物理包围盒
- [x] 4.3 对受支持的单一加法文字生成一个完整原生 `STRING`，严格保留 Unicode 内容并校验正式 mask 包围盒
- [x] 4.4 对多个来源、Subtract 和不支持的字体/变换保留单一聚合 `FILL` 回退或明确报错，并在 manifest 记录 strategy/reason

## 5. 修正底层坐标和轮廓质量

- [x] 5.1 根据 EasyEDA Pro fixture 实现 primitive-specific 单次底层坐标转换，移除重复路径镜像或镜像属性
- [x] 5.2 恢复 25 µm 正式 production pitch，避免以全局超采样掩盖导出轮廓问题
- [x] 5.3 实现强到弱的确定性轮廓候选，并验证偏差、无自交、绕向、岛屿、孔洞归属和包围盒
- [x] 5.4 聚合同一图片或回退层的全部环，报告精确阶梯边界回退，并增加性能上限测试

## 6. 端到端验证与交付

- [x] 6.1 运行前端单测、typecheck、build、Rust workspace tests、format 和 OpenSpec 严格校验
- [x] 6.2 从用户提供的 `未命名标准艺术卡.pcba` 重新导出版本化 `.eprj2`，检查 SQLite/JSON 结构、图层、图元数量、`Kamome` 和包围盒
- [x] 6.3 在 EasyEDA Pro 中检查新工程的正背 2D、3D 文字完整性和放大轮廓，与 baseline `.eprj2/.epro2` 对比并记录结果
- [x] 6.4 审查 diff、更新已完成任务，仅提交自动与人工验收均通过的修复

验收记录：使用嘉立创 EDA 专业版 V3.2.166 打开
`pcb-atelier-eeab3a6d9242-v0012.eprj2`。隔离底层丝印层后可见完整六字符
`Kamome`；顶面 2D 按底层制造语义镜像，与 baseline 一致；3D 翻至背面后文字
顺序正常且无缺字。正面图片与背面阻焊图在适配视图中轮廓连续，导出报告中各图层
`exactContourFallbacks` 均为 `0`。
