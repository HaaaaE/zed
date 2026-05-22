# 重构目标（REFACTOR）

本文档描述 **Markdown 编辑器从 Zed IDE 依赖栈中剥离** 的工程目标、约束和实施计划。

- **产品目标**（单文件 Typora 式 Markdown 编辑器、功能边界、阶段验收）仍以 [`GOAL.md`](./GOAL.md) 为准。
- **本文档** 只负责：**依赖架构、crate 划分、从 Zed 抽代码的方式、阶段顺序**。
- 不新建外部仓库；所有工作在当前 Zed fork 的 workspace 内完成。
- 本次重构的硬目标是：**除 GPUI 运行时闭包外，`markdown_editor` 产品线不依赖任何 Zed 业务 / 编辑 / IDE crate，同时尽量保持 Zed 原代码质量与编辑体验**。

---

## 最终目标

### 依赖边界

`markdown_editor`（及为其服务的 crate）最终只允许依赖：

| 类别 | 允许 | 约束 |
|------|------|------|
| GPUI 运行时闭包 | `gpui`、`gpui_platform`、`gpui_macros`、`gpui_shared_string`、`gpui_util`、`gpui_tokio`、平台后端等 | 仅作为 UI 框架；不得通过 GPUI 的内部依赖反向使用 Zed IDE 能力 |
| 产品自有 crate | `md_*`、`markdown_wysiwyg` | `md_*` 可以包含从 Zed 搬来的源码，但搬入后即视为 Markdown 产品线代码，必须清零对原 Zed 非 GPUI crate 的依赖 |
| 社区 crate | `tree-sitter`、`tree-sitter-md`、`serde`、`anyhow`、`ureq`、`regex` 等 | 必须服务单文件 Markdown 产品，不能作为重新引入 Zed IDE 能力的侧门 |

以下依赖在最终产品线中不允许出现：

| 不允许 | 处理方式 |
|--------|----------|
| `editor`、`language`、`multi_buffer`、`text`、`rope`、`sum_tree`（Zed 版） | 通过 `md_editor`、`md_buffer`、`md_text`、`md_rope`、`md_sum_tree` 替代 |
| `project`、`workspace`、`client`、`lsp`、`git`、`dap`、`telemetry`、`task`、`remote`、`collab` 等 IDE / 服务端能力 | 不搬；如有编译依赖，删除调用路径或替换为 Markdown 单文件语义 |
| `settings`、`theme`、`theme_settings`、`ui`、`assets`、`icons` 等 Zed 应用壳 | 通过 `md_settings`、`md_theme`、`md_ui`、`md_assets` 或 `markdown_editor` 内 GPUI 薄封装替代 |
| `markdown` / `markdown_preview` 的全量 preview 路径 | 迁移期可在 `legacy-editor` 中保留；最终 WYSIWYG 主路径必须由 `markdown_wysiwyg` + `md_editor` 承载 |
| `editor::init`、`workspace::register_*`、`project` 初始化链 | 禁止出现在 `md-editor` 路径 |

说明：

- 「除 GPUI 外零 Zed 依赖」分两层执行：
  - **产品层硬门禁**：`markdown_editor` 与所有 `md_*` crate 不得直接依赖任何原 Zed 非 GPUI crate。
  - **最终审计门禁**：如果 `cargo tree` 中仍出现 `gpui` 自身拉入的非 `gpui*` workspace crate（当前 `gpui` 直接依赖 `collections`、`http_client`、`scheduler`、`sum_tree`、`media` 等），必须登记在 `docs/md-dependency-allowlist.md`，并在后续 GPUI 闭包独立化阶段处理；产品代码不得直接使用这些 crate。
- Zed 主程序 `crates/zed` **继续**使用原版 `editor` / `project` / `workspace`，与 Markdown 产品线 **并行**，互不影响。

### 不可协商原则

1. **不以牺牲编辑器质量换取剥离速度。** 复制 Zed 成熟代码、保留测试和逐步删 IDE 分支，优先于从零写一个低质量编辑器。
2. **不把临时依赖变成长期架构。** `legacy-editor` 只用于迁移对照；新功能优先落到 `markdown_wysiwyg` / `md_*`。
3. **不污染上游 Zed crate。** 只有通用、可被原 Zed IDE 接受的编辑器能力才允许继续改 `crates/editor`；Markdown 产品专属逻辑必须进入 `markdown_editor`、`markdown_wysiwyg` 或 `md_*`。
4. **不做大爆炸替换。** 每一阶段都必须保持至少一条可编译、可运行路径。
5. **不接受无记录搬迁。** 从 Zed 搬来的每批文件必须记录来源、修改点、删除的 IDE 分支和验证结果。

