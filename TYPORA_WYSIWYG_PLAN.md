# Typora-style WYSIWYG Markdown Plan

## 目标

把当前单文件 Markdown 编辑器从“源码编辑 + 预览”推进到完整 Typora 语义的所见即所得 Markdown 编辑器。

最终产品仍然是单文件 Markdown 编辑器：磁盘上的真实文件保持 Markdown 文本格式，应用内部不引入 workspace、project、多文档、标签页或富文本文档私有格式。用户编辑时看到的是接近最终文档的富文本形态，保存时得到的是可移植、可读、可被其他 Markdown 工具打开的 `.md` / `.markdown` 文件。

当前计划跳过“源码编辑器 + Markdown 语法增强”阶段，直接进入 focused-block WYSIWYG，再逐步走到完整 Typora 语义。

## 最终成果定义

最终成果不是一个 Markdown preview，也不是一个把 Markdown 渲染成只读页面的组件，而是一个可编辑的 Markdown 投影编辑器。

最终版本必须满足：

- 源文件仍然是 Markdown 文本，`Buffer` 中的源文本是唯一真相。
- 编辑区默认显示富文本结果，而不是完整 Markdown 源码。
- 用户可以直接编辑标题、段落、粗体、斜体、链接、图片、列表、引用、代码块、表格、任务列表等结构。
- 用户不需要在日常写作中手动管理大部分 Markdown 标记。
- 保存、另存为、dirty 状态、撤销、重做、查找、选择、复制、粘贴、鼠标定位、键盘移动、IME 输入都稳定工作。
- 复制到纯文本时符合用户预期，复制为 Markdown 时保留源语义，粘贴 Markdown / HTML / 纯文本时都能合理进入文档。
- 不依赖 `markdown` crate 作为编辑或预览渲染路径。最终 `markdown_editor` 不再需要 `markdown` crate。
- 不暴露 Zed 的 workspace、project、file tree、tabs、Git UI、terminal、debugger、AI、collaboration、remote dev 等 IDE 功能。

## 约束

- 优先复用 Zed 的 `Editor`、`Buffer`、`MultiBuffer`、GPUI、theme、settings、keymap、text layout、selection、undo/redo 和 display map 经验。
- 不为了快速实现而重写一个独立富文本编辑器。需要先证明 Zed 编辑器基础设施不能满足某个具体需求，才允许局部新增抽象。
- 不把 Markdown 结构保存为私有 JSON、HTML、Delta、ProseMirror document 或其他富文本模型。
- 不用 `markdown` crate 作为最终 WYSIWYG 渲染层。计划中可以参考其行为、fixtures 和视觉结果，但不能把它作为最终编辑区依赖。
- 不把 current file 变成 project。图片路径、链接路径、资源解析都以当前 Markdown 文件所在目录为基准。
- 不引入多文档、多标签或 workspace 心智。
- 第一阶段不以瘦身依赖图为目标，但 WYSIWYG 主路径不能新增临时解析器或临时渲染层。任何过渡依赖只能服务于旧 preview 或迁移验证，不能进入 Typora 编辑路径。

## 核心判断

完整 Typora 语义的难点不是 Markdown 解析，也不是 GPUI 画富文本。难点是同一份源 Markdown 文本同时具备两套坐标系统：

- 源坐标：真实 buffer offset、row、column，包括所有 Markdown 标记。
- 显示坐标：用户看到和编辑的富文本坐标，很多 Markdown 标记被隐藏、替换或折叠。

所有输入、删除、光标移动、鼠标点击、选择、复制、撤销、查找、高亮、软换行、滚动、IME composition 都必须稳定地在这两套坐标之间转换。

因此核心工作是构建一个可编辑 Markdown 投影层，而不是构建另一个 preview。

## 推荐架构

### Source Of Truth

`language::Buffer` 继续保存原始 Markdown 文本。所有保存、dirty、undo/redo、selection transaction 都围绕 buffer 进行。

原则：

- 源 Markdown 不被隐藏状态污染。
- 任何富文本编辑最终都转成一组最小 Markdown 源文本 edit。
- Undo/redo 仍然走 Zed 现有 edit transaction，不能绕开 buffer。
- 文档换行、编码、尾随换行、路径大小写等文件级细节尽量保留。

### Markdown Semantic Tree

