## Context

目前工程文件生命周期、编辑代理和 EasyEDA handoff 分布在 Tauri capability、React
workspace、Rust core 与 CLI 多条链路中。用户提供的对照样例揭示了三个独立但相关的问题：

1. Tauri 默认 capability 缺少保存对话框授权，且 React 命令在进入 `try/catch` 前调用
   dialog，导致权限错误表现为“点击没有反应”。
2. 编辑画布对文字使用固定前景色，没有读取其生产映射；因此文字虽然存在，编辑态与最终
   材质外观不一致。
3. 当前 handoff 只接收 `ResolvedFabricationBoard`，把所有来源展平为 `FILL`。文字被
   拆成字符轮廓，图片像素边界在复杂环上因安全检查回退而保留阶梯。底层内容又容易在路径
   预镜像、图元镜像属性和 EasyEDA 底层查看语义之间发生二次转换。

EasyEDA Pro 原生基准 `.epro2` 中，相同前后图片使用两个 `IMAGE`，共享同一局部路径且
`mirror: false`；背面 `Kamome` 使用单个 `STRING`，完整保存文本，`mirror: false`。
这说明适配器应保留可验证的原生 primitive 语义，而不是继续为所有内容选择最底层的
静态轮廓。

约束：

- `ResolvedFabricationBoard` 和正式生产 mask 继续作为制造几何真相。
- Rust core 必须独立于 Tauri/React；GUI 和 CLI 使用同一 handoff operation。
- `.pcba` schema 不变；EasyEDA 工程是单向下游副本。
- 轮廓质量不能依赖把全局正式生产 pitch 从 25 µm 降低到 20 µm，否则只会显著增加编译
  和导出成本，仍无法消除被展平后的阶梯语义。

## Goals / Non-Goals

**Goals:**

- 让 `.pcba` 打开、保存、另存为在桌面端形成可观察、可恢复、可回环验证的闭环。
- 让文字编辑代理遵循实际生产层外观，并保持正向设计语义。
- 对单一加法图片和文字输出 EasyEDA 原生 `IMAGE` / `STRING`，保证字符完整、对象数量
  可控、方向与原生基准一致。
- 对聚合轮廓实施有界误差的渐进平滑，自动验证拓扑与最大偏差。
- 对复杂组合保留安全、可追溯的聚合 `FILL` 回退。

**Non-Goals:**

- 不导入或解析任意第三方 EasyEDA 工程为 PCB Atelier 可编辑文档。
- 不改变生产 mask、图片处理配方或 `.pcba` schema。
- 不承诺 EasyEDA 私有渲染器的像素级材质一致。
- 不在本变更中实现任意字体文件嵌入、完整 PCB 电路图元或所有 EasyEDA primitive。

## Decisions

### 1. 文件生命周期错误边界覆盖 dialog 与领域操作

工程菜单先关闭，再在同一个 `try/catch/finally` 内调用系统 dialog 和 workspace service。
Tauri capability 同时声明 open/save 权限。取消由 `null` 路径表示，不算错误；权限、
I/O、校验错误进入现有可见错误状态。保存仍由 Rust 领域服务执行原子替换。

备选方案是让菜单保持打开并等待 dialog 返回；这会造成焦点层和原生窗口竞争，也无法解决
capability 缺失，因此不采用。

### 2. 编辑文字颜色从目标生产映射派生

Workspace canvas 使用与图片/生产层相同的 mapping-aware 外观选择函数。它只改变编辑
代理颜色，不重新栅格化文字，也不改变内容、字体、变换或生产 mask。

备选方案是所有文字固定为白色；这无法表达铜、阻焊开窗和丝印的差异，也继续破坏编辑到
预览的一致性。

### 3. 为 handoff 增加带文档上下文的导出入口

现有只接收 `ResolvedFabricationBoard` 的低层 API 保留为聚合 `FILL` fallback。新增
document-aware handoff 输入，把已经校验的 `ProjectBundle/CardDocument` 与同 revision
的 resolved board 一起交给适配器。适配器以生产层的 resolved operations 为索引：

- 恰好一个可表达的加法 `ImageLayer` → `IMAGE`；
- 恰好一个可表达的加法 `TextLayer` → 一个完整 `STRING`，并附带同层、同正式 mask
  的重合 `FILL` 兼容几何；
- board fill、多个来源、Subtract 或不支持的变换 → 聚合 `FILL`，并记录原因。

原生 primitive 和文字兼容几何的路径仍从对应正式生产 mask/operation mask 提取，而不是
重新读取原图或用系统字体生成制造几何。文本字符串仅作为下游可编辑元数据；兼容 `FILL`
用于处理 EasyEDA V3.2.166 接受 `STRING` 记录却不把它实例化到 2D/3D 场景的行为。
结构测试同时校验字符串、兼容几何、正式 mask 的包围盒和来源指纹。若 EasyEDA 显示
`STRING`，两者完全重合，不改变制造结果。

