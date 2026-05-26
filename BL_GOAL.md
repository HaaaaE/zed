# Markdown Editor 布局重构进度

## 目标

重构当前项目中的 `markdown-editor` 路径，使 Source 和 Rendered 模式都支持基于宽度的自动换行，并使 Rendered 模式能够把行内 GPUI 元素以及当前纳入目标的块级渲染内容路径作为一等内容来布局：独立远程图片块和块级公式。更广义的通用块级 GPUI 元素暂时保留为未来方向，不属于当前实现目标；真实公式渲染器接入也暂时延后。最终状态必须保证文本、Markdown 样式、图片、预览、自定义 GPUI 元素、光标、选择、高亮命中、键盘移动、滚动和模式切换都与可见布局保持一致，并避免不必要的整篇重排。

## 架构约束

- 保持编辑器 / 列表外层虚拟化单位仍然是 Markdown source row。除非目标被显式重置，不要把方案改成 visual-row 或 chunk-level virtualizer。
- 在 source-row 模型内优化大文档表现：缓存 row projection / layout 工作、缩小失效范围、避免立即整篇重测，并让不可见行继续以估算值表示。
- 接受极长单行 source row 仍然是 row-local 最坏情况。要通过 source-row 级别的缓存和测量改进来缓解，而不是重设计外层架构。
- 在完成目标的同时改进可维护性：新行为要放在明确的 row / layout / cache 概念之后，避免继续增加 Rendered projection、GPUI list measurement 和输入处理之间的隐藏耦合；当局部抽取能显著降低风险时，要拆清责任边界。

## 当前进展

### 布局与换行

- Source 和 Rendered 文本行现在共用一条 display-row 布局路径，支持基于宽度的 GPUI 文本 shaping、软换行 `VisualDisplayRow` 范围、visual-row 高度、光标几何、选择框和与可见布局一致的鼠标命中测试。
- 行渲染现在通过 `DisplayRowLayout` / `DisplayBlockLayout` 区分文本行与块行，在保持 source-row virtualization 的同时，让块内容拥有自己的测量高度和渲染路径。
- Home / End、垂直移动、鼠标命中、选择框、空选中行、模式切换、窗口尺寸变化后的重排以及 undo / redo 现在都考虑 visual-row 局部几何，并会在可见 projection 变化时清理过期的布局相关 selection goal。
- 对于没有 inline atom 的文本行，如果未换行的 shaped line 已经能放下当前宽度，会跳过第二次 wrap shaping。

### Inline Atom 与行内图片

- Rendered 文本行现在会构建 `DisplayInlineFragment` 和 `DisplayInlineAtom`，把非激活状态下的行内公式和行内图片表示成原子布局内容，而不是普通可编辑文本。
- Inline atom 已参与换行、行高、显式宽高测量、选择样式、光标吸附、鼠标命中、水平 / 垂直键盘移动、Home / End，以及边界上的 Backspace / Delete 行为。
- Inline atom 的宽度可以从渲染后的 GPUI element tree 中测量；行内图片在可用时会通过 `ImgResourceLoader` 使用已加载图片尺寸，并在受限几何内保持宽高比。
- 空 alt 的行内图片会插入内部 object-replacement placeholder，从而复用与可见 alt 图片相同的 atom 路径和 source / display 映射。
- 对于仍在加载中的行内图片 atom，行会使用 fallback 尺寸渲染，但不会把这些 fallback 文本布局缓存成最终测量结果。

### 块布局与渲染元素

- 独立远程图片使用块布局路径，支持已加载资源尺寸、保持宽高比的缩放、块级 padding、整行鼠标命中目标、边界 caret、整块选择样式，以及原子化的边界移动 / 删除。
- 独立的非文本渲染内容目前先收窄到 descriptor 驱动的图片 / 公式处理；fenced code 和 pipe table 暂时继续留在文本管线中，而不是走通用块布局路径。
- 公式处理正在拆成明确的行内与块级结构路径：`$...$` 代表 inline formula，`$$...$$` 代表 block formula；二者不应再被压成同一种仅行内模型。
- 在真实公式渲染器接入前，公式继续使用占位或文本式 fallback 渲染；当前目标是先把公式的语义、布局、交互和缓存边界做对，而不是先接入最终渲染器。
- 块级公式现在已经走 descriptor 驱动的 block rendered-element 路径：`$$...$$` 会在 projection 中隐藏双分隔符，Rendered 模式下作为独立块布局、测量和渲染，而 `$...$` 继续保留为 inline atom。
- 块级公式当前使用文本式 fallback 块渲染，支持块级 padding、整行鼠标命中、边界 caret、整块 Shift-selection、左右边界移动，以及边界上的 Backspace / Delete；布局当前可缓存，不依赖异步资源。
- 当满足条件时，渲染元素边界以及整块 / 包含块的 rendered-element selection 仍会保持 inactive，这样 inline atom 和独立图片块可以继续以渲染形态显示，同时周围被选中的 Markdown 仍可显露 source 语法。
- 块几何现在统一通过共享的块接口提供 source range、可见 x 位置、鼠标目标、行边界移动、caret 位置和选择状态。
- 远程图片块布局现在显式区分是否可缓存：已加载图片尺寸可以缓存，仍在加载或图片资源无效时的 placeholder 布局不缓存。
- Block 行现在也显式区分非渲染交互 fallback 布局与渲染阶段的实测布局：键盘移动 / 行边界命中可以在不进入 GPUI layout / paint 阶段的情况下遍历图片块与公式块，而真正渲染时仍会测量并缓存可缓存的块尺寸。
- 复现 GPUI `ListState` re-entry panic 之后，渲染阶段的 list remeasure 已被移除；块行通过正常的 list layout pass 测量。

