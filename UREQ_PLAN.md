# Use ureq for Markdown Network Images

## Summary

Use a lightweight, non-Tokio HTTP client for standalone `markdown_editor` remote image loading. The implementation should use `ureq`, remove the accidental `ReqwestClient` wiring, and keep networking out of the `md_editor` library. It must handle documents with many images by queueing downloads with explicit concurrency, timeout, and response-size limits.

## Key Changes

- Remove the current wrong-direction `ReqwestClient` changes from `crates/markdown_editor`: no `reqwest_client` dependency and no `ReqwestClient::proxy_and_user_agent` startup wiring.
- Add an app-private `MarkdownHttpClient` in `crates/markdown_editor/src/lightweight_http_client.rs` that implements `http_client::HttpClient`.
- Add `ureq = { version = "3.3.0", default-features = false, features = ["rustls", "gzip", "platform-verifier"] }` to workspace dependencies, and use it only from `markdown_editor`.
- Configure a shared `ureq::Agent` with `markdown-editor/<CARGO_PKG_VERSION>` User-Agent, proxy environment behavior, and a 30 second request timeout.
- Use a bounded worker queue: 4 global download workers, at most 2 concurrent downloads per host, and a 20MB response body limit.
- Convert successful transport responses into `http_client::Response<AsyncBody::from_bytes(...)>`; preserve HTTP error statuses like 404/500 so GPUI's existing image loader can handle them.
- Support `AsyncBody::Empty` and `AsyncBody::Bytes` request bodies. Return a clear unsupported error for `AsyncBody::AsyncReader`.
- In `md_editor_app.rs`, call `configure_http_client(cx)` after `init_standalone(cx)` and before opening the window.

## Test Plan

- Unit test 200 responses from a local `TcpListener`, verifying status, headers, and body.
- Unit test 404 responses, verifying they are returned as HTTP responses rather than transport errors.
- Unit test the 20MB body limit.
- Unit test 100 queued requests and assert active downloads never exceed 4.
- Unit test same-host requests and assert active downloads never exceed 2 for that host.
- Unit test `AsyncBody::AsyncReader` returns unsupported.
- Verify dropped futures do not panic when workers later complete.
- Run `cargo fmt -p markdown_editor`.
- Run `cargo test -p markdown_editor`.
- Run `cargo check -p markdown_editor`.
- Run `cargo tree -p markdown_editor | rg "tokio|reqwest|reqwest_client"` and confirm there is no Tokio/Reqwest path in `markdown_editor`.

## Assumptions

- This client is only for standalone `markdown_editor` remote image loading, not a general-purpose HTTP implementation for the whole repo.
- 1000 images should load in controlled batches, not through 1000 simultaneous requests.
- The default constraints are 4 global downloads, 2 downloads per host, 30 seconds per request, and 20MB per response.
- `md_editor`, GPUI `ImageAssetLoader`, inline atom layout, and row layout caching behavior should not change.