### 代码策略

- **尽量复用 Zed 源码**：以在当前仓库 **新建 `md_*` crate → 从对应 Zed crate 拷贝/剪切文件 → 改 import 与 crate 名 → 删除 IDE 分支** 为主，不从零重写编辑器。
- 拷贝文件保留 GPL 版权与来源注释（`ported from crates/editor/...`）。
- 只有确认 Zed 实现与单文件 Markdown 边界冲突、且无法用 `cfg` 删掉时，才写新逻辑，并在进展记录中说明原因。

### 与 `GOAL.md` 的关系

| 阶段 | `GOAL.md` | `REFACTOR_GOAL.md` |
|------|-----------|-------------------|
| 已完成（入口、WYSIWYG、工作台） | 产品功能迭代 | 可继续，但新功能优先落在 `md_*` 上 |
| 当前起 | 功能仍可追加 | **并行**推进依赖剥离 |
| 终点 | 单文件 Markdown 产品成熟 | `cargo tree -p markdown_editor` 无 Zed 编辑/IDE crate |

---

## 现状（剥离前）

### 入口

- 产品 crate：`crates/markdown_editor`（binary：`markdown-editor`）
- 分支：`markdown-editor-only`

### 直接依赖的 Zed crate（`markdown_editor/Cargo.toml`）

`assets`、`clock`、`collections`、`editor`、`gpui`、`gpui_platform`、`http_client`、`language`、`markdown`、`markdown_wysiwyg`、`multi_buffer`、`settings`、`theme`、`theme_settings`、`ui`

### 传递依赖热点

- **`editor`** 是传递依赖最多的直接依赖（约 85+ 个 Zed workspace crate，不含 gpui 系）。
- 运行时已使用 `Editor::for_buffer(..., None, ...)`（无 `Project`），但 **`editor::init` 仍会注册 workspace**，且编译期仍拉满 IDE 链。

### 可独立迁出的资产

| 资产 | 说明 |
|------|------|
| `markdown_wysiwyg` | 仅依赖 `tree-sitter` / `tree-sitter-md`，已可视为非 Zed 业务 crate |
| WYSIWYG 逻辑 | reveal、语义树、Rendered/Source 策略、行高、图片块等（目前在 `markdown_editor` + 对 `editor` API 的调用） |
| 已修改的 Zed `editor` 行为 | `row_height_overrides`、`hide_text`、垂直居中等——须在 `md_editor` 中 **移植**，不能继续依赖 `editor` crate |

### 当前仓库事实

- 目前 **尚未存在任何 `md_*` crate**；剥离工作应从 R0 脚手架开始。
- `crates/editor/Cargo.toml` 直接依赖 `project`、`workspace`、`lsp`、`git`、`dap`、`task`、`telemetry`、`client`、`markdown`、`ui`、`theme`、`settings` 等，不能作为长期产品依赖。
- `crates/language/Cargo.toml` 直接依赖 `lsp`、`settings`、`theme`、`task`、`fs`、`http_client` 等，不能整体搬入。
- `crates/multi_buffer/Cargo.toml` 依赖 `language`、`settings`、`theme`、`buffer_diff` 等，只能抽 singleton / snapshot 所需子集。
- `crates/text`、`crates/rope`、`crates/sum_tree` 相对接近底层，但仍直接依赖 Zed `clock`、`collections`、`util`、`ztracing` 等，需要改为 `md_*` 子集或社区依赖。
- `crates/gpui` 自身仍依赖若干非 `gpui*` workspace crate；这些只允许作为 GPUI 闭包的传递依赖出现，不能被 Markdown 产品代码直接调用。

---

## 目标架构（同仓库内）

```text
crates/
  markdown_editor/     # 应用入口：窗口、标题栏、预览、命令、文件 I/O
  markdown_wysiwyg/    # tree-sitter Markdown 语义树与投影（保持独立）
  md_rope/             # 自 crates/rope 抽出
  md_sum_tree/         # 自 crates/sum_tree 抽出（若 md_rope 需要）
  md_text/             # 自 crates/text 抽出：Buffer、undo、快照
  md_buffer/           # 自 multi_buffer + language::Buffer 单文件路径抽出
  md_editor/           # 自 editor 核心子集抽出：显示、输入、高亮、块、行高
  md_theme/            # 自 theme 最小子集
  md_settings/         # 自 settings 最小子集：JSON + keymap
  md_assets/           # 最小字体、主题、keymap、图标资产；替代 Zed assets/icons
  md_ui/               # 可选：极薄 UI；或直接在 markdown_editor 中用 gpui

  # 保留不动，服务 Zed IDE：
  zed/, editor/, project/, workspace/, ...
```

