# Final Plan: Typora-Style Markdown Editing

## Summary

目标是把当前 Markdown editor 的 Rendered mode 收敛成 Typora 风格主编辑体验：

- 打开和新建文档默认进入 Rendered mode。
- 用户日常看到的是富文本形态，而不是完整 Markdown 源码。
- 源文件仍然是唯一真相，保存结果仍然是标准 Markdown。
- Source mode 保留为逃生口，不改变其源码编辑行为。
- 表格保持当前 structured table 行级编辑方案，不在本轮做 grid editing。
- 默认复制仍复制 Markdown source；后续可单独增加 Copy Rendered Text。

本计划不是要引入私有富文本模型，也不是做只读 preview。最终编辑器仍通过 source Markdown edit 完成所有修改，undo/redo、dirty state、selection、hit-test 都围绕源文本坐标工作。

## Decisions

- **主体验**：Rendered mode 是默认模式，Source mode 保留。
- **源码露出策略**：严格 Typora。只有 collapsed caret 命中特定 token 或结构边界时，才局部 reveal 必要源码 marker。
- **Selection 行为**：非空 selection 不触发新的 raw reveal。
- **拖选行为**：mouse drag 期间冻结 drag 开始时的 projection，避免拖选过程中 reflow。
- **表格**：当前 table structured rendering 足够好，本轮不做 cell grid editing、行列增删、pipe 自动格式化。
- **Clipboard**：默认复制 Markdown source，和当前行为一致。
- **HTML**：继续 raw text 显示，不解析、不执行、不转 DOM。
- **架构边界**：不重新引入 Zed IDE product path，不引入 `project`、`workspace`、`editor`、`language`、`multi_buffer` 等 crate 到产品路径。

## Current State

当前代码已经有较好的底座：

- `markdown_wysiwyg` 使用 tree-sitter-backed Markdown block/inline parsing。
- `MarkdownSyntaxTree` 已能识别 heading、paragraph、list、task list、blockquote、table、code block、HTML、link reference、image/math、inline style 等结构。
- `MarkdownProjectionMap` 已支持 hide/replace，并能做 source/display 双向映射。
- `RenderedDisplayIndex` 已把 rendered mode 从 source row 逐步推进到 item/paragraph/heading/table row/structured block。
- paragraph merge、soft break、hard break、空段、Rendered Enter/Shift+Enter、部分 Backspace/Delete 已有实现。
- image/math 已有 source-backed rendered replacement 路径。
- table 已有 structured row layout、cache、hit-test、vertical movement 和 active source row reveal。
- task checkbox 点击已经能改写 `[ ]` / `[x]`。

主要缺口：

- active reveal 粒度仍偏粗，尤其 list/blockquote。
- mouse drag selection 还没有 projection freeze。
- list/blockquote 还缺 Typora 式 Enter/Backspace/Tab 行为。
- inline formatting commands 还不完整。
- list marker、ordered marker、quote bar、code block container、spacing 还没有清晰 presentation/adornment 层。

## Architecture

最终 rendered 编辑路径按职责分层：

```text
Markdown source
  -> markdown_wysiwyg: Markdown semantics and source ranges
  -> md_projection: rendered items and source/display topology
  -> DisplayRow: projected text plus rendered metadata
  -> layout: text layout, content origin, wrap width, row height
  -> render: text, replacements, adornments, containers
  -> interaction/actions: hit-test to source edit
```

### 1. Projection

Projection 只负责文本投影和映射：

- 隐藏 Markdown marker，例如 `# `、`**`、list marker、blockquote marker、fence marker。
- 替换 source span，例如 entity、escape、soft/hard break、task marker text projection。
- 维护 `source_to_display` 和 `display_to_source`。
- 不负责画 bullet、quote bar、背景、padding、spacing。

### 2. RenderedReplacement

`RenderedReplacement` 表示 source span 的富文本替代物。

当前属于这一类：

- image
- inline math
- block math

特征：

- 有明确 source span。
- 视觉对象替代这段 source。
- 在正文 flow 中占一个位置。
- 点击、删除、移动基本围绕整个 source span。

继续复用当前路径：

- `MarkdownInlineKind::Image`
- `MarkdownInlineKind::InlineMath`
- `RenderedElementDescriptor`
- `DisplayInlineAtom`
- `DisplayBlockLayout`

本轮不把 image/math 挪进 adornment。

### 3. RenderedAdornment

`RenderedAdornment` 表示 Markdown 结构产生的非文本视觉控件或装饰。

属于这一类：

- unordered bullet
- ordered marker
- task checkbox
- blockquote quote bar

特征：

