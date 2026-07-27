## Context

当前导入草稿把 `bytes: number[]` 保存在前端，每次预览都经 JSON 再次发送完整源图。后端随后
重新猜测格式、解码、应用 EXIF、采样灰度、阈值、形态学处理、生成 mask，再逐像素编码 PNG 和
base64。真实 1003×1568、约 1.54 MB 图片会形成约 5.47 MB 请求；本机实测缓存命中仍约
330 ms，改变阈值约 480–540 ms，而协调器每 100 ms 即可发出新请求。

已存在图片的检查器还会先执行 `SetTreatmentRecipe`，使每个中间参数进入 document revision 和
撤销历史，再由检查器和画布分别请求可能不同精度的代理。调度器能拒绝排队中的旧任务和丢弃
陈旧结果，但已经运行的处理无法协作停止。画布拖动本身使用 Konva 本地矩阵且只在手势结束提交，
但高精度代理纹理和持续后台编译会争抢 CPU、内存与 GPU 上传资源。

工程尚未 release，因此无需兼容旧 schema；本变更也不需要修改持久化 schema。原始资产、
最终配方、物理变换和生产映射继续是工程真相，所有交互会话和代理缓存均为可丢弃状态。

## Goals / Non-Goals

**Goals:**

- 图片源在一次交互会话中只传输、方向归一和解码一次。
- 参数输入立即更新本地 draft，异步预览只保留最新意图，不污染 document 或 undo history。
- 检查器与画布共享代理处理、编码结果和客户端纹理，并以字节预算管理缓存。
- 拖动、缩放和旋转只变换已有纹理，手势期间不发生图片处理或正式生产编译。
- 正式生产继续复用 Rust 领域语义，并以黄金 fixture 证明代理与正式输出的方向、极性、裁切、
  拓扑和物理边界一致。
- 用真实图片和发布构建建立可持续回归的延迟、并发、传输量与帧时间指标。

**Non-Goals:**

- 不新增焊盘、钻孔、局部铜区或完整 EDA 能力。
- 不将源图片永久替换成矢量轮廓；矢量仅可作为正式输出或导出缓存。
- 不在首轮引入独立 GPU 图像算法实现，以免与 Rust 正式语义产生第二套算法真相。
- 不承诺所有制造性形态学处理都在一帧内完成；它们必须异步、可取消且保持上一帧可用。

## Decisions

### 1. 建立显式 `ImagePreviewSession`

Tauri/Web workspace service 新增会话级操作：

- `begin_image_preview_source`：接收一次源字节或引用已嵌入 `assetId`，返回不持久化的
  `sourceHandle`、内容哈希、方向归一尺寸和媒体类型。
- `request_image_preview`：接收 `sourceHandle`、完整 draft recipe、物理尺寸、代理精度及
  单调 `generation`，返回编码代理、诊断、配方指纹和 generation。
- `release_image_preview_source`：减少引用并释放无消费者的临时资源。

已存在资产可由后端直接取得嵌入字节，导入中的外部图片只在 `begin` 时传输一次。Web 测试 bridge
和 Tauri 使用相同领域请求，但句柄注册表属于运行时适配层，不进入独立 Rust core 或 `.pcba`。

备选方案是继续传字节并只增加 debounce。它不能消除约 5 MB 的重复 JSON 和解码成本，在快速
输入下仍会积压，因此拒绝。

### 2. Rust core 拆分准备阶段与确定性处理阶段

core 提供与 UI 无关的 `PreparedImage`：包含方向归一后的源像素、灰度表示和按需缩放层；
`prepare_image` 只依赖资产字节，`compile_prepared_image` 接收不可变 prepared source、
版本化 recipe 和物理采样请求。原有 `compile_image_treatment(bytes, ...)` 保留为 CLI/正式
输出的组合入口并调用同一内部实现，避免算法分叉。

缓存键由资产 SHA-256、算法版本、方向归一结果、配方指纹、裁切、物理尺寸和代理精度构成。
平移、旋转及画布缩放不进入局部 mask 缓存键。prepared source 与代理分别按估算字节执行 LRU，
而不是当前固定 128 项的无尺寸上限缓存。活跃 `ImagePreviewSession` 固定基础 prepared source
并计入预算，只允许逐出其缩放层和代理；会话释放后基础 prepared source 才可进入 LRU。无法
容纳新活跃源时返回资源错误，而不是重复解码。

备选方案是把阈值和形态学全部迁到 WebGL。它可能更快，但会形成浏览器与 Rust 两套实现，
增加跨平台精度和拓扑差异；首轮只允许未来对可证明等价的简单步骤增加 GPU 快路。

### 3. draft 与领域提交分离

`ImageTreatmentEditor` 持有完整 draft recipe。输入事件只更新 draft 和提交预览 generation，
不调用 `setTreatmentRecipe`。导入确认创建资产、处理、实例和映射；既有图片在滑块
`pointerup` 或数值输入 `change/blur` 时，把相对本次手势或输入开始状态的最终配方作为一个
领域命令提交；已完成手势之后关闭常驻检查器不会回滚该命令。导入弹窗则维持完整确认/取消事务，
只有点击确认才创建工程实体，取消丢弃全部 draft。

正式预览和导出只读取已提交 document，绝不读取 UI draft。需要在画布同时观察 draft 时，
画布由 preview broker 临时覆盖该对象的显示纹理，覆盖层不改变 mapping 或正式 revision。

