# Development Reference

## Targets

| Target | Entry point | Responsibility |
| --- | --- | --- |
| `postui_core` | `src/lib.rs` | Configuration, HTTP execution, application state, terminal events, rendering |
| `postui` | `src/main.rs`, `src/launch.rs` | CLI and workspace startup |
| `frontend_preview` | `design-lab/frontend_preview.rs`, `design-lab/preview/` | Shared TUI, temporary workspace, local example API |

```bash
cargo check --lib
cargo check --bin postui
```

The complete TUI preview runs with a temporary workspace and an embedded local API:

```bash
cargo run --example frontend_preview
```

## Source map

| Area | Files |
| --- | --- |
| YAML loading and validation | `src/config.rs`, `src/config/` |
| Template expansion and HTTP | `src/template.rs`, `src/http.rs` |
| Application state | `src/app.rs`, `src/app/` |
| Focus and input commands | `src/shortcuts.rs`, `src/app/input.rs`, `src/ui/focus.rs` |
| Rendering | `src/ui.rs`, `src/ui/` |
| Response formatting and indexing | `src/response_document.rs`, `src/response_document/` |
| User settings and themes | `src/settings.rs` |
| Localized interface text | `src/i18n.rs` |

## State rules

- `WorkspaceSession` owns request sources, drafts, runtime results, and the
  selected request.
- `ViewState` owns focus, dialogs, editors, tabs, search state, and scroll state.
- `RequestRuntimeState` has idle, sending, received, and failed phases.
- A sending phase contains an operation ID. Completion is applied only when the
  request ID and operation ID still match.
- Scenario switching commits drafts to in-process scenario state. It does not
  write YAML.
- Keyboard and mouse handlers dispatch the same application operations.

## Response processing

The default display limit is 16 MiB. The default receive limit is 64 MiB. JSON
uses a sparse byte index and viewport formatting. XML and HTML use sparse markup
indexes. YAML, JavaScript, CSS, and Markdown use paged Syntect highlighting.
Raw views retain source byte order. Copy and download operate on original bytes.

Network execution uses one Tokio runtime and connection pool. At most eight
requests execute concurrently. Cancellation aborts the async request task.
Completed work with an obsolete operation ID is discarded.

Response extraction uses one function for the completion path and for the
response menu `Extract` action. Both read the effective request `extracts` and
write runtime variables. The effective request applies the session extraction
order, so extraction follows the order shown on the extraction order page.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build
```

Interaction, request, upload, download, and cross-platform changes require one
manual flow with temporary configuration. Temporary logs, caches, downloads,
screenshots, and configuration files are removed after the check.

## Local API

The versioned `example-api/` directory contains the local service and request
fixtures. Its generated cache, logs, Python bytecode, and upload test files
remain ignored.

```bash
./example-api/start.sh
cargo run -- example-api --scenario local
```

## Diagnostics

Debug logging is available in debug builds:

```bash
cargo run -- --debug
cargo run -- --debug --log-file ./logs/postui-debug.log
cargo run -- /path/to/workspace --perf --log-file /tmp/postui-perf.log
```

`--perf` records performance events without request URLs, headers, bodies,
variables, search text, or key characters. Log files rotate at 8 MiB. Logging
uses a bounded asynchronous queue and may omit trailing records on shutdown.

## Packaging

```bash
rustup target add x86_64-unknown-linux-musl
./package-linux.sh

./package-macos.sh --target x86_64-apple-darwin
./package-macos.sh --target aarch64-apple-darwin
```

```powershell
rustup target add x86_64-pc-windows-msvc
.\package-windows.ps1
```
