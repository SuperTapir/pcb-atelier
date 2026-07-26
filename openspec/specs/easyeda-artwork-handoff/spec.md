# easyeda-artwork-handoff Specification

## Purpose
TBD - created by archiving change add-card-editor-foundation. Update Purpose after archive.
## Requirements
### Requirement: 从统一生产板导出嘉立创 EDA 工程
系统 MUST 仅从已编译生产层导出嘉立创 EDA 工程，不得从 UI 画布或预览位图重新推断
图层、尺寸、极性或方向。

#### Scenario: 导出简单艺术卡
- **WHEN** 用户导出符合首版限制的双层艺术卡
- **THEN** 系统必须生成可由嘉立创 EDA 打开的工程产物，并报告产物路径与导出版本

#### Scenario: 导出后打开工程
- **WHEN** 嘉立创 EDA 工程成功导出
- **THEN** 客户端必须提供“使用嘉立创 EDA 打开”和“在文件管理器中显示”操作；打开失败必须保留导出产物并显示可理解的错误

### Requirement: 导出前检查
系统 MUST 在导出前检查不支持的层、映射、板框或机械特征，并在无法无损交接时明确失败；
系统不得静默丢弃内容或将底层内容写入错误层。

#### Scenario: 导出不支持的生产映射
- **WHEN** 工程包含当前嘉立创 EDA 适配器不支持的生产映射
- **THEN** 系统必须阻止导出并指出不支持的内容层和目标生产层

### Requirement: 正反面与图层一致性验证
系统 MUST 使用非对称黄金卡片验证导出工程的板尺寸、层语义、阻焊开窗极性和正反面方向；
生成的原生工程必须通过结构校验。

#### Scenario: 验证非对称黄金卡片
- **WHEN** 自动化测试导出带 `F` / `B` 标记和偏心特征的黄金卡片
- **THEN** 测试必须验证原生工程结构有效，且各标记位于预期的嘉立创 EDA 面与方向

### Requirement: 单向下游交接
系统 MUST 将嘉立创 EDA 工程视为下游副本；用户在 EDA 中的任意修改不得自动回写或
破坏 PCB Atelier 源工程。

#### Scenario: 重新导出已在 EDA 修改过的工程
- **WHEN** 用户在 EDA 中修改一个先前导出的工程后，从未改变的 PCB Atelier 工程再次导出
- **THEN** 系统必须创建新的下游导出版本，并保持 PCB Atelier 源工程不变
