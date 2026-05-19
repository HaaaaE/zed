# Zed → 纯 Markdown 编辑器 改造计划

> 基于 Zed v1.2.6 (tag: v1.2.6, branch: markdown-editor-only)
> 生成日期: 2026-05-19

---

## 一、项目目标

将 Zed 精简为一个**纯 Markdown 编辑器**，保留：
- Markdown 文件编辑（语法高亮、自动补全、括号匹配等）
- Markdown 实时预览（侧边面板）
- 基础编辑功能（搜索、跳转、大纲、代码片段等）
- 设置/主题/快捷键配置

删除所有无关功能：AI Agent、协作、调试器、终端、Copilot、Vim、远程等。

---

## 二、代码库概况

| 指标 | 数值 |
|---|---|
| workspace crate 总数 | 234 |
| 扩展目录 | 4 (extensions/) |
| 工具目录 | 3 (tooling/) |
| 默认成员 | crates/zed |

---

## 三、Markdown 功能架构

### 3.1 四大组成部分

| 功能 | 核心 crate | 作用 |
|---|---|---|
| Markdown 渲染引擎 | `crates/markdown` | 将 Markdown 文本渲染为 GPUI 可视元素 |
| Markdown 预览面板 | `crates/markdown_preview` | 在侧边/新面板中预览 .md 文件 |
| Markdown 编辑支持 | `editor` + `grammars` + `languages` | 编辑 .md 文件时的语法高亮、自动补全等 |
| HTML 转 Markdown | `crates/html_to_markdown` | HTML → Markdown 格式转换 |

### 3.2 渲染引擎 (`crates/markdown`) 被 15+ crate 依赖

被以下组件使用（不仅仅是 md 预览）：
- `markdown_preview` — 预览面板
- `editor` — Hover 弹窗、签名帮助
- `agent_ui` — AI 对话渲染
- `workspace` — 通知消息
- `git_ui` — Blame、Commit 提示
- `diagnostics` — 诊断信息渲染
- `repl` — Notebook 单元格
- `auto_update_ui` — 更新通知
- `edit_prediction_ui` — 预测弹窗
- `acp_thread` / `acp_tools` — Agent 工具
- `remote_connection` — 远程连接
- `ui_prompt` — 提示框
- `zed` 主 crate — 遥测日志、迁移通知

### 3.3 Markdown 子模块

- `parser.rs` — Markdown 解析（pulldown-cmark 封装）
- `mermaid.rs` — Mermaid 图表支持
- `html/` — HTML 解析和渲染
- `path_range.rs` — 路径范围解析

### 3.4 Tree-sitter 语法

- `crates/grammars/src/markdown/config.toml` — Markdown 语言配置
- `crates/grammars/src/markdown-inline/config.toml` — 内联语法配置
- `crates/grammars/src/markdown/injections.scm` — 代码块注入查询
- `crates/grammars/src/grammars.rs:29-30` — 注册 markdown 和 markdown-inline 语法
- 外部依赖：`tree-sitter-md`

---

## 四、依赖传递链分析

### 4.1 核心发现

**`workspace` 不依赖 `sidebar`/`agent`/`agent_ui`/`language_models`！**

`workspace` 内部自己定义了 `Sidebar` 类型（在 `multi_workspace` 模块中），
`sidebar` crate 是一个独立的 UI crate，反向依赖 `workspace`。

### 4.2 `markdown_preview` 传递闭包（必须保留）

```
markdown_preview
  → editor, workspace, project, markdown, language, ui, settings, theme, theme_settings, zed_actions
  ├─ workspace → agent_settings, client, remote, session, db, fs, git, task, telemetry, ...
  │   ├─ agent_settings → language_model, agent-client-protocol
  │   │   └─ language_model → language_model_core, credentials_provider, env_var, icons
  │   └─ client → cloud_api_client, cloud_api_types, cloud_llm_client, rpc, proto
  └─ project → terminal, dap, extension, context_server, lsp, prettier, buffer_diff, worktree, ...
```

### 4.3 关键耦合点（即使不需要也无法删除）

| 耦合 | 原因 |
|---|---|
| `workspace` → `agent_settings` → `language_model` | AI 抽象层被 workspace 强制引入 |
| `client` → `cloud_api_client`/`cloud_llm_client` | 云 API 被 client 强制引入 |
| `project` → `dap`/`extension`/`context_server`/`terminal` | 调试/扩展/终端被 project 强制引入 |