### 目标依赖图

```text
markdown_editor → md_editor, md_buffer, md_theme, md_settings, md_assets, markdown_wysiwyg, gpui_platform
md_editor       → md_buffer, md_text, markdown_wysiwyg, md_theme, gpui
md_buffer       → md_text, markdown_wysiwyg?
md_text         → md_rope, md_sum_tree?
md_rope         → md_sum_tree?
md_theme        → gpui
md_settings     → gpui?, md_assets?
```

依赖方向必须保持单向：

```text
应用层(markdown_editor)
  → 编辑器层(md_editor)
  → 文档层(md_buffer, md_text)
  → 文本底层(md_rope, md_sum_tree)
  → GPUI / 社区 crate
```

`markdown_wysiwyg` 是 Markdown 语义核心，可被 `markdown_editor` / `md_editor` / `md_buffer` 消费，但不能反向依赖 UI 或编辑器层。

**验收命令（终点）：**

```powershell
cargo tree -p markdown_editor --edges normal -q | Select-String "zed\\crates\\"
```

输出中应 **仅出现** GPUI 运行时闭包相关路径；任何非 GPUI workspace crate 都必须在 `docs/md-dependency-allowlist.md` 登记原因、来源链和后续处理计划。

**产品代码硬门禁：**

```powershell
rg "use (editor|language|multi_buffer|text|rope|sum_tree|project|workspace|settings|theme|theme_settings|ui|assets|icons|markdown)::|from (editor|language|multi_buffer|text|rope|sum_tree|project|workspace)" crates/markdown_editor crates/md_*
```

终点应无结果；迁移期只能在 `legacy-editor` cfg 内出现。

---

## 从 Zed 抽出的文件指引（优先清单）

### `md_rope` / `md_sum_tree`

| 来源 | 操作 |
|------|------|
| `crates/rope/src/**` | 拷贝后改 crate 名；优先替换 `util` / `ztracing` 为产品内最小实现或社区 crate |
| `crates/sum_tree/src/**` | 按需拷贝；若 `md_rope` 必需则先迁出，保持测试等价 |

### `md_text`

| 来源 | 操作 |
|------|------|
| `crates/text/src/**` | 拷贝；去掉/替换对 Zed `gpui` 的耦合（若有） |
| `crates/clock` 最小子集 | 仅搬 Buffer 版本所需部分 |

必须保留或补齐的测试：插入、删除、undo/redo、snapshot、line/offset 转换、CRLF、CJK、emoji、combining mark、随机编辑不 panic。

### `md_buffer`

| 来源 | 操作 |
|------|------|
| `crates/multi_buffer` | **仅** singleton / `for_buffer` 相关代码 |
| `crates/language` | **仅** `Buffer::local`、snapshot、tree-sitter 挂载路径 |

明确不搬：LSP、诊断、task、project settings、远程文件系统、workspace 符号、语言服务器生命周期。

### `md_editor`（核心，分批搬）

| 优先搬 | 来源路径 |
|--------|----------|
| 显示管线 | `editor/src/display_map/**` |
| 渲染 | `editor/src/element.rs` |
| 滚动 | `editor/src/scroll/**` |
| 选区 | `editor/src/selections_collection.rs` |
| 移动/点击 | `editor/src/movement.rs`（删除 `project`/`workspace` 引用） |
| 编辑器主体 | `editor/src/editor.rs`（仅 `project: None`、`for_buffer` 路径） |
| 自定义高亮 | `editor/src/display_map/custom_highlights.rs` |
| 自定义块 | `editor/src/display_map/block_map.rs` |

**明确不搬进 `md_editor`：**

`lsp_ext`、`git/`、`code_lens`、`hover_popover`、`tasks`、`runnables`、`items.rs`（workspace 注册）、`persistence`、`edit_prediction*`、`workspace::register_*` 相关初始化。

**必须移植的已定制能力：**

- `set_row_height_overrides` / `EditorRowMetrics` / 行内垂直居中
- `HighlightStyle::hide_text`、Rendered marker reveal、拖选 freeze
- `MarkdownRenderer` + `BlockPlacement::Replace`（网络图片块）

