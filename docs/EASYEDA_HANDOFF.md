# 嘉立创 EDA 导出与单向交接

## 结论

嘉立创 EDA 是 PCB Atelier 的**单向下游出口**，不是双向同步编辑器。PCB Atelier
工程及其 `ResolvedFabricationBoard` 是源数据；`.epro2`、`.eprj2` 和导出报告是
可继续检查或编辑的下游副本。

在嘉立创 EDA 中做出的修改不会回写 `.pcba`，PCB Atelier 也不会读取一个修改后的
`.eprj2` 来更新内容层、映射或资产。需要修改源设计时，应回到 PCB Atelier 后重新
编译并创建新的导出版本。

## 当前导出产物

Core 的 `export_easyeda_handoff(outputDirectory, title, resolvedBoard)` 每次选择新的
序号，不覆盖先前导出：

```text
<ascii-title>-<resolved-hash前12位>-v0001.epro2
<ascii-title>-<resolved-hash前12位>-v0001.eprj2
<ascii-title>-<resolved-hash前12位>-v0001.manifest.json
```

再次导出相同生产板会使用 `v0002`、`v0003` 等新序号。因此，用户对旧 `.eprj2`
所做的本地修改与后续 Atelier 导出彼此隔离。

- `.epro2`：公开归档中间产物；
- `.eprj2`：嘉立创 EDA 专业版原生 SQLite 工程；
- `.manifest.json`：导出格式版本、导出版本、三条产物路径、生产编译输入/输出哈希、
  两份 EDA 文件的 SHA-256、图元统计及公开/原生结构校验结果。

公开归档和原生工程分别写到临时文件、完成结构校验后再替换目标；报告最后原子写入。
失败不会把一个未经校验的文件报告为成功导出。

CLI 已提供 `export-easyeda` 并直接调用此 Core API。Tauri 顶部“导出 EDA”按钮已接入
系统目录选择和同一个 `WorkspaceService` 导出命令；按钮驱动的完整 Tauri 实际交互
仍待最终验收。Web 开发模式共享生产编译和导出服务，但当前没有 Web 目录选择能力，
production Web bridge 也未实现，因此不能从 Web 界面完成导出。

## 当前可交接的制造语义

适配器只读取 `ResolvedFabricationBoard`，不会读取图片文件、文字对象、Konva 节点
或预览纹理。当前支持：

- 矩形和圆角矩形板框；
- 顶/底铜、顶/底丝印、顶/底阻焊开窗六张最终 mask；
- 圆形 NPTH 和圆形 PTH；PTH 带圆形焊盘；
- mask 岛、孔洞轮廓和板边非整像素裁切；
- 从顶部原点、整数微米坐标转换为嘉立创 EDA 的底部原点和 mil 单位；
- 保持正反面非对称物理位置，不在底层数据上预镜像背面。

生产 mask 会转成静态 `FILL`，机械孔会转成 `PAD`。图片、文字、映射来源和
`add/subtract` 历史已经被压平，不会成为 EDA 中可还原的原始图片层或文字层。

## 导出前检查

当前 preflight 会阻止以下输入：

- 不是恰好六个标准生产层；
- 缺少任一标准生产层；
- mask 尺寸与生产网格不一致；
- mask 内容与其记录的 SHA-256 不一致；
- 空标题或无法生成有效公开归档的记录。

随后还会校验公开归档包含有效 BOARD/PCB 文档、物理尺寸和引用关系，并校验原生工程
的 SQLite schema、工程/分支/历史身份链、加密历史载荷、板尺寸、图元数量和层 ID。

## 当前限制

- 不包含原理图、网络、器件、封装、布线、铺铜规则、DRC 或 PCBA 数据。
- 不保留 EDA 侧可继续参数化编辑的图片、文字、分组或生产映射，只交接静态几何。
- 只支持当前 schema 中的矩形/圆角矩形板框与圆形 NPTH/PTH；任意板框、槽孔、盲埋孔
  等尚未建模。
- 位图生产栅格器明确支持 PNG、JPEG、BMP 和 WebP；深色且具有足够不透明度的像素
  被视为生产图形，白色或透明背景不写入。阈值暂时固定且无 UI 参数。
- 文字生产结果固定使用嵌入的 Noto Sans CJK SC；系统字体和字体文件嵌入尚未实现。
- 静态几何精度受所选生产网格间距影响。默认常量是 `25 µm`，测试也会使用更粗网格
  加速；调用导出前必须明确选择适合实际制造的间距。
- 自动化测试已验证 `.epro2` / `.eprj2` 的内部结构、六层语义、尺寸和非对称方向；
  黄金工程也已在嘉立创 EDA 专业版 V3.2.166 中人工打开并检查，记录见
  [嘉立创 EDA 黄金工程人工验收](../artifacts/acceptance/easyeda/MANUAL_ACCEPTANCE.md)。
- Gerber 直接导出尚未在本轮实现。
- EDA 产物是压平后的静态制造几何，不会保存 Atelier 左树中的多层关联关系；若同一
  图片或文字同时关联铜和阻焊开窗，EDA 中只会看到各目标层最终的 `FILL`。

## 推荐交接流程

1. 在 PCB Atelier 的“板体 → 正面/背面 → 铜/阻焊开窗/丝印”左树完成对象构图与
   多层关联。源对象和 `ProductionMapping` 由工程内部保存，不需要单独进入内容模式。
2. 编译为 `ResolvedFabricationBoard`，确认六层 preflight；编辑态双画板的 2D 生产
   纹理和 3D 预览都消费这一份结果。
3. 导出一个新的版本目录，保留 `.manifest.json`。
4. 在嘉立创 EDA 中打开对应 `.eprj2`，复核板尺寸、正反面、阻焊开窗、丝印、孔位
   和制造规则。
5. 若几何或层语义错误，回到 Atelier 修正并重新导出；不要把 EDA 修改当作源工程。
6. 若必须在 EDA 中增加复杂电路或规则，把该 `.eprj2` 视为独立分支并自行管理。
