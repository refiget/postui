"""Local HTTP fixture for PostUI."""

import asyncio
import json
from enum import Enum
from typing import Any, AsyncIterator

from fastapi import FastAPI, File, Form, HTTPException, Query, Request, UploadFile
from fastapi.responses import (
    JSONResponse,
    PlainTextResponse,
    RedirectResponse,
    Response,
    StreamingResponse,
)


app = FastAPI(title="PostUI Local API", version="0.3.0")


def request_info(request: Request) -> dict[str, Any]:
    return {
        "method": request.method,
        "path": request.url.path,
        "query": dict(request.query_params),
        "headers": {
            "x-debug-token": request.headers.get("x-debug-token"),
            "cookie": request.headers.get("cookie"),
            "user-agent": request.headers.get("user-agent"),
            "referer": request.headers.get("referer"),
            "content-type": request.headers.get("content-type"),
        },
    }


@app.get("/v1/health")
async def health(request: Request) -> dict[str, Any]:
    return {
        "ok": True,
        "service": "postui-example-api",
        "data": {"ready": True},
        "request": request_info(request),
    }


@app.get("/v1/search")
async def search(request: Request) -> dict[str, Any]:
    return {
        "data": {
            "query": dict(request.query_params),
            "items": [
                {"id": "result-1", "name": "first"},
                {"id": "result-2", "name": "second"},
            ],
        },
        "request": request_info(request),
    }


@app.post("/v1/tasks")
async def create_task(request: Request) -> JSONResponse:
    payload = await request.json()
    task_id = payload.get("taskId") or "task-local-001"
    return JSONResponse(
        status_code=201,
        content={
            "data": {
                "taskId": task_id,
                "status": "queued",
                "itemId": "item-created-001",
                "payload": payload,
            },
            "request": request_info(request),
        },
    )


@app.get("/v1/tasks/{task_id}")
async def task_status(task_id: str, request: Request) -> dict[str, Any]:
    return {
        "data": {
            "taskId": task_id,
            "status": "processing",
            "items": [
                {"id": "file-001", "state": "done"},
                {"id": "file-002", "state": "waiting"},
            ],
        },
        "request": request_info(request),
    }


@app.api_route("/v1/items/{item_id}", methods=["PUT", "PATCH"])
async def update_item(item_id: str, request: Request) -> dict[str, Any]:
    return {
        "data": {
            "method": request.method,
            "itemId": item_id,
            "body": await request.json(),
        },
        "request": request_info(request),
    }


@app.delete("/v1/items/{item_id}")
async def delete_item(item_id: str, request: Request) -> dict[str, Any]:
    return {
        "data": {"itemId": item_id, "deleted": True},
        "request": request_info(request),
    }


@app.post("/v1/form")
async def echo_form(request: Request) -> dict[str, Any]:
    form = await request.form()
    return {
        "data": {"form": {key: str(value) for key, value in form.multi_items()}},
        "request": request_info(request),
    }


@app.post("/v1/upload")
async def upload_files(
    request: Request,
    file: UploadFile = File(...),
    second: UploadFile | None = File(default=None),
    note: str = Form(""),
    user: str = Form(""),
) -> dict[str, Any]:
    files = []
    for field, upload in [("file", file), ("second", second)]:
        if upload is None:
            continue
        content = await upload.read()
        files.append(
            {
                "field": field,
                "filename": upload.filename,
                "content_type": upload.content_type,
                "size": len(content),
                "preview": content[:120].decode("utf-8", errors="replace"),
            }
        )
    return {
        "data": {"files": files, "note": note, "user": user},
        "request": request_info(request),
    }


@app.get("/v1/headers")
async def echo_headers(request: Request) -> dict[str, Any]:
    return {
        "data": {
            "token": request.headers.get("x-debug-token"),
            "cookie": request.headers.get("cookie"),
            "user_agent": request.headers.get("user-agent"),
            "referer": request.headers.get("referer"),
        },
        "request": request_info(request),
    }


@app.get("/v1/redirect")
async def redirect() -> RedirectResponse:
    return RedirectResponse(url="/v1/health", status_code=307)