`md_editor` 分批顺序：

1. 只读显示：buffer snapshot → display rows → GPUI text layout。
2. 光标和 selection：点击、键盘移动、滚动、soft wrap、IME composition 不回归。
3. 编辑事务：insert/delete、undo/redo、dirty/saved。
4. decoration：custom highlights、hidden marker、row height overrides。
5. block map：image/code/table 等 replacement。
6. 命令/action/keymap 接入。

### `md_theme` / `md_settings` / `md_assets`

| 来源 | 操作 |
|------|------|
| `crates/theme` | 只保留颜色、字体、`HighlightStyle` 等 Markdown 所需部分 |
| `crates/settings` | 只保留 JSON 加载、keymap 绑定；不搬 Settings UI / 全量 migrator |
| `crates/assets` / `assets/**` | 只保留 markdown-editor 启动所需字体、默认主题、默认 keymap、少量图标 |

### `markdown_editor` 应用层

- 去掉对 `editor`、`language`、`multi_buffer`、`ui`、`theme`、`settings`（Zed 版）的依赖。
- 预览：可保留轻量方案，不强制依赖 Zed `markdown` crate 的全量渲染路径。
- 启动：使用 `md_editor::init_standalone`（或等价），**禁止**调用 `editor::init`。

---

## 搬迁记录和质量门

### 搬迁记录

每批从 Zed 搬迁的文件必须在 `docs/md-extraction.md` 追加记录：

```md
## YYYY-MM-DD - md_editor display map batch

- 来源文件：
  - crates/editor/src/display_map.rs
  - crates/editor/src/display_map/custom_highlights.rs
- 目标文件：
  - crates/md_editor/src/display_map.rs
  - crates/md_editor/src/display_map/custom_highlights.rs
- 来源基线：当前 fork commit / git diff 摘要
- 保留能力：...
- 删除能力：workspace/project/lsp/...
- 手写替代：...（必须说明为什么不能搬）
- 验证：
  - cargo check -p md_editor
  - cargo test -p md_editor
  - cargo test -p markdown_wysiwyg
```

### 每批变更必须通过的质量门

1. **依赖门禁**：`md_*` crate 内不得出现原 Zed 非 GPUI crate 依赖。
2. **行为门禁**：如果搬迁文件原本有测试，优先同步测试；如果删测试，必须说明原因并补等价 markdown-editor 测试。
3. **边界门禁**：所有 `project` / `workspace` / `lsp` 分支只能删除、替换或留在 `legacy-editor`，不能进入 `md-editor`。
4. **WYSIWYG 门禁**：新增 Markdown 语义必须先进入 `markdown_wysiwyg`，再由 `md_editor` 消费；不能把 Markdown parser 写进 UI 层。
5. **性能门禁**：输入路径不得引入无界全量重算；无法增量化时必须记录当前复杂度和后续修复点。
6. **人工验证门禁**：每个 R3 以后阶段都至少手工验证打开、输入、保存、undo/redo、selection、Source/Rendered、IME 或 CJK 基本场景。

### 推荐新增的守卫命令

迁移过程中建议新增 `script/check-md-boundary` 或等价 CI step，内容至少包含：

```powershell
cargo check -p markdown_editor --features legacy-editor
cargo check -p markdown_editor --features md-editor --no-default-features
cargo test -p markdown_wysiwyg
rg "use (editor|language|multi_buffer|text|rope|sum_tree|project|workspace|settings|theme|theme_settings|ui|assets|icons|markdown)::" crates/markdown_editor crates/md_*
cargo tree -p markdown_editor --edges normal -q
```

---

## 阶段计划

### 阶段 R0 — 脚手架（约 2–3 天）

**目标：** 在同一 workspace 建立 `md_*` 空壳并纳入编译。

**范围：**

- 新建 `md_rope`、`md_sum_tree`（按需）、`md_text`、`md_buffer`、`md_editor`、`md_theme`、`md_settings`、`md_assets` 空壳。
- 更新根 `Cargo.toml` `members` 与 `[workspace.dependencies]`。
- `markdown_editor` 增加 feature `legacy-editor`（默认 true）与 `md-editor`（默认 false），便于双轨对比。
- 新增 `docs/md-extraction.md` 与 `docs/md-dependency-allowlist.md`，先写入格式和当前基线。
- 新增或记录 `script/check-md-boundary`，即使 R0 只做 warning，也要把后续依赖门禁固定下来。

**验收：**

