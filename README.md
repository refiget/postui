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
.postui/config.yaml    # 请求集合设置
.postui/requests/      # 每个请求一个 .http 文件
```

进入目录后运行：

```bash
./postui
```

从发布目录启动时，启动脚本会使用发布目录中的 `config.yaml`；从其他目录启动时，如果当前目录存在 `.postui/` 集合目录，则优先使用当前项目的请求集合。`postui init` 会把当前程序所在目录加入当前用户 `~/.zshrc` 或 `~/.bashrc` 的 `PATH`，随后执行提示中的 `source` 命令即可使用 `postui`。

Windows amd64 发布目录包含：

```text
postui.exe             # 二进制文件
config.yaml            # 全局配置
.postui/config.yaml    # 请求集合设置
.postui/requests/      # 每个请求一个 .http 文件
```

在 PowerShell 中运行：

```powershell
.\postui.exe --config .\config.yaml --requests .\.postui
```

安装后，程序会从 `%APPDATA%\postui\config.yaml` 读取用户配置；直接运行未安装的发布目录时请显式指定配置文件。`postui init` 只修改当前用户的 PATH（HKCU），不会写入系统 PATH，也不需要管理员权限；执行后重新打开 PowerShell 即可使用 `postui`。

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

在 Windows 构建个人发布包（读取本地的 `config.yaml` 和 `.postui/`，不会把它们加入 Git）可以运行：

```powershell
.\package-windows.ps1
```

默认输出到 `打包区\postui-windows-amd64.zip`。也可以用 `-ConfigPath`、`-CollectionPath` 和 `-OutputDir` 指定输入及输出位置。

### 全局配置查找顺序

程序按以下顺序寻找全局配置：

1. `--config <文件>`。
2. Linux 的 `$HOME/postui.yaml`、`$HOME/.postui.yaml`；`HOME` 不是 `/root` 时也会检查 `/root` 下的同名文件。Windows 的 `%USERPROFILE%\postui.yaml`、`%USERPROFILE%\.postui.yaml`。
3. Linux 的 `${XDG_CONFIG_HOME:-$HOME/.config}/postui/config.yaml`；Windows 的 `%APPDATA%\postui\config.yaml`。

显式指定的文件读取失败会直接报错，不会继续查找其他位置。没有显式指定 `--config` 或 `--requests` 时，程序从当前目录向父目录查找 `.postui/` 集合目录，再使用全局配置的 `request_config`；两者都没有时使用内置配置路径并按目录不存在处理。这样从项目的 `.postui/requests/` 子目录启动也能找到项目集合。

请求集合也可以单独覆盖：

```bash
postui --config ./config.yaml --requests ./.postui/other-collection
```

## 界面操作

主界面左侧的 `Requests/` 列表用于选择接口，集合名称和 `Variables` 入口位于列表上方。右侧分为两个固定容器：上方是 `Preview`，下方是 `Response`。每次切换接口都会立即进入 `Overview`，完整展示解析后的 URL、Headers、Body、Form、Files 和下载设置；内容较长时可用 `j/k`、上下键或鼠标滚轮查看。`Params`、`Headers`、`Body` 是可编辑子页：按 `Enter`、`p`、`h`、`b` 或点击编辑按钮后直接在当前面板中修改，`a` 新增一行、`d` 删除一行、`Esc` 放弃修改，完成后点击 `Apply`。`Send` 是独立的主操作，存在未应用修改时会禁用。`Response` 显示状态码、耗时和响应体，JSON 会使用语法高亮，内容过长时使用终端滚动条。

目前发送范围限定为 `GET` 和 `POST`。其他方法仍可在集合中预览，但发送按钮会禁用。键盘可以使用 `Tab` 在接口列表、变量、预览和发送操作之间切换；预览聚焦时用 `←→` 切换标签，`v` 打开 Variables，`h` 编辑 Headers，`r` 发送，`q` 退出。变量以及 Params、Headers、Body 的修改只在本次运行生效，不会回写配置文件。

## Debug 日志

`--debug` 只在 debug 构建中可用：

```bash
cargo run -- --debug --config ./config.yaml
cargo run -- --debug --log-file ./logs/postui-debug.log --config ./config.yaml
```

日志包含终端事件、界面操作、配置加载、请求构造、文件读取、请求头、响应头、响应体和错误上下文。单个日志字段最多记录 64 KiB；文件达到 8 MiB 后轮转为 `.1`。Authorization、Cookie、Token、Secret、Password、API key 等字段会隐藏，剪贴板文本不会写入日志。

默认路径是全局配置所在目录的 `logs/postui-debug.log`；没有全局配置时使用当前目录的 `logs/postui-debug.log`。release 二进制不包含 debug 日志写入器，使用 `--debug` 会报错。

## 配置文件

全局配置和请求集合配置使用 YAML；主题文件支持 YAML 或 JSON。未知字段会在加载时报告错误。

### 全局配置

全局配置只决定请求集合和界面样式：

```yaml
request_config: .postui
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

