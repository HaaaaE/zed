# Rendered Mode GFM Plan

## Summary

当前 `md_editor` 的 rendered mode 仍以 source row 为核心：`DisplayRow.row`、cache key、`MdListState` item index、gutter、vertical movement、hit-test 都默认一项对应一行源码。表格和 image/math 已有局部结构化 layout，inline projection/selection 也已有可复用基础。

目标是：

- source mode 不动。
- rendered mode 的视觉语义按 GFM。
- HTML 永远显示源码文本，不解析、不执行、不转 DOM。
- 编辑坐标继续以 source `Point` 和 byte range 为真相。
- active reveal 使用 token/行混合粒度，不粗暴展开整段。

## Key Changes

- 增加 rendered-only `RenderedDisplayIndex`，按 snapshot version 缓存，稳定映射：
  - `item_index -> source_range/row_range/kind`
  - `source_row/source_offset -> item_index`
  - rendered mode 的 item count 来自 index，source mode 继续来自 text row count。
- 引入 `DisplayItemId` 和 `DisplayItemLayout` 适配层；先让每个 item 仍包装现有 `DisplayRow`，再逐步支持 paragraph/code/thematic/link-ref 等非 1:1 item。
- 将 rendered gutter 改成 mode-aware left rail：
  - source mode 保留 `48px` 行号。
  - rendered mode 最终使用 `24px` rail，不显示 source 行号，只在 active item 显示 subtle marker。
  - 所有 mouse x、block/table hit-test、wrap width 从硬编码 `gutter_width()` 改为 `left_rail_width(mode)`。
- 保持 selection/copy/cut/delete/backspace 的 source range 语义；新增 item 级 projection/highlight，把 source selection clip 到 item source range 后投影到 display ranges。

## Implementation Changes

### Rendered Item Index

- `Paragraph`：inactive 时合并同一个 Markdown paragraph 的多 source rows，soft break 渲染为空格；超过 `8192 bytes` 或 `128 rows` 的 paragraph 降级为 row-chunked items，避免超大 shaping。
- `PipeTable`：继续一 source row 一个 rendered item；inactive 走现有 structured table；active reveal 当前 source row。
- `FencedCodeBlock`/`IndentedCodeBlock`：inactive 合成 code block item，隐藏 fence/info marker，保留 raw code 内容与换行；cursor 在 fence/info 行时 reveal 该 source row。
- `ThematicBreak`：inactive 单 item 画 horizontal rule；active 时显示 source row。
- `LinkReferenceDefinition`：inactive 保留 0-height item 以稳定 index；active 时显示 source text。
- `HtmlBlock`/`InlineHtml`：不执行、不渲染 HTML，始终按源码文本显示，可用 muted/raw 样式降噪。

### Projection and Text Layout

- 扩展 rendered projection 使用现有 `MarkdownProjectionOperation::Replace`：
  - inactive soft break -> `" "`
  - inactive hard break -> forced visual break
- 将 text layout 从单 `ShapedLine` 升级为可包含 forced breaks 的 flow layout；`VisualDisplayRow` 记录所属 shaped line，所有 `display_x_for_offset`、mouse target、selection bounds 通过 layout helper 访问，不再直接依赖单行 `shaped_line`。
- paragraph item 的 `projection.source_to_display/display_to_source` 必须覆盖跨行 source range，包含 newline replacement、inline marker hiding、entity/escape replacement、inline atom insertion。

### Active Reveal Policy

- Inline strong/emphasis/code/link/image/math：只 reveal 当前 inline span/token，不 reveal 整行或整段。
- Table：active item reveal 当前 table source row，保持现有行为。
- Soft break：只有 caret/selection overlap 或贴近该 break source range 时 reveal 该 break；其它 soft breaks 仍显示为空格。
- Hard break：inactive 显示 GFM line break；active 在 break marker 附近显示源码 marker。
- List/blockquote marker：只 reveal 当前编辑命中的 list/quote marker 所在 source row/token；inactive 用 adornment 画 bullet/number/quote bar。
- Image/math block：inactive 显示 rendered block；cursor 在边界保持 block，进入 source 内容或 marker 时 reveal 对应 source token/row。

### Presentation

- 新增 `RenderedBlockPresentation`/`DisplayAdornment` 内部模型，集中定义 spacing、quote bar、list marker、container bg、active reveal style。
- Heading 使用现有 heading metrics，并加 rendered block spacing：
  - H1 `10/6px`
  - H2 `8/4px`
  - H3 `6/3px`
  - H4-H6 `4/2px`
- Paragraph after spacing `6px`；list item spacing `2px`；blockquote 外侧 `6px`，内部紧凑。
- List marker 作为 adornment 绘制：unordered bullet、ordered marker、task checkbox；wrapped continuation 与内容文本对齐。
- Blockquote 按 depth 画 `2px` quote bar，gap `10px`，不改变 source selection。
- Code block 使用 monospace-like raw styling、background、`8px` vertical/`10px` horizontal padding；table/header/border 沿用现有 table layout 并微调 header bg、cell padding。

### Caching and Perf

- Rendered index 只按 snapshot version 重建；selection 移动不重建 index。
- Display/layout cache key 从 `row` 扩展为 `item_id + source_range + projection_signature + wrap_width + row_style`。
- Active reveal 变化只 remeasure previous/current affected item。
- Rendered prewarm 改为按 item index，使用 inactive projection 预热，避免 selection 每次移动重置整条 prewarm queue。
- Async image/math measurement 继续只清 affected item/row。

## Test Plan

### Unit Tests

- rendered index：paragraph merge、table row keep 1:1、code block item、0-height link reference、source row/item 双向映射。
- projection：soft break -> space、hard break -> forced line、active soft/hard break reveal、HTML raw text。
- selection：multi-row paragraph selection highlight、copy/cut/delete 仍返回/修改 source text。
- active reveal：bold token、link token、table row、table cell inline token、soft break neighbor、thematic/code/link-ref active cases。

### GPUI Interaction Tests

- rendered gutter 不显示 source line number，click x 使用 rendered rail。
- mouse hit-test/drag selection 跨 merged paragraph、跨 item、跨 table row。
- vertical movement 在 paragraph visual rows、table rows、code block、image/math block 间稳定。
- task checkbox click 继续 toggle source marker。

### Visual/Layout Tests

- list marker 与 wrapped continuation 对齐。
- blockquote bar depth 正确。
- thematic break/code block/table delimiter/header/image/math placeholder 尺寸稳定。

### Validation Commands

- `cargo test -p markdown_wysiwyg`
- `cargo test -p md_editor`
- `cargo check -p updraft_editor`
- perf triage 时运行 `cargo perf-test -p md_editor`，source segments 不退化超过 5%，rendered cached redraw/scroll 不退化超过 10%，rendered first draw 不退化超过 15%。

## Assumptions

- HTML 不进入 rendered semantics：不解析、不执行、不转 DOM，只显示源码文本。
- 默认复制/剪切仍是 Markdown source；不新增 "copy rendered text"。
- source mode 行为、cache fast path、row-based list state 不做重构。
- 为控制性能，超长 paragraph 允许降级为 chunked rendered items；普通文档必须按 GFM soft break 合并显示。
- v1 不追求 GitHub CSS 像素级复刻，只追求 GFM 语义等价、编辑稳定、视觉完整可用。
