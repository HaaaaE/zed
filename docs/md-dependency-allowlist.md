# Markdown Editor — Dependency Allowlist

Non-GPUI Zed workspace crates that appear in `cargo tree -p markdown_editor`
must be documented here.  Every entry must include:

- **crate** — the workspace crate name
- **appears via** — direct dependency or transitive chain
- **reason** — why it cannot be replaced right now
- **owner** — which `md_*` crate will absorb or replace it
- **target stage** — when it should be cleaned up (R1–R6)

Scope: this allowlist tracks the current standalone `markdown_editor` product
path (`cargo tree -p markdown_editor --edges normal -q`).

The temporary `legacy-editor` feature path was removed in R5. Historical
removals from the migration path remain documented below for traceability.

---

## md_* Direct Dependencies (R6 scope)

The following non-`gpui*` workspace crates are **direct dependencies** of
`md_*` crates (not just GPUI transitive).  Product code in `md_editor` /
`markdown_editor` does not call them directly, but `md_text`, `md_rope`,
`md_sum_tree` depend on them because they were ported verbatim from Zed.

There are no remaining direct non-GPUI Zed workspace dependencies in
`markdown_editor` or `md_*` crates.

## GPUI Closure (transitive — not direct product use)

The following non-`gpui*` workspace crates appear in the current
`markdown_editor` tree as
transitive dependencies pulled in by `gpui` itself.  Product code does **not**
call them directly.  They are documented here per the R6 audit requirement.

| crate | appears via | reason | owner | target stage |
|-------|-------------|--------|-------|--------------|
| `collections` | `gpui`, `gpui_windows`, `util`, `perf`, `zlog` | GPUI framework/tooling collection aliases | GPUI closure | R6 / GPUI closure follow-up |
| `http_client` | `gpui` | GPUI asset loading | GPUI closure | R6 / GPUI closure follow-up |
| `scheduler` | `gpui` | GPUI task scheduler | GPUI closure | R6 / GPUI closure follow-up |
| `sum_tree` | `gpui` | GPUI internal tree data structure | GPUI closure | R6 / GPUI closure follow-up |
| `util` | `gpui` (via `http_client`), `gpui_platform` (via `gpui_windows`) | GPUI / platform helper utilities | GPUI closure | R6 / GPUI closure follow-up |
| `refineable` | `gpui` | GPUI derive macro support | GPUI closure | R6 / GPUI closure follow-up |
| `derive_refineable` | `gpui` (via `refineable`) | GPUI proc-macro helper | GPUI closure | R6 / GPUI closure follow-up |
| `util_macros` | `gpui` | GPUI proc-macro helper | GPUI closure | R6 / GPUI closure follow-up |
| `perf` | `gpui` (via `util_macros`) | GPUI proc-macro helper dependency | GPUI closure | R6 / GPUI closure follow-up |
| `ztracing` | `gpui` (via `sum_tree`) | GPUI tracing wrapper | GPUI closure | R6 / GPUI closure follow-up |
| `ztracing_macro` | `gpui` (via `sum_tree` -> `ztracing`) | GPUI tracing proc-macro helper | GPUI closure | R6 / GPUI closure follow-up |
| `zlog` | `gpui` (via `ztracing`) | GPUI tracing/logging backend | GPUI closure | R6 / GPUI closure follow-up |

---

## Cleared from standalone product path

The following crates appeared during migration but are **no longer present**
in the standalone `markdown_editor` product path after the R4 audit and
the R5 legacy-path deletion.

| crate | cleared by | date |
|-------|-----------|------|
| `editor` | `md_editor` replacement | 2026-05-23 |
| `language` | `md_buffer` replacement | 2026-05-23 |
| `multi_buffer` | `md_buffer` replacement | 2026-05-23 |
| `settings` | `md_settings` replacement | 2026-05-23 |
| `theme` | `md_theme` replacement | 2026-05-23 |
| `theme_settings` | `md_theme` replacement | 2026-05-23 |
| `ui` | inline GPUI in `md_editor_app.rs` | 2026-05-23 |
| `assets` | `md_assets` replacement | 2026-05-23 |
| `markdown` | `markdown_wysiwyg` replacement | 2026-05-23 |
| `clock` (legacy path) | `md_text` re-exports version types | 2026-05-23 |
| `collections` (legacy path) | `md_text` only uses via direct dep (R6) | 2026-05-23 |
| `http_client` (legacy path) | not needed in md-editor path | 2026-05-23 |
| `ztracing` | direct `tracing::instrument` in `md_sum_tree` / `md_rope` | 2026-05-24 |
| `zlog` | removed unused md_* test logger initialization | 2026-05-24 |
| `util` (`md_rope`) | local UTF-8/debug/test helpers in `md_rope` | 2026-05-24 |
| `util` (`md_text`) | local debug/test helpers and marked-text parsing in `md_text` | 2026-05-24 |
| `collections` (`md_text`) | direct `rustc_hash` aliases plus `std::collections::BTreeMap` | 2026-05-24 |
| `clock` (`md_text`) | local `md_text::clock` version-vector module | 2026-05-24 |