新增一个 Markdown 语义树层，暂称 `MarkdownSyntaxTree`。

职责：

- 从 buffer snapshot 解析 Markdown。
- 保存 block 节点和 inline 节点的源 range。
- 给稳定节点分配 `MarkdownNodeId`，用于缓存、点击、命令、block element 和增量更新。
- 区分真实内容 range、marker range、container range、generated display range。
- 记录当前解析版本，和 `Buffer` snapshot version 对齐。

解析器路线已经确定为 tree-sitter-backed，而不是手写 Markdown 扫描器。低层核心 crate `markdown_wysiwyg` 必须同时使用 tree-sitter Markdown block grammar 和 inline grammar，保存 block tree 与 inline trees。`pulldown-cmark` 可以作为兼容性参考和测试 oracle，但不进入编辑投影主路径。

### Markdown Projection Map

新增一个可编辑投影层，暂称 `MarkdownProjectionMap`。

职责：

- 把源 Markdown range 转成显示 range。
- 隐藏 marker，例如 `# `、`**`、`_`、`` ` ``、link destination、list marker、blockquote marker、fence marker。
- 替换结构，例如 checkbox、image、table cell separator、folded code fence chrome。
- 给内容附加样式，例如 heading font size、bold、italic、inline code、link color、quote border、code block background。
- 提供 source-to-display 和 display-to-source 双向映射。
- 支持 hit testing、selection、copy、find highlights、IME composition 和 soft wrap。

不要把完整 Typora 逻辑塞进现有 `fold_map`。`fold_map`、`inlay_map`、`block_map` 和 `custom_highlights` 是 editor display pipeline 的机制层，只能消费 `MarkdownProjectionMap` 产出的投影结果；Markdown 语义、marker 边界、source/display 映射和 focused-block reveal 必须归属于 `markdown_wysiwyg`，不能伪装成普通 code fold。

### Editor Integration

短期策略：在 `markdown_editor` crate 里包一层 Markdown WYSIWYG controller，把 `markdown_wysiwyg` 的 tree-sitter-backed semantic tree 和 projection 输出同步到 `Editor` 的 display pipeline。

长期策略：为 `editor` crate 增加通用投影/decoration 扩展点，让 Markdown WYSIWYG 以插件式 display transform 接入，而不是把 Markdown 逻辑硬编码到通用 editor。

需要重点复用或改造的 Zed 区域：

- `crates/editor/src/display_map.rs`
- `crates/editor/src/display_map/fold_map.rs`
- `crates/editor/src/display_map/inlay_map.rs`
- `crates/editor/src/display_map/block_map.rs`
- `crates/editor/src/display_map/custom_highlights.rs`
- `crates/editor/src/display_map/wrap_map.rs`
- `crates/editor/src/element.rs`
- `crates/editor/src/movement.rs`
- `crates/editor/src/selections_collection.rs`
- `crates/editor/src/actions.rs`
- `crates/language/src/buffer.rs`
- `crates/multi_buffer/src/multi_buffer.rs`

### Rendering Layer

最终渲染不能走 `markdown::MarkdownElement`。WYSIWYG 渲染要在 editor display pipeline 内完成，这样光标、selection、hit test、scroll、find、IME、undo 才能自然工作。

渲染分三类：

- Inline styling：heading text、bold、italic、strikethrough、inline code、link、highlight。
- Inline replacement：hidden marker、checkbox、link URL popover、image placeholder。
- Block replacement：image block、code fence block、table block、HTML block、Mermaid block、math block。

## Markdown 语义覆盖范围

最终完整版本至少覆盖以下 Markdown/GFM 结构。

### Inline

- Emphasis：`*text*`、`_text_`
- Strong：`**text**`、`__text__`
- Strong emphasis nesting
- Strikethrough：`~~text~~`
- Inline code：`` `code` ``，包括反引号长度处理
- Link：`[text](url)`、`[text][ref]`、自动链接
- Image：`![alt](src)`、reference image
- Escape：反斜杠转义
- Entity：HTML entity 显示和保存
- Hard break 和 soft break
- Footnote reference
- Plain URL detection，如果最终产品决定支持自动链接

### Blocks

- Paragraph
- ATX heading：`#` 到 `######`
- Setext heading
- Thematic break
- Blockquote，含嵌套 block
- Unordered list
- Ordered list
- Nested list
- Task list checkbox
- Indented code block
- Fenced code block，含 language info string
- Table，含 alignment
- HTML block
- Front matter / metadata block
- Footnote definition
- Mermaid diagram block
- Math block，如果产品决定与 Typora 行为对齐

