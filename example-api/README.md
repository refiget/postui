# Local API

These endpoints are for manually exercising PostUI. They are not an automated test suite or a production service. Bind the server to localhost only, and never upload personal files or enter real tokens.

From the repository root:

```bash
./example-api/start.sh
```

The command creates `.venv` and installs missing dependencies on first use. Running it again while this API is active exits successfully. If another process owns port `18080`, startup fails without stopping that process.

In another terminal, run `cargo run -- example-api --scenario local`. Endpoint details are available at `http://127.0.0.1:18080/docs` in a local browser. Use `--scenario debug` when the request override header is needed.

## Response bodies

The `responses/` group in the request list maps to `GET /v1/responses/{response_format}`:

| Parameter | Purpose |
| --- | --- |
| `json` | Compact JSON with nested data, booleans, null, a large integer, and a high-precision decimal |
| `invalid-json` | Invalid JSON fallback; Raw shows the original text, and the display scanner is not strict validation |
| `xml` | XML indentation and syntax highlighting |
| `mixed-xml` | Mixed text stays unchanged so formatting cannot alter its meaning |
| `invalid-xml` | Original text fallback for mismatched tags |
| `html` | HTML highlighting, a multiline comment, and preserved `pre` whitespace; scripts are not executed |
| `form` | Form decoding with duplicate keys, empty values, and JSON-escaped control characters |
| `yaml`, `javascript`, `css`, `markdown` | Source formatting and Syntect syntax highlighting |
| `plain` | Leading spaces and JSON-looking content preserved as plain text |
| `binary` | Binary summary in the response pane; the original bytes remain available for download |

After sending, click a tab or focus the response pane and press `←` / `→` to switch between Raw, Formatted, and Headers. Raw and Formatted both support `/` search plus `n` / `N` navigation. Switching tabs clears stale matches. Use `o` to copy or download the original body; formatting never changes the exported bytes.

JSON keeps viewport-based formatting and immediate highlighting, even beyond 1 MiB or 64 KiB. The default receive limit is 64 MiB and the default display limit is 16 MiB; both are configurable. Other formats use background, paged highlighting at roughly 64 KiB per page. Very long lines or complex syntax can fall back locally.

## Requests

The top-level request examples cover request headers, forms, file uploads, redirects, HTTP errors, empty responses, and timeouts. Prepare non-sensitive sample files locally for upload; do not commit uploaded files or downloaded artifacts.

`GET /v1/large-response` produces a marked large response with slow, streamed chunks:

| Parameter | Values | Behavior |
| --- | --- | --- |
| `format` | `json`, `plain`, `xml`, `html` | Selects the streamed response format |
| `count` | `1..100000` | Number of unique JSON records; ignored for plain text, XML, and HTML |
| `size_kb` | `1..65536` | Exact target size for plain text, XML, and HTML; ignored for JSON |
| `delay_ms` | `0..1000` | Delay between streamed chunks |

Large responses are explicitly marked with `X-PostUI-Large-Response: true` and `X-PostUI-Data-Profile: unique-records`. JSON records expose `record_id`, `marker`, and `sequence`; plain-text rows expose `record` and `marker`; XML and HTML elements carry unique IDs, markers, and sequence values. The generator varies every record instead of replaying one placeholder, while keeping complete markup elements and valid UTF-8. Plain text, XML, and HTML reach the requested byte size by adding only trailing whitespace outside records.

`responses/14-large-xml.yaml` and `15-large-html.yaml` request 64 MiB by default with a 120-second timeout. Change `size_kb` to `1024` (1 MiB), `16384` (16 MiB), or `65536` (64 MiB), then press `R` to reload. Set `delay_ms=50` to observe slow reception and press `r` to cancel. Restart the example API after changing its server code.

Manually check receive size, Raw / Formatted switching, zoom, scrolling, search, and cancellation. XML supports indentation; HTML keeps its source layout while highlighting. The default display limit remains 16 MiB, so receiving 64 MiB does not mean all of it is rendered. XML indentation can make the formatted view larger. Large JSON keeps its original indentation and immediate colors. `GET /v1/delay/{seconds}` is also available for cancellation checks.

Stop the server when finished. The checked-in `example-api/` fixtures use
localhost and synthetic data. Keep generated cache, logs, downloads, uploaded
files, and personal request data out of tracked paths.
