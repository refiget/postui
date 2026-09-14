#!/usr/bin/env sh
set -eu

project_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)
venv_dir="$project_dir/.venv"
python="$venv_dir/bin/python"

if [ ! -x "$python" ]; then
    python3 -m venv "$venv_dir"
fi

if ! "$python" -c 'import fastapi, multipart, uvicorn' 2>/dev/null; then
    "$python" -m pip install -r "$project_dir/example-api/requirements.txt"
fi

exec "$python" "$project_dir/example-api/start.py"