请求集合是一个目录，默认是当前工作目录下的 `.postui/`。首次成功解析后，会在该目录生成 `requests.cache.json`；集合配置或任意请求文件内容变化时缓存自动失效并重新解析：

```text
.postui/
├── config.yaml
├── requests.cache.json
└── requests/
    ├── 01-user-list.http
    └── 02-user-detail.http
```

`config.yaml` 只保存集合级设置，接口本身直接写在请求文件中：

```yaml
name: 我的接口
file_directory: ../files
download_directory: tmp
timeout_seconds: 30

headers:
  Accept: application/json
  X-Environment: "{{environment}}"

variables:
  host: https://api.example.com
  token:
  user_id:
```

字段说明：

- `name`：请求集合名称，可省略。
- `file_directory`：上传文件的根目录，默认是 `files`。相对路径以 `config.yaml` 所在目录为基准。
- `download_directory`：下载文件的根目录，默认是 `tmp`。相对路径以 `config.yaml` 所在目录为基准；默认集合位于 `.postui/` 时，文件保存到 `.postui/tmp/`。
- `timeout_seconds`：集合级请求超时时间，默认 30 秒；写成 0 也使用 30 秒。
- `headers`：集合级默认请求头，所有接口继承；请求文件中同名 Header 会覆盖集合默认值。
- `variables`：变量默认值映射。值省略或写成 `null` 表示没有默认值。

请求中出现但未在 `variables` 中声明的变量仍会被识别。程序使用配置中的默认值替换变量，不会回写配置文件。集合 Header 中出现的变量也会自动加入变量列表。

### 请求文件

每个 `.http`、`.rest` 或 `.curl` 文件就是一个接口，文件内容直接是静态 curl 文本，不再套一层 `requests` 或 `request` 字段。文件中的注释指令可以提供名称、说明、超时和返回字段提取：

~~~text
# @name 用户详情
# @description 查询指定用户。
# @timeout 10
# @extract task_id = data.taskId
curl --location "{{host}}/users/{{user_id}}"
~~~

没有 `@name` 时使用文件名；文件名开头的数字序号和连接符会被去掉，例如 `02-user-detail.http` 显示为 `user-detail`。相对路径会作为请求的稳定 id，因此可以用子目录组织请求。`@extract` 可以写多行，响应路径支持点号路径、数组下标和 JSON Pointer。

解析器会合并反斜杠换行和 PowerShell 反引号换行，并按静态命令参数拆分文本，但不会执行命令替换、管道、重定向或多个命令。PowerShell 中请使用 `curl.exe`，不要使用会被 PowerShell 解析为 `Invoke-WebRequest` 的 `curl` 别名：

~~~powershell
curl.exe --request GET "https://example.test/items/{{item_id}}" `
  --header "Accept: application/json"
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

~~~text
curl --request POST "{{host}}/files" \
  --form "file=@{{upload_file}};type=application/pdf"
~~~

`--data-urlencode` 会在变量展开后编码字段；合法 JSON 请求体会在响应区使用 JSON 高亮。

带有输出参数的 curl 请求会按字节保存响应，不会把二进制内容当作文本显示。没有输出参数、但请求或响应表明内容是附件/二进制文件时，也会自动保存到默认目录。响应区会显示实际保存路径；重复发送同一个下载请求会覆盖同名文件。

### 返回字段提取

`@extract` 语法保留在请求文件格式中。Variables 窗口显示每个变量的当前值和默认值；Preview 的 Headers 标签显示集合继承的 Header 和当前请求 Header，编辑窗口可以临时修改、启用或停用请求级 Header。

缓存文件是 `.postui/requests.cache.json`，指纹同时覆盖集合 `config.yaml` 和 `requests/` 下的全部请求文件。新增、修改或删除请求后，下一次启动会重新解析。

## 开发检查

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```
