# Markdown 语法层重构计划

## Summary

- 目标不是直接“换 parser”，而是把 parser 退回到粗结构层，把细 marker、projection、table cell、rendered element 统一交给共享语义层。
- 当前 `MarkdownSyntaxData` 先保留为兼容输出，不再作为新架构的目标 IR；`MarkdownSyntaxTree` 继续做对外门面。
- 热路径优先验证 pulldown；comrak 作为结构校验/参考后端；tree-sitter 继续作为现有生产 baseline。

## Progress

- 2026-06-01：已确认仓库当前实现仍以 tree-sitter 为生产 baseline，`MarkdownSyntaxData` 仍是对外兼容输出。
- 2026-06-01：`tooling/markdown_syntax_bench` 已加入 pulldown 适配器雏形和 `BenchmarkSyntaxData` 兼容快照，用于和生产输出做精确对比。
- 2026-06-01：`markdown_wysiwyg` 侧仍是 `parser.rs` 直接产出解析树，`blocks.rs`、`inline.rs`、`tables.rs` 仍依赖 tree-sitter 节点；共享 `MarkdownStructure` / `MarkdownSemanticsAssembler` 还未落地。
- 2026-06-01：下一步应先补齐 backend trait 和粗结构层，再把 benchmark 从“兼容对比”推进到“结构/语义 diff 报告”。
- 2026-06-01：已在 `markdown_wysiwyg` 里补入 `MarkdownBackend` 边界和 `MarkdownStructure` 载体，现有 tree-sitter 路径已通过新边界返回原有 `MarkdownSyntaxData`。
- 2026-06-01：已验证 `cargo check -p markdown_wysiwyg`、`cargo test -p md_sum_tree`、`cargo test -p md_rope`、`cargo test -p md_text` 全部通过。
- 2026-06-01：`tooling/markdown_syntax_bench` 仍受 `entities` 版本冲突影响，当前无法直接 `cargo check`，这是现有依赖状态问题，不是本轮实现引入的回归。
- 2026-06-01：已加入 `MarkdownSemanticsAssembler`，把现有 full/incremental 语义拼装入口收束到 `MarkdownStructure -> MarkdownSyntaxData` 边界下；当前 assembler 内部仍复用 tree-sitter 解析树和原有 helper。
- 2026-06-01：再次验证 `cargo check -p markdown_wysiwyg`、`cargo test -p md_sum_tree`、`cargo test -p md_rope`、`cargo test -p md_text` 全部通过。
- 2026-06-01：`MarkdownStructure` 已开始承载粗结构块列表，包含 block kind、source range、row range，并在 assembler 入口做 debug 一致性检查；语义输出仍保持原有 tree-sitter helper 生成。
- 2026-06-01：第三轮验证 `cargo check -p markdown_wysiwyg`、`cargo test -p md_sum_tree`、`cargo test -p md_rope`、`cargo test -p md_text` 全部通过。
- 2026-06-01：`MarkdownStructure` 现在真正驱动 full parse 的 block 产出，`MarkdownBlock::from_structure` 已接入 assembler；`blocks.rs` 里的旧 full-parse 收集器暂时仅保留为未接线实现。
- 2026-06-01：第四轮验证 `cargo check -p markdown_wysiwyg` 与 `cargo test -p markdown_wysiwyg` 全部通过，现有 Markdown 语义回归测试未见失败。
- 2026-06-01：增量 parse 的 block 语义也已改为从当前 `MarkdownStructure` 生成，block 层 full/incremental 路径统一到结构层；旧 `blocks.rs` helper 已作为退役对照模块局部保留。
- 2026-06-01：第五轮验证 `cargo check -p markdown_wysiwyg` 与 `cargo test -p markdown_wysiwyg` 全部通过且无新增警告。
- 2026-06-01：table 入口也已迁到 `MarkdownStructure` 的 `PipeTable` 结构块，`MarkdownBlock` -> `MarkdownTable` 的转换保留在 assembler 侧；旧 `tables.rs` 仍保留单元格/行扫描实现作为 helper。
- 2026-06-01：第六轮验证 `cargo check -p markdown_wysiwyg` 与 `cargo test -p markdown_wysiwyg` 全部通过。
- 2026-06-01：full parse 的 inline span 与 projection replacement 入口已通过 `MarkdownStructure` 暴露的 inline tree 访问边界收集；旧 `inline.rs` full-parse helper 已作为退役对照入口局部保留。
- 2026-06-01：第七轮验证 `cargo check -p markdown_wysiwyg` 与 `cargo test -p markdown_wysiwyg` 全部通过且无新增警告。
- 2026-06-01：增量 inline span 与 projection replacement helper 的当前输入也已迁到 `MarkdownStructure`，dirty-window 复用逻辑保持不变，并移除了 `MarkdownStructure::parse_tree` 退役入口。
- 2026-06-01：第八轮验证 `cargo check -p markdown_wysiwyg` 与 `cargo test -p markdown_wysiwyg` 全部通过且无新增警告。
- 2026-06-01：block structure 构建 helper 已从 `markdown_wysiwyg.rs` 收回到 `blocks.rs`，`blocks.rs` 不再作为退役 tree-sitter block 对照模块；第九轮验证 `cargo check -p markdown_wysiwyg` 与 `cargo test -p markdown_wysiwyg` 全部通过。
- 2026-06-01：已清理 `inline.rs`、`tables.rs` 和 `markdown_wysiwyg.rs` 中剩余的退役 dead-code helper 入口，去掉旧 whole-tree inline/table wrapper 与旧 block row-shift 复用路径；第十轮验证 `cargo check -p markdown_wysiwyg` 与 `cargo test -p markdown_wysiwyg` 全部通过。
- 2026-06-01：full parse 的 structure table / inline span / projection replacement 汇总 wrapper 已从 `markdown_wysiwyg.rs` 移回 `tables.rs` 与 `inline.rs`，assembler 入口只保留编排调用；第十一轮验证 `cargo check -p markdown_wysiwyg` 与 `cargo test -p markdown_wysiwyg` 全部通过。
- 2026-06-01：block 语义组装 helper 已继续回收到 `blocks.rs`，包括 structure block 校验、blank block 补齐、full/incremental `MarkdownBlock` 汇总与增量 dedup；第十二轮验证 `cargo check -p markdown_wysiwyg` 与 `cargo test -p markdown_wysiwyg` 全部通过。
- 2026-06-01：增量 inline span、projection replacement 与 projection marker dependency 的复用 helper 已回收到 `inline.rs`，`markdown_wysiwyg.rs` 继续只负责 assembler 编排；第十三轮验证 `cargo check -p markdown_wysiwyg` 与 `cargo test -p markdown_wysiwyg` 全部通过。

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
- benchmark 只负责测量和 diff，不再承担 parser 兼容层的主逻辑。
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
- 当前已存在的 benchmark 快照只能作为兼容回归检查，不能替代语义等价测试。