---

## 五、可删除 crate 详细分类

### 5.1 AI/Agent 系统 — 全删

| crate | 说明 |
|---|---|
| `agent` | AI 代理核心，管理对话线程和工具调用 |
| `agent_servers` | Agent 服务器连接，管理外部 Agent 进程 |
| `agent_ui` | Agent 面板 UI，渲染 AI 对话界面 |
| `acp_thread` | Agent Client Protocol 线程管理 |
| `acp_tools` | ACP 工具实现（文件操作、Shell 命令等） |
| `action_log` | Agent 操作日志记录 |
| `ai_onboarding` | AI 功能首次使用引导 |
| `prompt_store` | 用户自定义 Prompt 的存储和管理 |
| `rules_library` | AI 规则库（.rules 文件管理） |
| `language_models` | LLM 提供商的具体实现（调用各家 API） |
| `language_models_cloud` | 云端 LLM 提供商实现 |
| `language_onboarding` | LLM API Key 配置引导 |
| `language_extension` | 语言扩展的 Agent 集成 |
| `language_tools` | 语言工具（LSP 按钮、日志等） |
| `html_to_markdown` | HTML 转 Markdown 格式（Agent 网页抓取用） |
| `shell_command_parser` | Shell 命令解析器 |
| `streaming_diff` | 流式 Diff 渲染（AI 代码修改预览） |
| `opencode` | OpenCode 集成 |
| `eval_cli` | Agent 评估命令行工具 |
| `eval_utils` | Agent 评估工具函数 |
| `zeta_prompt` | Zeta Prompt 模板 |
| `edit_prediction_context` | 编辑预测的上下文收集 |
| `edit_prediction_metrics` | 编辑预测的指标采集 |

### 5.2 LLM 提供商 — 全删

| crate | 说明 |
|---|---|
| `anthropic` | Anthropic Claude API 客户端 |
| `open_ai` | OpenAI GPT API 客户端 |
| `google_ai` | Google Gemini API 客户端 |
| `bedrock` | AWS Bedrock API 客户端 |
| `deepseek` | DeepSeek API 客户端 |
| `mistral` | Mistral API 客户端 |
| `ollama` | Ollama 本地模型客户端 |
| `lmstudio` | LM Studio 本地模型客户端 |
| `codestral` | Codestral API 客户端 |
| `x_ai` | xAI (Grok) API 客户端 |
| `open_router` | OpenRouter API 客户端 |
| `aws_http_client` | AWS HTTP 客户端封装 |

### 5.3 Copilot — 全删

| crate | 说明 |
|---|---|
| `copilot` | GitHub Copilot 补全集成 |
| `copilot_chat` | Copilot Chat 对话功能 |
| `copilot_ui` | Copilot 的 UI 组件 |

### 5.4 协作系统 — 全删

| crate | 说明 |
|---|---|
| `call` | 语音/视频通话（基于 LiveKit） |
| `collab` | 协作服务端（zed.dev 后端） |
| `collab_ui` | 协作 UI（频道面板、共享屏幕等） |
| `livekit_api` | LiveKit 服务端 API SDK |
| `livekit_client` | LiveKit 客户端（音视频流处理） |
| `media` | 媒体处理（音视频编解码） |
| `denoise` | 音频降噪处理 |
| `audio` | 音频播放/录制抽象层 |

### 5.5 远程/SSH — 全删

| crate | 说明 |
|---|---|
| `remote_connection` | 远程连接管理（SSH 隧道建立） |
| `remote_server` | 远程服务器端守护进程 |

### 5.6 自动更新 — 全删

| crate | 说明 |
|---|---|
| `auto_update` | 自动更新检查和下载逻辑 |
| `auto_update_helper` | 更新辅助进程（替换正在运行的二进制） |
| `auto_update_ui` | 更新通知和进度 UI |

### 5.7 扩展系统 — 全删

| crate | 说明 |
|---|---|
| `extension_api` | 扩展的 WASM API 定义 |
| `extension_cli` | 扩展开发/发布 CLI 工具 |
| `extension_host` | 扩展宿主（WASM 运行时管理） |
| `extensions_ui` | 扩展管理 UI（安装/卸载/配置面板） |

