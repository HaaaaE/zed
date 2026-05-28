# Updraft Editor

A local Markdown editor built with GPUI.

Updraft Editor is not Zed and is not affiliated with Zed Industries. It uses
GPUI as a Cargo dependency and includes portions of Zed's editing infrastructure
adapted under the licenses described in `NOTICE`.

## License

This project is licensed primarily under GPL-3.0-or-later. See `LICENSE`.

Some components adapted from Zed remain under Apache-2.0. See
`LICENSE-APACHE`, per-crate license metadata, and `NOTICE` for details.

For installer or binary distributions, include `LICENSE`, `LICENSE-APACHE`,
`NOTICE`, and `THIRD_PARTY_LICENSES.html`. See `DISTRIBUTION.md` for the
release checklist and the command used to regenerate third-party notices.

## Development

Build the default application:

```sh
cargo check -p updraft_editor
```

Run the application:

```sh
cargo run -p updraft_editor
```

Run focused tests for the editor stack:

```sh
cargo test -p md_sum_tree
cargo test -p md_rope
cargo test -p md_text
```

## Attribution

Portions of this project are adapted from Zed, Copyright 2022-2025 Zed
Industries, Inc. See `NOTICE` for the component-level attribution.
