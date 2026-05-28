# Distribution Checklist

This project is GPL-3.0-or-later overall and also carries Apache-2.0 notices for
some adapted components. Installers and binary packages should include these
files:

- `LICENSE`
- `LICENSE-APACHE`
- `NOTICE`
- `THIRD_PARTY_LICENSES.html`

Publish the corresponding source for the exact release revision alongside any
installer or binary package.

## Third-Party License Report

Install the generator once:

```sh
cargo install cargo-about --locked --features cli
```

Regenerate the Windows x64 installer notice from the locked dependency graph:

```sh
cargo about generate about.hbs --manifest-path crates/markdown_editor/Cargo.toml --target x86_64-pc-windows-msvc --frozen --fail -o THIRD_PARTY_LICENSES.html
```

For another installer target, replace `x86_64-pc-windows-msvc` with the target
triple used to build that package. Commit the regenerated report whenever
`Cargo.lock`, dependency features, or release targets change.