- `cargo check -p md_text`
- `cargo check -p markdown_editor --features legacy-editor`
- `cargo check -p markdown_editor --features md-editor --no-default-features` 通过；R0 可以只显示占位编辑区，但不能留下不可编译的新路径。

---

### 阶段 R1 — 文本底层（约 1–2 周）

**目标：** `md_text` + `md_rope` 可独立运行单测（插入、删除、undo、行迭代）。

**范围：**

- 按「从 Zed 抽出的文件指引」拷贝 `sum_tree` / `rope` / `text`，删 IDE 无关模块。
- 先让 `md_sum_tree` / `md_rope` 编译，再让 `md_text` 编译；不要让 `md_text` 反向依赖 `language`。
- 保留 Zed 原有文本行为测试，缺失处补 Markdown 产品必需测试。

**验收：**

- `cargo test -p md_sum_tree`（若存在）
- `cargo test -p md_rope`
- `cargo test -p md_text`
- `cargo tree -p md_text --edges normal -q` 不出现 `language`、`editor`、`project`、`workspace`

---

### 阶段 R2 — 单文件 Buffer（约 1–2 周）

**目标：** `md_buffer::Buffer::local` 对齐当前 `language::Buffer::local` 能力（含 tree-sitter-md）。

**范围：**

- 从 `multi_buffer` / `language` 抽单文件路径；不接 LSP / Project。
- 对齐当前 `Buffer::local`、snapshot、dirty/saved、undo/redo、selection transaction 所需 API。
- Markdown parser 接入优先消费 `markdown_wysiwyg`，不把 Zed `markdown` preview crate 带入 buffer 层。

**验收：**

- `md_buffer` 可对 `.md` 内容生成 snapshot 与语法树
- `markdown_editor` 可选：仅 **文件读写** 走 `md_buffer`（编辑仍 `legacy-editor`）
- `cargo tree -p md_buffer --edges normal -q` 不出现 `language`、`multi_buffer`、`lsp`、`project`、`workspace`

---

### 阶段 R3 — `md_editor` 核心（约 4–8 周）

**目标：** 在 GPUI 上提供与当前等价的编辑体验，且无 Zed `editor` 依赖。

**范围：**

- 分批搬迁 `md_editor` 文件清单；实现 `init_standalone`；移植行高、hide_text、reveal、图片块。
- `markdown_editor --features md-editor --no-default-features` 必须能创建窗口、打开单文件、输入、保存。
- `legacy-editor` 与 `md-editor` 在一段时间内共存，作为行为对照；行为差异记录到 `docs/md-extraction.md`。
- Markdown WYSIWYG 的 source/display 映射、marker reveal 和 block replacement 进入 `md_editor` 或 `markdown_wysiwyg`，不再继续扩大对 Zed `editor` 的 patch。

**验收：**

- `cargo check -p markdown_editor --features md-editor --no-default-features`
- `cargo test -p md_editor`
- 手工验证：Source/Rendered、保存 dirty、拖选 freeze、heading 行高、图片块
- `cargo test -p markdown_wysiwyg` 仍通过
- `cargo tree -p md_editor --edges normal -q` 不出现 `editor`、`language`、`multi_buffer`、`project`、`workspace`

---

### 阶段 R4 — 主题、设置、应用壳（约 1–2 周）

**目标：** `markdown_editor` 只依赖 `md_*` + gpui 栈 + `markdown_wysiwyg`。

**范围：**

- `md_theme`、`md_settings`、`md_assets`；去掉 `ui`、`assets`（Zed 版）或改为极薄 `md_ui`。
- 删除 `editor::init`；改为 `md_editor::init_standalone` 或 `markdown_editor` 自己注册最小 action/keymap。
- 去掉 `markdown` / `markdown_preview` 作为核心 preview 依赖；保留 preview 时必须由 `markdown_wysiwyg` 或轻量自渲染承载。
- 处理 `http_client`：产品代码不得直接依赖 Zed `http_client`；如果 GPUI 图片加载需要 HTTP client trait，需包装到 `md_assets` / `markdown_editor` 内的产品自有适配或登记为 GPUI 闭包限制。

**验收：**

- `cargo tree -p markdown_editor` 无 `editor` / `language` / `project` / `workspace`
- 产品代码硬门禁 `rg` 无结果
- `cargo run -p markdown_editor --bin markdown-editor` 日常流程可用

---

### 阶段 R5 — 收尾（约 3–5 天）

**目标：** 删除 `legacy-editor` 路径与无用依赖；更新 `GOAL.md` 运行说明。

