# Repository Layout

```text
src/                 Rust source
design-lab/          Runnable UI studies
docs/                English technical documentation
example-api/         Local API service and request fixtures
test_files/          Upload directory with one tracked placeholder
config.example.yaml  User settings template
install.sh           Linux and macOS installer
install.ps1          Windows installer
package-linux.sh     Linux amd64 package script
package-windows.ps1  Windows amd64 package script
package-macos.sh     macOS package script
```

## Tracked content

Tracked examples use `example.test`, `127.0.0.1`, and template variables. The
`example-api/` contains the local service and request fixtures. The
`test_files/` directory tracks only `PLACE_UPLOAD_FILES_HERE.txt`.

## Ignored content

| Path | Contents |
| --- | --- |
| `.postui/` | Root local workspace and requests |
| `**/.postui/cache/` | Derived cache in any workspace |
| `config.yaml` | Local user settings |
| `example-api/test_files/` | Upload data for manual checks |
| `example-api/temp/` | Download output from manual checks |
| `example-api/test-bin/` | Local clipboard fallback commands |
| `test_files/*` | Upload data except the tracked placeholder |
| `themes/` | Local theme files |
| `target/` | Cargo output |
| `.venv/` | Python environment |
| `.codegraph/` | Local code index |
| `打包区/` | Local package output |
| Logs, caches, and downloads | Runtime output |

## Repository checks

```bash
git status --short
git diff --check
git check-ignore -v .postui example-api/.postui/cache config.yaml
```

Personal URLs, requests, accounts, tokens, upload files, personal workspace
files, logs, caches, screenshots, and downloaded responses remain untracked.
