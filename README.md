# PostUI

PostUI is a cross-platform terminal API client. Requests, variables, and scenarios are stored in YAML files.

## Installation

macOS and Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/refiget/postui/main/install.sh | bash
```

Windows PowerShell:

```powershell
& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing https://raw.githubusercontent.com/refiget/postui/main/install.ps1).Content))
```

Follow the installer output to configure `PATH`, reopen the terminal, and run `postui --version`.

Uninstall:

```bash
postui uninstall
```

The uninstall command preserves `.postui` workspaces and user configuration files.

## Quick Start

Create a request file in your project:

```text
.postui/
└── requests/
    └── health.yaml
```

```yaml
name: Health
method: GET
url: https://example.test/health
```

Start PostUI from the project directory:

```bash
postui
```

You can also specify a workspace and scenario:

```bash
postui /path/to/project
postui /path/to/project/.postui --scenario test
```

Without a path, PostUI searches upward from the current directory for the nearest `.postui` directory. A workspace can contain `postui.yaml`, `scenarios/*.yaml`, and `requests/*.yaml`. Reference variables as `{{name}}`. See the [configuration reference](docs/configuration.md) for available fields and merge rules.

## Controls

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Move between panels |
| `j` / `k` | Move the cursor or scroll content |
| `h` / `l` | Switch request or response tabs |
| `Enter` | Edit the selected field |
| `s` | Send or cancel a request |
| `/` | Search requests or response content |
| `n` / `N` | Move between response matches; create a request elsewhere |
| `r` | Reload the workspace |
| `c` | Select a scenario |
| `v` | Open variable configuration in an external editor |
| `e` | Open the request file or edit the active request draft |
| `m` | Open response actions |
| `f` | Switch between Raw and Formatted responses |
| `z` | Expand or restore the response panel |
| `u` / `U` | Reset request or scenario draft changes |
| `?` / `F1` | Show shortcuts for the active panel |

Press `a` to add a parameter or header and `d` or `Delete` to remove one. Press `Space` to enable or disable a header. Drag over response text with the mouse and release to copy the selection.

PostUI selects an external editor from `$VISUAL`, `$EDITOR`, or the platform default. Request bodies use temporary JSON files; parameters and headers use temporary YAML files. Valid content is applied to the active request draft when the editor closes.

## User Configuration

See [config.example.yaml](config.example.yaml) for available settings. The default location is `${XDG_CONFIG_HOME:-$HOME/.config}/postui/config.yaml` on Linux and the Roaming AppData directory on Windows. Use `--config <path>` to load another file.

## Development

PostUI uses Rust 2024 edition and requires Rust 1.85 or later.

```bash
cargo run -- example-api --scenario local
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build
```

The local API is in `example-api/`; start it with `./example-api/start.sh`. UI references are in [design-lab](design-lab/README.md). Use `package-linux.sh`, `package-macos.sh`, or `package-windows.ps1` for release packaging.
