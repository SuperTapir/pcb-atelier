# React + Konva 画布性能基准

这是双画板编辑器的独立技术验证，不包含产品 UI，也不复用正式工程模型。

## 场景

- 正面和背面两个独立 Konva `Stage` 始终同时挂载，并排显示。
- 每面 100 个可拾取、可拖动对象，合计 200 个对象：120 个图形、48 个文本、
  32 个代理图片节点。
- 8 份 `256 × 160` 代理图片资产。
- 正反面各 3 张生产层纹理，合计 6 张 `512 × 640` 纹理。
- 每面 12 对象多选和 `Transformer`。
- 正反面各 180 帧连续平移/缩放，各 90 帧多选变换。
- 24 次活动卡面与当前面生产层显隐循环。
- 监听两个画板各自的 Konva `draw` 事件；活动面执行视口或显隐循环时，记录非活动面
  是否被连带持续重绘。
- Chromium 可用时，用 `performance.memory` 记录 GC 后的 JS heap。

生产层使用少量合成纹理，而不是把生产几何展开成成千上万个 Konva 节点。这是本轮
需要验证的架构假设。

## 自动运行

在 `apps/desktop` 执行：

```bash
npm run benchmark:konva
```

脚本先构建 production bundle，再启动本地 Vite preview，使用本机 Chrome/Chromium
无头运行，输出 JSON 并按阈值设置退出码。若浏览器不在默认路径：

```bash
CHROME_PATH="/path/to/chrome" npm run benchmark:konva
```

## 可视运行

```bash
npm run benchmark:konva:ui
```

打开 <http://127.0.0.1:1422/benchmark.html>。页面会自动执行，也可手动重新运行。
开发服务器包含 HMR、source map 与 React 开发期开销，仅用于观察。

## 阈值

- 平均 FPS `>= 45`。
- P95 帧耗时 `<= 33.3 ms`，即绝大多数帧不低于 30 FPS。
- 超过 `22.2 ms` 的慢帧比例 `<= 20%`。
- 每个活动面循环中，非活动画板额外 draw `<= 6` 次，且所有循环累计的非活动 /
  活动画板 draw 比例 `<= 2%`。这是布局状态变化允许一次性重绘、但禁止逐帧连带重绘
  的边界。
- 从基准开始到结束，强制 GC 后的 heap 增量 `<= 16 MB`，用于约束一次性缓存。
- 24 次切面/显隐循环期间，首个采样点到最后采样点：
  - 增量 `<= 4 MB`；且
  - 增幅 `<= 10%`，用于判断是否随轮次持续增长。

如果运行时未暴露 `performance.memory`，页面输出“不可用”，不能据此通过内存验收；
自动脚本会把 heap 不可测视为失败。

## 目标机验收

浏览器结果只能验证技术路线，不等同于 Tauri/macOS WebView 的真实性能。在目标
Mac 上执行三轮 release WKWebView 基准：

```bash
npm run benchmark:konva:tauri
```

该命令会：

1. 构建 production 前端与启用 `tauri/custom-protocol` 的独立 release binary；
2. 使用 `tauri.benchmark.conf.json` 直接打开 `benchmark.html`，不修改正式 App；
3. 运行三次并记录每轮帧指标；
4. 在活动面/显隐的第 4、8、12、16、20、24 轮由 Tauri 宿主采样自身 RSS；
5. 输出三轮明细和中位数，并按阈值设置退出码。

WKWebView 不提供 Chromium 的 `performance.memory`，所以自动验收使用宿主进程 RSS
作为粗粒度泄漏门槛：切面循环首末增量不得超过 `32 MB` 且不得超过 `20%`。
RSS 无法区分 JavaScript heap、纹理缓存和宿主内存；出现持续增长、需要修改阈值或正式
编辑器引入更多缓存时，必须再用 Safari Web Inspector 的 JavaScript Allocations
或 Instruments 的 Allocations 模板比较第 4 与第 24 轮 retained allocations。

## 图片交互 `interactive-image-v1`

图片交互基准固定使用
[`profile.json`](./interactive-image-v1/profile.json) 中记录的 1003×1568 PNG/JPEG、
51.17×80 mm 物理尺寸和 250 µm warm 代理。执行：

```bash
npm run benchmark:interactive-image
```

release bridge 会验证源图只在 `begin_image_preview_source` 出现一次，随后 30 个非缓存
阈值/反相请求只携带 handle、recipe 与 generation；再以 60 个超吞吐输入验证最终
generation、协作取消和单 stream 的“一个运行 + 一个最新 pending”。2026-07-27
Mac13,1 / Apple M1 Max / macOS 26.5.2 / AC 实测：

- 源 PNG `136,309 bytes`；一次性 begin JSON `481,913 bytes`；
- 后续预览 JSON `602 bytes`，不含源图；
- prepared source `1` 次，60 次 burst 取消 `59` 个陈旧请求；
- 205×320 代理的 30 个非缓存结果 `p50 2.97 ms / p95 3.71 ms`；
- 完整结果见
  [`baseline-macos-2026-07-27.json`](./interactive-image-v1/baseline-macos-2026-07-27.json)。

bridge 数字包含 Rust 处理、PNG 编码、base64、HTTP 和 JSON 解析，但不包含浏览器
`Image.decode()`/纹理可绘制边界；浏览器 E2E 和 Tauri 手势基准继续负责 120 ms
drawable feedback 与 16.7 ms 拖动帧目标。首轮保留 PNG data URL：当前整体边界只占
120 ms 预算的一小部分；若浏览器分段 profile 证明 PNG 编码、base64 和客户端解码合计
超过 20%，再切换二进制响应或受控资源 URL。

## 双画板实测

2026-07-26 在 Headless Chrome 150、`1440 × 900` 上执行 production bundle：

- 平均 FPS：`71.39`。
- P95 帧耗时：`26.1 ms`。
- 慢帧比例：`13.25%`。
- 非活动画板 draw：`0 / 1104`。
- 显隐循环 JS heap：`5,075,085 → 5,074,377 bytes`，未持续增长。

同日在 `Mac13,1`、macOS `26.5.2`、arm64、`1280 × 800` release WKWebView
完成一轮双画板验证：

- 平均 FPS：`52.54`。
- P95 帧耗时：`18 ms`。
- 慢帧比例：`0.68%`。
- 非活动画板 draw：`6 / 1110`，比例 `0.54%`。
- 显隐循环 RSS：`104,864 → 104,880 KiB`，增长 `16 KiB`。

该轮通过帧、重绘隔离与 RSS 阈值。此前的
[tauri-macos-2026-07-26.json](./results/tauri-macos-2026-07-26.json)
是旧单画板基准，只保留为迁移前基线，不代表当前双画板结果。