### 5.8 编辑预测 — 全删

| crate | 说明 |
|---|---|
| `edit_prediction` | 基于上下文的代码自动补全预测引擎 |
| `edit_prediction_cli` | 编辑预测的命令行训练/评估工具 |
| `edit_prediction_ui` | 编辑预测的 UI 渲染（灰色 ghost text） |
| `edit_prediction_types` | **不能删**，editor 依赖它 |

### 5.9 调试器 — 全删

| crate | 说明 |
|---|---|
| `dap_adapters` | 各语言调试适配器的具体实现 |
| `debug_adapter_extension` | 调试适配器的扩展注册机制 |
| `debugger_tools` | 调试器辅助工具（变量查看、断点管理） |
| `debugger_ui` | 调试器完整 UI（断点/变量/调用栈/控制台） |

### 5.10 预览/查看器 — 全删

| crate | 说明 |
|---|---|
| `image_viewer` | 图片文件查看器 |
| `svg_preview` | SVG 文件预览面板 |
| `csv_preview` | CSV/表格数据预览面板 |

### 5.11 Git 扩展 — 全删

| crate | 说明 |
|---|---|
| `git_graph` | Git 分支/提交历史图形化 |
| `git_ui` | Git 操作 UI（diff 装饰、blame、提交、冲突解决） |

### 5.12 Vim 模式 — 全删

| crate | 说明 |
|---|---|
| `vim` | Vim 键位模式完整实现 |
| `which_key` | 前缀键快捷键提示面板 |

> `vim_mode_setting` **不能删**，editor 依赖它。

### 5.13 过重 UI 组件 — 全删

| crate | 说明 | 无法保留原因 |
|---|---|---|
| `settings_ui` | 设置界面完整 UI | 依赖 agent/copilot/edit_prediction |
| `title_bar` | 窗口标题栏 | 依赖 call/git_ui/livekit_client |
| `recent_projects` | 最近项目面板 | 依赖 dev_container/extension_host |
| `project_panel` | 文件树侧边栏 | 依赖 git_ui |
| `component_preview` | UI 组件开发预览 | 开发工具，非用户功能 |

### 5.14 其他 UI — 全删

| crate | 说明 |
|---|---|
| `language_selector` | 语言模式手动选择器 |

### 5.15 杂项 — 全删

| crate | 说明 |
|---|---|
| `dev_container` | Dev Container 支持 |
| `journal` | 日记/笔记功能 |
| `onboarding` | 首次启动引导流程 |
| `web_search` | 网页搜索抽象层 |
| `web_search_providers` | 网页搜索提供商实现 |
| `zed_env_vars` | Zed 环境变量管理 |
| `zlog_settings` | 日志系统配置 |
| `docs_preprocessor` | 文档预处理器 |

### 5.16 其他工具 — 全删

| crate | 说明 |
|---|---|
| `theme_importer` | 从其他编辑器导入主题 |
| `scheduler` | 任务调度器（定时/延迟任务执行） |
| `time_format` | 时间格式化工具（相对时间显示如“3分钟前”） |

### 5.17 基准测试

已确认保留，不删除。

### 5.18 工具目录

已确认保留，不删除。

### 5.19 扩展目录 — 全删

| 目录 | 说明 |
|---|---|
| `extensions/glsl` | GLSL 着色器语法高亮 |
| `extensions/html` | HTML 语言支持 |
| `extensions/proto` | Protocol Buffers 支持 |
| `extensions/test-extension` | 测试用示例扩展 |

### 5.20 构建辅助

已确认保留，不删除。

---

## 六、需重构/额外依赖的保留 crate

### 6.1 需重构

| crate | 理由 | 重构内容 |
|---|---|---|
| `sidebar` | 侧边栏容器 | workspace 只有 trait 接口，需重写不依赖 agent 的最小实现（宽度拖拽、开关状态、空白面板容器） |
| `file_finder` | 文件查找 | 移除 `project_panel` 依赖 |

### 6.2 需拉入额外依赖

| crate | 理由 | 额外依赖 |
|---|---|---|
| `notifications` | 用户通知 | `channel`（已在保留列表） |
| `keymap_editor` | 快捷键编辑 | `json_schema_store`、`command_palette`（见下方） |