@app.post("/v1/error")
async def error_response(request: Request) -> JSONResponse:
    payload = await request.json()
    return JSONResponse(
        status_code=422,
        content={
            "error": {
                "code": "INVALID_EXAMPLE_REQUEST",
                "message": "The example request is invalid.",
            },
            "received": payload,
            "request": request_info(request),
        },
    )


@app.get("/v1/empty")
async def empty_response() -> Response:
    return Response(status_code=204)


@app.get("/v1/plain")
async def plain_response() -> PlainTextResponse:
    return PlainTextResponse("PostUI example API plain-text response\n")


class ResponseFormat(str, Enum):
    json = "json"
    invalid_json = "invalid-json"
    xml = "xml"
    mixed_xml = "mixed-xml"
    invalid_xml = "invalid-xml"
    html = "html"
    form = "form"
    yaml = "yaml"
    javascript = "javascript"
    css = "css"
    markdown = "markdown"
    plain = "plain"
    binary = "binary"


RESPONSE_EXAMPLES: dict[ResponseFormat, tuple[str, str | bytes]] = {
    ResponseFormat.json: ("application/problem+json", '{"title":"Response display","number":123456789012345678901234567890,"decimal":0.1234567890123456789,"items":[true,false,null,{"message":"hello\\nworld"}]}'),
    ResponseFormat.invalid_json: ("application/json", '{"message":"broken", "items":[1,2,}'),
    ResponseFormat.xml: ("application/xml", '<?xml version="1.0"?><response><message lang="en">Hello from XML</message><items><item id="1"/><item id="2"/></items></response>'),
    ResponseFormat.mixed_xml: ("application/xml", '<message>Hello <strong>PostUI</strong> world!</message>'),
    ResponseFormat.invalid_xml: ("application/xml", '<response><item></response>'),
    ResponseFormat.html: ("text/html", '<!doctype html>\n<html>\n<head><title>PostUI</title></head>\n<body>\n<!-- multiline\ncomment -->\n<pre>  keep spacing\n    exactly</pre>\n<p>Hello <strong>PostUI</strong>!</p>\n</body>\n</html>\n'),
    ResponseFormat.form: ("application/x-www-form-urlencoded", 'name=PostUI&tag=rust&tag=tui&message=hello+world&empty=&line=one%0Atwo'),
    ResponseFormat.yaml: ("application/yaml", 'name: PostUI\nenabled: true\nitems:\n  - id: 1\n    message: "Hello from YAML"\n'),
    ResponseFormat.javascript: ("text/javascript", '// PostUI response example\nconst message = "hello";\nexport function greet(name) {\n  return `${message}, ${name}`;\n}\n'),
    ResponseFormat.css: ("text/css", '/* PostUI response example */\nbody {\n  color: #336699;\n  margin: 12px;\n}\n'),
    ResponseFormat.markdown: ("text/markdown", '# PostUI\n\nA **highlighted** response with `inline code`.\n\n- Raw preserves text\n- Formatted improves readability\n'),
    ResponseFormat.plain: ("text/plain", '  Keep leading spaces.\nHello PostUI\n{"this":"is explicitly plain text"}\n'),
    ResponseFormat.binary: ("application/octet-stream", b'\x00\xff\x89PostUI\r\n\x01\x02'),
}


@app.get("/v1/responses/{response_format}")
async def response_example(response_format: ResponseFormat) -> Response:
    media_type, body = RESPONSE_EXAMPLES[response_format]
    return Response(content=body, media_type=media_type)