备选方案是永远输出 `FILL`。它最简单，但无法可靠保留完整文字，也把复杂图片变成低质量、
超大对象。另一个备选是直接把原始图片交给 EasyEDA 重新描摹；这会绕过正式生产 mask，
违反生产真相约束。

### 4. 坐标转换按 primitive 类型集中建模

定义独立的 `BoardToEasyedaTransform`，明确：

- PCB Atelier：左上原点、正反面均为正向板坐标；
- EasyEDA：mil 单位、Y 轴按其工程坐标转换；
- 底层 `IMAGE` / `STRING`：使用基准工程确认的底层锚点规则，局部路径与字符串本身不预反转，
  `mirror` / `reverse` 保持与原生图元一致；
- fallback `FILL`：只执行板坐标到 EasyEDA 层坐标所需的单次几何转换。

每个 primitive 的转换通过非对称包围盒、首尾字符和局部路径快照测试验证，禁止同时修改
几何和设置第二个镜像标志。

### 5. 轮廓使用“候选序列 + 几何验收”而非降低生产 pitch

轮廓器针对每个环生成从强到弱的确定性候选：

1. 亚像素角点平滑及 Douglas-Peucker 简化；
2. 低强度单次平滑和较小简化容差；
3. 仅合并共线段；
4. 精确像素边界。

每个候选必须通过：闭合、非退化、无自交、绕向不变、包围盒偏差不超过一个生产 pitch、
孔洞仍位于父环、岛屿/孔洞数量不变。选择第一个安全候选，并把所有环聚合到一个
`IMAGE.path` 或一个 fallback `FILL`。若落到精确边界，报告记录可诊断 warning。

自动质量测试除点数外，还计算 Hausdorff 上界近似和“连续单位栅格正交步进”比例；后者
用于捕获视觉上仍明显的阶梯，而不是依赖截图主观判断。

### 6. 回归基准分为可提交 fixture 与本地真实文件验收

从用户基准中提取不含私有图片字节的最小结构 fixture：`IMAGE`/`STRING` 字段、非对称
坐标和小型复杂 mask。自动测试不依赖 `/Users/tapir/...` 绝对路径。实现完成后另用用户
提供的真实 `.pcba` 导出并与 `.eprj2/.epro2` 做结构统计和人工 EasyEDA 检查。

## Risks / Trade-offs

- [EasyEDA Pro 格式未公开且版本可能变化] → 将 primitive serializer 封装在适配器内，
  固定基准 fixture 和结构校验，失败时保留聚合 `FILL` fallback。
- [原生 `STRING` 的字体渲染与正式 mask 不完全一致] → 正式 mask 仍为制造真相；结构和
  包围盒必须校验。输出同 mask 的重合兼容几何；若目标字体/变换无法满足容差，则回退
  聚合轮廓，不冒充可无损原生文字。
- [复杂轮廓安全检查增加 CPU 时间] → 保持 25 µm 正式 pitch，按环短路选择首个安全候选，
  对同一 mask 哈希缓存结果，并设置性能回归上限。
- [底层方向规则在 2D 与 3D 看起来相反] → 分别测试“正向设计坐标”“EasyEDA 底层编辑”
  和“物理背面查看”，不以单张截图混淆三个视角。
- [已有下游工程内部图元发生变化] → handoff 本来就是版本化单向副本；不覆盖旧导出，
  新 manifest 记录 adapter/version 与 primitive strategy。

## Migration Plan

1. 先加入会失败的 capability、保存回环、字符完整性、底层方向和复杂轮廓测试。
2. 修复桌面 dialog/文字代理，两者不改变文件 schema。
3. 引入 document-aware exporter 和原生 primitive serializer，保留旧 API fallback。
4. 恢复正式 production pitch 为 25 µm，实施渐进轮廓器和报告字段。
5. 用用户真实 `.pcba` 重新导出新版本，完成结构校验、EasyEDA 2D/3D 人工验收与性能对比。
6. 若原生 primitive 在目标 EasyEDA 版本不可打开，可回滚到聚合 `FILL` strategy；`.pcba`
   源工程和既有导出不受影响。

## Open Questions

- EasyEDA `STRING` 对非默认字体、粗体、斜体和旋转的可无损范围需要由 fixture 测试逐步扩展；
  首次实现只对基准覆盖的默认字体、零旋转和单一加法映射启用原生输出。
- board fill 是否进一步映射为 `POUR/POURED` 不影响本轮三个故障的验收，首版保留聚合
  `FILL`，后续可单独提案实现原生铺铜。
