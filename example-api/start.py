import json
import os
import shutil
import socket
import sys
import time
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import ProxyHandler, build_opener


HOST = "127.0.0.1"
PORT = 18080
HEALTH_URL = f"http://{HOST}:{PORT}/v1/health"
LOCK_DIR = Path(f"/tmp/postui-example-api-{PORT}.lock")
HTTP = build_opener(ProxyHandler({}))


def example_api_is_running() -> bool:
    try:
        with HTTP.open(HEALTH_URL, timeout=0.4) as response:
            body = json.load(response)
            return isinstance(body, dict) and body.get("service") == "postui-example-api"
    except (HTTPError, URLError, TimeoutError, json.JSONDecodeError):
        return False


def port_is_open() -> bool:
    with socket.socket() as connection:
        connection.settimeout(0.4)
        return connection.connect_ex((HOST, PORT)) == 0


def lock_owner_is_running() -> bool:
    try:
        pid = int((LOCK_DIR / "pid").read_text().strip())
        os.kill(pid, 0)
        return True
    except (FileNotFoundError, ValueError, ProcessLookupError):
        return False
    except PermissionError:
        return True


def acquire_lock() -> bool:
    for _ in range(30):
        try:
            LOCK_DIR.mkdir()
            (LOCK_DIR / "pid").write_text(str(os.getpid()))
            return True
        except FileExistsError:
            if example_api_is_running():
                print(f"example API already running: {HEALTH_URL}")
                return False
            if not lock_owner_is_running():
                shutil.rmtree(LOCK_DIR, ignore_errors=True)
                continue
            time.sleep(0.1)
    raise RuntimeError("example API startup is already in progress")


def main() -> int:
    if example_api_is_running():
        print(f"example API already running: {HEALTH_URL}")
        return 0
    if not acquire_lock():
        return 0

    try:
        if example_api_is_running():
            print(f"example API already running: {HEALTH_URL}")
            return 0
        if port_is_open():
            raise RuntimeError(f"port {PORT} is in use by another service")

        import uvicorn

        api_dir = Path(__file__).resolve().parent
        sys.path.insert(0, str(api_dir))
        print(f"example API listening: {HEALTH_URL}")
        uvicorn.run("main:app", host=HOST, port=PORT, app_dir=str(api_dir))
        return 0
    finally:
        shutil.rmtree(LOCK_DIR, ignore_errors=True)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as error:
        print(f"example API: {error}", file=sys.stderr)
        raise SystemExit(1) from error