### Resource Handling

- 相对路径基于当前 Markdown 文件所在目录。
- 未保存文件里的相对图片路径需要有明确降级显示。
- 图片加载失败要显示可编辑 fallback，不丢失源码。
- 远程图片默认需要安全策略，避免阻塞 UI 或隐式泄露路径上下文。
- HTML 渲染默认禁用脚本和危险交互。
- Mermaid / math 等需要异步渲染、错误状态、取消旧任务和缓存。

## 编辑语义要求

### 光标和选择

必须定义并测试：

- 点击富文本内容后定位到正确源 offset。
- 点击隐藏 marker 附近时选择最接近的可编辑语义位置。
- 左右移动跨过隐藏 marker 时不产生视觉卡顿或跳进不可见字符。
- Shift selection 跨越隐藏 marker 时范围稳定。
- 多光标如果保留，必须不破坏 Markdown 结构；如果阶段内暂不支持，需要显式禁用或降级。
- 只有 collapsed caret 位于 Markdown 语义元素内部时，才 reveal 该元素的 Markdown 原型。
- 非空 selection 不触发新的 raw reveal。
- 拖拽选择开始后冻结拖拽开始前的 projection 状态：如果拖拽开始时该元素已 reveal，则拖拽期间保持 reveal；如果拖拽开始时是 rendered，则拖拽期间保持 rendered。
- 拖拽过程中不能因为 selection range 变化、鼠标移动到其他元素或 hover 状态改变而重排 layout。
- mouse up 后再根据最终状态重新计算 projection：非空 selection 保持 selection projection；collapsed selection 根据 caret 所在语义元素 reveal。

### 输入

必须支持：

- 普通文本输入。
- 中文、日文、韩文等 IME composition。
- Enter、Backspace、Delete、Tab、Shift-Tab。
- 粘贴纯文本、Markdown、HTML。
- 输入触发结构转换，例如 `# `、`- `、`1. `、`> `、```。
- 输入成对 marker 时的合理合并和转义。

### 删除和退化

必须支持：

- 在 heading 开头 Backspace 退化为 paragraph。
- 在空 list item 上 Enter 或 Backspace 退出 list。
- 删除 link text 时处理 link marker。
- 删除 image 时删除整个 image syntax 或进入源码 reveal 状态。
- 删除 table cell 边界时保持表格结构或明确退化为 plain text。
- 删除 code fence 边界时进入源码 reveal，避免无意破坏整个代码块。

### 格式命令

至少需要这些命令：

- Toggle bold
- Toggle italic
- Toggle strikethrough
- Toggle inline code
- Toggle heading level
- Toggle blockquote
- Toggle unordered list
- Toggle ordered list
- Toggle task list
- Insert link
- Edit link
- Insert image
- Insert code block
- Insert table
- Toggle source reveal for current block
- Toggle full source mode as escape hatch

所有命令必须通过源 Markdown edit 实现，不能只改变显示样式。

### Clipboard

复制策略必须明确：

- `Copy` 默认复制用户看到的 plain text，同时在 clipboard 中附带 Markdown flavor，如果平台支持。
- `Copy as Markdown` 复制源 Markdown range。
- `Copy as HTML` 可选，后期实现。
- 从外部粘贴 HTML 时转换成 Markdown。
- 从外部粘贴 Markdown 时保留 Markdown 语义。
- 从富文本应用粘贴图片时需要资源保存策略，未确定前先走文件选择或 data fallback。

## Source Reveal 策略

为了跳过源码增强但降低完整 Typora 的风险，第一阶段采用 focused-block WYSIWYG。

规则：

- 非活动 block 显示富文本结果。
- 活动 block 只有在 selection collapsed 且 caret 位于该 block 的 Markdown 语义元素内部时，才显示必要 Markdown marker。
- selection 不是 reveal 触发器；非空 selection 必须选择当前 rendered/frozen projection，而不是强制切回 raw。
- 拖拽开始后冻结当前 projection，直到 mouse up 后再重新计算 reveal 状态。
- 用户编辑的 inline 结构可以局部 reveal marker，例如 link destination、image source、code fence info string。
- 当某个结构无法安全富文本编辑时，临时 reveal 该结构源码，而不是猜测用户意图。
- 提供全局 `Toggle Source Mode` 作为逃生口，但它不是主体验。

阶段推进时逐步减少 reveal 场景，直到大多数 Typora 语义可以直接富文本编辑。

## 分阶段计划

### 阶段 0：架构验证和测试基线

目标：建立可持续实现完整 Typora 语义的技术基线。这个阶段不是用户可见的源码增强阶段，因此不违背“跳过 A”。

任务：

- 列出 Zed `editor` display pipeline 当前可复用点和必须新增的扩展点。
- 验证 `fold_map` / `inlay_map` / `block_map` 能否承载 marker hiding、replacement 和 block widgets。
- 确认 parser 路线：固定为 tree-sitter Markdown block grammar + inline grammar，不引入手写 Markdown 扫描器。
- 建立 Markdown fixture 集合，覆盖 inline、block、nesting、invalid Markdown、large file、CJK、emoji、Windows path、relative image。
- 建立 source offset 和 display offset 的 property test 框架。
- 写下 `markdown` crate 替换清单，明确当前哪些 preview 能力需要在 WYSIWYG 路径重建。

验收：

- 已有一个 tree-sitter-backed `MarkdownSyntaxTree` / `MarkdownParseTree` 数据结构。
- 有一个明确的 `MarkdownProjectionMap` 数据结构草案。
- Parser fixture 直接覆盖 block tree 与 inline tree，不接受手写扫描器替代。
- 可以解释为什么某些能力走现有 display map，某些能力需要新扩展点。

### 阶段 1：Focused-block WYSIWYG MVP

目标：用户开始看到非活动 block 的富文本显示，当前 block 仍可安全编辑。

范围：

- Paragraph、ATX heading、basic emphasis、strong、inline code。
- 非活动 heading 隐藏 `#`，显示不同字号和粗细。
- 非活动 bold / italic / inline code 隐藏 marker，显示样式。
- 活动 block reveal 必要 marker，保证编辑稳定。
- 保留保存、dirty、undo、redo、find、copy、paste 的基本行为。
- 不再使用 `markdown::MarkdownElement` 做右侧 preview。迁移期可以保留旧 preview 开关用于对照，但 Typora 编辑路径不能调用 `markdown` crate，不能把 preview 当作 rendered mode 调试入口。