**范围：**

- 移除 `markdown_editor` 对 Zed `editor` 等的 feature 与依赖
- 文档：`docs/md-extraction.md` 记录已搬/未搬/不搬文件清单
- 给 `md_*` 加 CI job 或脚本，只编 `markdown-editor` 产品线
- 删除 `legacy-editor`、`legacy-preview`、过渡 allowlist 中已处理项

**验收：**

- 默认 feature 仅 `md-editor`
- 进展记录写入本文档底部
- `cargo tree -p markdown_editor --edges normal -q` 的非 GPUI workspace crate 均已清零或登记在 allowlist
- `cargo test -p markdown_wysiwyg`、`cargo test -p md_text`、`cargo test -p md_editor`、`cargo check -p markdown_editor` 通过

---

### 阶段 R6 — GPUI 闭包审计（后续独立化）

**目标：** 处理 `gpui` 自身拉入的非 `gpui*` workspace crate，确保“除 GPUI 外零 Zed crate”的审计口径清晰。

**范围：**

- 维护 `docs/md-dependency-allowlist.md`。
- 将产品代码直接使用的 `collections`、`http_client`、`sum_tree` 等全部改为 `md_*` 或社区 crate。
- 对 GPUI 内部仍依赖的 workspace crate，逐项判断是 GPUI 框架合理闭包、应迁入 GPUI、还是应替换为社区 crate。

**验收：**

- `markdown_editor` / `md_*` 无直接非 GPUI Zed crate 依赖。
- allowlist 中每项都有来源链、保留原因、后续处理计划。

---

## 开发规则（重构期）

1. 遵守 `.rules`；重构期不修改 `.rules`，除非走其建议流程。
2. **先搬后删**：`md_editor` 可用前，保留 `legacy-editor`。
3. 每批搬迁后运行：`cargo check -p md_editor`、`cargo test -p markdown_wysiwyg`。
4. 在 `md_*` 内用 `rg 'project::|workspace::|lsp::'` 清零 IDE 引用。
5. 新 WYSIWYG 功能优先接在 `markdown_wysiwyg` + `md_editor`，避免再加对 Zed `editor` 的调用。
6. Zed 主程序 `crates/zed` 的编译与测试不应因 `md_*` 抽取而破坏。
7. 不把 `crates/editor` 的 Markdown 专属 patch 继续扩大；已存在的通用改动要迁入 `md_editor`，再评估是否保留在 Zed `editor`。
8. 搬迁提交应小批量、可 bisect；每批都要能说明“为什么搬这些文件、删了哪些 IDE 分支、怎么验证”。
9. 所有产品层新增直接依赖必须先检查是否为原 Zed 非 GPUI crate；如果是，默认拒绝，除非改为 `md_*` 或登记为 GPUI 闭包问题。

---

## 双轨与迁移开关（建议）

```toml
# crates/markdown_editor/Cargo.toml（示意）
[features]
default = ["legacy-editor"]
legacy-editor = ["dep:editor", "dep:language", ...]
md-editor = ["dep:md_editor", "dep:md_buffer", ...]
```

迁移期要求：

- 默认仍可用 `legacy-editor`，保证用户和开发者有稳定路径。
- `md-editor` 必须尽早保持可编译，即使功能不完整。
- 任何新功能如果只在 `legacy-editor` 实现，必须写入进展记录并说明迁移计划。

终点：`default = ["md-editor"]`，删除 `legacy-editor`。

---

## 风险与预期

| 风险 | 缓解 |
|------|------|
| `editor` 与 `project` 缠结过深 | 只搬 `project: None` 路径；搬一批清一批 `rg` |
| 与上游 Zed 漂移 | `md_*` 文件头注明来源 commit；大版本合并时按文件对照 |
| 工期低估 | R3 可拆多个小 PR；先文本输入，再 WYSIWYG，再图片块 |
| `gpui` API 不足 | 在 `md_editor` 内扩展，避免回退依赖 Zed `editor` |
| 为了剥离而重写低质量编辑器 | 复制 Zed 成熟代码和测试优先；只有边界冲突时才手写替代 |
| `md_*` 复制过多 IDE 分支 | 每批搬迁必须列“不搬清单”；CI/脚本扫描 `project`、`workspace`、`lsp` |
| GPUI 自身仍拉 Zed workspace crate | 产品代码禁止直接使用；通过 allowlist 管理传递依赖，后续 R6 独立化 |
| `markdown_editor/src/main.rs` 继续膨胀 | 在 R0/R3 之间拆出 document、wysiwyg、rendering、file_io 模块，但只为双轨迁移服务 |
| 测试覆盖不足导致编辑体验退化 | 文本层搬原测试，WYSIWYG 保留 fixture，R3 起补 editor integration / 手工验证清单 |

