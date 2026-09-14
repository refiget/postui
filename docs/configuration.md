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
parent search. Missing, unreadable, or invalid workspaces produce an error.
PostUI does not create a workspace automatically.

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
Runtime variable edits are not written to YAML.

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
headers. Empty `params`, `form`, `files`, or `extracts` lists clear those
request fields.

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
extracts:
  - variable: updated_id
    path: data.id
```

`url` is required. `method` defaults to `GET`. Standard and syntactically valid
extension methods are accepted. Supported fields are `name`, `description`,
`method`, `url`, `timeout`, `skip_ssl_verification`, `headers`, `params`,
`body`, `form`, `files`, and `extracts`.

Headers are mappings. A value is a string or a non-empty list of strings.
`params` and `form` preserve entry order and repeated names. Set
`has_equals: false` on a parameter to emit a key without `=`.

Multipart file entries use `field`, `path`, optional `filename`, and optional
`content_type`. Relative paths use `directories.uploads`.

`extracts` run for JSON responses with HTTP status below 400. Paths support dot
segments, array indexes, and JSON Pointer. Extracted values update runtime
variables.

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
- `.postui/cache/` stores derived parse data. Cache failures do not replace
  source parsing errors.
