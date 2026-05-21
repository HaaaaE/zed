# 目标

当前仓库原本是 Zed 的 fork。我们的目标不是长期维护一个完整的 Zed 分支，而是在大规模复用 Zed 现有 crate、编辑器能力、UI 架构和实现经验的基础上，做出一个专注 Markdown 的单文件文档编辑器。

当前阶段先做一个新的 Markdown-only 入口，把产品形态独立出来。成品也会继续保留大量 Zed crate；其中有些 crate 可能暂时比较臃肿，或者仍包含当前产品用不到的能力。只要这些能力不进入 Markdown 编辑器的用户界面，就可以先保留。等新入口和核心体验跑通后，再逐步删掉或瘦身对这个 Markdown 编辑器不必要的功能、入口和依赖。

## 期望的用户体验

- 用户打开的是一个 Markdown 文件，而不是项目、工作区或文件夹。
- 当前应用始终围绕一个活动 Markdown 文档工作。
- 打开新 Markdown 文件时，替换当前文档，而不是创建标签页或多文档工作区。
- Markdown 预览是核心功能，不是附加功能。
- 预览应该容易打开、关闭，并且跟随当前文档保持同步。
- 应用应支持常见单文档操作：新建文件、打开文件、保存、另存为、撤销、重做、查找、复制、粘贴。
- 编辑器应保留 Zed 中适合写作的部分：快速编辑、语法高亮、主题，以及必要的键盘操作。
- 需要保留 Command Palette 或等价的命令入口，作为查找和执行常用命令的键盘驱动入口，也可以承载换主题、打开设置文件、打开 keymap 文件等常用操作。

## 产品边界

- 用户体验上不暴露 workspace / project 概念；内部是否临时复用相关 crate，不影响这个目标。
- 当前阶段面向单个 Markdown 文档，默认关注 `.md` / `.markdown` 文件。
- 当前阶段不做多文档标签页，不做文件夹、项目或工作区组织方式。
- Markdown 预览应跟随当前文档，而不是作为项目级功能出现。
- 不提供项目型入口，例如文件夹树、项目搜索、远程工作区、协作空间或开发环境。

## 配置和快捷键

- 当前阶段不做设置 UI，但可以保留 settings 文件和必要的默认设置。
- 当前阶段不做 Keymap 编辑器，但可以保留默认快捷键和 keymap JSON 配置能力。
- 常用配置项优先通过 Command Palette 或等价命令入口暴露。

## 开发规则

- 开发过程中遵守仓库现有的 `.rules` Agent 规范。
- 如果发现对后续 agent 开发有帮助的非显而易见规则，按 `.rules` 里的流程记录为建议，不在普通功能开发中随手修改 `.rules`。

## 当前阶段不做

- 多文档标签页
- AI Agent 功能
- 终端
- 调试器
- Git UI
- 扩展管理
- 协作
- 远程开发
- Project Panel 或文件树
- 以 workspace 形式打开文件夹
- 设置 UI
- Keymap 编辑器

## 期望结果

最终效果应该是一个专注 Markdown 的单文件文档编辑器，具备 Zed 质量的编辑体验和 Markdown 预览。它不需要在第一阶段成为依赖最少的轻量项目；相反，应该优先复用 Zed 现有 crate、代码和实现方式来保证体验和实现速度。后续再逐步删除或瘦身对这个目标不必要的 Zed 功能、入口和依赖。

## 实现指引

起点不是裁剪现有 Zed，而是先做新的 Markdown-only 入口，让它成为一个明确的单文件 Markdown 文档编辑器。

写代码时遵守复用优先的原则：需要实现一个功能时，先评估 Zed 现有 crate 能不能直接复用；如果不能直接复用，就参考 Zed 现有实现和代码风格做包装或改造；如果仍然不合适，再自己实现。

复用不是只复用 crate 名称，而是尽量复用 Zed 已有的代码、逻辑、初始化顺序、数据结构、交互模式和架构边界。每次新增能力前，都要先读 Zed 对应功能的实现，经过判断后再决定是直接引入、包装复用、局部改造，还是最后才自己实现。

