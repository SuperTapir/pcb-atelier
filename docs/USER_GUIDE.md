# 当前版本用户操作说明

本文只说明仓库当前代码实际具备的能力，不把 OpenSpec 计划当作已交付功能。当前版本
用于验证“类 EDA 双面卡片编辑 → 生产层编译 → 3D 成板预览 → 嘉立创 EDA 单向交接”
这条垂直链路；打开、保存 `.pcba` 的桌面 UI 仍未完成。

工程结构与坐标见[工程 schema、坐标与图层语义](PROJECT_SCHEMA.md)，制造出口见
[嘉立创 EDA 导出与单向交接](EASYEDA_HANDOFF.md)。

## 启动方式与运行契约

### Tauri 桌面应用

在仓库根目录执行：

```bash
npm --prefix apps/desktop run tauri dev
```

Tauri 前端通过 `workspace_invoke` 调用 Rust `WorkspaceService`。当前应用创建一张内存
卡片；关闭应用后本次会话内容不会保存。打开、另存为和恢复 `.pcba` 的界面仍在开发中。

### 本地 Web 开发

本地 Web 不使用伪造的前端工程模型。需要分别启动同一个 Rust
`WorkspaceService` 的 localhost bridge 和 Vite：

```bash
# 终端一
cargo run -p atelier-desktop --bin workspace-bridge

# 终端二
npm --prefix apps/desktop run dev
```

Vite 将 `/__atelier_bridge` 转发到 `127.0.0.1:1424/workspace`。Web 与 Tauri 使用
同一版本化请求/响应契约、同一领域命令和同一生产编译器。Web production build
故意不启用本地 bridge；Web 端也没有系统目录选择能力，所以当前不能从 Web 界面完成
EDA 导出。真实 Web/Tauri 全流程交互仍需在最终验收中继续验证。

## 编辑与预览

顶层只有两个模式：

- **编辑**：图片、文字、位置变换、图层操作、生产层关联和基础铺铜都在这里完成。
- **预览**：只读显示 Core 从最终生产 mask 生成的整体 3D 板体；拖动旋转、滚轮缩放、
  右键平移只改变相机，不修改工程。

编辑模式默认同时显示正面和背面。点击任一画板后，该面成为活动卡面，后续插入、左树
工作层和右侧检查器都跟随它。“同时查看 / 聚焦当前面”只改变布局；两面分别保存选择
和视口，聚焦后再返回不会丢失另一面的状态。窄空间下双画板自动改为上下排列。

滚轮以指针位置为中心缩放，范围为 `25%–400%`；选择工具下拖动画布可平移，双击空白
或点击“适配画布”可复位当前面的视口。

背面按“从板背观看”水平镜像。命中、插入和导出仍使用未镜像的物理板坐标；显示镜像
不会改写对象 X 坐标，也不会改变 EDA 导出方向。

## 左侧图层树

左树没有单独的“内容”入口，也没有“内容 / 生产 / 材质”三套模式。固定层级是：

```text
板体
├── 正面
│   ├── 铜层
│   ├── 阻焊开窗
│   └── 丝印层
└── 背面
    ├── 铜层
    ├── 阻焊开窗
    └── 丝印层
```

图片、文字和基础铺铜直接列在其关联的生产层下面。用户编辑的是这些生产对象；内部仍
以“稳定源对象 + 显式 `ProductionMapping`”保存，生产层下显示的是同一源对象的引用，
不是可以各自漂移的副本。

- 点击铜、阻焊开窗或丝印会同时选择卡面和工作层。
- 新插入图片或文字会按当前工作层自动建立 `add` 关联。
- 在检查器勾选其他生产层，可以让同一对象同时关联多层；左树用“关联”标记提示它们
  来自同一源对象。
- 阻焊使用“添加开窗 / 减少开窗”语义，不把阻焊颜色区域误称为开窗。
- 生产层的显示/隐藏、隔离控件只维护临时检查状态，不改变对象的
  `exportEnabled` 或映射；画板叠加显示当前工程实际编译的六层生产纹理。

编辑态 2D 检查、3D 预览和 EDA 导出都从 Rust Core 的
`ResolvedFabricationBoard` 读取最终 mask。

