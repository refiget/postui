"""FastAPI endpoints used by the PostUI configuration and HTTP smoke tests."""

import asyncio
from typing import Any

from fastapi import FastAPI, File, Form, Request, UploadFile
from fastapi.responses import JSONResponse, PlainTextResponse, RedirectResponse, Response


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


@app.get("/v1/delay/{seconds}")
async def delayed_response(seconds: float, request: Request) -> dict[str, Any]:
    await asyncio.sleep(seconds)
    return {"data": {"delayed": seconds}, "request": request_info(request)}