验收：

- 编辑一个包含 heading、bold、italic、inline code 的文档时，非活动 block 有 WYSIWYG 效果。
- 点击非活动 block 可进入编辑，marker reveal 后光标位置正确。
- 修改、保存、undo、redo 后源 Markdown 正确。
- `cargo check -p markdown_editor` 通过。

### 阶段 2：Inline 语义完整化

目标：把常见 inline Markdown 从“显示样式”推进到可直接富文本编辑。

范围：

- Strong / emphasis nesting。
- Strikethrough。
- Link inline display，默认显示 text，URL 通过 popover 或 reveal 编辑。
- Reference link display 和编辑降级。
- Escape 和 literal marker。
- Hard break。
- Search highlight 与 hidden marker 共存。
- Copy / Copy as Markdown 的差异化行为。

验收：

- Toggle bold / italic / inline code / strikethrough 能正确包裹、取消和合并 marker。
- 点击 link text 可以编辑文字，命令可以编辑 URL。
- selection 跨越多个 inline span 时复制和删除稳定。
- CJK 和 IME 输入不会破坏 marker。

### 阶段 3：列表、引用和段落编辑语义

目标：实现 Markdown 写作中最高频的 block 编辑行为。

范围：

- Unordered list。
- Ordered list。
- Nested list。
- Task list checkbox。
- Blockquote。
- Enter 自动续行。
- Backspace / Enter 退出列表和引用。
- Tab / Shift-Tab 调整列表层级。
- Checkbox 点击更新源 Markdown。

验收：

- Typora 式 list continuation 行为稳定。
- 嵌套 list 的缩进、软换行和光标移动稳定。
- 点击 checkbox 改写 `- [ ]` / `- [x]`，dirty 状态正确。
- 删除空 list item 的行为符合常见 Markdown 编辑器预期。

### 阶段 4：代码块和语言高亮