- 通常不替代正文内容。
- 附着在 item/row/container 边缘。
- 影响 content indent、wrapped continuation、hit-test。
- 可以携带 source marker range，但不直接改 buffer。

建议数据形态：

```rust
struct RenderedAdornment {
    kind: RenderedAdornmentKind,
    source_range: Option<Range<usize>>,
    row_range: Range<usize>,
    placement: RenderedAdornmentPlacement,
}

enum RenderedAdornmentKind {
    ListBullet,
    OrderedMarker { text: String },
    TaskCheckbox { checked: bool },
    QuoteBar { depth: u16 },
}

enum RenderedAdornmentPlacement {
    Leading,
    BlockEdge,
    Overlay,
}
```

### 4. RenderedItemPresentation

`RenderedItemPresentation` 表示 item/block 的容器视觉和 spacing。

属于这一类：

- heading/paragraph/list/blockquote spacing
- code block background
- code block padding
- blockquote/list content padding
- rendered current item/container style

特征：

- 不对应单个可点击源码 token。
- 不参与文本 selection 内容。
- 影响 layout height、content origin 和 wrap width。

建议数据形态：

```rust
struct RenderedItemPresentation {
    before_spacing: RenderedSpacing,
    after_spacing: RenderedSpacing,
    content_padding: RenderedPadding,
    background: Option<RenderedBackgroundKind>,
    container: Option<RenderedContainerKind>,
}

enum RenderedBackgroundKind {
    CodeBlock,
    BlockQuote,
}

enum RenderedContainerKind {
    CodeBlock,
    BlockQuote { depth: u16 },
}
```

实际实现时应避免在可缓存数据里直接存 theme color；优先存 enum/key，由 render 层用 theme resolve。

### 5. StructuredLayout

表格属于 structured layout，不属于 replacement/adornment/presentation。

当前 `DisplayTableRowLayout` 继续保留：

- inactive table row 显示 grid。
- active table source row reveal Markdown row。
- delimiter row 显示 separator。
- table layout cache 继续按 table source range、wrap width、row style 复用。

## Implementation Plan

### Phase 1: Default Rendered Mode And Strict Reveal

目标：先把体验从 rendered preview-ish 调整成 Typora 主路径。

Changes:

- `markdown_editor` shell 新建/打开文档默认使用 `MarkdownEditorMode::Rendered`。
- Source/Rendered 切换按钮保留。
- Source mode 行为不变。
- `active_source_range_for_selection` 调整为：
  - 非空 selection 返回 `None`，不触发 raw reveal。
  - collapsed caret 根据具体 span/marker/token 判断 reveal。
  - caret 位于 image/math 边界时保持 rendered，进入内容/marker 时 reveal。
- list/blockquote marker reveal 改为 marker-hit reveal：
  - caret 在正文中不 reveal `- `、`1. `、`>`。
  - caret 在 marker source range 或 marker 边界附近时 reveal。
- 更新 active projection dependency：
  - marker dependency 仍保留 owner range。
  - reveal 决策不能再简单等价于 owner source range active。

Acceptance:

- 点击普通 list item 正文时，仍显示 bullet/adornment，不显示 `- `。
- 点击 list marker 区域时，可以 reveal source marker。
- 选中 `**bold**` 对应文字时不强制显示 `**`。
- Source mode 与当前一致。

### Phase 2: Drag Projection Freeze

目标：拖选时不因为 selection 变化不断切换 projection。

Changes:

- 在 `MarkdownEditor` 增加 rendered drag state：

```rust
rendered_drag_projection_state: Option<RenderedProjectionState>
```

- mouse down:
  - Rendered mode 下保存当前 projection state。
  - task checkbox click 这种直接 action 不进入 drag freeze。
- mouse move:
  - 如果处于 drag selection，则 cached display row/layout 使用 frozen projection state。
- mouse up:
  - 清除 frozen state。
  - 根据最终 selection 重新同步 affected rendered items。
- cache key 增加 projection signature，确保 frozen/non-frozen state 不互相污染。

Acceptance:

- 鼠标拖选跨 bold/link/list/table/image/math 时不出现反复 reveal/reflow。
- 拖选结束后，collapsed selection 才按最终 caret 位置 reveal。
- 非空 selection 保持 rendered projection。

### Phase 3: Rendered Adornment And Presentation Layer

目标：把 Typora 视觉结构从 projected text 中分离出来。

Changes:

- `DisplayRow` 增加：

```rust
presentation: RenderedItemPresentation,
adornments: Vec<RenderedAdornment>,
```

- `rendered_display_row` 构建时派生 presentation/adornments。
- 不在 render/layout 层重新扫描 Markdown。
- list/task/blockquote marker 从 Markdown semantics 派生：
  - unordered list -> `ListBullet`
  - ordered list -> `OrderedMarker`
  - task list -> `TaskCheckbox`
  - blockquote depth -> `QuoteBar`