**粗估工期（单人）：** R0–R2 约 3–4 周，R3 约 4–8 周，R4–R5 约 2–3 周，R6 视 GPUI 闭包情况单独评估。

---

## 进展记录

每完成一个重构阶段，在下方追加记录。

```md
### YYYY-MM-DD - 阶段 Rx：阶段名称

- 对应目标：
- 完成情况：
- 验收结果：
- 对后续目标的影响：
```

### 2026-05-22 - 文档：确立 REFACTOR 目标

- 对应目标：明确最终依赖边界（仅 GPUI 栈 + `md_*`）、同仓库抽 crate 策略、阶段 R0–R5。
- 完成情况：新增 `REFACTOR_GOAL.md`；与 `GOAL.md` 分工：产品 vs 架构剥离。
- 验收结果：无代码变更。
- 对后续目标的影响：后续 agent/开发以本文档为剥离路线图；新功能开发逐步从 `editor` 转向 `md_*`。

### 2026-05-22 - 文档：强化剥离成功条件

- 对应目标：把“除 GPUI 外零 Zed crate”从方向性目标强化为可执行门禁，同时保留 Zed 代码质量。
- 完成情况：补充依赖边界两层门禁、`md_assets`、目标依赖方向、产品代码硬门禁、搬迁记录模板、质量门、R0–R6 阶段验收、双轨迁移要求和风险缓解。
- 验收结果：无代码变更；文档已明确下一步应从 R0 脚手架、allowlist、extraction log 和 boundary check 开始。
- 对后续目标的影响：后续实现不能只“能编译”，还必须证明没有直接使用原 Zed 非 GPUI crate，并记录每批搬迁来源与验证结果。

### 2026-05-22 - 阶段 R0：脚手架

- 对应目标：建立 md_* 空壳并纳入编译；设立双轨迁移开关；固定依赖门禁脚本。
- 完成情况：新建 8 个空壳 crate；根 Cargo.toml 已加入 members 和 workspace.dependencies；markdown_editor 新增 legacy-editor/md-editor features；main.rs 改为双轨 dispatcher，逻辑提取至 legacy_editor.rs；新增 docs/md-extraction.md、docs/md-dependency-allowlist.md、script/check-md-boundary.ps1。
- 验收结果：cargo check -p md_text ✓；cargo check -p markdown_editor --features legacy-editor ✓；cargo check -p markdown_editor --features md-editor --no-default-features ✓
- 对后续目标的影响：R1 可直接开始向 md_text/md_rope 移植 crates/text 和 crates/rope。

### 2026-05-22 - 阶段 R1：文本底层

