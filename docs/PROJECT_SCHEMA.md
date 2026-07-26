# 工程 schema、坐标与图层语义

## `.pcba` 容器

当前工程格式标识为 `pcb-atelier`，schema 版本为 `2`，文件扩展名为 `.pcba`。
`.pcba` 是 ZIP 容器：

```text
example.pcba
├── manifest.json
└── assets/
    ├── <asset UUID>.png
    └── <asset UUID>.webp
```

`manifest.json` 顶层包含 `format`、`schemaVersion` 和 `document`。`document` 是
`AtelierDocument`，主要字段如下：

| 字段 | 含义 |
| --- | --- |
| `id` / `title` | 稳定文档 ID 和显示名称 |
| `board` | 矩形或圆角矩形物理板框 |
| `stackup` | 基材、板厚、阻焊颜色和表面处理 |
| `front` / `back` | 固定存在的正面和背面内容树 |
| `assets` | 嵌入资产的路径、媒体类型、像素尺寸和 SHA-256 |
| `mappings` | 内容层到生产层的显式映射 |
| `mechanicalFeatures` | 当前为圆形 NPTH 或 PTH |

JSON 使用 camelCase；各类 ID 是 UUID 字符串。打开工程时会校验格式版本、层和映射
引用、板框与机械特征，以及嵌入资产路径和 SHA-256。当前可以读取 schema v1，并在
打开时自动迁移到 v2；未知的新版本仍会拒绝。

### 板体与叠层字段

`board` 是带 `type` 标签的枚举：

- `rectangle`：`widthUm`、`heightUm`；
- `roundedRectangle`：除宽高外包含 `cornerRadiusUm`，圆角不能超过短边的一半。

`stackup` 当前包含：

| 字段 | 当前契约 |
| --- | --- |
| `substrate` | 仅支持 `fr4`；首版基材固定为 FR-4 |
| `thicknessUm` | 正整数微米，默认 `1600` |
| `solderMaskColor` | black、white、green、red、blue、purple、yellow |
| `surfaceFinish` | `enig` 或 `haslLeadFree` |

Core 的 `SetBoardOutline` 与 `SetStackup` 都进入撤销/重做历史。板框改变时，内容对象的
`TransformUm` 保持原值，不随板宽高缩放或平移；Core 返回包含卡面、层 ID、旋转后
物理包围框和新板尺寸的越界诊断。Web/Tauri 工作区通过板体检查器调用这些命令，
并列出越界对象；FR-4 只作为固定事实显示，不提供材料下拉框。

## 内容层

`front.layers` 和 `back.layers` 中的每个 `ContentLayer` 都包含：

- 稳定 `id`、`name` 和可选 `parentId`；
- `visible`：只控制内容视图，不决定生产输出；
- `locked`：阻止变换、改名、替换内容和修改映射等领域命令；
- `exportEnabled`：决定自身及子层是否参加生产编译；
- `transform`：整数物理变换；
- `kind`：`image`、`text`、`group` 或 `boardFill`。

数组顺序是绘制顺序：越靠后的同级内容越在上方。组通过子层的 `parentId` 表达；组
本身不能直接映射到生产层。

`boardFill` 是 schema v2 新增的基础铺铜源对象，保存 `edgeClearanceUm`。每面最多
一个；Core 的幂等创建命令在重复调用时返回已有层 ID，而不是生成第二个对象。它只能
映射到对象所在面的铜层，编译时按矩形或圆角板框向内缩进；后续 `subtract` 映射可从
铺铜中挖空。它不是 EDA 动态覆铜，不包含网络、热焊盘、避让或孤岛规则。

图片内容引用 `assets` 中的 `assetId`，裁切框使用百万分比整数。文字保存正文、
字体名称、字号和 `autoWidth` / `fixedFrame` 布局；当前确定性生产栅格器统一使用
嵌入的 Noto Sans CJK SC，尚不按 `fontFamily` 查找系统字体。

## 物理坐标

- 持久化长度单位为整数微米：`1 mm = 1000 µm`。
- 板坐标原点为板框左上角，X 向右增加，Y 向下增加。
- `xUm`、`yUm` 是未旋转对象矩形的左上角；`widthUm`、`heightUm` 必须为正。
- `rotationMdeg` 是千分之一度，围绕对象矩形中心旋转。
- `flipX`、`flipY` 是对象局部坐标内的水平和垂直翻转。