### 缓存与大文档性能

- 行布局现在按 source row、mode、wrap width 以及 Rendered 模式下相关 active source range 进行缓存，并被渲染、鼠标命中、Home / End 和 visual-row 移动复用。
- Display row 现在按 buffer version、row、mode 和 marker visibility 依赖跨渲染与交互路径缓存；projection state 在一次 pass 中只构建一次，并被复用来生成 row layout cache key。
- Rendered 行的 inline span 查询现在是 range-local，并带有按 span start 建立的索引结构，避免对每个可见行都做全文件 inline span 扫描。
- Source 行现在使用 text-snapshot 快路径来完成 display-row 查询 / API、渲染、row-local 文本布局、选择 / caret 绘制、鼠标目标和纯文本 fragment projection。这些路径避免了 Rendered 专用的 style / atom / hidden-range 工作，也避免了当 Source 调用者只需要文本时刷新 Markdown 语法树。
- Display row 现在携带原始 source text / source range，row layout 路径因此不需要再次回读 buffer 来做 row-local Markdown 检查。
- Rendered 渲染帧现在每帧只快照一次 buffer 并裁剪一次 selection，同时 buffer snapshot 通过 `Arc` 共享缓存后的 Markdown 语法树。
- 文本优先的编辑器路径，例如 row count、row text 读取、cursor clipping、Source 模式下的水平与垂直移动 / 选择、换行感知的 Home / End、普通 replace / backspace / delete 编辑、cursor reveal、auto-indent 和 Source 编辑缓存失效，现在都直接使用 buffer 的 text snapshot，而不是为了刷新 Markdown 语法而走完整 buffer snapshot。
- Source 模式交互路径即使不需要测量 inline atom，也会缓存可缓存的纯文本 row layout，从而提升换行键盘移动和换行感知 Home / End 的复用率。
- Rendered 模式交互路径现在也会在布局本身可缓存时缓存 row layout，因此纯文本 Rendered 行可以在非渲染交互调用之间复用布局；只有带未测量 inline atom fallback 的行保持不缓存。
- 已加载的远程图片块布局可以参与 row-layout cache 复用；但 loading placeholder 布局保持不缓存，以便最终图片尺寸能替换它。
- 对需要异步资源或后续重测的 rendered element，fallback 布局不能被当成最终可缓存布局；真实尺寸或最终测量结果就绪后，应只让受影响的 row 做局部失效与重测。
- `ListState::with_default_size_hint` 为长的可变高度列表提供了默认的未测量行高，减少在行尚未测量前的滚动条塌陷和滚动位置抖动。
- 宽度变化会清理编辑器行布局状态和过期的 selection goal，但不会触发额外的整列表重测，除 GPUI list 自身的宽度失效之外不增加额外 remeasure。
- Source 模式下普通的单行编辑现在只会清理并重测被编辑那一行的缓存布局，并将可复用的 Source display-row cache entry 重新挂到新的 buffer version。长度变化的编辑只保留编辑之前的行，因为之后的 source range 可能整体偏移；长度不变的编辑则也会保留之后的行。Undo / redo 在 editor selection history 能证明该事务只发生在一个 Source row 上时，也会复用同样的局部失效逻辑；Rendered 编辑、跨行编辑、row-count 变化，以及没有 selection history 的事务仍然走保守路径。

### 模块形状与测试

