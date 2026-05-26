# Local Image Support Plan

## Goal

Add local image rendering support to the standalone markdown editor path without changing the existing source-row virtualization model.

The first target is a minimal usable implementation:

- inline local images can render in `Rendered` mode
- block local images can render in `Rendered` mode
- local images can participate in measurement and layout, not only fallback text
- failures still fall back cleanly to the current placeholder behavior

## Current State

Today, `md_editor` effectively assumes image URLs are remote strings.

Observed constraints in the current implementation:

- block-image detection only treats remote images as block candidates
- image measurement paths build `Resource::Uri(...)`
- the editor stores image references as `String`, without classifying them into remote URI vs local file path vs embedded asset

GPUI itself already supports path-backed images, but `md_editor` does not yet route markdown image references into those code paths.

## Non-Goals

This plan does not aim to solve every image-path problem up front.

Out of scope for the first pass:

- generalized document workspace resolution rules beyond the editor's immediate needs
- every possible URI scheme
- cross-platform path normalization edge cases beyond practical local file support
- broader markdown asset pipeline redesign
- changing virtualization from source-row to visual-row

## First-Pass Scope

Support these inputs in `Rendered` mode:

- absolute local file paths
- document-relative local paths when the editor has a document path to resolve against
- existing remote `http` / `https` image URLs

Keep current behavior for unsupported or invalid paths:

- preserve fallback rendering
- avoid panics
- avoid silently caching invalid measurements as final geometry

## Design Direction

### 1. Classify image references explicitly

Introduce a small internal image source classification layer for `md_editor`.

Likely categories:

- remote URI
- local file path
- embedded asset path, if we intentionally choose to keep that distinction

This should replace the current assumption that every image reference should be measured through `Resource::Uri`.

### 2. Reuse GPUI image source capabilities

Inline and block rendering paths should use GPUI's existing support for file-backed image sources instead of forcing everything through URI loading.

Measurement and rendering should agree on the same classified source so layout and paint do not drift.

### 3. Extend block-image detection

Current block-image treatment is remote-only. That should be widened so standalone local images can use the same block layout path as remote images when they occupy a whole source row.

### 4. Resolve relative paths from document context

If the editor has a known document path, relative markdown image paths should resolve against that file's parent directory.

If there is no document path available, relative local paths should fall back safely rather than guessing.

### 5. Preserve cache correctness

Loaded local image dimensions may be cacheable.

Missing-file or not-yet-available fallback layouts should remain non-cacheable, matching the current remote-image placeholder policy.

## Implementation Steps

1. Add an internal image reference classifier in `md_editor`.
2. Thread the classified source through inline-image measurement and rendering.
3. Thread the classified source through block-image measurement and rendering.
4. Extend block-image detection so local standalone images can become block layouts.
5. Add document-relative path resolution where document path context exists.
6. Keep invalid or unresolved local paths on the current fallback path.
7. Add tests for inline, block, measurement, and failure behavior.

## Testing Plan

Add or extend tests for:

- rendered inline local image fragment creation
- rendered block local image detection
- inline local image measurement cacheability
- block local image measurement cacheability
- absolute local path rendering
- relative local path rendering when document path is known
- fallback behavior for missing files
- no regression for existing remote image behavior

If feasible, also add action-level interaction coverage for:

- keyboard movement across local image blocks
- selection extension across local image blocks

## Risks

### Path resolution ambiguity

Relative paths need a clear base directory. If the editor does not always know the document path, behavior must stay explicit and conservative.

### Windows path parsing

Windows drive-letter paths can be confused with URI-like strings if classification is too naive.

### Cache poisoning

Fallback local-image measurements must not be cached as if they were final loaded dimensions.

### Behavior drift between measure and render

If measurement and rendering choose different image source interpretations, visual layout bugs will follow.

## Recommended First Milestone

Ship the smallest end-to-end version with:

- absolute local file support
- document-relative local file support when document path exists
- inline and block rendering support
- fallback on missing files
- tests proving local and remote paths both still work

After that, reevaluate whether broader asset semantics are worth expanding.