目标：实现 fenced code block 的富文本编辑体验，并复用 Zed 语言能力。

范围：

- Fenced code block display。
- Info string 编辑。
- 代码内容使用对应语言高亮。
- Code block 内部保持源码编辑器行为，包括 Tab、缩进、多行选择。
- Copy code。
- 从普通段落输入 ``` 创建 code block。
- 删除 fence 时安全退化。

验收：

- Markdown 中的 ```rust / ```typescript 等代码块显示为代码块。
- 代码块内部编辑不破坏外层 Markdown。
- Info string 修改后语言高亮更新。
- 大代码块滚动和编辑性能可接受。

### 阶段 5：图片、链接资源和文件路径

目标：实现 Markdown 文档里的资源显示和编辑。

范围：

- Image inline syntax 显示为图片 block 或 inline image。
- 相对路径基于当前文件目录解析。
- 未保存文档图片降级显示。
- 图片加载、缓存、失败状态。
- Insert image 命令。
- Edit image alt/src 命令。
- Drop / paste 图片的资源保存策略。
- Link open / edit / copy URL。

验收：

- `![alt](relative/path.png)` 能显示图片。
- 图片失败时仍能编辑 alt/src，不丢源。
- 另存为后相对资源路径解释明确。
- 粘贴或插入图片不会默默写到错误目录。

### 阶段 6：表格

目标：实现 Typora 体验里最难的结构之一：Markdown table 的 grid 编辑。

范围：

- Pipe table parse。
- Table grid display。
- Cell text editing。
- 添加 / 删除 row 和 column。
- Alignment 编辑。
- Tab / Shift-Tab 在 cell 间移动。
- Enter 在 cell 内或新增 row 的行为定义。
- 源 Markdown table 格式化策略。

关键决策：

- 表格保存时是否自动重新对齐 pipe。
- 用户手写不规则 table 是否保留原格式还是规范化。
- 多行 cell 不属于标准 GFM table，默认不支持或转义。

验收：

- 常规 GFM table 可以像表格一样编辑。
- 修改 cell 后源 Markdown table 合法。
- Undo/redo 可以恢复 table 结构。
- 大表格不会造成整篇文档卡顿。

### 阶段 7：高级 block 和扩展语义

目标：补齐完整 Markdown/Typora 语义中低频但重要的结构。

范围：

- Thematic break。
- Setext heading。
- Footnote reference 和 definition。
- HTML block 安全显示和源码 reveal。
- Front matter / metadata block。
- Mermaid diagram block。
- Math inline / block，如果产品决定支持。
- Reference definitions。

验收：

- 不支持直接富文本编辑的高级 block 必须有安全源码 reveal。
- Mermaid / math 异步渲染失败时不影响文档编辑。
- HTML 不执行脚本，不引入危险交互。
- 所有高级 block 保存后源 Markdown 不被破坏。

### 阶段 8：完整编辑命令和命令入口

目标：把 WYSIWYG 行为接进产品命令系统，用户不依赖手写 Markdown marker。

范围：

- Command Palette 中暴露 Markdown formatting commands。
- 常用快捷键：bold、italic、link、heading、list、quote、code block。
- Context menu：link、image、table、code block。
- Floating popover：link URL、image src、table controls。
- Source reveal / source mode 命令。
- Find / replace 与富文本显示共存。

验收：

- 常用 Markdown 操作可以通过命令完成。
- 快捷键不会和 Zed editor 基础快捷键冲突。
- 命令执行后源 Markdown edit 最小且可撤销。

### 阶段 9：性能、稳定性和大文档

目标：让完整 WYSIWYG 在真实文档上稳定可用。

范围：

- 增量解析或可见区域解析。
- Projection cache。
- Block render cache。
- 异步图片 / diagram 渲染取消。
- 大文档 scrolling。
- 大量 inline marker 的 mapping 性能。
- Memory profile。
- Crash hardening。

目标指标需要实测后确定，初始建议：

- 1 MB Markdown 文档可编辑。
- 10,000 行文档滚动不卡死。
- 普通输入延迟保持接近 Zed 源码编辑器。
- 异步渲染失败不会阻塞输入。

验收：

- 大文档 fixture 下输入、滚动、查找、保存都可用。
- Parse/render 任务有取消策略。
- 没有已知 panic、offset 越界或 stale snapshot 崩溃。

### 阶段 10：删除 preview 依赖和收敛产品体验

目标：完成从 preview 模式到 WYSIWYG 编辑器的产品收敛。

范围：

- 删除 `markdown_editor` 对 `markdown` crate 的依赖。
- 删除右侧 Markdown preview 作为核心体验。
- 如果保留 preview，只能作为独立可选只读检查视图，且不能依赖 `markdown` crate。
- 清理过渡 debug UI。
- 整理 Command Palette 命令。
- 文档更新：`GOAL.md` 记录阶段完成情况。

验收：

- `crates/markdown_editor/Cargo.toml` 不再依赖 `markdown`。
- WYSIWYG 编辑区覆盖原 preview 的核心阅读能力。
- `cargo check -p markdown_editor` 通过。
- 用户不再需要打开 preview 才能确认文档结果。

## 测试计划

### Parser Tests

- 每个 Markdown 结构有 fixture。
- 每个 fixture 验证 source ranges、node kind、marker ranges、content ranges。
- Invalid Markdown fixture 不允许 panic。
- Windows path、CJK、emoji、combining marks、CRLF 都要覆盖。

### Mapping Tests

- source offset 到 display position。
- display position 到 source offset。
- 隐藏 marker 边界。
- selection 跨 marker。
- edit 后 mapping 更新。
- soft wrap 后 mapping。
- property tests：随机 Markdown-like 文本不 panic，不产生越界 range。

### Editor Integration Tests

- 鼠标点击定位。
- 键盘移动。
- Backspace / Delete。
- Enter list continuation。
- Toggle formatting。
- Undo / redo。
- Copy / paste。
- IME composition。
- Find / replace。

### Visual Tests

- Heading、list、quote、code block、table、image 的 golden render。
- Theme change 后颜色和字体更新。
- High DPI 和不同 window width 下软换行。

### Regression Tests

每发现一个 offset、selection、undo、IME、table 或 list bug，都必须补最小复现 fixture。WYSIWYG 编辑器最容易在边界处退化，不能只靠手工验证。

## 风险和缓解

### 风险：把 Markdown 逻辑硬编码进通用 editor

缓解：新增通用 display projection 扩展点，Markdown 逻辑留在 `markdown_editor` 或新的 Markdown WYSIWYG crate 中。

### 风险：滥用 fold_map 导致 selection 和 undo 语义混乱

缓解：前期可以用 fold 验证，最终实现独立 `MarkdownProjectionMap`，把语义隐藏和用户折叠分开。

### 风险：全量 parse 导致大文档输入卡顿

缓解：`markdown_wysiwyg` 从阶段 0 起使用 tree-sitter incremental parse。初期可以先重建语义索引，但不能把全量字符串扫描作为 parser 路线；接入 UI 前必须支持 visible range projection、task cancellation 和 projection cache。

### 风险：IME 和隐藏 marker 冲突

缓解：IME composition 期间当前 inline 或 block 强制 reveal，composition commit 后再重新投影。

### 风险：表格编辑成本过高

缓解：先做源码 reveal fallback，再实现 grid editing。表格进入最终完成标准，但不能阻塞 heading/list/inline/code block 主路径。

### 风险：图片、Mermaid、HTML 引入安全和性能问题

缓解：默认禁用危险 HTML；异步渲染必须可取消；远程资源策略显式化；失败时保留可编辑源码 fallback。

### 风险：完整 Typora 语义没有清晰边界

缓解：以本文语义覆盖范围为验收边界。任何新增扩展必须先加入 plan 或明确作为后续增强。

## 依赖策略

短期允许：

- 继续依赖 Zed `editor`、`language`、`multi_buffer`、`gpui`、`ui`、`theme`、`settings`。
- 临时保留 `markdown` crate 直到 WYSIWYG path 覆盖当前 preview 的阅读能力。

最终要求：

- `markdown_editor` 不依赖 `markdown` crate 作为 Typora 编辑路径；旧 preview 依赖只能在迁移期存在，且必须被隔离。
- Markdown parsing / projection / editing 逻辑位于 tree-sitter-backed WYSIWYG path。
- 共享 Markdown parser 固定在低层 `markdown_wysiwyg` / tree-sitter path，不能依赖 preview renderer crate，也不能引入手写扫描 parser。
- 不引入 workspace/project UI，只允许内部复用不会泄露到产品心智的基础 crate。

## 成功标准

最终可以认为完成完整 Typora 语义时，必须同时满足：

- 用户打开单个 Markdown 文件后，可以在一个主编辑区内完成阅读和编辑，不需要 preview pane。
- 常见 Markdown 结构都以富文本形态显示和编辑。
- 保存后的文件是标准 Markdown。
- Undo/redo、selection、copy/paste、find、IME、mouse hit test、keyboard movement 全部稳定。
- 图片、table、code block、list、blockquote、link、task list 都有可用编辑体验。
- 高级 block 有安全渲染或源码 reveal fallback，不会破坏源文档。
- 大文档不会因为 WYSIWYG 投影而明显不可用。
- `markdown_editor` 不再依赖 `markdown` crate。
- 产品仍然不暴露 workspace/project/tabs/IDE 功能。

## 下一步

下一步不是直接实现完整 Typora，而是执行阶段 0 和阶段 1：

- 先建立 `MarkdownSyntaxTree` 和 `MarkdownProjectionMap` 的最小数据结构。
- 用 heading、bold、italic、inline code 做 focused-block WYSIWYG。
- 同时建立 mapping tests，防止后续所有编辑语义建立在不稳定 offset 映射上。

如果阶段 1 的 offset mapping 无法稳定，必须暂停新增语义，先修正 projection 架构。完整 Typora 语义的成败取决于这个映射层，而不是取决于渲染多少 Markdown 节点。

## 实施记录

### 2026-05-20 - 阶段 0 启动

- 新增 `crates/markdown_wysiwyg` 作为 Typora/WYSIWYG 的低层核心 crate。
- 该 crate 当前不依赖 `markdown` crate，也不依赖 GPUI/UI 层；这是长期边界，不是临时限制。
- 第一版直接采用 tree-sitter Markdown block grammar + inline grammar，不保留手写 Markdown 扫描器作为过渡实现。
- 当前实现保存 `MarkdownParseTree`，包含 block tree、inline trees 和 inline parent 映射；`MarkdownSyntaxTree` 在其上提取 block 元数据、hidden marker ranges 和 focused-block reveal 所需投影。
- 当前覆盖 ATX heading、paragraph、blank、fenced code block 的基础 block 识别，并验证 inline tree 可以识别 strong emphasis 和 inline link。
- 已通过 `cargo test -p markdown_wysiwyg` 和 `cargo check -p markdown_editor`。
- 下一步优先级：把 tree-sitter-backed semantic tree 接入 Zed `Editor` display map；继续扩展 inline/block 语义时必须沿用该解析路径，不应把 `markdown` crate 引入 WYSIWYG 渲染路径。

### 2026-05-20 - 阶段 1 display-map 接入启动

- `markdown_editor` 新增 `MarkdownWysiwygController`，作为产品入口和 WYSIWYG 核心层之间的接入点。
- 控制器从当前 singleton `Buffer` 读取文本版本；只有文本版本变化时才解析 `MarkdownSyntaxTree`。selection 变化不能作为 raw reveal 的直接触发器。
- 曾尝试用 `Editor` display-map folds 做 ATX heading marker hiding，但拖拽选择时 fold replacement 会改变 display/source 几何，在 CJK/UTF-8 文本中触发 hit-test 越界和非 char-boundary panic。因此普通 `fold_map` 不再作为 inline marker hiding 的长期方案。
- 当前安全接入范围改为 ATX heading marker weakening：`# ` marker 通过 `Editor` text highlights 淡化显示，不改变文本几何、hit-test 或 source/display 坐标。
- marker 真隐藏、heading 字号/行高变化和 Typora 级 inline replacement 需要后续新增真正的 Markdown projection transform，不能再用普通 fold 代替。
- `editor` crate 已新增两个通用 API：`newest_selection_point_range` 和 `replace_folds_with_type`；其中 fold 刷新 API 可继续服务块级/显式折叠场景，但不用于 inline marker hiding。
- 已通过 `cargo check -p markdown_editor` 和 `cargo test -p markdown_wysiwyg`。
- 下一步优先级：先用不改变文本几何的 `custom_highlights` 增加 heading content 加粗、inline strong/emphasis/inline-code/link 等视觉变化；同时设计真正的 projection transform 来支持隐藏 marker、heading 字号和 selection-safe rendered editing。