- Inline atom 布局、atom 命中几何、块渲染和块布局构造已经分别移动到各自的 atom / block 接口后面，以减少 row pipeline 中的临时分支。
- Inline atom 的常量、尺寸、测量、fragment atom typing 和 atom 渲染 helper 现在都放在内部 `inline_atom` 模块里，使主编辑器文件更聚焦于 row layout 和交互流。
- Inline span 到 atom 的分发，以及 atom 构造，现在都通过共享入口来路由，减少 `lib.rs` 中按具体 kind 分支的逻辑，并为后续 inline atom 类型提供统一扩展点。
- 远程图片块的布局、测量、命中几何、渲染以及 inactive-image 检测现在都放在内部 `block` 模块里，使主编辑器文件只负责把 block row 接入共享 display-row layout 路径。
- Block row selection 现在会先经过共享的 rendered-element discovery 阶段，再 materialize `DisplayBlockLayout`，从而把独立图片处理隔离开，同时为未来非文本 block descriptor 留出接口，而不把它们纳入当前工作范围。
- Rendered-element 边界与 active-range helper 现在都放在内部 `rendered_element` 模块里，降低了 `lib.rs` 中与非布局逻辑的耦合，同时保持 movement、selection reveal / hide 和 inactive rendered-element 行为不变。
- `md_editor` 仍然需要继续做更多内部模块边界清理；`lib.rs` 现在仍承载 projection、row layout / cache、selection / movement、hit-testing、rendering 和大量测试。
- 目前测试覆盖已经包括：wrapped movement、action-level Source wrapped keyboard movement、Rendered wrapped inline-image keyboard movement、Rendered image / formula block keyboard boundary movement、Rendered image / formula block keyboard selection extension、Source display-row 与 render 快路径、Source wrapped mouse hit testing 与跨 visual row 的 shift-selection、Rendered marker reveal / hide 过渡、resize reflow、模式切换时 wrapped position 处理、visual-row bounds、rendered inline math、inline image、空 alt inline image、remote image block、block formula projection / detection / interaction、Rendered image block 与 formula block 的鼠标命中和 shift-selection、cache dependency key、range-local span query、source-row fast path、Source undo / redo 局部缓存失效、Rendered interaction layout caching、remote image block cacheability、snapshot sharing、default list size hint，以及 rendered image block 的 crash 路径。

## 验证

- 最近相关 crate 的检查已经通过，包括 `cargo fmt -p markdown_wysiwyg -p md_editor`、`cargo check -p markdown_wysiwyg -p md_editor`、`cargo test -p markdown_wysiwyg -p md_editor`；当前 `markdown_wysiwyg` 为 17 个测试，`md_editor` 为 144 个测试。
- 当前的聚焦测试覆盖已验证 wrapped movement、inline atom / image、source display-row / cache / render 快路径、Source edit 与 undo / redo 缓存失效、Rendered interaction layout caching、remote image block cacheability、default list size hint、Rendered image block 绘制与鼠标交互，以及 block formula 的 projection、边界移动、Shift-selection、Backspace / Delete 与鼠标命中。
- 新补的回归确认了一个此前未覆盖的 Rendered wrapped-layout 缺口：当同一 source row 内的 inline image 让 visual row 从 atom 边界开始或结束时，`MoveDown` / `End` / `Home` 会继续保持当前 visual-row 语义，不会因为 rendered active-range 同步而意外退化到整条 source row 的行首/行尾语义。
- 新补的 action-level block 回归还确认了另一类此前未覆盖的缺口：`Rendered` 模式下跨图片块 / 公式块做 `MoveUp` / `MoveDown` / `Home` / `End` 不再依赖只能在 GPUI request-layout / prepaint / paint 阶段调用的测量 API；非渲染交互路径使用 fallback 块几何，渲染阶段再测量真实尺寸。
- 新补的 block selection 回归进一步确认了 `Rendered` 模式下跨图片块 / 公式块做 `SelectDown` / `SelectUp` / `SelectToBeginningOfLine` / `SelectToEndOfLine` 时，会按块边界扩展和收缩选择，而不是退回普通 source-row 语义。
- `git diff --check` 当前只剩 LF / CRLF 警告；在修复 Rendered image block 问题后做过简短的 markdown-editor smoke run，没有复现之前的 panic。

## 已知剩余工作

下面这些条目是类别，不代表优先级或执行顺序。

- 用 300KB 级 Markdown 文件在 Source 和 Rendered 模式下做 profiling，定位剩余的 source-row-local 热点，再决定后续性能修改。
- 继续在 source-row 架构内做优化：降低 row layout 成本、减少 string / fragment churn、增强 row-layout cache 复用、缩小 remeasure 和缓存失效范围，并改善大但不过分极端文档的表现。
- 将 inline atom 的 measurement 继续泛化，超出当前 inactive inline math atom 路径的假设范围；同时补上当未来 atom 内容可能在缓存后继续变尺寸时的失效机制。
- 为 profiling 或手工使用中发现的剩余 visual-row 键盘移动缺口补更强的 runtime 或 visual tests。
- 为 Rendered image / block 行为以及剩余 wrapped-layout 交互缺口补更强的 runtime 或 visual tests。

## 未来方向

- 更广义的通用块级 GPUI 元素不属于当前实现目标。当前块级 rendered-element 目标只包括独立远程图片块和块级公式。
- table 未来如果要从文本管线升级为真正的渲染块，不应落到 generic block 文本包裹路径里，而应走跨 source row 的 structured layout；这属于后续独立课题，不属于当前范围。
- 任意 GPUI block measurement、自定义 block widget、table-specific structured layout，以及真实公式渲染器接入，都可以在 inline atom 泛化、profiling 和 `lib.rs` 边界清理完成后再重新评估。
