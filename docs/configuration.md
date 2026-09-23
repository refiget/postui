# Configuration Reference

PostUI reads four YAML document types.

| Scope | Path | Contents |
| --- | --- | --- |
| User settings | Platform configuration directory | Language, theme, response limits |
| Workspace defaults | `.postui/postui.yaml` | Variables, headers, timeout, directories |
| Scenario | `.postui/scenarios/<name>.yaml` | Scenario values and request overrides |
| Request | `.postui/requests/**/*.yaml` | HTTP request definitions |

Unknown fields are rejected.

## Workspace discovery

Without a path, PostUI searches the current directory and each parent for the
nearest `.postui` directory. An explicit project or `.postui` path disables the
parent search. Without a discovered workspace, PostUI displays the recent
workspace picker. Explicit paths that are missing, unreadable workspaces, and
invalid configuration produce an error. PostUI does not create a workspace
automatically.

The picker uses the language and theme from user settings, including `--config`.
Arrow keys select an entry, Enter opens it, `/` filters the list, `o` opens a
directory input, `d` removes a recent entry, and `q` exits. Directory input accepts
absolute paths, paths relative to the launch directory, and `~/` paths.

Successful workspace loads update `recent-workspaces.json` in the platform user
local data directory. On Linux this is
`${XDG_DATA_HOME:-$HOME/.local/share}/postui/recent-workspaces.json`.
The list contains up to 30 workspace names and absolute paths, most recent first.
Removing an entry leaves its workspace files intact. The picker does not create
request collections or send requests. Debug logging starts after workspace
selection; the default log remains `.postui/logs/postui-debug.log`.

## Workspace defaults

Every field in `.postui/postui.yaml` is optional.

```yaml
name: Example API
default_scenario: dev
timeout: 30
skip_ssl_verification: false
directories:
  uploads: test_files
  downloads: temp
variables:
  host: https://api.example.test
  token:
    value:
    secret: true
  order_id:
    value:
headers:
  Accept: application/json
```

| Field | Default |
| --- | --- |
| `name` | Project directory name |
| `default_scenario` | First scenario by name, or `default` |
| `timeout` | `30` seconds |
| `skip_ssl_verification` | `false` |
| `directories.uploads` | `test_files` |
| `directories.downloads` | `temp` |
| `variables` | Empty mapping |
| `headers` | Empty mapping |

Relative upload and download paths are resolved from the project root. Secret
variables are masked in the variable editor and excluded from debug values.
Variable values are read from YAML. Structured variables accept `value` and
`secret`.

## Scenarios

The scenario name is the filename without its extension.

```yaml
variables:
  host: https://dev-api.example.test
timeout: 60
headers:
  X-Debug: "true"
overrides:
  users/detail.yaml:
    method: PATCH
    url: "{{host}}/users/1"
```

Scenario documents accept `variables`, `headers`, `timeout`,
`skip_ssl_verification`, and `overrides`. Override keys are paths relative to
`.postui/requests/` and must identify existing request files.

Scenario selection order is `--scenario`, `default_scenario`, then the first
scenario by name. A workspace without scenario files exposes `default`.

Merge order:

| Value | Order |
| --- | --- |
| Variables | Workspace, then scenario by exact name |
| Headers | Workspace, scenario, request; names are case-insensitive |
| Timeout | Workspace, request, scenario, request override |
| TLS verification | Workspace, request, scenario, request override |
| Method | Request, then request override |

Lists are replaced as complete values. An empty request header mapping clears
request-level headers but does not clear inherited workspace or scenario
headers. Empty `params`, `form`, or `files` lists clear those request fields.

## Request files

Request files use `.yaml` or `.yml` extensions and may be stored in
subdirectories.

```yaml
name: Update user
description: Update one user
method: PATCH
url: "{{host}}/users/{{user_id}}"
timeout: 10
skip_ssl_verification: false
headers:
  Accept: application/json
  X-Trace-Tag: [one, two]
params:
  - name: include
    value: profile
body: |
  {"name": "Example"}
```

`url` is required. `method` defaults to `GET`. Standard and syntactically valid
extension methods are accepted. Supported fields are `name`, `description`,
`method`, `url`, `timeout`, `skip_ssl_verification`, `headers`, `params`,
`body`, `form`, and `files`.

Headers are mappings. A value is a string or a non-empty list of strings.
`params` and `form` preserve entry order and repeated names. Set
`has_equals: false` on a parameter to emit a key without `=`.

Multipart file entries use `field`, `path`, optional `filename`, and optional
`content_type`. Relative paths use `directories.uploads`.

## User settings

```yaml
language: en
theme: gruvbox-dark
max_response_display_bytes: 16777216
max_response_bytes: 67108864
```

`language` accepts `en` or `zh`. Built-in themes are `gruvbox-dark`, `dracula`,
`catppuccin-mocha`, `tokyo-night`, `nord`, `one-dark`, `solarized-dark`,
`kanagawa`, `rose-pine`, and `monokai`. Both byte limits must be positive.

The display limit applies to formatting and rendering. Copy and download use
the original response. The response limit terminates reads that exceed the
configured size.

## Runtime behavior

- Undefined and empty variables expand to empty strings.
- `R` reloads and validates the workspace in the background. A failed reload
  leaves the current state active.
- Request, parameter, header, and variable edits remain in the current process.
- Deleting a request from the request list requires confirmation and removes
  the request file.
- Workspace configuration remains in memory while PostUI is running.