## Rollout

- 第一步只改 `markdown_wysiwyg` 和 benchmark，不改 `md_editor` 消费接口。
- 第二步让 pulldown 适配器和 tree-sitter baseline 在同一批 fixture 上完全对齐，再决定是否切默认 backend。
- 第三步只有在 pulldown 端到端更快且语义全等时，才把它设成默认；comrak 保留为参考/正确性后端，不进入热路径。
- 当前状态已完成第一步中的 benchmark 雏形，并在 `markdown_wysiwyg` 主体里建立 backend / structure / assembler 的最小边界；`MarkdownStructure` 已实际驱动 full/incremental block 输出、table 生成、full/incremental inline 与 projection 收集，block structure 构建和 block 语义组装逻辑已回收到 `blocks.rs`，full/incremental inline 与 projection helper 已回收到 `inline.rs`，table wrapper 已回收到 `tables.rs`，退役 dead-code 对照入口已清理，下一步是继续把剩余共享 helper 拆回模块边界。

## Assumptions

- parser backend 只负责粗结构，不负责精 marker。
- 细 marker / replacement / dependency 统一由共享 assembler 扫源码得到。
- `MarkdownSyntaxData` 先保留为兼容输出，等消费方完全迁移后再考虑改名或收口。
- pulldown 是默认候选热路径；comrak 是结构参考与正确性对照。