备选方案是对每个输入继续写领域命令再合并历史。即使合并 undo step，仍会引发 revision 失效、
代理缓存抖动和正式预览重编译，因此拒绝。

### 4. latest-only 调度采用“一个运行 + 一个最新待处理”

每个 `(workspaceId, previewStreamId)` 维护一个运行槽和一个 pending 槽。同源不同实例或独立
编辑手势使用不同 stream；相同稳定代理键仍可在 broker 层跨 stream 合并。新 generation 覆盖 pending；
运行任务持有取消令牌，解码之后的每个阶段和耗时形态学循环按行块检查。取消不会改变确定性输出，
只提前结束已无消费者的计算。结果写回同时检查 source generation、recipe fingerprint 和
workspace revision。

前端不清空已有预览，而是显示最后一次 accepted 代理并标记 updating。简单阈值请求可以在
当前任务取消点后立即运行；输入空闲 150–250 ms 后，broker 才允许请求更高精度档位。

备选方案是提高并发让所有 generation 同时运行。它会加剧 CPU 争用并拖慢画布，所以并发必须
按图片对象受限，全局调度仍保留较小上限。

### 5. 检查器与画布通过共享 `ImageProxyBroker` 订阅

broker 的稳定键与 Rust 代理缓存键一致。相同键只产生一个 Promise/任务和一个解码后的
`HTMLImageElement`/`ImageBitmap`；消费者使用引用计数订阅。编辑器活跃时，画布优先复用当前
观察窗代理；画布需要更高档位时只能在输入空闲后升级，并继续显示旧纹理直到新纹理就绪。

代理响应首轮明确继续使用紧凑 PNG data URL；不得返回 RGBA JSON 数组。该选择避免同时引入
自定义协议和文件生命周期，且源图一次注册已消除主要传输开销。benchmark 必须单独记录 PNG
编码、base64 和客户端解码时间；若三者合计超过交互延迟的 20%，后续 change 再切换二进制响应
或受控资源 URL，而不改变 core API。

### 6. 性能与一致性作为可执行契约

新增发布构建 benchmark 和集成测试，至少覆盖约 1000×1500 的 PNG/JPEG、60 次连续阈值输入、
后台重形态学任务期间拖动、检查器/画布双消费者和清缓存重建。运行时暴露仅用于测试与诊断的
计数快照：源字节数、prepare 次数、代理编译次数、coalesce/cancel 次数、active/pending 数及
gesture frame samples；生产 bundle 不包含可修改工程的测试 IPC。

代理与正式输出使用非对称、孔洞、孤岛和细线 fixture 比较配方指纹、方向、极性、裁切，以及
尺寸不小于两个代理像素结构的连通关系、孔洞数和一个对应精度像素以内的边界误差。低于表达范围
的 fixture 必须证明只影响粗代理可见性、不改变正式结果。

版本化 `interactive-image-v1` profile 固定真实 fixture SHA-256、51.17×80 mm、250 µm warm
代理、输入事件到对应 generation 纹理可绘制的计时边界、至少 30 个非缓存预览样本和至少 300
个拖动帧，并记录应用 commit、macOS 基准机硬件/系统/供电模式和输出尺寸。当前基准机上阈值/
反相首个新代理 p95 不高于 120 ms，拖动帧更新 p95 不高于 16.7 ms；其他平台先记录相同 profile，
新增硬门槛需另行校准，不套用未经测量的绝对数字。

## Risks / Trade-offs

- [prepared image 和多档代理增加内存] → 使用独立字节预算、引用计数和 LRU；活跃会话固定基础
  prepared source，只允许丢弃其缩放层与代理，会话结束后全部派生数据才可回收。
- [协作取消检查过密会降低正式编译吞吐] → 只在阶段边界或固定行块检查，正式无取消编译走同一
  算法但不承担 UI generation 检查。
- [较粗代理无法表达亚像素细节] → 只对不小于两个代理像素的结构约束拓扑和边界；更小细节标记为
  需要高精度确认，输入空闲后自动升级档位，正式检查仍显示正式 mask。
- [共享纹理生命周期复杂] → broker 使用稳定键、引用计数和显式 dispose，并用双消费者测试验证。
- [120 ms 指标受机器影响] → 固定 fixture、release build 和基准环境记录绝对值，同时记录相对旧
  链路的传输量与编译次数；CI 只对稳定计数契约硬失败，硬件延迟阈值在指定基准机验收。

## Migration Plan

1. 先增加 prepared-image API、运行时会话注册表和计数测试，不改变现有 UI。
2. 将导入对话框切到一次注册和 read-only draft preview，保留旧接口作为测试期回退。
3. 将既有图片检查器切到 draft 单次提交，并接入共享 broker。
4. 将画布代理缓存迁移到 broker 和字节预算，增加手势期间零编译断言。
5. 全量 Rust/Web/E2E/发布 benchmark 通过后删除重复字节预览接口和旧 coordinator 路径。

工程未 release 且持久化 schema 不变，无需数据迁移或兼容分支。回滚时可恢复旧 UI 请求适配器，
正式生产 core 与已保存工程不受影响。

## Open Questions

本 change 开工前没有阻塞性开放问题。PNG data URL 与当前 macOS profile 已作为首轮明确决策；
其他平台门槛或二进制代理传输需要基于本 change 的分阶段 profile 另开提案。