- `display_row.rendered_indent_width()` 升级为更明确的 layout helpers：

```rust
content_origin_x()
content_wrap_width(wrap_width)
```

- selection/caret/text layout 使用 content origin。
- adornments 画在 leading lane 或 block edge。
- quote bar 和 list marker 不参与普通 selected text。

Acceptance:

- list wrapped continuation 与正文对齐。
- blockquote depth 正确显示 quote bar。
- task checkbox 是可点击控件，不只是文本字符。
- selection highlight 不把 quote bar/bullet 当正文文本处理。

### Phase 4: Inline Formatting Commands

目标：用户不需要手写常见 inline marker。

Add actions:

- `ToggleBold`
- `ToggleItalic`
- `ToggleInlineCode`
- `ToggleStrikethrough`
- `InsertLink`
- `EditLink`
- `ToggleSourceRevealCurrentBlock`

Default keybindings:

- `ctrl-b` / `cmd-b` -> `ToggleBold`
- `ctrl-i` / `cmd-i` -> `ToggleItalic`
- inline code、strikethrough、link 可先只暴露 action，不必须马上加默认快捷键，避免和现有键位冲突。

Behavior:

- collapsed selection:
  - 在普通文本中插入成对 marker。
  - caret 放在 marker 内部。
- non-empty selection:
  - selection 未在同类 span 内：包裹 marker。
  - selection 完整覆盖同类 span 内容：移除 marker。
- 遇到复杂结构：
  - table active source row 可以按 source text 操作。
  - image/math/code block 不猜测包裹，退化为 reveal 或 no-op。
- 所有命令通过 source edit 实现，记录 undo/redo selection history。

Acceptance:

- Toggle bold/italic/code/strikethrough 对 source 产生最小 edit。
- undo/redo 恢复 source 和 selection。
- CJK/emoji selection 不产生非 char-boundary range。
- rendered text projection 正确更新。

### Phase 5: List And Blockquote Editing

目标：补齐 Typora 高频写作行为。

Rendered Enter:

- 普通段落：保留当前 paragraph split / empty paragraph 逻辑。
- list item 非空：插入同级 list marker。
- task list 非空：插入同级 unchecked task marker。
- ordered list 非空：插入下一编号 marker。
- 空 list item：退出 list，删除当前 marker，生成普通段落位置。
- blockquote 非空：续 `>`。
- 空 blockquote：退出 quote。

Rendered Backspace:

- list item 内容开头：退出当前 list 层级或移除 marker。
- 空 list item：退出 list。
- blockquote 内容开头：退出 quote。
- 其他位置保留当前 source deletion / rendered replacement deletion 行为。

Rendered Tab / Shift+Tab:

- list item 内调整层级。
- 非 list 中保留当前 tab/soft tab 插入。
- Shift+Tab 需新增 action/keybinding。

Acceptance:

- `- item` 行尾 Enter 生成下一项。
- 空 `- ` Enter 退出 list。
- task item Enter 生成 `- [ ] `。
- ordered item Enter 生成下一编号。
- nested list Tab/Shift+Tab 调整 source indent。
- blockquote Enter/Backspace 行为符合 Typora 写作预期。

### Phase 6: Code Block Presentation

目标：代码块看起来像代码块，同时保持逐 source row 编辑。

Changes:

- fenced/indented code content rows 保持普通 text editing path。
- inactive fence/info rows 继续隐藏。
- active fence/info row reveal source text。
- code content rows 获得 continuous visual container：
  - background
  - vertical padding
  - horizontal padding
  - raw/code styling
- 不把整个 code block 合成一个 single block widget。
- content row hit-test、vertical movement、selection 继续按 source row/item 工作。

Acceptance:

- code block 内容可逐行编辑。
- fence 行 inactive 时不可见，active 边界可 reveal。
- code block background 连续，不破坏 caret/selection。

### Phase 7: Spacing And Visual Polish

目标：补足 Typora 的文档节奏，但不污染源码。

Changes:

- heading spacing 通过 `RenderedItemPresentation` 表达。
- paragraph/list/blockquote spacing 通过 item layout metadata 表达。
- separator blank source rows 不再直接显示成可见空行，仍由 item/blank role 表达段间关系。
- spacing 不生成虚拟 source row。

Recommended defaults:

- heading spacing 先采用现有 GFM plan 数值：
  - H1: top 10px, bottom 6px
  - H2: top 8px, bottom 4px
  - H3: top 6px, bottom 3px
  - H4-H6: top 4px, bottom 2px
- paragraph after: 6px
- list item: 2px
- blockquote outside: 6px

