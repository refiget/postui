"""Public FastAPI examples for manually exercising PostUI."""

import asyncio
import json
import random
import string
from typing import Any, AsyncIterator
from enum import Enum

from fastapi import FastAPI, File, Form, HTTPException, Query, Request, UploadFile
from fastapi.responses import (
    JSONResponse,
    PlainTextResponse,
    RedirectResponse,
    Response,
    StreamingResponse,
)


app = FastAPI(title="PostUI Mock", version="0.2.0")


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
        "service": "postui-fastapi-mock",
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
    task_id = payload.get("taskId") or "task-generated-by-mock"
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
            "error": {"code": "MOCK_VALIDATION", "message": "请求被 mock 拒绝"},
            "received": payload,
            "request": request_info(request),
        },
    )


@app.get("/v1/empty")
async def empty_response() -> Response:
    return Response(status_code=204)


@app.get("/v1/plain")
async def plain_response() -> PlainTextResponse:
    return PlainTextResponse("postui mock plain text\n")


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
    ResponseFormat.json: ("application/problem+json", '{"title":"响应展示","number":123456789012345678901234567890,"decimal":0.1234567890123456789,"items":[true,false,null,{"message":"hello\\nworld"}]}'),
    ResponseFormat.invalid_json: ("application/json", '{"message":"broken", "items":[1,2,}'),
    ResponseFormat.xml: ("application/xml", '<?xml version="1.0"?><response><message lang="zh">你好</message><items><item id="1"/><item id="2"/></items></response>'),
    ResponseFormat.mixed_xml: ("application/xml", '<message>Hello <strong>PostUI</strong> world!</message>'),
    ResponseFormat.invalid_xml: ("application/xml", '<response><item></response>'),
    ResponseFormat.html: ("text/html", '<!doctype html>\n<html>\n<head><title>PostUI</title></head>\n<body>\n<!-- multiline\ncomment -->\n<pre>  keep spacing\n    exactly</pre>\n<p>Hello <strong>PostUI</strong>!</p>\n</body>\n</html>\n'),
    ResponseFormat.form: ("application/x-www-form-urlencoded", 'name=PostUI&tag=rust&tag=tui&message=%E4%BD%A0%E5%A5%BD+world&empty=&line=one%0Atwo'),
    ResponseFormat.yaml: ("application/yaml", 'name: PostUI\nenabled: true\nitems:\n  - id: 1\n    message: "你好"\n'),
    ResponseFormat.javascript: ("text/javascript", '// PostUI response example\nconst message = "hello";\nexport function greet(name) {\n  return `${message}, ${name}`;\n}\n'),
    ResponseFormat.css: ("text/css", '/* PostUI response example */\nbody {\n  color: #336699;\n  margin: 12px;\n}\n'),
    ResponseFormat.markdown: ("text/markdown", '# PostUI\n\nA **highlighted** response with `inline code`.\n\n- Raw preserves text\n- Formatted improves readability\n'),
    ResponseFormat.plain: ("text/plain", '  Keep leading spaces.\n你好 PostUI\n{"this":"is explicitly plain text"}\n'),
    ResponseFormat.binary: ("application/octet-stream", b'\x00\xff\x89PostUI\r\n\x01\x02'),
}


@app.get("/v1/responses/{response_format}")
async def response_example(response_format: ResponseFormat) -> Response:
    media_type, body = RESPONSE_EXAMPLES[response_format]
    return Response(content=body, media_type=media_type)


async def paced_plain_response(size_bytes: int, delay_ms: int) -> AsyncIterator[bytes]:
    chunk = b"PostUI long response stability sample. 0123456789abcdef\n" * 64
    remaining = size_bytes
    while remaining:
        part = chunk[:remaining]
        yield part
        remaining -= len(part)
        if remaining and delay_ms:
            await asyncio.sleep(delay_ms / 1000)


async def paced_json_response(item_count: int, delay_ms: int) -> AsyncIterator[bytes]:
    chunk_size = 64 * 1024
    prefix = b'{"data":{"items":['
    suffix = b"]}}"
    characters = string.ascii_letters + string.digits
    random_source = random.Random()
    chunk = bytearray(prefix)
    for index in range(item_count):
        message_length = random_source.randint(20, 100)
        item = json.dumps(
            {
                "index": index,
                "message": "".join(
                    random_source.choices(characters, k=message_length)
                ),
                "tags": ["large", "scroll", "json"],
            },
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
        prefix = b'<?xml version="1.0" encoding="UTF-8"?>\n<items>\n'
        item = (
            '<item id="42" enabled="true"><name>中文 XML 示例</name>'
            '<message>Scroll &amp; search large responses</message>'
            '<tags><tag>large</tag><tag>xml</tag></tags></item>\n'
        ).encode("utf-8")
        suffix = b"</items>\n"
    else:
        prefix = (
            b'<!DOCTYPE html>\n<html lang="zh"><head><meta charset="utf-8">\n'
            b'<title>PostUI large HTML</title>\n'
            b'<style>article { color: #369; padding: 1em; }</style>\n'
            b'</head><body>\n'
        )
        item = (
            '<!-- Large response sample -->\n<article class="sample">\n'
            '<h2>中文 HTML 示例</h2>\n<p>Scroll &amp; search <strong>large</strong> responses.</p>\n'
            '<pre>  Keep spaces\n    and indentation.</pre>\n</article>\n'
        ).encode("utf-8")
        suffix = b"</body></html>\n"

    # Repeat complete elements; pad outside them to keep exact byte sizes and valid UTF-8.
    item_count, padding = divmod(size_bytes - len(prefix) - len(suffix), len(item))
    items_per_chunk = max(1, (64 * 1024) // len(item))
    chunk = item * items_per_chunk
    yield prefix
    while item_count:
        current_count = min(item_count, items_per_chunk)
        yield chunk[:current_count * len(item)]
        item_count -= current_count
        await asyncio.sleep(delay_ms / 1000)
    yield b" " * padding + suffix


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
    if response_format == "json":
        stream = paced_json_response(count, delay_ms)
        media_type = "application/json"
        response_headers = {"X-PostUI-Response-Items": str(count)}
    elif response_format == "plain":
        stream = paced_plain_response(size_bytes, delay_ms)
        media_type = "text/plain; charset=utf-8"
        response_headers = {"X-PostUI-Response-Bytes": str(size_bytes)}
    else:
        stream = paced_markup_response(response_format, size_bytes, delay_ms)
        media_type = "application/xml" if response_format == "xml" else "text/html"
        response_headers = {"X-PostUI-Response-Bytes": str(size_bytes)}
    response_headers["X-PostUI-Response-Format"] = response_format
    return StreamingResponse(
        stream,
        media_type=media_type,
        headers=response_headers,
    )


@app.get("/v1/delay/{seconds}")
async def delayed_response(seconds: float, request: Request) -> dict[str, Any]:
    await asyncio.sleep(seconds)
    return {"data": {"delayed": seconds}, "request": request_info(request)}