def large_record_profile(index: int) -> dict[str, str | int]:
    record_number = index + 1
    regions = ("north", "south", "east", "west", "central")
    operations = ("ingest", "index", "validate", "publish", "archive", "restore")
    states = ("queued", "running", "review", "complete", "held")
    region = regions[index % len(regions)]
    operation = operations[(index // len(regions)) % len(operations)]
    state = states[(index // (len(regions) * len(operations))) % len(states)]
    fingerprint = f"{record_number * 2_654_435_761 & 0xFFFFFFFF:08x}"
    return {
        "record_number": record_number,
        "ticket": f"EX-{record_number:06d}-{fingerprint}",
        "region": region,
        "operation": operation,
        "state": state,
        "duration_ms": 17 + (record_number * 37) % 9_983,
        "payload_bytes": 512 + (record_number * 811) % 130_560,
        "fingerprint": fingerprint,
    }


async def paced_plain_response(size_bytes: int, delay_ms: int) -> AsyncIterator[bytes]:
    chunk_size = 64 * 1024
    remaining = size_bytes
    record_index = 0
    chunk = bytearray()

    while remaining:
        profile = large_record_profile(record_index)
        record_number = profile["record_number"]
        line = (
            f"[LARGE-PLAIN record={record_number:06d} "
            f"marker=LARGE-PLAIN-{record_number:06d} "
            f"ticket={profile['ticket']}] "
            f"{profile['operation']} in {profile['region']} is {profile['state']}; "
            f"duration={profile['duration_ms']}ms payload={profile['payload_bytes']}B "
            f"fingerprint={profile['fingerprint']}\n"
        ).encode("ascii")

        if len(line) > remaining:
            chunk.extend(b" " * remaining)
            remaining = 0
        else:
            chunk.extend(line)
            remaining -= len(line)
            record_index += 1

        if len(chunk) >= chunk_size or not remaining:
            yield bytes(chunk)
            chunk.clear()
            if remaining and delay_ms:
                await asyncio.sleep(delay_ms / 1000)


def large_json_record(index: int) -> dict[str, Any]:
    profile = large_record_profile(index)
    record_number = profile["record_number"]
    return {
        "index": index,
        "record_id": f"large-json-{record_number:06d}",
        "marker": f"LARGE-JSON-{record_number:06d}",
        "sequence": record_number,
        "ticket": profile["ticket"],
        "operation": profile["operation"],
        "region": profile["region"],
        "state": profile["state"],
        "metrics": {
            "duration_ms": profile["duration_ms"],
            "payload_bytes": profile["payload_bytes"],
        },
        "message": (
            f"Ticket {profile['ticket']} ran {profile['operation']} in "
            f"{profile['region']} and finished as {profile['state']} after "
            f"{profile['duration_ms']} ms."
        ),
        "tags": [
            "large-response",
            str(profile["operation"]),
            str(profile["region"]),
            f"batch-{index // 1000 + 1:03d}",
        ],
    }


async def paced_json_response(item_count: int, delay_ms: int) -> AsyncIterator[bytes]:
    chunk_size = 64 * 1024
    prefix = b'{"data":{"items":['
    suffix = b"]}}"
    chunk = bytearray(prefix)
    for index in range(item_count):
        item = json.dumps(
            large_json_record(index),
            ensure_ascii=False,
            separators=(",", ":"),
        ).encode("utf-8")
        fragment = item if index == 0 else b"," + item
        chunk.extend(fragment)
        if len(chunk) >= chunk_size:
            yield bytes(chunk)
            chunk.clear()
            if delay_ms:
                await asyncio.sleep(delay_ms / 1000)
    chunk.extend(suffix)
    if chunk:
        yield bytes(chunk)


async def paced_markup_response(
    response_format: str, size_bytes: int, delay_ms: int
) -> AsyncIterator[bytes]:
    if response_format == "xml":
        prefix = (
            b'<?xml version="1.0" encoding="UTF-8"?>\n'
            b'<items dataset="large-xml" record-key="id" marker-key="marker">\n'
        )
        suffix = b"</items>\n"
    else:
        prefix = (
            b'<!DOCTYPE html>\n<html lang="en"><head><meta charset="utf-8">\n'
            b'<title>PostUI large HTML</title>\n'
            b'<style>article { color: #369; padding: 1em; }</style>\n'
            b'</head><body data-dataset="large-html" '
            b'data-record-key="data-record-marker">\n'
        )
        suffix = b"</body></html>\n"

    def markup_record(index: int) -> bytes:
        profile = large_record_profile(index)
        record_number = profile["record_number"]
        marker = f"LARGE-{response_format.upper()}-{record_number:06d}"
        if response_format == "xml":
            return (
                f'<item id="large-xml-{record_number:06d}" '
                f'marker="{marker}" sequence="{record_number}" '
                f'ticket="{profile["ticket"]}">'
                f"<operation>{profile['operation']}</operation>"
                f"<region>{profile['region']}</region>"
                f"<state>{profile['state']}</state>"
                f"<metrics duration-ms=\"{profile['duration_ms']}\" "
                f"payload-bytes=\"{profile['payload_bytes']}\"/>"
                f"<message>{profile['ticket']} completed {profile['operation']} in "
                f"{profile['region']} with state {profile['state']}.</message></item>\n"
            ).encode("ascii")
        return (
            f'<!-- ticket {profile["ticket"]} -->\n'
            f'<article class="sample {profile["state"]}" id="large-html-{record_number:06d}" '
            f'data-record-marker="{marker}" data-sequence="{record_number}">\n'
            f"<h2>{profile['ticket']}: {profile['operation']}</h2>\n"
            f"<p><strong>{profile['state']}</strong> in {profile['region']}; "
            f"duration {profile['duration_ms']} ms, payload {profile['payload_bytes']} bytes.</p>\n"
            f"<pre>record={record_number:06d}\n  fingerprint={profile['fingerprint']}\n"
            f"    marker={marker}</pre>\n"
            "</article>\n"
        ).encode("ascii")

    # Keep every element complete and pad only outside elements to hit the exact byte size.
    remaining = size_bytes - len(prefix) - len(suffix)
    chunk_size = 64 * 1024
    chunk = bytearray()
    record_index = 0
    yield prefix
    while remaining:
        item = markup_record(record_index)
        if len(item) > remaining:
            chunk.extend(b" " * remaining)
            remaining = 0
        else:
            chunk.extend(item)
            remaining -= len(item)
            record_index += 1

        if len(chunk) >= chunk_size or not remaining:
            yield bytes(chunk)
            chunk.clear()
            if remaining and delay_ms:
                await asyncio.sleep(delay_ms / 1000)
    yield suffix


@app.get("/v1/large-response")
async def large_response(
    response_format: str = Query("json", alias="format"),
    size_kb: int = Query(1024, ge=1, le=65536),
    count: int = Query(100000, ge=1, le=100000),
    delay_ms: int = Query(0, ge=0, le=1000),
) -> StreamingResponse:
    if response_format not in {"json", "plain", "xml", "html"}:
        raise HTTPException(
            status_code=422,
            detail="format must be 'json', 'plain', 'xml', or 'html'",
        )

    size_bytes = size_kb * 1024
    response_headers = {
        "X-PostUI-Large-Response": "true",
        "X-PostUI-Data-Profile": "unique-records",
        "X-PostUI-Response-Format": response_format,
    }
    if response_format == "json":
        stream = paced_json_response(count, delay_ms)
        media_type = "application/json"
        response_headers["X-PostUI-Response-Items"] = str(count)
        response_headers["X-PostUI-Record-Identity"] = "record_id,marker,sequence"
    elif response_format == "plain":
        stream = paced_plain_response(size_bytes, delay_ms)
        media_type = "text/plain; charset=utf-8"
        response_headers["X-PostUI-Response-Bytes"] = str(size_bytes)
        response_headers["X-PostUI-Record-Identity"] = "record,marker"
    else:
        stream = paced_markup_response(response_format, size_bytes, delay_ms)
        media_type = "application/xml" if response_format == "xml" else "text/html"
        response_headers["X-PostUI-Response-Bytes"] = str(size_bytes)
        response_headers["X-PostUI-Record-Identity"] = (
            "id,marker,sequence"
            if response_format == "xml"
            else "id,data-record-marker,data-sequence"
        )
    return StreamingResponse(
        stream,
        media_type=media_type,
        headers=response_headers,
    )


@app.get("/v1/delay/{seconds}")
async def delayed_response(seconds: float, request: Request) -> dict[str, Any]:
    await asyncio.sleep(seconds)
    return {"data": {"delayed": seconds}, "request": request_info(request)}