硬约束：凡是 Zed 已经有的功能，Markdown 编辑器对应的界面和逻辑必须优先从 Zed 抄、搬、包或直接调用。只有确认 Zed 现有实现不适合 Markdown-only 单文件产品边界，或者会强制暴露当前阶段不允许的 IDE 功能时，才允许做局部改造或自定义实现，并且要在阶段记录里说明原因。

架构上也优先沿用 Zed 的组织方式、交互方式和抽象方式，能抄就先抄，先保证产品一致性，再谈简化。

新入口可以做成新的 crate，但它不是为了脱离 Zed 生态或重写一个轻量编辑器，而是为了建立清晰的 Markdown 产品入口。这个入口应尽量调用和组合 Zed 现有 crate，包括那些短期内还比较臃肿的 crate。

第一阶段不要以删除 crate、压缩依赖图或重写底层架构为目标。只要用户界面不暴露当前阶段不需要的功能，底层暂时保留相关 crate 是可以接受的。

等这个入口跑通后，再看底层是否需要进一步瘦身。裁剪不是起点，而是后面基于新入口再做的收敛动作。

## 阶段计划

### 阶段 1 - Markdown-only 入口骨架

目标：建立新的 Markdown-only crate 和入口骨架，得到一个可以独立编译、独立启动的最小应用入口。

范围：

- 新增 `crates/markdown_editor`，作为 Markdown 编辑器的新产品入口。
- 入口优先复用 Zed 的 GPUI、assets、settings、theme、ui 等基础设施。
- 第一版先打开一个 Markdown-only 窗口骨架，不急于接入完整 workspace/editor/preview。
- 不调用现有 `zed::initialize_workspace`，因为它会自动挂 Agent、Project Panel、Git、终端、状态栏等 IDE 型入口。

验收标准：

- `markdown_editor` crate 可以独立 `cargo check`。
- 用户可以运行当前入口并看到 Markdown-only 窗口骨架。
- 入口代码不暴露当前阶段不需要的 IDE 功能。

后续方向：在这个新入口上逐步接入 Zed 的 editor、单文件打开保存、命令入口和 Markdown preview。

### 阶段 2 - 可编辑 Markdown 文档区域

目标：把 Markdown-only 窗口从静态骨架推进到可输入的 Markdown 文档编辑界面。

范围：

- 在 `crates/markdown_editor` 中接入 Zed 的 `editor` / `language` / `multi_buffer` 等必要 crate。
- 启动后创建一个空的 Markdown 文档 buffer，并在窗口中渲染可输入的编辑器。
- 优先复用 Zed editor 的现有行为、主题和输入处理。
- 暂不做打开文件、保存文件、Command Palette 或 Markdown preview。
- 不做多文档标签页。
- 不引入 Project Panel、Agent、Terminal、Git、Debugger 等 IDE 型入口。

验收标准：

- `cargo check -p markdown_editor` 通过。
- 用户可以运行当前入口，并在窗口中看到可聚焦、可输入的 Markdown 编辑区域。
- 新入口仍然不暴露当前阶段不需要的 IDE 功能。

后续方向：在可编辑文档区域稳定后，再接入单文件打开/保存、命令入口和 Markdown preview。

### 阶段 3 - 文件打开与保存

目标：让 Markdown-only 入口从临时空 buffer 变成可以打开和保存单个 Markdown 文件的文档编辑器。

范围：

- 支持从命令行传入 `.md` / `.markdown` 文件路径并打开内容。
- 未传入文件时继续打开空文档。
- 支持保存当前文档内容回原路径。
- 优先复用 Zed 已有的文件、buffer、project 或 workspace 相关逻辑；开始实现前必须先阅读 Zed 的打开文件和保存文件链路。
- 不为了阶段 3 自己发明独立文档模型，除非确认 Zed 现有路径不适合当前入口。
- 暂不做命令入口、Markdown preview 或 Project Panel。
- 不做多文档标签页。

