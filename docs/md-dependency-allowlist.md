# Markdown Editor — Dependency Allowlist

Non-GPUI Zed workspace crates that appear in `cargo tree -p markdown_editor`
must be documented here.  Every entry must include:

- **crate** — the workspace crate name
- **appears via** — direct dependency or transitive chain
- **reason** — why it cannot be replaced right now
- **owner** — which `md_*` crate will absorb or replace it
- **target stage** — when it should be cleaned up (R1–R6)

---

## Active Entries (migration in progress)

| crate | appears via | reason | owner | target stage |
|-------|-------------|--------|-------|--------------|
| `editor` | `markdown_editor` (legacy-editor feature) | Main editing engine; migration in progress | `md_editor` | R3 |
| `language` | `markdown_editor` (legacy-editor feature) | `Buffer::local` used for single-file buffer | `md_buffer` | R2 |
| `multi_buffer` | `markdown_editor` (legacy-editor feature) | Snapshot API used for anchor/offset conversion | `md_buffer` | R2 |
| `settings` | `markdown_editor` (legacy-editor feature) | Keymap loading and theme initialization | `md_settings` | R4 |
| `theme` | `markdown_editor` (legacy-editor feature) | Color tokens used in highlight styles | `md_theme` | R4 |
| `theme_settings` | `markdown_editor` (legacy-editor feature) | `LoadThemes::All` initialization | `md_theme` | R4 |
| `ui` | `markdown_editor` (legacy-editor feature) | Button/layout components in title bar | `md_ui` or inline GPUI | R4 |
| `assets` | `markdown_editor` (legacy-editor feature) | Bundled fonts and default keymap | `md_assets` | R4 |
| `markdown` | `markdown_editor` (legacy-editor feature) | Preview panel rendering | drop or `markdown_wysiwyg` | R4 |
| `clock` | `markdown_editor` (legacy-editor feature) | Buffer version type (`Global`) | `md_text` | R1 |
| `collections` | `markdown_editor` (legacy-editor feature) | `HashSet` used in block cleanup | stdlib or inline | R3 |
| `http_client` | `markdown_editor` (legacy-editor feature) | HTTP trait for image loading | `md_assets` adapter | R4 |

## GPUI Closure (transitive — not direct product use)

The following non-`gpui*` workspace crates appear in the tree as transitive
dependencies pulled in by `gpui` itself.  Product code does **not** call them
directly.  They are documented here per the R6 audit requirement.

| crate | pulled in by | status |
|-------|-------------|--------|
| `collections` | `gpui` | GPUI framework internal; R6 evaluation needed |
| `http_client` | `gpui` | GPUI asset loading; R6 evaluation needed |
| `scheduler` | `gpui` | GPUI task scheduler; R6 evaluation needed |
| `sum_tree` | `gpui` (via collections) | GPUI internal; R6 evaluation needed |
| `media` | `gpui` | GPUI media backend; R6 evaluation needed |

---

## Cleared Entries

_(none yet — updated as stages R1–R5 complete)_