## 插入图片和文字

### 图片

图片插入当前活动卡面与当前工作层，可通过文件按钮、拖入窗口或剪贴板粘贴触发。新图
保持宽高比，在卡片宽高各 `80%` 范围内等比适配并居中。替换图片会保留层 ID、物理
变换、层级、锁定状态和生产层关联。

生产栅格器明确支持 PNG、JPEG、BMP 和 WebP。SVG 等浏览器可显示格式未必能进入生产
编译，制造素材应使用前述位图格式。

### 文字

1. 点击“文字”工具。
2. 在活动画板内单击创建点文字，或拖出矩形创建定宽文字。
3. 在原生 `textarea` 输入；按 `Escape`、`Command/Ctrl + Enter` 或点击外部完成。

已有文字可双击、选中后按 `Enter`，或从检查器再次编辑。生产编译固定使用随应用嵌入
的 Noto Sans CJK SC，不读取系统字体；当前界面没有字体、字号和字重选择器。

## 对象、映射与铺铜

- 单击画布对象或生产层下条目可选择；`Shift` 用于多选，重叠对象支持候选循环。
- 检查器可精确修改 X、Y、宽、高和旋转；方向键按 `0.1 mm` 微调，`Shift` 为
  `1 mm`。锁定对象或锁定祖先会阻止变换。
- 图层条目支持显隐、锁定、排序、分组和解组；`visible` 只控制编辑显示，
  `exportEnabled` 独立决定是否参加生产编译。
- 检查器可增加或移除当前对象的铜、阻焊开窗、丝印关联。图片和文字可关联同面任意
  三类生产层；跨面关联会被 Core 拒绝。
- 铜层的“添加基础铺铜”创建板框内缩源对象，当前默认板边间距为 `0.50 mm`。
  每面最多一个基础铺铜；它只能关联所在面的铜层，不是带网络、热焊盘或孤岛规则的
  EDA 动态覆铜。

## 板体与叠层

选择左树“板体”后，检查器可以编辑板宽、板高、圆角、板厚、阻焊颜色和表面处理，
并显示“FR-4（首版固定）”；主要界面不提供材料选择。

Core 的 `SetBoardOutline` 和 `SetStackup` 已接入 Web/Tauri 工作区。修改板框时不会
按比例缩放、移动或裁掉源对象，而是保留整数微米变换；越界对象会在检查器列出。
基材枚举当前只有 FR-4。

## 3D 预览与 EDA 导出

进入“预览”时，WorkspaceService 以交互网格编译当前工程，返回正反面 RGBA 生产纹理、
板框、圆角和板厚。Three.js 只读组件显示这份结果；它不是照片级材质模拟，生产几何
仍以最终 mask 为准。

Tauri 顶部“导出 EDA”按钮会选择目录并调用同一个 WorkspaceService 导出 `.epro2`、
`.eprj2` 和 manifest。Core/CLI 导出与嘉立创 EDA 人工打开已有验证记录；按钮驱动的
完整 Tauri 实际交互仍列为最终验收项。Web 开发模式目前不能弹出系统目录选择器。

## 当前仍未完成

- 桌面新建自定义尺寸、打开、保存和恢复 `.pcba`；
- 字体、字号、字重、图像阈值等生产参数 UI；
- Web production 后端与 Web 目录选择；
- 完整 Web、Tauri、3D、导出全流程最终交互验收；
- Gerber 直接导出。

## CLI

CLI 已支持创建、应用领域命令、校验、检查生产几何和导出嘉立创 EDA：

```bash
cargo run -p atelier-cli -- new demo.pcba \
  --title "示例卡片" --width-mm 64 --height-mm 100

cargo run -p atelier-cli -- validate demo.pcba

cargo run -p atelier-cli -- apply demo.pcba commands.json \
  --output demo-updated.pcba

cargo run -p atelier-cli -- production-inspect demo.pcba

cargo run -p atelier-cli -- export-easyeda demo.pcba output/
```

`apply` JSON 使用 `DocumentCommand` 的 `operation` 标签和 camelCase 字段。它是自动化
底层入口；手写命令前应备份工程，并在输出后执行 `validate`。