验收标准：

- `cargo check -p markdown_editor` 通过。
- `cargo run -p markdown_editor --bin markdown-editor path/to/file.md` 可以打开指定 Markdown 文件。
- 用户可以编辑内容并保存回磁盘。
- 新入口仍然不暴露当前阶段不需要的 IDE 功能。

后续方向：文件 I/O 跑通后，再接入命令入口和 Markdown preview。

### 阶段 4 - 文档外壳与保存状态

目标：让单文件 Markdown 编辑器显示当前文档身份和保存状态，避免用户只能看到裸编辑区。

范围：

- 在编辑器上方显示当前文件名。
- 显示当前文件路径，未保存文档显示为未保存 Markdown 文档。
- 显示保存状态，并在编辑后切换为未保存状态。
- 保存成功后恢复为已保存状态。
- 优先复用 Zed `EditorEvent` / `Buffer` 的 dirty 和 saved 语义，不另建独立文档状态模型。
- 暂不做 Command Palette、Markdown preview、Save As 或文件选择器。
- 不做多文档标签页。

验收标准：

- `cargo check -p markdown_editor` 通过。
- 打开 Markdown 文件时，窗口顶部显示文件名和路径。
- 编辑后状态显示为未保存。
- `Ctrl+S` 保存成功后状态显示为已保存。
- 新入口仍然不暴露当前阶段不需要的 IDE 功能。

后续方向：单文件文档外壳稳定后，再接入命令入口、GUI 打开/另存为和 Markdown preview。

### 阶段 5 - 单文件 Markdown 工作台成型

目标：把当前单文件编辑器推进成一个可以日常使用的单文件 Markdown 工作台，用户可以通过界面完成打开、编辑、保存、预览和常用命令操作，而不是只靠命令行。

范围：

- 提供 GUI 打开文件入口，用户可以从应用内选择一个 `.md` / `.markdown` 文件。
- 支持 `Ctrl+O` 打开 Markdown 文件。
- 支持从 GUI 新建空 Markdown 文档。
- 打开新文件时替换当前文档。
- 提供轻量 Zed 风格标题栏，显示当前文件路径、文件名、保存状态和常用操作。
- 提供 Markdown preview，允许在编辑与预览之间切换，并让预览跟随当前文档实时更新。
- 保留一个最小命令入口，并把打开文件、新建文档、保存、另存为、切换预览等常用动作放进去。
- 保留常见文档编辑能力的入口，包括撤销、重做、查找、复制、粘贴、另存为。
- 优先学习并复现 Zed 已有的标题栏、打开文件、preview、command palette、dirty-state 和 UI 逻辑；不直接引入会暴露 Project Panel、workspace 文件夹、Git、Terminal、Agent、Debugger、扩展管理等 IDE 型入口的 crate。
- 不暴露 Project Panel、文件树、workspace 文件夹打开、Git、Terminal、Agent、Debugger、扩展管理等 IDE 型入口。
- 不做多文档标签页。

验收标准：

- `cargo check -p markdown_editor` 通过。
- 用户可以启动应用后，通过 GUI 选择并打开一个 Markdown 文件。
- 用户可以新建空 Markdown 文档。
- 用户可以打开和关闭 Markdown preview，并且预览会跟随当前文档更新。
- 用户可以通过最小命令入口执行常用文档命令。
- 编辑后标题栏能反映未保存状态；保存后恢复已保存状态。
- 新入口仍然不暴露当前阶段不需要的 IDE 功能。
- 入口中没有 tab / tabs / TabBar / 多文档切换相关代码。

后续方向：单文件工作台稳定后继续完善单文件编辑体验。多标签页不是当前产品目标。

## 运行方式

当前 Markdown-only 入口位于 `crates/markdown_editor`。

验证编译：

```powershell
cargo check -p markdown_editor
```

启动当前入口：

```powershell
cargo run -p markdown_editor --bin markdown-editor
```

打开指定 Markdown 文件：

