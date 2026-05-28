# Updraft Editor Release Plan

This file tracks local release and installer preparation work. It is not a
replacement for `DISTRIBUTION.md`; that file records what must ship with binary
packages.

## Release Build

- Install a Windows SDK that includes `fxc.exe`.
- Locate `fxc.exe`:

```powershell
Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin" -Recurse -Filter fxc.exe
```

- Build the release binary with GPUI's shader compiler path set:

```powershell
$env:GPUI_FXC_PATH = "C:\Program Files (x86)\Windows Kits\10\bin\<version>\x64\fxc.exe"
cargo build -p updraft_editor --release --locked
```

- Confirm the binary exists:

```powershell
Test-Path target\release\updraft.exe
```

## Installer Preparation

- Choose the first Windows installer tool.
- Include these files in every installer or binary archive:
  - `target\release\updraft.exe`
  - `LICENSE`
  - `LICENSE-APACHE`
  - `NOTICE`
  - `THIRD_PARTY_LICENSES.html`
- Use the product name `Updraft Editor`.
- Use `updraft.exe` as the executable name.
- Use release artifact names like:

```text
updraft-editor-0.1.0-x86_64-pc-windows-msvc
```

## Application Metadata

- Add an application icon.
- Add Windows executable metadata:
  - product name: `Updraft Editor`
  - file description: `Updraft Editor`
  - file version: current release version
  - product version: current release version
  - copyright holder
- Add or verify a Windows application manifest.

## Release Checks

- Run:

```powershell
cargo check -p updraft_editor --locked
cargo test -p md_sum_tree --locked
cargo test -p md_rope --locked
cargo test -p md_text --locked
cargo about generate about.hbs --manifest-path crates/markdown_editor/Cargo.toml --target x86_64-pc-windows-msvc --frozen --fail -o THIRD_PARTY_LICENSES.html
```

- Clear the existing `md_editor` warnings or explicitly decide to ship with
  them.
- Review remaining git dependencies before release:
  - `async-task`
  - `tree-sitter-md`
- Generate a SHA256 checksum for each release artifact.
- Test install, launch, upgrade, and uninstall on a clean Windows environment.

## GPL Source Release

- Tag the exact release revision.
- Publish the installer or binary archive with corresponding source for the same
  tag.
- Make sure the release page links to the source archive and includes the
  installer checksum.
