# 开发、测试与发布

## 环境

项目使用 Rust 2024 edition，最低 Rust 版本为 1.85。Linux amd64 静态构建需要 musl 工具链；Windows 构建需要 MSVC、Visual C++ Build Tools 和 Windows SDK。

## 本地检查

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

运行仓库中的 FastAPI mock：

```bash
python3 -m venv .venv
.venv/bin/python -m pip install -r mock/requirements.txt
.venv/bin/python -m uvicorn main:app --app-dir mock --host 127.0.0.1 --port 18080
```

另开终端运行标记为 `ignored` 的 HTTP 全场景测试：

```bash
POSTUI_MOCK_HOST=127.0.0.1:18080 \
POSTUI_E2E_LOG=target/postui-fastapi-e2e.log \
cargo test http::tests::fastapi_mock_covers_configured_requests -- \
  --ignored --exact --nocapture
```

mock 配置中的 token、Cookie 和接口地址都是本地测试值，不要替换成个人项目凭据后提交。

## Linux 打包

安装目标并构建发布包：

```bash
rustup target add x86_64-unknown-linux-musl
./package-linux.sh
```

脚本默认读取根目录的 `config.yaml` 和 `.postui/`，输出到 `打包区/postui-linux-amd64.tar.gz`。也可以指定：

```bash
./package-linux.sh \
  --config /path/to/config.yaml \
  --collection /path/to/.postui \
  --output-dir /path/to/output
```

发布包包含二进制、启动脚本、配置、请求集合、文档和可选的 `test_files/`、`themes/`；运行时 `requests.cache.json` 不会打包。

## Windows 打包

在 Windows PowerShell 5.1 或更高版本中运行：

```powershell
rustup target add x86_64-pc-windows-msvc
.\package-windows.ps1
```

脚本会优先使用 PATH 中的 `cargo`，并自动为 MSVC 构建启用静态 CRT。默认输出到 `打包区\postui-windows-amd64.zip`；`-ConfigPath`、`-CollectionPath` 和 `-OutputDir` 可以覆盖输入和输出位置。

## 安装脚本

Linux：

```bash
./install.sh --skip-init
```

Windows：

```powershell
.\install.ps1 -SkipInit
```

安装脚本默认保留已有的用户配置、请求文件和上传文件。需要用发布包中的配置覆盖同名文件时，Linux 使用 `--force-config`，Windows 使用 `-ForceConfig`；额外文件仍会保留。

## Debug 日志

Debug 构建可以使用：

```bash
cargo run -- --debug
cargo run -- --debug --log-file ./logs/postui-debug.log
```

日志文件默认位于全局配置所在目录的 `logs/postui-debug.log`。日志字段限制为 64 KiB，文件达到 8 MiB 后轮转。敏感请求头、查询参数、表单字段和 JSON 值会脱敏，剪贴板内容不会写入日志。

## 发布前检查

发布或提交前，确认根目录的个人配置没有进入变更：

```bash
git status --short
git diff --check
git check-ignore -v config.yaml .postui themes 打包区
```

发布脚本会读取本地项目配置，因此生成的 `打包区/` 可能包含个人请求集合；它只适合本地分发，不应直接作为公共源码提交。