```powershell
cargo run -p markdown_editor --bin markdown-editor -- path\to\file.md
```

当前 crate 显式声明了 binary 名称为 `markdown-editor`，所以运行时需要带 `--bin markdown-editor`。如果后续删除 `crates/markdown_editor/Cargo.toml` 里的 `[[bin]]` 显式声明，启动命令可以简化为 `cargo run -p markdown_editor`。

## 进展记录

每完成一个阶段性任务，都在这里追加记录。记录应对应上面的 `阶段计划`，并说明完成了哪个阶段目标、是否达到验收标准，以及对后续目标有什么影响。

### 2026-05-20 - 阶段 1：Markdown-only 入口骨架

- 对应目标：建立新的 `crates/markdown_editor` crate 和可独立启动的 Markdown-only 入口骨架。
- 完成情况：已新增 `crates/markdown_editor`，并接入 `gpui`、`assets`、`settings`、`theme` 和 `ui` 的最小初始化链路。
- 验收结果：`cargo check -p markdown_editor` 通过；用户已本地运行 `cargo run -p markdown_editor --bin markdown-editor` 并成功打开 Markdown-only 窗口骨架。
- 对后续目标的影响：阶段 1 已完成。下一阶段可以在这个入口上接入 Zed 的 editor，使窗口从静态骨架变成可编辑的单文件 Markdown 文档界面。

### 2026-05-20 - 阶段 2：可编辑 Markdown 文档区域

- 对应目标：把 Markdown-only 窗口从静态骨架推进到可输入的 Markdown 文档编辑界面。
- 完成情况：已在 `crates/markdown_editor` 中接入 `editor`，并将窗口内容切换为可聚焦的编辑器实体；随后根据用户反馈改为允许默认 keymap 部分加载，以便在新入口里仍然复用 Zed 的 editor 快捷键，而不会因为缺少 workspace/git/agent 等无关 action 而整包失败。
- 验收结果：`cargo check -p markdown_editor` 通过；用户已本地运行验证，可以输入并使用 `Enter` 正常换行。
- 对后续目标的影响：阶段 2 已完成。下一阶段可以继续接入单文件打开/保存、命令入口和 Markdown preview。

### 2026-05-20 - 阶段 3：文件打开与保存

- 对应目标：让 Markdown-only 入口从临时空 buffer 变成可以打开和保存单个 Markdown 文件的文档编辑器。
- 完成情况：已支持从命令行传入 `.md` / `.markdown` 文件路径，并用 Zed 的 `Buffer` + `Editor::for_buffer` 创建文档编辑器；已新增入口层 `Save` action，并绑定 `Ctrl+S` 保存当前编辑内容回原路径。
- 验收结果：`cargo check -p markdown_editor` 通过；用户已本地运行验证，可以通过命令行打开 `.md` 文件，编辑内容，并用 `Ctrl+S` 保存回原文件。
- 对后续目标的影响：阶段 3 已完成最小闭环。当前文件路径和磁盘写入还是入口层薄适配，后续如要升级文件模型，应优先研究 Zed 的 `Project::open_local_buffer` / `save_buffer` 链路，但不能引入 project/workspace 用户心智。

### 2026-05-20 - 阶段 4：文档外壳与保存状态

- 对应目标：让当前单文件 Markdown 编辑器显示文档身份和保存状态。
- 完成情况：已在 `MarkdownEditorShell` 中新增顶部文档栏，显示文件名、路径和保存状态；已订阅 Zed `EditorEvent::DirtyChanged` / `EditorEvent::Saved`，并在保存成功后调用 `Buffer::did_save` 复用 Zed 的 dirty/saved 语义。
- 验收结果：`cargo check -p markdown_editor` 通过；仍需要用户本地运行验证标题栏显示、编辑后状态变为 `Unsaved`、`Ctrl+S` 后恢复为 `Saved`。
- 对后续目标的影响：阶段 4 已完成最小实现。后续命令入口和 Markdown preview 可以从当前单文件 document shell 获取上下文。