- 对应目标：md_text + md_rope 可独立运行单测（插入、删除、undo、行迭代）。
- 完成情况：
  - crates/sum_tree/src/* 整体复制至 crates/md_sum_tree/src/，Cargo.toml 依赖与原版相同（仅外部 crate）。
  - crates/rope/src/* 整体复制至 crates/md_rope/src/，Cargo.toml 通过 package rename 将 md_sum_tree 别名为 sum_tree，源码零修改。
  - crates/text/src/* 整体复制至 crates/md_text/src/，Cargo.toml 通过 package rename 将 md_rope 别名为 rope、md_sum_tree 别名为 sum_tree，源码零修改。
  - md_rope bench harness 暂时移除（待后续阶段补充）。
  - docs/md-extraction.md 已追加搬迁记录。
- 验收结果：
  - cargo check -p md_sum_tree / md_rope / md_text ✓
  - cargo test -p md_sum_tree: 10/10 ✓
  - cargo test -p md_rope: 24/24 ✓
  - cargo test -p md_text: 36/36 ✓
  - cargo tree -p md_text 无 language / editor / project / workspace ✓
- 对后续目标的影响：R2 可开始向 md_buffer 移植单文件 Buffer（使用 md_text 而非 language::Buffer）。

### 2026-05-22 - 阶段 R2：单文件 Buffer

- 对应目标：让 [md_buffer::Buffer::local](cci:1://file:///c:/Users/zwl31/code/rust/zed/crates/language/src/buffer.rs:931:4-942:5) 对齐 [language::Buffer::local](cci:1://file:///c:/Users/zwl31/code/rust/zed/crates/language/src/buffer.rs:931:4-942:5) 的单文件能力，并在不接入 LSP / Project 的前提下完成 markdown-editor 文件读写侧消费。
- 完成情况：
  - 已实现 standalone `md_buffer`：本地构造、snapshot、Markdown 语法树、dirty/saved、undo/redo、transaction helpers、preview version、version waiters、line ending metadata、低层 identity/history inspection。
  - 依赖边界已收紧为 `md_buffer -> md_text + markdown_wysiwyg`，并通过 `md_text` re-export 消费版本类型，未重新引入 [language](cci:1://file:///c:/Users/zwl31/code/rust/zed/crates/language/src/buffer.rs:1704:4-1707:5) / `multi_buffer` / `project` / `workspace` 依赖。
  - `markdown_editor` 的 `legacy-editor` 文件打开/保存路径现已接入 `md_buffer`：打开时构造 standalone buffer，保存时通过 `md_buffer` 序列化文本并保留 LF/CRLF 语义；编辑 UI 仍保留在 legacy `editor` 路径，等待 R3 的 `md_editor` 替换。
  - 当前 Markdown 语法刷新仍在版本变化后全量 reparse；这一性能优化点已在 extraction log 登记，留待后续 R3 批次处理。
- 验收结果：
  - cargo test -p md_text: 37/37 ✓
  - cargo test -p md_buffer: 15/15 ✓
  - cargo tree -p md_buffer --depth 1 -q：直接依赖仅 markdown_wysiwyg + md_text ✓
  - cargo check -p markdown_editor --features legacy-editor ✓
  - cargo check -p markdown_editor --features md-editor --no-default-features ✓
- 对后续目标的影响：R2 已收尾，后续工作可聚焦 R3 的 `md_editor` UI / display / selection / editing path 迁移，不再需要回补单文件 buffer 或文件读写基础能力。

### 2026-05-22 - 阶段 R3：只读显示启动片

- 对应目标：启动 `md_editor` 核心迁移的第一小步：buffer snapshot → display rows → GPUI text layout。
- 完成情况：
  - `md_editor` 从 R0 占位改为可渲染 GPUI entity，直接消费 `md_buffer::Buffer`，提供只读 display rows、行号 gutter、focus handle 和 virtualized row rendering。
  - `markdown_editor --features md-editor --no-default-features` 不再只打印占位提示，现可打开窗口，并从命令行路径读取 Markdown 文件到 md-buffer-backed 只读编辑表面。
  - `legacy-editor` 默认路径保持不变；未调用 `editor::init`，未在 md-editor 路径引入 `editor` / `language` / `multi_buffer`。
  - 该批次是 R3 bootstrap，尚未迁入 Zed `display_map` / selection / input / WYSIWYG decoration；这些继续作为 R3 后续小批次。
- 验收结果：
  - cargo test -p md_editor: 3/3 ✓
  - cargo check -p markdown_editor --features md-editor --no-default-features ✓
  - cargo check -p markdown_editor --features legacy-editor ✓
- 对后续目标的影响：R3 后续可以在这个可运行的 md-editor feature path 上逐步替换为 Zed-quality display pipeline、selection 与编辑事务，而不再需要 R0 占位入口。

### 2026-05-22 - 阶段 R3：基础光标移动

- 对应目标：推进 `md_editor` 核心迁移的第二小步：光标状态、键盘移动和最小视觉反馈。
- 完成情况：
  - `md_editor` 新增独立光标状态与 `MoveLeft` / `MoveRight` / `MoveUp` / `MoveDown` / line start / line end actions。
  - 光标移动直接基于 `md_buffer::BufferSnapshot` / `md_text::Point`，跨行移动、行尾/行首移动和 UTF-8 边界均有单测覆盖。
  - 只读渲染表面新增当前行背景和竖线 caret；纯 `md-editor` 入口绑定方向键、Home、End 到 `MarkdownEditor` key context。
  - 本批仍不接编辑事务、鼠标选择、文本输入或 IME；后续 R3 继续迁入 Zed selection/input 语义。
- 验收结果：
  - cargo test -p md_editor: 6/6 ✓
  - cargo check -p markdown_editor --features md-editor --no-default-features ✓
  - cargo check -p markdown_editor --features legacy-editor ✓
- 对后续目标的影响：`md_editor` 已具备可聚焦、可键盘移动的最小 caret 模型，可作为下一批 selection range、鼠标定位和编辑事务的承载点。