正面和背面始终共享同一套板物理坐标。背面对象不会在 schema 中预先水平镜像；从
板背观看所需的镜像属于渲染器视图变换。转换到嘉立创 EDA 时只适配其 Y 轴原点和 mil
单位，不因为对象属于背面而额外镜像 X。

编辑器状态不写进 `AtelierDocument`。正背面各自的选择路径、缩放和平移，以及
“同时查看 / 聚焦当前面”、当前工作层和 3D 相机都属于会话 UI 状态；切换活动卡面
不会改写物理坐标或生产映射。

## 内部源对象与六张生产层

schema 中的内容层是内部稳定源对象，生产层描述制造结果。两者通过
`ProductionMapping` 连接：

```text
内容层 ID + 目标卡面 + 目标生产层 + add/subtract
```

每面固定三张生产 mask，共六张：

| schema 名称 | 嘉立创层 ID | 语义和极性 |
| --- | ---: | --- |
| `topCopper` | 1 | 正面铜，正极性 |
| `bottomCopper` | 2 | 背面铜，正极性 |
| `topSilkscreen` | 3 | 正面丝印，正极性 |
| `bottomSilkscreen` | 4 | 背面丝印，正极性 |
| `topSolderMaskOpen` | 5 | 正面阻焊开窗，开口极性 |
| `bottomSolderMaskOpen` | 6 | 背面阻焊开窗，开口极性 |

“阻焊开窗”中的有效像素表示没有阻焊油墨、露出下面铜或基材的区域，不是“绿色阻焊
填充”。同一内容层可以映射到多个同面生产层；跨面映射会被 schema 校验拒绝。
`add` 写入图形，`subtract` 从此前组合结果挖空，操作顺序会影响结果。

桌面左树不提供独立“内容”入口，而是投影为“板体 → 正面/背面 → 铜层/阻焊开窗/
丝印层”。生产层下的对象条目仍指向同一源对象 ID；选择工作层后插入图片或文字，会
创建源对象并自动添加当前面的 `add` 映射。检查器可以为同一对象增加其他同面生产层
关联。Core 命令本身会拒绝跨面映射及 `boardFill` 到非铜层的映射，不能依赖 UI
禁用状态维持领域正确性。

`visible` 与 `exportEnabled` 故意分离：隐藏但仍启用导出的层会继续进入生产 mask；
关闭某个组的 `exportEnabled` 也会排除其后代。

## 编译后的制造数据

生产链路为：

```text
AtelierDocument
→ FabricationPlan
→ ResolvedFabricationBoard
→ 编辑态 2D 生产检查 / 3D 成板预览 / 嘉立创 EDA 适配器
```

`ResolvedFabricationBoard` 在固定物理网格上保存六张 bit-packed mask、板框、叠层和
机械特征，并记录编译器版本、像素间距、字体指纹、输入与输出哈希。3D 预览和 EDA
导出已经只消费这份结果，不能从 Konva 画布反推制造几何。2D
`ProductionRenderer` 也消费同一结果，并已作为只读纹理栈接入编辑双画板；显隐和
隔离只改变检查视图，不会改写领域映射或导出参与状态。

默认生产网格常量为 `25 µm`，调用者也可以显式选择其他正整数间距；网格会向外取整，
最终边界裁切到实际板尺寸。桌面 3D 交互预览使用显式 `100 µm` 网格减少 IPC 与纹理
开销，但仍经同一编译器、极性和板框裁切生成；正式导出默认使用 `25 µm`。

## Web 与 Tauri 传输契约

前端统一发送 `pcb-atelier-workspace-v1` 请求，字段为 `contractVersion`、`command`
和 `args`；Rust `WorkspaceService` 返回相同契约版本、单调 `revision`、`payload`
或 `error`。读操作不推进 revision，成功修改才推进。

- Tauri 使用薄 `workspace_invoke` adapter；
- 本地 Web 开发通过 Vite 代理访问只监听 localhost 的 `workspace-bridge`；
- 两者调用同一 `WorkspaceService`，没有各自实现命令语义；
- Web production build 不启用该本地 bridge，系统目录选择也仍是 Tauri 专属。

Rust 测试已验证版本拒绝、当前文档生成的生产预览和 Tauri 薄 adapter 等价；完整 Web
与 Tauri 真实交互流程仍需最终验收。
