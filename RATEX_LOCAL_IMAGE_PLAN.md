# RaTeX Formula Rendering and Local Image Rendering Plan

## Goal

Add two rendered-mode capabilities to `md_editor` without changing the source-row
virtualization model:

- render Markdown formulas through RaTeX instead of text-only placeholders
- render local Markdown images through GPUI path-backed image loading

The implementation must keep measurement and rendering in agreement. The same
resolved image or formula source should drive layout, cache keys, and paint.

## Hard Constraints

- Do not add Tokio.
- Do not add HTTP client dependencies to `md_editor`.
- Do not change outer virtualization from source rows to visual rows.
- Do not add local file watchers in v1.
- Do not build a general Markdown asset pipeline in v1.
- Do not render formulas for the full document up front.

Remote image downloading is handled by the standalone app-level HTTP client from
`UREQ_PLAN.md`. This plan should keep remote images as `Resource::Uri(...)` and
let GPUI's `ImgResourceLoader` use the configured app HTTP client.

## Current Problems To Fix

- Image destinations are currently stored as raw strings and treated as remote
  URIs in measurement.
- `img(raw_string)` treats non-URI strings as embedded assets, so local file
  paths are not routed to `Resource::Path(...)`.
- Block image detection is remote-only, so standalone local image rows cannot
  use image block layout.
- Formula rendering currently measures and paints styled text, not rendered
  formula output.
- Document-relative image paths cannot be resolved because `MarkdownEditor`
  does not carry the Markdown document path.

## Design Overview

### 1. Use One Internal Image Source Type

Add an internal `markdown_image` module in `md_editor`.

Core types:

- `MarkdownImageDestination`
  - raw Markdown destination string
  - optional alt text
- `ResolvedMarkdownImage`
  - `Remote { uri }`
  - `Local { path }`
  - `Invalid { reason }`
- `MarkdownImageSource`
  - raw destination
  - resolved image state
  - fallback label
  - stable cache key

Required helpers:

- `resource(&self) -> Option<Resource>`
- `image_source(&self) -> Option<ImageSource>`
- `cache_key(&self) -> MarkdownImageSourceKey`
- `is_renderable(&self) -> bool`

Measurement and rendering must receive `MarkdownImageSource`; they should not
re-parse raw strings independently.

### 2. Resolve Image Paths Conservatively

Resolution rules, in order:

1. `http://` and `https://` become `Resource::Uri(...)`.
2. `file://` becomes `Resource::Path(...)` if it parses to a valid local path.
3. Absolute local file paths become `Resource::Path(...)`.
4. Relative paths become `Resource::Path(document_parent.join(path))` only when
   the editor has a document path.
5. Unsupported schemes and unresolved relative paths become `Invalid`.

Important details:

- Keep the raw destination for fallback labels and source mapping.
- Do not call `canonicalize` in v1; missing files should still have a stable
  local resource identity.
- Avoid treating Windows drive-letter paths as unsupported URI schemes.
- Prefer the workspace `url` crate for `file://` parsing if it is already
  available to `md_editor`; otherwise keep a small parser scoped to `file://`.

### 3. Add Document Path State

Add document path support to `MarkdownEditor`:

- `MarkdownEditor::for_text_with_document_path(text, path, cx)`
- `MarkdownEditor::set_document_path(path, cx)`
- `MarkdownEditor::document_path()`

`markdown_editor` should pass the opened path when constructing or replacing the
editor. New unsaved documents should pass `None`. After Save As succeeds, update
the editor document path before or together with marking the document saved.

Changing the document path must clear:

- display row cache
- row layout input cache
- row layout cache
- block layout cache
- inline atom measurement cache
- any pending local image row invalidation state

### 4. Generalize Image Blocks

Rename the internal remote-image block concept to image block:

- `DisplayBlockKind::RemoteImage` -> `Image`
- `DisplayBlockLayout::RemoteImage` -> `Image`
- `RenderedImageBlock.url` -> `RenderedImageBlock.image_source`

Block detection should accept any renderable image source, not only remote URLs.
The existing rule still applies: a Markdown image becomes a block only when its
source range is the only non-whitespace content in the display row and the image
source is inactive.

Unsupported or unresolved image sources should stay on the fallback text path.

### 5. Thread Image Sources Through Inline Atoms

Replace inline atom `image_url: Option<String>` with
`image_source: Option<MarkdownImageSource>`.

Inline image measurement should:

- use `image_source.resource()` with `ImgResourceLoader`
- return `Pending(fallback)` while the asset is loading
- return `Ready(decoded_size)` when image dimensions are available
- return `Invalid(fallback)` for unsupported paths, unresolved paths, or loader
  failures

Pending measurements must not make row layout cacheable. Invalid measurements may
be cached as invalid fallback geometry because v1 has no file watcher; they must
not be cached as loaded image geometry.

Inline image rendering should call `img(image_source.image_source())` when the
source is renderable, not `img(raw_string)`.

### 6. Thread Image Sources Through Block Layout

Block image measurement should:

- use the same `MarkdownImageSource` as rendering
- use `ImgResourceLoader` with `Resource::Path` for local files and
  `Resource::Uri` for remote images
- compute height from decoded dimensions
- keep placeholder height while pending
- keep fallback placeholder for invalid sources

Block image rendering should call `img(image_source.image_source())` and keep the
current fallback element for loading or failed images.

### 7. Add RaTeX Formula Assets

Add a `formula_render` module in `md_editor`.

Use RaTeX crates for v1:

- `ratex-parser`
- `ratex-layout`
- `ratex-render`