### 2026-05-20 - 阶段 5：单文件 Markdown 工作台成型

- 对应目标：把当前单文件编辑器推进成可以通过界面打开、编辑、保存、预览和执行常用命令的单文件 Markdown 工作台。
- 完成情况：阶段 5 经历过一次方向变化：最初曾短暂尝试把工作台做成多文档/多标签形态，但随后根据产品边界调整，明确当前目标是单文件 Markdown 编辑器。因此已将阶段 5 收回单文件方向，移除多标签页和多文档切换目标；实现保留 GUI 打开单个 Markdown 文件、新建文档、保存、另存为、Markdown preview 开关、预览跟随当前文档更新、轻量 Zed 风格标题栏，以及一个最小命令入口。实现中继续复用 Zed 的 `Editor`、`Buffer` dirty/saved 语义、GPUI 系统文件选择器、Zed 标题栏视觉结构和 `markdown` crate 的 `Markdown` / `MarkdownElement` 渲染。
- 验收结果：`cargo check -p markdown_editor` 通过；仍需要用户本地运行验证 GUI 打开文件、另存为、预览和命令入口的实际交互。
- 对后续目标的影响：阶段 5 回到单文件产品边界。当前不做 tab，也不再保留 tab 相关代码；后续如果重新引入多标签，需要重新评估是否仍符合产品目标。

### 2026-05-20 - Typora WYSIWYG 阶段 0：性能优先核心层启动

- 对应目标：开始执行 `TYPORA_WYSIWYG_PLAN.md` 的阶段 0，建立未来完整 Typora 语义所需的高性能核心基础，而不是继续扩展 `markdown` crate preview 路径。
- 完成情况：新增 `crates/markdown_wysiwyg` 低层 crate，并加入 workspace；该 crate 不依赖 `markdown`、GPUI 或 UI 层，当前保存 tree-sitter Markdown block tree、inline trees、源 range、block 元数据、可见 source range 和 hidden marker range。第一版实现了 `MarkdownParseTree`、`MarkdownSyntaxTree`、`MarkdownProjectionMap`、ATX heading / paragraph / blank / fenced code block 的基础语义索引，以及 focused-block reveal 所需的可见区投影 API；并验证 inline tree 可以识别 strong emphasis 和 inline link。WYSIWYG 主路径从阶段 0 起固定为 tree-sitter-backed，不引入手写 Markdown 扫描器或 `markdown` crate 渲染路径。
- 验收结果：`cargo test -p markdown_wysiwyg` 和 `cargo check -p markdown_editor` 通过。当前尚未接入 `markdown_editor` UI，也尚未替换旧 preview；这是刻意选择，避免从一开始绑定到全量 Markdown preview 渲染模型。
- 对后续目标的影响：后续 WYSIWYG 路径必须继续走 tree-sitter-backed `markdown_wysiwyg` / editor display map，而不是直接使用 `markdown` crate。下一步应将 projection 接入 `Editor` 的 display-map 扩展点；`fold_map`、`block_map`、`inlay_map` 可以作为投影机制，但不能成为 Markdown 语义层本身。

### 2026-05-20 - Typora WYSIWYG 阶段 1：接入 Editor display map

