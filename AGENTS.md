# PostUI Development Constraints

## Implementation

- Implement from requirements, code structure, and observed runtime behavior.
- Do not use test-driven development.
- Do not add public interfaces, state, or abstractions solely for testability.
- Keep responsibilities and boundaries explicit.
- 所有文档的编写,前端提示,展示的撰写都只要说明当前状态,描述事实;禁止任何描述原因,背景

## Tests

- Do not add function-level automated tests, including Rust `#[test]`,
  `#[tokio::test]`, or `#[cfg(test)]` modules.
- Do not add test files, test directories, test runners, snapshots, property
  tests, mock frameworks, or test reports.
- Fix production behavior directly. Regression tests are not required.

## Verification

Run the minimum checks required by the change:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build
```

For interaction, request, upload, download, or cross-platform changes, exercise
one real flow with a temporary configuration. Remove logs, caches, screenshots,
temporary files, and personal configuration after verification.

## Repository boundaries

- Do not commit personal project URLs, requests, accounts, tokens, upload files,
  or workspace configuration.
- Do not commit `target/`, `.venv/`, `.postui/`, `打包区/`, logs, caches, or
  downloaded files.
- Inspect existing worktree changes before editing. Do not overwrite or remove
  unrelated user changes.
