# PostUI

PostUI 是一个配置驱动的终端接口测试工具，适用于需要快速发送固定接口请求的场景。

界面以鼠标操作为主，同时保留 Vim 风格快捷键作为辅助。

## 运行

### 发布目录

发布目录需要至少包含：

```text
postui                 # 启动脚本
postui.bin             # 二进制文件
config.yaml            # 全局配置
.postui/requests.yaml  # 请求集合
```

进入目录后运行：

```bash
./postui
```

从发布目录启动时，启动脚本会使用发布目录中的 `config.yaml`；从其他目录启动时，如果当前目录存在 `.postui/requests.yaml`，则优先使用当前项目的请求集合。`postui init` 会把当前程序所在目录加入当前用户 `~/.zshrc` 或 `~/.bashrc` 的 `PATH`，随后执行提示中的 `source` 命令即可使用 `postui`。

Windows amd64 发布目录包含：

```text
postui.exe             # 二进制文件
config.yaml            # 全局配置
.postui/requests.yaml  # 请求集合
```

在 PowerShell 中运行：

```powershell
.\postui.exe --config .\config.yaml --requests .\.postui\requests.yaml
```

安装后，程序会从 `%APPDATA%\postui\config.yaml` 读取用户配置；直接运行未安装的发布目录时请显式指定配置文件。`postui init` 只修改当前用户的 PATH（HKCU），不会写入系统 PATH，也不需要管理员权限；执行后重新打开 PowerShell 即可使用 `postui`。

### 安装

安装命令暂不在文档中展开，后续统一提供一键安装方式。

### 从源码运行

```bash
cargo run -- --config ./config.yaml
```

构建 Linux amd64 静态版本：

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
./target/x86_64-unknown-linux-musl/release/postui --config ./config.yaml
```

构建机需要安装 Rust 的 `rustfmt`、`clippy` 和 musl 工具链。

Windows 10 amd64 使用 MSVC 目标构建：

```powershell
rustup target add x86_64-pc-windows-msvc
$env:RUSTFLAGS = "-C target-feature=+crt-static"
cargo build --release --target x86_64-pc-windows-msvc
Copy-Item .\target\x86_64-pc-windows-msvc\release\postui.exe .\postui.exe
```

Windows 构建使用原生 MSVC 工具链；`+crt-static` 用于减少对 VC 运行库安装的依赖，但仍依赖 Windows 系统 DLL。项目只发布 Windows x86_64，不提供 32 位或 ARM 版本。

在 Windows 构建个人发布包（读取本地的 `config.yaml` 和 `.postui/requests.yaml`，不会把它们加入 Git）可以运行：

```powershell
.\package-windows.ps1
```

默认输出到 `打包区\postui-windows-amd64.zip`。也可以用 `-ConfigPath`、`-RequestsPath` 和 `-OutputDir` 指定输入及输出位置。

### 全局配置查找顺序

程序按以下顺序寻找全局配置：

1. `--config <文件>`。
2. Linux 的 `$HOME/postui.yaml`、`$HOME/.postui.yaml`；`HOME` 不是 `/root` 时也会检查 `/root` 下的同名文件。Windows 的 `%USERPROFILE%\postui.yaml`、`%USERPROFILE%\.postui.yaml`。
3. Linux 的 `${XDG_CONFIG_HOME:-$HOME/.config}/postui/config.yaml`；Windows 的 `%APPDATA%\postui\config.yaml`。

显式指定的文件读取失败会直接报错，不会继续查找其他位置。没有显式指定 `--config` 或 `--requests` 时，程序优先读取当前目录的 `.postui/requests.yaml`，再使用全局配置的 `request_config`；两者都没有时使用内置配置路径并按文件不存在处理。

请求集合也可以单独覆盖：

```bash
postui --config ./config.yaml --requests ./.postui/other.yaml
```

## 界面操作

「粘贴」优先使用系统原生剪贴板接口。Linux 如果不可用，会尝试 `wl-paste`、`xclip` 或 `xsel`；Windows 会尝试系统 PowerShell 的 `Get-Clipboard`。

## Debug 日志

`--debug` 只在 debug 构建中可用：

```bash
cargo run -- --debug --config ./config.yaml
cargo run -- --debug --log-file ./logs/postui-debug.log --config ./config.yaml
```

日志包含终端事件、界面操作、配置加载、请求构造、文件读取、请求头、响应头、响应体和错误上下文。单个日志字段最多记录 64 KiB；文件达到 8 MiB 后轮转为 `.1`。Authorization、Cookie、Token、Secret、Password、API key 等字段会隐藏，剪贴板文本不会写入日志。

默认路径是全局配置所在目录的 `logs/postui-debug.log`；没有全局配置时使用当前目录的 `logs/postui-debug.log`。release 二进制不包含 debug 日志写入器，使用 `--debug` 会报错。

## 配置文件

配置可以使用 YAML；文件扩展名为 `.json` 时使用 JSON 解析。当前版本只接受下面的格式，未知字段会在加载时报告错误。

### 全局配置

全局配置只决定请求集合和界面样式：

```yaml
request_config: .postui/requests.yaml
language: en
theme: gruvbox-dark

highlight:
  enabled: true
  syntax: base16-mocha.dark
  variable: "#d3869b"
