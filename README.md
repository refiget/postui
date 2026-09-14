# PostUI

PostUI 是一个终端界面的 HTTP 请求工具，支持标准及扩展 HTTP 方法，支持 Linux、Windows 和 macOS。

请求文件通过 `method: PUT`、`method: PATCH` 等声明方法，场景可单独覆盖；完整写法见 [HTTP 方法配置](docs/configuration.md#http-方法配置)。

界面以鼠标操作为主。

请求与场景由配置文件定义。界面中的请求调整仅在当前会话生效，不写回 YAML。按 `R` 重新加载配置，按 `?` 查看快捷键帮助。

## 界面预览

![PostUI 终端界面预览](assets/screenshot.png)

## 一键安装

从 GitHub Release 下载预编译程序，无需安装 Rust，也无需管理员权限。

### macOS / Linux

支持 macOS Intel、Apple Silicon 和 Linux amd64，需要 `curl`、`tar`，默认安装到 `~/.local/share/postui`（遵循 `XDG_DATA_HOME`）。

```sh
curl -fsSL https://raw.githubusercontent.com/refiget/postui/main/install.sh | sh
```

### Windows

支持 Windows amd64，在 PowerShell 5.1 或更新版本中执行，默认安装到 `%LOCALAPPDATA%\Programs\PostUI`：

```powershell
& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing https://raw.githubusercontent.com/refiget/postui/main/install.ps1).Content))
```

脚本会运行 `postui init` 配置命令路径；Linux/macOS 支持 bash、zsh。安装后重新打开终端，运行 `postui --version`。再次执行同一命令可更新到最新 Release。执行远程脚本前可先下载审阅；不要使用 `sudo`。

指定版本、目录或跳过 shell 初始化：

```sh
curl -fsSL https://raw.githubusercontent.com/refiget/postui/main/install.sh | sh -s -- --version 0.1.2 --prefix "$HOME/.local/share/postui" --skip-init
```

```powershell
& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing https://raw.githubusercontent.com/refiget/postui/main/install.ps1).Content)) -Version 0.1.2 -InstallDir "$env:LOCALAPPDATA\Programs\PostUI" -SkipInit
```

跳过初始化后使用安装目录中的 `postui`（Windows 为 `postui.exe`）启动，或自行加入 PATH。也可解压 Release 发布包后运行其中的 `install.sh` / `install.ps1`，直接安装本地二进制。

## 构建

### 本地构建

环境：

- 能构建当前源码和锁定依赖的 Rust stable（最低版本声明的限制见[开发指南](docs/development.md#环境)）
- Rust 2024 edition
- Linux amd64：`x86_64-unknown-linux-musl`
- Windows amd64：`x86_64-pc-windows-msvc`
- macOS Intel：`x86_64-apple-darwin`
- macOS Apple Silicon：`aarch64-apple-darwin`

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

Linux 发布包：

```bash
./package-linux.sh
```

Windows amd64 构建：

```powershell
rustup target add x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc
```

Windows 发布包：

```powershell
.\package-windows.ps1
```

macOS 发布包（在对应架构的 macOS 上运行）：

```bash
./package-macos.sh
```

也可以显式指定架构：

```bash
./package-macos.sh --target x86_64-apple-darwin
./package-macos.sh --target aarch64-apple-darwin
```

### GitHub 自动发布

仓库中的 GitHub Actions 会在推送 `v*` 版本标签后，自动构建 Linux amd64、Windows amd64、macOS Intel 和 macOS Apple Silicon，并创建 GitHub Release。发布新版本只需要推送标签：

```bash
git tag -a v0.1.2 -m "Release v0.1.2"
git push origin main v0.1.2
```

如需重发，在 Actions 中手动运行发布工作流并填写已有标签（例如 `v0.1.2`）；工作流会检出该标签并更新附件，不需要移动标签。

## 配置

最小工作区只需要一个请求文件：

```yaml
# .postui/requests/health.yaml
url: https://example.test/health
```

运行 `postui /path/to/project` 或 `postui /path/to/project/.postui`；在项目目录及子目录中直接运行 `postui` 会向上查找最近的工作区。未找到或无法访问时退出，不自动创建目录。

需要多个场景时，共享请求保持一份：

```text
.postui/
├── postui.yaml          # 可选：项目默认值
├── scenarios/           # 可选：场景差异
│   ├── dev.yaml
│   └── test.yaml
└── requests/
    └── health.yaml
```

```yaml
# .postui/postui.yaml
default_scenario: dev
headers:
  Accept: application/json
variables:
  token:
    value:
    secret: true

# 场景变量分别写在 .postui/scenarios/dev.yaml、test.yaml
# variables:
#   host: https://dev-api.example.test
```

```bash
postui /path/to/project --scenario test
```

个人界面偏好使用独立配置文件，示例见 [config.example.yaml](config.example.yaml)：

```yaml
language: en
theme: catppuccin-mocha
max_response_display_bytes: 16777216
```

默认从各平台标准配置目录读取个人偏好，具体路径见[个人偏好](docs/configuration.md#个人偏好)，也可使用 `--config <路径>`。显示上限不影响完整响应下载。

响应区支持点击或用左右键切换 Raw / Formatted / Headers。JSON 保留按视口格式化与即时高亮；XML、表单支持美化，其他常见代码格式使用 Syntect 后台分页高亮，HTML 等保留原文排版。复制与下载始终保留原始 Body。本地接口可放在 Git 忽略的 `example-api/` 目录中手工验证。

单击接口选择，双击接口直接发送；发送中的接口忽略双击。

常用快捷键：`/` 搜索请求（响应区聚焦时搜索响应体），`n`/`N` 跳转响应匹配项，`r` 发送或取消，`R` 重载配置，`w` 切换场景，`v` 打开变量，`o` 打开响应操作，`u` 恢复当前请求，`X` 恢复当前场景全部请求修改，`?` 查看当前区域的按键表。

快捷键按当前焦点或弹层生效，底栏同步显示当前操作。`F1` 随时打开当前区域的按键表；帮助内用 `↑/↓` 滚动，`Esc` 关闭。`Tab` / `Shift+Tab` 正反切换焦点；请求与响应区可用 `Alt+←/→` 切换页签，参数和请求头表格中的 `←/→` 切换列。

参数、请求头用 `a` 添加行、`Enter` 编辑、`Delete` / `d` 删除行，请求头用空格启停。表格删除仅影响会话；接口列表的 `Delete` 会先确认，再删除源文件。编辑和搜索输入内，`Enter` / `Tab` 应用，`Esc` / `Ctrl+C` 取消输入；其他界面的 `Ctrl+C` 请求退出，确认框内则取消确认。普通字母快捷键不接受额外的 Ctrl/Alt 修饰键。

Header 使用映射，重复值使用字符串列表。详细字段、场景合并和会话修改规则见[配置与请求文件](docs/configuration.md)，可运行示例见[公共 API 双场景示例](examples/public-api/README.md)。