Acceptance:

- 文档看起来像连续富文本，而不是源码行列表。
- source row/item mapping 不被 spacing 破坏。
- scrollbar/item height 正确更新。

## Testing Plan

### markdown_wysiwyg Tests

- list marker extraction:
  - unordered
  - ordered `1.` / `1)`
  - task checked/unchecked
  - nested list
  - blockquote + list combination
- blockquote marker ranges by row and depth。
- source range helpers do not panic on malformed Markdown、CJK、emoji、CRLF。

### Projection Tests

- collapsed caret in inline content reveals matching marker only when appropriate。
- non-empty selection does not reveal raw markers。
- caret in list item content does not reveal list marker。
- caret in marker source range reveals marker。
- caret in blockquote content does not reveal `>`。
- caret near image/math boundaries keeps rendered replacement inactive。
- source/display mapping remains char-boundary safe。

### Layout/Render Tests

- list marker adornment renders in leading lane。
- ordered marker text uses source marker number/delimiter。
- task checkbox visual state follows `[ ]` / `[x]` / `[X]`。
- wrapped continuation aligns with content, not marker start。
- blockquote quote bar depth matches nested quote depth。
- code block background/padding does not alter code row source range。
- heading/paragraph spacing changes item height without source mutation。
- table existing rendered tests continue passing。

### Interaction Tests

- mouse click on checkbox toggles source marker。
- mouse click on list marker places caret/reveal near marker。
- mouse click on list content does not reveal source marker。
- drag selection across rendered inline/list/table/image/code does not reflow mid-drag。
- Shift-selection keeps rendered projection while non-empty。
- vertical movement across paragraph visual rows、list rows、blockquote rows、table rows、code rows remains stable。

### Editing Tests

- Toggle bold/italic/inline code/strikethrough:
  - collapsed insert
  - selection wrap
  - already formatted unwrap
  - undo/redo
- Insert/Edit link:
  - collapsed insertion
  - selection link wrapping
  - source fallback for malformed link
- list Enter/Backspace/Tab/Shift+Tab。
- blockquote Enter/Backspace。
- task list continuation。
- source mode behavior unchanged。

### Validation Commands

- `cargo test -p markdown_wysiwyg`
- `cargo test -p md_editor --lib`
- `cargo check -p updraft_editor`
- `git diff --check`

## Acceptance Criteria

本轮完成后应满足：

- 打开文件后默认进入 Rendered mode。
- 用户可以像 Typora 一样写标题、段落、bold、italic、inline code、strikethrough、link、list、task list、blockquote、code block。
- 大部分 Markdown marker 默认隐藏。
- 只有 caret 命中具体 token/边界时才局部 reveal。
- 鼠标拖选不触发布局抖动。
- 表格维持当前 structured row 体验，不退化。
- image/math 维持现有 rendered replacement 体验，不被新 adornment 层破坏。
- 保存文件仍是标准 Markdown。
- 默认复制仍是 Markdown source。
- Source mode 可随时切回完整源码。

## Non-Goals

- 不做 table grid editing。
- 不做 table row/column insert/delete controls。
- 不做 Copy as HTML。
- 不做默认 rendered plain-text copy。
- 不执行或渲染 HTML DOM。
- 不做 Mermaid。
- 不做 footnote/front matter 完整 Typora 语义。
- 不引入私有富文本文档模型。
- 不重新引入 Zed IDE product path。

## Implementation Order

建议按以下顺序实施，避免大范围返工：

1. 默认 Rendered mode。
2. 严格 active reveal 和非空 selection 不 reveal。
3. drag projection freeze。
4. `RenderedAdornment` / `RenderedItemPresentation` 数据结构，但先只接 list/task/quote。
5. layout content origin/wrap width 改造。
6. adornment render + hit-test。
7. inline formatting commands。
8. list/blockquote Enter/Backspace/Tab 行为。
9. code block presentation。
10. spacing polish。
11. 全量测试和 validation。

每一步都应保持 Source mode 不变，并优先补 targeted regression tests。

## Progress Log

### 2026-05-31

- Phase 1 部分完成：`markdown_editor` shell 新建/打开默认进入 Rendered mode。
- Phase 1 严格 reveal 部分推进：非空 selection 不再触发 raw reveal；list/blockquote/list item/task marker 只有 caret 命中 marker 或 marker 边界时才 reveal，正文内部保持 rendered。
- 更新 regression tests 覆盖 list/blockquote marker-hit reveal、非空 selection 隐藏 marker、部分选中 inline atom 保持 rendered。
- 验证：`cargo test -p markdown_wysiwyg` 通过；`cargo test -p md_editor --lib` 通过；`cargo fmt --check` 通过。