- 对应目标：验证 tree-sitter-backed `markdown_wysiwyg` 能驱动真实 Zed `Editor` display pipeline，为 focused-block WYSIWYG 建立长期接入链路。
- 完成情况：`markdown_editor` 新增 `MarkdownWysiwygController`，读取当前单文件 `Buffer` 的文本版本并在版本变化时解析 `MarkdownSyntaxTree`。曾尝试把非活动 ATX heading 的 marker range 转成 `Editor` folds 来隐藏 `# ` marker，但拖拽选择时 fold replacement 会改变 display/source 几何，在 CJK/UTF-8 文本中触发 hit-test 越界和非 char-boundary panic。因此当前安全实现改为用 `Editor` text highlights 弱化 ATX heading 的 `# ` marker，不改变文本几何、selection 或 hit-test 坐标。交互规则也更新为：只有 collapsed caret 位于语义元素内时才 reveal 原型；非空 selection 不触发 raw reveal；拖拽开始后冻结拖拽开始前的 projection 状态。
- 验收结果：`cargo check -p markdown_editor` 和 `cargo test -p markdown_wysiwyg` 通过。
- 对后续目标的影响：已形成第一条真实接入链路：tree-sitter Markdown semantic tree -> `markdown_wysiwyg` projection -> Zed `Editor` display map styling。普通 `fold_map` 不再作为 inline marker hiding 的长期方案；后续 marker 真隐藏、heading 字号/行高变化和 selection-safe rendered editing 需要真正的 Markdown projection transform。下一步应先用 `custom_highlights` 增加 heading 内容加粗、inline strong/emphasis/inline-code/link 等不改变文本几何的视觉变化，同时设计长期 projection 扩展。

### 2026-05-20 - Typora WYSIWYG 阶段 1：安全视觉语义增强

- 对应目标：在不改变文本几何、不破坏 selection/hit-test 的前提下，让 Markdown 编辑区开始呈现明显的富文本视觉差异。
- 完成情况：`markdown_wysiwyg` 新增 tree-sitter inline span 提取，输出 emphasis、strong、inline code、link、strikethrough 的 source/content/marker ranges；同时继续输出 heading content 和 marker ranges。`markdown_editor` 将这些语义 ranges 映射到 `Editor` text highlights：heading 内容加粗/分级着色，strong 加粗，emphasis 斜体，inline code 使用 accent 文字和弱背景，link 使用链接色和下划线，strikethrough 使用删除线，Markdown marker 继续弱化显示。range 映射改用 `MultiBufferOffset` 到 `Anchor`，避免手算 point column。
- 验收结果：`cargo test -p markdown_wysiwyg` 和 `cargo check -p markdown_editor` 通过；未运行 `cargo run`。
- 对后续目标的影响：当前视觉增强已经覆盖 heading 和常见 inline 语义，但仍不改变字号/行高，也不真正隐藏 marker。heading 字号/行高和 rendered selection 需要继续设计并实现通用 editor projection/layout 扩展，不能用普通 fold 或半成品 per-run font-size hack 代替。

### 2026-05-20 - Typora WYSIWYG 阶段 1：Source/Rendered 模式切换

- 对应目标：把当前 Markdown-aware source editing 固化为 Source Mode，同时引入 Rendered Mode 作为后续 Typora-like projection 的入口。
- 完成情况：`markdown_editor` 新增 `MarkdownEditMode::{Source, Rendered}`，标题栏新增模式按钮，命令面板新增 `Toggle Source/Rendered Mode`，快捷键为 `Ctrl+Shift+M`。两个模式共享同一个 `Editor` 和 `Buffer`，切换只改变 display/highlight policy，不改变文档模型。
- Source Mode 策略：保留 Markdown marker 原文，marker 仅弱化显示；heading/strong/emphasis/code/link/strikethrough 继续走语义高亮，但不改变 heading 字号或行高。
- Rendered Mode 策略：新增 `gpui::HighlightStyle::{font_size, hide_text}`；heading 通过普通 editor 文本 shaping 使用更大字号；marker 通过 `HighlightedChunk` 的 `ChunkReplacement::Str("")` 零宽替换隐藏，source range len 保留用于映射，不使用 fold/block replacement。
- 验收结果：`cargo check -p markdown_editor` 和 `cargo test -p markdown_wysiwyg` 通过；未运行 `cargo run`。
- 对后续目标的影响：Rendered Mode 已有模式边界和 marker hiding 入口，但当前 Zed editor 不支持真实单行变高：`position_map.line_height` 是全局单值，row->y、scroll、selection bounds 和 hit-test 均按统一行高计算。已撤回视觉 line-height 方案；后续若要 heading 真正占据更高行，需要先扩展 editor position map 的可变行高模型。

