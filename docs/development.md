# 开发与构建

## 环境

项目使用 Rust 2024 edition，最低 Rust 版本为 1.85。Linux amd64 静态构建需要 musl 工具链；Windows 构建需要 MSVC、Visual C++ Build Tools 和 Windows SDK。

## 本地验证

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release
```

需要人工验证请求发送时，可以运行仓库中的 FastAPI 示例服务：

```bash
python3 -m venv .venv
.venv/bin/python -m pip install -r mock/requirements.txt
.venv/bin/python -m uvicorn main:app --app-dir mock --host 127.0.0.1 --port 18080
```

在 `mock/` 目录启动 `postui`，手工检查编辑、发送和响应展示。示例配置中的 token、Cookie 和接口地址均为虚构值，不要替换成个人项目凭据后提交。

## Linux 打包

安装目标并构建发布包：

```bash
rustup target add x86_64-unknown-linux-musl
./package-linux.sh
```

脚本构建二进制并输出到 `打包区/postui-linux-amd64.tar.gz`。项目配置和个人配置不进入软件包。可以指定输出目录：

```bash
./package-linux.sh \
  --output-dir /path/to/output
```

发布包包含二进制、启动脚本和文档。工作区由用户独立维护。

## Windows 打包

在 Windows PowerShell 5.1 或更高版本中运行：

```powershell
rustup target add x86_64-pc-windows-msvc
.\package-windows.ps1
```

脚本会优先使用 PATH 中的 `cargo`，并自动为 MSVC 构建启用静态 CRT。默认输出到 `打包区\postui-windows-amd64.zip`；`-OutputDir` 可以覆盖输出位置。

## 安装脚本

Linux：

```bash
./install.sh --skip-init
```

Windows：

```powershell
.\install.ps1 -SkipInit
```

安装脚本只安装程序，不复制或覆盖工作区和用户配置。

## Debug 日志

Debug 构建可以使用：

```bash
cargo run -- --debug
cargo run -- --debug --log-file ./logs/postui-debug.log
```

日志文件默认位于工作区的 `.postui/logs/postui-debug.log`。日志字段限制为 64 KiB，文件达到 8 MiB 后轮转。敏感请求头、查询参数、表单字段和 JSON 值会脱敏，剪贴板内容不会写入日志。

## 发布前检查

发布或提交前，确认根目录的个人配置没有进入变更：

```bash
git status --short
git diff --check
git check-ignore -v .postui 打包区
```

工作区可能包含内部地址或凭据，提交前应单独检查 `.postui/postui.yaml` 和请求文件。
