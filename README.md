# PostUI

PostUI 是一个跨平台的接口测试工具。接口定义存放在配置文件中，TUI 提供请求编辑和执行操作。


## 特性

### 多主题

![多主题](assets/themes-switches.gif)

### 可编辑

![可编辑](assets/editable.gif)

### 高性能

![高性能](assets/perf.gif)

## 安装

### macOS 和 Linux

```bash
curl -fsSL https://raw.githubusercontent.com/refiget/postui/main/install.sh | bash
```

**注意**: 该操作不会 append `path` 到 `zsh` 或者 `bash`. 如有需要,留意安装后的提示手动 append 即可.

### Windows

```powershell
& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing https://raw.githubusercontent.com/refiget/postui/main/install.ps1).Content))
```

安装完成后重新打开 PowerShell，或执行脚本输出的 PATH 命令，再运行 `postui --version`。

## 卸载

### macOS 和 Linux

```bash
postui uninstall
```

该命令删除安装器管理的 PostUI 启动文件和程序，并从 `~/.bashrc`、`~/.zshrc`
删除 PostUI 初始化区块。工作区 `.postui` 和个人配置文件保留。

### Windows

在 PowerShell 中运行：

```powershell
postui uninstall
```

该命令从当前用户的 PATH 删除 PostUI 安装目录，并在当前进程退出后删除程序目录。
工作区 `.postui` 和个人配置文件保留。

## 构建

项目使用 Rust 2024 edition，`Cargo.toml` 声明 `rust-version = "1.85"`。

```bash
cargo build --release
```

平台发布包使用对应脚本构建：

```bash
./package-linux.sh
./package-macos.sh
./package-macos.sh --target x86_64-apple-darwin
./package-macos.sh --target aarch64-apple-darwin
```

```powershell
.\package-windows.ps1
```

## 配置

最小工作区只需要一个请求文件：

```text
.postui/
└── requests/
    └── health.yaml
```

```yaml
# .postui/requests/health.yaml
name: Health
method: GET
url: https://example.test/health
```

完整目录可以包含项目默认值和场景配置：

```text
.postui/
├── postui.yaml
├── scenarios/
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
```

```bash
postui /path/to/project
postui /path/to/project/.postui --scenario test
```

未指定路径时，PostUI 从当前目录向上查找最近的 `.postui`。未找到时显示最近工作区，可选择记录或输入目录打开。显式指定的路径无效或工作区无法访问时显示错误。

工作区选择页使用个人配置中的语言和主题。`↑` / `↓` 选择，`Enter` 打开，`/` 筛选，`o` 输入目录，`d` 移除最近记录，`q` 退出。目录支持绝对路径、相对路径和 `~/`。最近记录保存在用户应用数据目录的 `recent-workspaces.json`，最多 30 项；移除记录不删除工作区文件。

变量使用 `{{name}}`。Header 使用映射，重复值使用字符串列表。按 `r` 重新读取 YAML。请求列表中的删除操作需要确认，并会删除源文件。

完整字段和合并规则见[配置参考](docs/configuration.md)。

## 个人配置

个人界面配置与工作区配置分开存放，模板见 [config.example.yaml](config.example.yaml)。

```yaml
language: en
theme: gruvbox-dark
max_response_display_bytes: 16777216
max_response_bytes: 67108864
```

Linux 默认路径为 `${XDG_CONFIG_HOME:-$HOME/.config}/postui/config.yaml`，Windows 默认使用 Roaming AppData。也可以通过 `--config <路径>` 指定文件。

## 操作

响应区域提供 Raw、Formatted 和 Headers 页签。复制与下载使用原始响应内容。JSON 按视口格式化；XML 和表单数据提供格式化视图；其他支持的文本格式使用分页语法高亮。

鼠标单击选择请求，双击发送请求。发送中的请求忽略双击。

| 按键 | 操作 |
| --- | --- |
| `Tab` / `Shift+Tab` | 在主页容器之间切换焦点 |
| `j` / `k` | 在当前容器内移动 |
| `h` / `l` | 在当前容器的字段或页签间移动 |
| `Alt+←` / `Alt+→` | 切换请求或响应页签 |
| `/` | 搜索请求；响应区聚焦时搜索响应内容 |
| `n` / `N` | 响应区聚焦时跳转到下一处或上一处匹配；其他区域中 `n` 新建请求 |
| `s` | 发送或取消请求 |
| `r` | 重新加载工作区 |
| `c` | 选择场景 |
| `v` | 打开变量 |
| `m` | 打开响应操作 |
| `u` | 恢复当前请求的临时修改 |
| `U` | 恢复当前场景的全部临时修改 |
| `?` / `F1` | 打开当前上下文的快捷键表 |

参数和请求头使用 `a` 添加、`Enter` 编辑、`Delete` 或 `d` 删除。空格启停请求头。这些表格操作只影响当前会话。

侧栏显示请求方法、状态和筛选数量。点击筛选栏或按 `/` 输入筛选条件，`Enter` 确认；输入期间按 `Esc` 保留已确认的筛选。滚轮滚动鼠标所在区域，键盘焦点由点击或 `Tab` 切换。

## 本地示例

本地接口和请求示例位于 `example-api/`：

```bash
./example-api/start.sh
cargo run -- example-api --scenario local
```

界面素材和交互位于 [design-lab](design-lab/README.md)。

完整前端预览包含临时工作区和内置本地接口，退出后删除本次运行的数据：

```bash
cargo run --example frontend_preview
```