### 2026-05-21 - Typora WYSIWYG 阶段 2：row metrics 与 reveal 骨架

- 对应目标：为 Rendered Mode 建立真正可用的几何基础和 source reveal 入口，不再依赖 block/fold 的错位方案。
- 完成情况：`editor` 新增 `EditorRowMetrics` 和 `row_height_overrides`，并通过 `Editor::set_row_height_overrides` / `clear_row_height_overrides` 将 row 高度偏移接入 `PositionMap` 计算；`markdown_editor` 在 Rendered Mode 中为 ATX heading 设置行高覆盖，Source Mode 清空覆盖。与此同时，`markdown_editor` 增加了 `RenderedRevealState` / `RevealTarget` 骨架，为 hover/caret reveal、drag freeze 和后续 block/inline source 暴露留出语义状态。
- 验收结果：`cargo check -p markdown_editor` 和 `cargo test -p markdown_wysiwyg` 通过；未运行 `cargo run`。
- 对后续目标的影响：这一阶段把 heading 高度从视觉 hack 变成 row metrics 数据，但 hover overlay、actual source reveal UI、table/image/math 的具体 projection 仍未实现。后续应继续把 `RevealTarget` 接到鼠标 hover、caret 进入和 selection freeze 上，再扩展到 table/image/code fence。

记录格式：

```md
### YYYY-MM-DD - 阶段 N：阶段名称

- 对应目标：
- 完成情况：
- 验收结果：
- 对后续目标的影响：
```

### 2026-05-21 - Typora WYSIWYG 阶段 2：行垂直对齐修复

- 对应目标：修复 Rendered Mode heading 行高已生效但文字贴顶部、行号区和文本区高度同步偏差、基线对齐不正确的问题。
- 完成情况：在 `editor` crate 的 `element.rs` 中，将所有使用 `line_height * (row - scroll_position.y)` 计算行 y 坐标的地方改为使用 `EditorRowMetrics::y_for_row(row, scroll_position)`，并计算 `y_offset = (height_for_row - line_height) / 2` 将文本在增高行中垂直居中。具体修改了以下方法：
  - `LineWithInvisibles::draw` 和 `draw_with_custom_offset`：使用 `row_metrics.y_for_row` 计算 `line_y`，添加 `y_offset` 垂直居中文本和不可见字符。
  - `LineWithInvisibles::draw_background`：同上。
  - `LineWithInvisibles::prepaint_with_custom_offset`：添加 `y_offset` 参数，在行内元素 pre-paint 时垂直居中。
  - `layout_line_numbers`：行号区 y 坐标改用 `row_metrics.y_for_row`，hitbox 高度改用 `row_metrics.height_for_row`，行号文本垂直居中。
  - `paint_highlighted_range`：`start_y` 改用 `row_metrics.y_for_row`，新增 `line_heights` 字段支持每行不同高度。
  - `HighlightedRange` 结构体：新增 `line_heights: Vec<Pixels>` 字段，`paint_lines` 方法使用累积行高代替统一 `line_height`。
  - 活动行高亮：`origin.y` 和 `size.height` 改用 `row_metrics` 计算。
  - `Gutter` 结构体：新增 `row_metrics` 字段（owned），`prepaint_button` y 坐标改用 `row_metrics.y_for_row`。
  - `prepaint_crease_toggles`：y 坐标改用 `row_metrics.y_for_row`，垂直居中。
  - `prepaint_crease_trailers`：y 坐标改用 `row_metrics.y_for_row`，垂直居中。
- 验收结果：`cargo check -p markdown_editor` 和 `cargo test -p markdown_wysiwyg` 通过；`cargo test -p editor` 全部 731 个测试通过。用户验证 heading 文字垂直居中、行号区同步、光标高度跟随行高均正常。
- 对后续目标的影响：heading 垂直对齐和光标高度均已修复。下一步可以继续实现 hover reveal overlay 和 caret reveal。

