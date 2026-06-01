# Markdown 语法层重构计划

## Summary

- 目标不是直接“换 parser”，而是把 parser 退回到粗结构层，把细 marker、projection、table cell、rendered element 统一交给共享语义层。
- 当前 `MarkdownSyntaxData` 先保留为兼容输出，不再作为新架构的目标 IR；`MarkdownSyntaxTree` 继续做对外门面。
- 热路径优先验证 pulldown；comrak 作为结构校验/参考后端；tree-sitter 继续作为现有生产 baseline。

## Implementation

- 在 `markdown_wysiwyg` 里拆成三层：
  - `MarkdownStructure`：只放粗结构节点，包含 `kind`、`source_range`、`row_range`、父子关系、少量粗属性（heading level、list ordered/checked、table row/cell 边界等）。
  - `MarkdownSemanticsAssembler`：只看 `MarkdownStructure + source`，补齐 marker ranges、table cell ranges、projection replacements、marker dependencies。
  - 现有 `MarkdownSyntaxData`：作为 assembler 的输出和兼容层，不能再由 parser 直接拼出来。
- `parser.rs` 只做 backend 适配，不再直接产出细语义；`blocks.rs`、`inline.rs`、`tables.rs` 改成共享 assembler helper，输入不再是 `tree_sitter::Node`，而是 `MarkdownStructure`。
- 新增统一 backend trait，三个实现：
  - tree-sitter backend：保持现有行为，作为 baseline。
  - pulldown backend：用 `Parser::into_offset_iter()` 组粗结构，不做细 marker 逻辑。
  - comrak backend：用 AST + `sourcepos` 组粗结构；先把行列位置转成 byte range，再交给 assembler。
- 细边界统一由源码扫描得出，不靠各 parser 复刻：
  - heading/list/blockquote/table 的 marker range
  - inline emphasis/strong/code/link/image/math 的 marker/content ranges
  - escape/entity/task marker 的 replacement
  - active projection 依赖
- `tooling/markdown_syntax_bench` 改成三类输出：
  - 裸 parser 时间
  - `source -> semantics` 端到端时间
  - 结构/语义 diff 报告
  - baseline 只算一次，不能在候选循环里重新 parse 生产路径

## Test Plan

- 语义等价：同一 fixture 下，tree-sitter / pulldown / comrak 的编辑器查询结果一致。
- 查询覆盖：`blocks()`、`tables()`、`inline_spans()`、`projection_for_source_range()`、`active_projection_source_ranges_for_source_range()`、`table_for_source_row()`。
- fixture 覆盖：标题、段落、嵌套列表、引用、表格、fenced/indented code、HTML、link reference、link/image、emphasis/strong/strike、escape/entity、inline/block math、task list、CJK/UTF-8。
- 性能门槛：以 `source -> semantics` 端到端时间为准，不以裸 parser 速度单独决策。
- 增量验证：dirty window 外语义不变，局部编辑只重算受影响范围。

## Rollout

- 第一步只改 `markdown_wysiwyg` 和 benchmark，不改 `md_editor` 消费接口。
- 第二步让 pulldown 适配器和 tree-sitter baseline 在同一批 fixture 上完全对齐，再决定是否切默认 backend。
- 第三步只有在 pulldown 端到端更快且语义全等时，才把它设成默认；comrak 保留为参考/正确性后端，不进入热路径。

## Assumptions

- parser backend 只负责粗结构，不负责精 marker。
- 细 marker / replacement / dependency 统一由共享 assembler 扫源码得到。
- `MarkdownSyntaxData` 先保留为兼容输出，等消费方完全迁移后再考虑改名或收口。
- pulldown 是默认候选热路径；comrak 是结构参考与正确性对照。
