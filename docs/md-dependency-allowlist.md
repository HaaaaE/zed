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

These are R6 items — they are infrastructure crates, not IDE business crates.

| crate | direct user | reason | owner | target stage |
|-------|------------|--------|-------|--------------|
| `clock` | `md_text` | Buffer version types (`Global`, `Lamport`, `ReplicaId`) | `md_text` inline or standalone | R6 |
| `collections` | `md_text` | `HashSet`/`HashMap` used in text operations | stdlib or `md_collections` | R6 |

## GPUI Closure (transitive — not direct product use)

The following non-`gpui*` workspace crates appear in the current
`markdown_editor` tree as
transitive dependencies pulled in by `gpui` itself.  Product code does **not**
call them directly.  They are documented here per the R6 audit requirement.

| crate | pulled in by | status |
|-------|-------------|--------|
| `collections` | `gpui` | GPUI framework internal; R6 evaluation needed |
| `http_client` | `gpui` | GPUI asset loading; R6 evaluation needed |
| `scheduler` | `gpui` | GPUI task scheduler; R6 evaluation needed |
| `sum_tree` | `gpui` | GPUI internal; R6 evaluation needed |
| `media` | `gpui` | GPUI media backend; R6 evaluation needed |
| `util` | `gpui` (via http_client) | GPUI internal; R6 evaluation needed |
| `refineable` | `gpui` | GPUI derive macro; R6 evaluation needed |
| `ztracing` | `gpui` (via sum_tree) | GPUI internal; R6 evaluation needed |
| `zlog` | `gpui` (via ztracing) | GPUI internal; R6 evaluation needed |

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