```

### 2026-05-21 - Typora WYSIWYG 阶段 3：光标靠近隐藏 marker 时显示源码

- 对应目标：Rendered Mode 中，当光标靠近被 hide_text 隐藏的 markdown 控制字符（`#`、`**`、`*`、`` ` ``、`~~`、`[]()`）时，局部显示这些字符。
- 完成情况：
  - 简化 `RenderedRevealState`：移除 `RevealTarget` 枚举和 `hover`/`caret` 字段，替换为 `revealed_marker_ranges: Vec<Range<usize>>`，记录当前光标附近应显示的 marker 字节范围。
  - 新增 `caret_revealed_marker_ranges(tree, mode, caret_offset)`：遍历所有 block 和 inline 的 `marker_ranges`，当光标 offset 在 marker 范围 ±1 字符内时，将该 marker 收入 revealed 集合。
  - 新增 `is_offset_near_range(offset, range, margin)` 辅助函数。
  - `MarkdownHighlightRanges` 新增 `revealed_marker` 字段，`markdown_highlight_ranges` 根据 `reveal_state.is_marker_revealed()` 将 marker 分为隐藏和显示两组。
  - 新增 `markdown_revealed_marker_highlight_key()` 和 `markdown_revealed_marker_highlight_style(cx)`：显示的 marker 使用 `fade_out: 0.3` + `color: text_muted.opacity(0.5)` 而非 `hide_text: true`。
  - `apply_markdown_highlights` 对 `revealed_marker` 单独应用显示样式。
  - 光标 offset 通过 `snapshot.point_to_offset(selection.start)` 从 `text::BufferSnapshot` 获取。
- Bug fix：`markdown_marker_highlight_key()` 和 `markdown_heading_1_highlight_key()` 同为 `SyntaxTreeView(usize::MAX - 1)`，导致 heading 样式覆盖了 marker 的 `hide_text`。marker key 改为 `usize::MAX - 11`。
- 验收结果：`cargo check -p markdown_editor` 和 `cargo test -p markdown_wysiwyg` 通过。
- 对后续目标的影响：光标靠近 marker 时字符级显示已实现，`hide_text` key 冲突已修复。下一步可扩展 margin 范围、支持 drag freeze、扩展到 block 级内容（table/image/code fence）。

- 对应目标：Rendered Mode 中，当光标靠近被 hide_text 隐藏的 markdown 控制字符（`#`、`**`、`*`、`` ` ``、`~~`、`[]()`）时，局部显示这些字符。
- 完成情况：
  - 简化 `RenderedRevealState`：移除 `RevealTarget` 枚举和 `hover`/`caret` 字段，替换为 `revealed_marker_ranges: Vec<Range<usize>>`，记录当前光标附近应显示的 marker 字节范围。
  - 新增 `caret_revealed_marker_ranges(tree, mode, caret_offset)`：遍历所有 block 和 inline 的 `marker_ranges`，当光标 offset 在 marker 范围 ±1 字符内时，将该 marker 收入 revealed 集合。
  - 新增 `is_offset_near_range(offset, range, margin)` 辅助函数。
  - `MarkdownHighlightRanges` 新增 `revealed_marker` 字段，`markdown_highlight_ranges` 根据 `reveal_state.is_marker_revealed()` 将 marker 分为隐藏和显示两组。
  - 新增 `markdown_revealed_marker_highlight_key()` 和 `markdown_revealed_marker_highlight_style(cx)`：显示的 marker 使用 `fade_out: 0.3` + `color: text_muted.opacity(0.5)` 而非 `hide_text: true`。
  - `apply_markdown_highlights` 对 `revealed_marker` 单独应用显示样式。
  - 光标 offset 通过 `snapshot.point_to_offset(selection.start)` 从 `text::BufferSnapshot` 获取。
- 验收结果：`cargo check -p markdown_editor` 和 `cargo test -p markdown_wysiwyg` 通过。
- 对后续目标的影响：光标靠近 marker 时字符级显示已实现。下一步可扩展 margin 范围、支持 drag freeze、扩展到 block 级内容（table/image/code fence）。
