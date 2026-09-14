# Architecture Boundaries

## Ownership

```text
App
├── WorkspaceSession
│   ├── active scenario and variables
│   ├── selected request
│   └── RequestSession
│       ├── source request
│       ├── session draft
│       └── runtime phase and response
├── ViewState
│   ├── request list state
│   ├── request preview state
│   ├── response state
│   └── focus, dialogs, prompts, and notices
├── RequestExecutor
├── ResponseActionExecutor
└── background reload, search, and highlight work
```

`App` applies state changes on the terminal thread. Background tasks return
messages. They do not mutate `App` directly.

## Request lifecycle

| Phase | Data |
| --- | --- |
| Idle | Optional feedback |
| Sending | Active operation ID |
| Received | `ResponseData` and `ResponseDocument` |
| Failed | Status, error detail, and feedback |

HTTP status determines success or HTTP failure after a response is received.
Transport and request-construction errors use the failed phase. Cancellation
returns the request to idle and invalidates the active operation ID.

## Drafts and scenarios

`RequestDraft` contains session-editable request fields. `RequestSession`
retains header row state per scenario. `WorkspaceSession` switches scenario
ownership, resets runtime results, and derives effective requests from workspace,
scenario, source, and draft values.

Baseline workspace and request values are retained for modified indicators and
restore operations. Restore operations affect in-process state. Request file
deletion is a separate confirmed disk operation.

## Focus and scrolling

The home screen focus model uses main containers. `Tab` and `Shift+Tab` move
between containers. `j` and `k` move within a container. `h` and `l` move among
fields or tabs where the focused container exposes a horizontal axis.

Selection and scroll offset are separate state. Moving a request selection does
not rewrite the scroll offset unless the user performs a scroll operation.
Keyboard, mouse wheel, scrollbar track, and scrollbar drag use the same bounded
scroll state.

## Response documents

`ResponseData` retains original bytes and response metadata.
`ResponseDocument` provides raw and formatted views within the display limit.
JSON and markup documents build sparse indexes. Other highlighted formats use a
paged cache. Search reads document content without constructing render styles.

Switching request, response tab, or document invalidates obsolete search and
highlight results. Copy and download use `ResponseData`, not formatted text.

## Reloading

Workspace reload scans and validates files in background work. Sending requests
and scenario switching are disabled while reload is active. A successful reload
replaces session edits. A failed reload retains the current workspace state.
