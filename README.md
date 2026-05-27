# Markdown Editor

A single-document Markdown editor built with GPUI.

This project is not Zed and is not affiliated with Zed Industries. It uses GPUI
as a Cargo dependency and includes portions of Zed's editing infrastructure
adapted under the licenses described in `NOTICE`.

## License

This project is licensed primarily under GPL-3.0-or-later. See `LICENSE`.

Some components adapted from Zed remain under Apache-2.0. See
`LICENSE-APACHE`, per-crate license metadata, and `NOTICE` for details.

## Development

Build the default application:

```sh
cargo check -p markdown_editor
```

Run the application:

```sh
cargo run -p markdown_editor
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