### 6.3 需拉入的新 crate

| crate | 被谁需要 | 自身依赖是否干净 |
|---|---|---|
| `json_schema_store` | `keymap_editor` | 干净（依赖均已在闭包） |
| `command_palette` | `keymap_editor` | 干净（依赖均已在闭包） |
| `channel` | `notifications`、`file_finder` | 干净（依赖均已在闭包） |

### 6.4 依赖链总结

```
keymap_editor → command_palette → (workspace, client, picker 等，均已在闭包)
keymap_editor → notifications → channel → (client, rpc，均已在闭包)
keymap_editor → json_schema_store → (dap, extension, language，均已在闭包)
file_finder → channel (同上)
file_finder → project_panel → git_ui (需重构移除)
sidebar → agent/agent_ui (需重写移除)
```

---

## 七、需要的代码重构

| 重构 | 目的 | 难度 |
|---|---|---|
| **重写 `sidebar` 最小实现** | workspace 只定义了 `Sidebar` trait，具体实现在 sidebar crate 中。需重写一个不依赖 agent 的极简 sidebar（宽度拖拽、开关状态、空白面板容器） | **中** |
| `file_finder` 移除 `project_panel` 依赖 | 使 file_finder 独立于 git_ui | 低 |
| `keymap_editor` 移除 `notifications` 依赖（可选） | 减少耦合 | 低 |
| `zed/src/main.rs` 移除所有已删 crate 引用 | 编译通过 | 中 |
| 重构 `workspace` 去除 `agent_settings` 依赖（可选） | 彻底剥离 AI | 高 |
| 重构 `project` 去除 `dap`/`extension` 依赖（可选） | 彻底剥离调试/扩展 | 高 |

---

## 八、风险提示

1. **`workspace` → `agent_settings` 耦合**：即使删掉 agent crate，workspace 仍会拉入 `language_model`、`language_model_core`、`agent-client-protocol` 等 AI 基础设施。要彻底剥离需重构 workspace。

2. **`project` → `dap`/`extension` 耦合**：project 强制依赖调试和扩展系统。要彻底剥离需重构 project。

3. **`client` → `cloud_api` 耦合**：client 强制依赖云 API 客户端。要彻底剥离需重构 client。

4. **编译时间**：即使删除 crate，如果核心 crate 的传递依赖不变，编译时间改善有限。真正的收益来自删除大量 UI 代码和外部依赖（如 livekit、wasmtime 等）。

5. **`edit_prediction_types` 不能删**：虽然名字像编辑预测，但它定义了 editor 使用的基础类型，在 editor 传递闭包中。

---

## 九、改造后能否正常运行

### 结论：能跑，但需要满足前提条件。

### 能跑的原因

`markdown_preview`、`editor`、`workspace`、`project`、`markdown`、`language`、`ui`、`settings`、`theme`、`gpui` 这些核心 crate 全部保留，它们提供了编辑器运行所需的全部基础能力。

### 前提条件

1. `crates/zed/src/main.rs` 中的初始化代码必须正确清理——删掉 agent/copilot/debugger 等的 `init()` 调用，但保留 editor/workspace/markdown_preview/settings/theme 的初始化
2. `crates/zed/src/zed.rs` 中的 action 注册、菜单构建、快捷键绑定必须保留核心部分
3. 重写后的 `sidebar` 最小实现必须能正常工作

### 最大风险点

`main.rs` 和 `zed.rs` 是高度耦合的入口文件，里面混杂了所有功能的初始化。比如一个函数里可能同时初始化了 editor、agent、copilot、debugger，需要小心只删 agent/copilot/debugger 的部分，不能把 editor 的也删了。

### 建议的执行方式

每删一类 crate 后立即 `cargo build`，逐个验证编译通过，而不是一次性全删再修。这样定位问题更容易。

建议顺序：
1. 先删最独立的：LLM 提供商 → Copilot → 协作 → 远程 → 自动更新
2. 再删有 UI 依赖的：Agent 系统 → 调试器 → 扩展系统
3. 再删编辑器周边：Vim → Git 扩展 → 预览/查看器
4. 最后处理过重 UI 组件：sidebar 重写 → file_finder 重构 → main.rs 清理
5. 每步都 `cargo build` 验证