```

`language` 可填写 `en` 或 `zh`，默认是 `en`。`request_config` 和 `theme_file` 的相对路径都以全局配置文件所在目录为基准。

`theme` 选择 PostUI 内置主题，默认是 `gruvbox-dark`。当前可用的内置主题有 `gruvbox-dark`、`ocean`、`nord` 和 `mono`。

需要自定义颜色时，用 `theme_file` 指向一个主题 YAML 或 JSON 文件：

```yaml
theme: gruvbox-dark
theme_file: themes/custom.yaml
```

主题文件中的字段位于顶层：

```yaml
name: custom-dark
primary: "#83a598"
secondary: "#fabd2f"
accent: "#8ec07c"
background: "#282828"
surface: "#3c3836"
text: "#ebdbb2"
muted: "#a89984"
error: "#fb4934"
success: "#b8bb26"
warning: "#fe8019"
selection: "#504945"
variable: "#d3869b"
syntax: base16-mocha.dark
```

### 请求集合

默认文件是当前工作目录下的 `.postui/requests.yaml`。首次成功解析后，会在同目录生成 `.postui/requests.cache.json`；配置文件内容变化时缓存自动失效并重新解析：

```yaml
name: 我的接口
file_directory: ../files
download_directory: tmp
timeout_seconds: 30

variables:
  - name: host
    default: https://api.example.com
  - name: token
  - name: user_id

requests:
  - id: user-list
    name: 用户列表
    description: 查询用户列表。
    request: |
      curl --location "{{host}}/users" \
        --header "Authorization: Bearer {{token}}"

  - id: user-detail
    name: 用户详情
    description: 查询指定用户。
    timeout_seconds: 10
    request: |
      curl --location "{{host}}/users/{{user_id}}"
```

字段说明：

- `name`：请求集合名称，可省略。
- `file_directory`：上传文件的根目录，默认是 `files`。相对路径以请求集合文件所在目录为基准。
- `download_directory`：下载文件的根目录，默认是 `tmp`。相对路径以请求集合文件所在目录为基准；默认请求集合位于 `.postui/requests.yaml` 时，文件保存到 `.postui/tmp/`。
- `timeout_seconds`：集合级请求超时时间，默认 30 秒；写成 0 也使用 30 秒。单个接口也可以设置同名字段覆盖集合级值。
- `variables`：变量声明列表。`default` 可省略，省略后初始为空。
- `requests`：接口列表。`id` 可省略，省略时按顺序生成；`name`、`description`、`request` 分别是名称、说明和 curl 文本；`timeout_seconds` 可覆盖集合级超时。

请求中出现的变量必须在 `variables` 中声明。变量既可以写成 `{{host}}`，也可以在 `extract` 的键中写成 `{{task_id}}`。

### curl 请求

`request` 是一段静态 curl 文本，不会执行 shell。可以直接写多行文本，也可以放在 `bash`、`sh` 或普通代码块中：

~~~yaml
request: |
  ```bash
  curl --request POST "{{host}}/upload" \
    --header "Authorization: Bearer {{token}}" \
    --form "file=@{{upload_file}};type=text/plain;filename={{upload_name}}" \
    --form-string "note={{note}}"
  ```
~~~

解析器会合并反斜杠换行和 PowerShell 反引号换行，并按静态命令参数拆分文本，但不会执行命令替换、管道、重定向或多个命令。PowerShell 中请使用 `curl.exe`，不要使用会被 PowerShell 解析为 `Invoke-WebRequest` 的 `curl` 别名：

~~~yaml
request: |
  ```powershell
  curl.exe --request GET "https://example.test/items/{{item_id}}" `
    --header "Accept: application/json"
  ```
~~~

支持的 curl 参数：

- `-X`、`--request`、`--url`。
- `-H`、`--header`。
- `-d`、`--data`、`--data-raw`、`--data-binary`、`--json`、`--data-urlencode`。
- `-F`、`--form`、`--form-string`。
- `-G`、`--get`，把 data 参数放进查询字符串。
- `-o`、`--output`，把响应保存为指定文件；相对路径位于 `download_directory` 下。
- `-O`、`--remote-name`、`-J`、`--remote-header-name`，按 URL 或 `Content-Disposition` 文件名保存响应。
- `-b`、`--cookie`、`-A`、`--user-agent`、`-e`、`--referer`，会转换成请求头。
- `--location`、`--compressed`、`--silent` 等不影响请求内容的选项会被忽略。

文件上传使用 `--form` 的 `@` 写法。路径相对 `file_directory`，也可以写绝对路径：

```yaml
request: |
  curl --request POST "{{host}}/files" \
    --form "file=@{{upload_file}};type=application/pdf"
```

`--data-urlencode` 会在变量展开后编码字段；合法 JSON 请求体会在「预览」和响应区使用 JSON 高亮。

带有输出参数的 curl 请求会按字节保存响应，不会把二进制内容当作文本显示。没有输出参数、但请求或响应表明内容是附件/二进制文件时，也会自动保存到默认目录。响应区会显示实际保存路径；重复发送同一个下载请求会覆盖同名文件。

### 返回字段提取

用 `extract` 声明响应字段和目标变量：

```yaml
requests:
  - name: 创建任务
    description: 创建任务并提取任务 ID。
    request: |
      curl --request POST "{{host}}/tasks" \
        --header "Content-Type: application/json" \
        --data-raw '{"name":"{{task_name}}"}'
    extract:
      task_id: data.taskId
      first_file: data.files[0].fileId
      status: /data/status
```

请求成功后，配置了 `extract` 的变量会在 Variables 区显示「提取」按钮。点击「提取」会按路径从当前接口最近一次成功响应的 JSON 中取值，并直接写入全局变量；「粘贴」从系统剪贴板写入变量，「清理」只清理本次运行的变量值。响应区不提供提取按钮。路径支持点号路径、数组下标和 JSON Pointer；响应必须是 JSON。

## 开发检查

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```