Enable embedded fonts for `ratex-render` if that is the supported crate feature
for the selected version. Verify the exact crate version and feature names during
implementation before editing `Cargo.toml`.

Core types:

- `FormulaRenderKey`
- `FormulaRenderAsset`
- `FormulaRenderState`
  - `Ready { image, logical_size }`
  - `Pending { fallback_size }`
  - `Invalid { fallback_size }`

The key must include:

- TeX source
- inline vs block mode
- text size
- line height or formula scale input
- formula color
- padding used by the rendered PNG
- window scale factor

The asset task should run:

`parse -> layout -> display list -> PNG render -> RenderImage`

Do not run this pipeline for every formula in the document. It should be invoked
only for rows that layout or render asks for.

### 8. Integrate RaTeX With Inline Math

Inline math should keep current fallback geometry while pending. Once the formula
asset is ready, inline atom measurement should use the rendered image logical
size plus inline padding.

The existing row-local atom invalidation model should be reused:

- when a formula transitions from pending to ready, clear only that row's layout
  cache
- schedule deferred `remeasure_items(row..row + 1)`
- do not re-enter list measurement from layout or render

Invalid formulas should show fallback source text and cache the invalid state for
the formula key.

### 9. Integrate RaTeX With Block Math

Block math should use the same formula asset system with block mode in the key.

While pending:

- keep the current fallback block height
- keep the row layout non-cacheable

When ready:

- use the rendered image height plus block padding
- render the formula image centered inside the existing block interaction area
- keep caret, selection, and mouse mapping based on the source range boundaries

Invalid block formulas should fall back to source text and cache invalid state.

## Recommended Execution Order

1. Add `MarkdownImageSource` classification and unit tests.
2. Add document path state to `MarkdownEditor` and pass it from `markdown_editor`.
3. Replace raw image URL fields in rendered descriptors with resolved image
   sources.
4. Generalize remote image block naming and detection to image blocks.
5. Update inline image measurement/rendering to use `Resource::Path` or
   `Resource::Uri` from `MarkdownImageSource`.
6. Update block image measurement/rendering to use the same source object.
7. Add local image tests for absolute paths, relative paths, `file://`, missing
   files, unsupported schemes, inline rendering, and block rendering.
8. Add RaTeX dependencies and the `formula_render` asset module.
9. Move inline math measurement/rendering onto formula assets.
10. Move block math measurement/rendering onto formula assets.
11. Add formula cache, invalid-formula, scale-factor, style-key, and interaction
    tests.
12. Run formatting, focused tests, and checks.

## Tests

Image source classification:

- `https://example.com/cat.png` resolves to remote URI.
- `http://example.com/cat.png` resolves to remote URI.
- absolute local paths resolve to local path resources.
- `file://` paths resolve to local path resources.
- relative paths resolve against the Markdown document parent.
- relative paths in unsaved documents are invalid.
- unsupported schemes are invalid.
- Windows drive-letter paths are not rejected as URI schemes on Windows.

Inline images:

- rendered inline local image creates an inline atom with a local image source.
- loaded local image dimensions affect inline atom geometry.
- loading local image fallback is not cached as final ready geometry.
- invalid local image uses fallback geometry.
- existing remote inline image behavior remains unchanged.

Block images:

- standalone local image rows become image blocks in rendered mode.
- local image block height uses decoded aspect ratio.
- missing local image block keeps fallback placeholder behavior.
- rows with surrounding text stay inline, not block.
- existing remote image block behavior remains unchanged.

Formulas:

- inline formula uses fallback while pending and rendered image size when ready.
- block formula uses fallback height while pending and rendered image height when
  ready.
- invalid formula caches invalid fallback state.
- formula key changes when TeX, mode, text size, color, padding, or scale factor
  changes.
- repeated formula/style pairs reuse the same asset key.

Interaction regressions:

- caret movement across inline images, image blocks, inline formulas, and block
  formulas still lands on source boundaries.
- mouse hit testing maps image and formula blocks to source range edges.
- Home/End and MoveUp/MoveDown use updated geometry after an image or formula
  becomes ready.
- active source selection still reveals source text instead of forcing rendered
  atoms or blocks.

Verification commands:

- `cargo fmt -p md_editor -p markdown_editor`
- `cargo test -p md_editor`
- `cargo test -p markdown_wysiwyg`
- `cargo check -p md_editor -p markdown_editor`
- after implementing `UREQ_PLAN.md`, confirm `markdown_editor` has no Tokio or
  Reqwest path with `cargo tree -p markdown_editor | rg "tokio|reqwest|reqwest_client"`

## Acceptance Criteria

- `![alt](./cat.png)` renders in rendered mode when the Markdown file has a
  document path and `cat.png` exists next to it.
- `![alt](/absolute/path/cat.png)` renders in rendered mode.
- `![alt](file:///absolute/path/cat.png)` renders in rendered mode.
- relative local images in unsaved documents show fallback rather than guessing a
  base directory.
- remote `http` and `https` images keep using GPUI URI loading.
- inline and block image measurement use the same resolved resource as paint.
- inline and block formulas render through RaTeX when assets are ready.
- pending image/formula assets do not poison final row layout caches.
- ready or invalid asset state invalidates only the affected row.

## Assumptions

- "ReX" means RaTeX for this work.
- RaTeX output is PNG-backed `RenderImage` in v1, not direct GPUI vector drawing.
- Local image files changing on disk during an editor session require reopening
  the document or a later explicit refresh feature.
- Broader remote image networking changes remain tracked in `UREQ_PLAN.md`.
