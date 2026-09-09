# PostUI

PostUI 是一个只支持 Linux amd64（x86_64）的终端 HTTP 请求工具。它从 YAML 或 JSON 读取接口定义，在界面中选择请求、填写变量、查看最终请求，并发送到目标服务。应用配置与请求集合分开：全局配置负责主题和高亮，请求配置负责接口、变量和上传目录。

## 快速开始

需要 Rust stable 和网络访问。开发时建议显式指定全局配置：

~~~bash
cargo run -- --config config.yaml
~~~

构建 release 版本后可直接执行二进制：

~~~bash
cargo build --release
./target/release/postui --config config.yaml
~~~

开发检查依赖如下：

- Rust stable 1.85 或更高版本，并启用 `rustfmt`、`clippy` 组件。
- Python 3.10 或更高版本及 `microsoft/tui-test` CLI 仅用于本机测试，不属于发布运行依赖。

发布目录中放有 `postui`、`config.yaml` 和 `.postui/requests.yaml` 时，进入该目录后显式指定全局配置：

~~~bash
./postui --config ./config.yaml
~~~

排查问题时使用 debug 构建并开启文件日志：

~~~bash
cargo run -- --debug --config config.yaml
~~~

默认日志写入配置文件目录下的 logs/postui-debug.log；也可以指定路径：

~~~bash
cargo run -- --debug --log-file ./logs/postui-debug.log --config config.yaml
~~~

debug 日志包含终端事件、界面操作、变量和剪贴板状态、请求构造、上传文件读取、响应头、响应体和错误上下文。请求体和响应体单字段最多记录 64 KiB；日志文件达到 8 MiB 后轮转为 .1，避免排障日志无限增长。请求头、表单字段、查询参数和 JSON 对象中名称包含 Authorization、Cookie、Token、Secret、Password 或 API key 的值会隐藏。--debug 仅在 debug 构建中可用，release 二进制不会生成这类日志。

也可以用 `--requests` 临时切换请求集合：

~~~bash
cargo run -- --config ./config.yaml --requests ./.postui/my-api.yaml
~~~

全局配置的查找优先级为：启动时显式指定的 `--config`；Home/root 目录下的 `postui.yaml` 或 `.postui.yaml`；`$XDG_CONFIG_HOME/postui/config.yaml` 或 `~/.config/postui/config.yaml`。三处都没有时使用内置默认 UI 配置，不再自动读取可执行文件同目录的配置。

## 操作

界面以鼠标操作为主，也保留键盘快捷键：

- 点击接口行可直接切换；点击左侧标题可打开接口选择层。
- 当前接口用到的变量会全部列出；点击变量行即可编辑，行末「清空」会清除该变量。
- 变量行的「填入」会读取系统剪贴板，响应区配置的「提取」会把指定响应字段复制到系统剪贴板。
- 点击信息区的「发送」按钮发送当前请求；在接口和变量列表上滚动可移动选择。

界面使用 ratatui 原生的 Block、Paragraph、List 和 Table 组件。窗口变窄时会收窄接口栏，必要时将接口列表移到上方，并让发送、变量操作和响应提取列采用紧凑布局；窗口太小无法容纳边框时会优先保留内容和点击区域。

预览和响应区中的 JSON 使用 syntect 的 JSON 语法高亮；接口地址、说明、状态提示、变量列表和响应提取目标中的双括号变量使用统一的变量色显示。

| 按键 | 作用 |
| --- | --- |
| ↑ / ↓、j / k | 移动接口或变量 |
| Tab / Shift+Tab | 切换接口、变量和发送区域 |
| Enter | 打开接口选择；在变量区开始编辑；在发送区发送 |
| r | 发送当前请求 |
| c / x / Delete | 清空当前变量 |
| 编辑时 Enter | 保存变量 |
| 编辑时 Esc | 放弃修改 |
| 编辑时 Ctrl+u | 清空输入 |
| q / Esc / Ctrl+c | 退出；下拉框和编辑状态会优先关闭 |

变量在整个会话中共享，但变量区只显示当前请求实际用到的名称。配置中的 default 是启动时的默认值；清空或编辑只影响当前运行，不会写回文件。重新打开 PostUI 后会恢复默认值。

### 麒麟、统信桌面环境的剪贴板

「填入」和「提取」优先使用原生 X11 或 Wayland 剪贴板。原生连接不可用时，Wayland 会尝试 `wl-copy`/`wl-paste`，X11 会尝试 `xclip`、`xsel`。如果按钮提示剪贴板不可用，请从系统软件源安装对应工具：Wayland 通常为 `wl-clipboard`，X11 通常为 `xclip` 或 `xsel`。开启 `--debug` 后，日志会记录实际尝试的后端和失败原因，不记录剪贴板文本内容。

## 配置文件

当前开发版只使用下面这种分层格式，不再接受旧的 method、url、body、form、files 字段。

全局配置（例如 `config.yaml`）只负责选择请求集合和界面样式：

~~~yaml
request_config: .postui/requests.yaml
theme: ocean

highlight:
  enabled: true
  syntax: base16-ocean.dark
  variable: "#c084fc"
~~~

支持的内置主题为 `ocean`、`nord`、`mono`。需要自定义颜色时，用 `theme_file` 指向主题 YAML：

~~~yaml
theme: ocean
theme_file: themes/custom.yaml
~~~

主题文件支持 `primary`、`secondary`、`accent`、`background`、`surface`、`text`、`muted`、`error`、`success`、`warning`、`selection`、`variable` 和 `syntax`。颜色可以写成 `#RRGGBB` 或 ratatui 的标准颜色名。`highlight.syntax` 和主题文件的 `syntax` 使用 syntect 内置语法主题名，`highlight` 中的值优先级更高。

请求集合默认放在项目的 `.postui/` 目录中（例如 `.postui/requests.yaml`），只负责接口和运行时数据：

~~~yaml
name: 我的接口
file_directory: "../files" # 请求集合在 .postui/ 时，文件目录位于项目根目录/files
timeout_seconds: 30

variables:
  - name: token
  - name: user_id

requests:
  - name: 用户列表
    description: 查询用户列表。
    request: |
      curl --location "https://api.example.com/users" \
        --header "Authorization: Bearer {{token}}"

  - name: 用户详情
    description: 查询指定用户。
    request: |
      curl --location "https://api.example.com/users/{{user_id}}"
~~~

请求集合字段：

- name：界面标题，可省略。
- file_directory：上传文件的根目录。相对路径以请求集合文件所在目录为基准，默认是 files。
- timeout_seconds：请求超时时间，默认 30 秒。
- variables：变量声明列表。
- requests：请求列表。

变量的 default 可省略。省略后变量初始为空，但仍然会显示在当前请求的变量列表中。请求中使用的变量必须提前声明，这样可以在加载配置时直接发现拼写错误。

request 是一段不会被执行的 curl 文本。它可以是普通 YAML 多行字符串，也可以包裹在 bash、sh 或普通代码块中。代码块中的反斜杠换行会被自动合并，随后使用 shell 引号规则拆分参数。

支持的常用 curl 参数：

- -X、--request：请求方法。
- -H、--header：请求头。
- -d、--data、--data-raw、--data-binary、--json：请求体。
- --data-urlencode：在变量展开后对字段或请求体片段进行 URL 编码。
- -F、--form、--form-string：普通表单和文件上传。
- -G、--get：将 data 参数放入查询字符串。
- --location、--compressed 等不影响请求内容的选项会被忽略。
- -b、--cookie、-A、--user-agent、-e、--referer 会转换成对应请求头。

文件上传直接使用 curl 的 form 写法：

~~~yaml
request: |
  curl --request POST "https://api.example.com/upload" \
    --form "file=@{{upload_file}};type=text/plain;filename={{upload_name}}" \
    --form-string "note={{note}}"
~~~

文件路径是 file_directory 下的文件名或相对路径；绝对路径也可以直接使用。请求体会保持 curl 中的原始文本，合法 JSON 会在预览中自动进行 JSON 染色。

### 返回变量提取

使用 extract 将 JSON 响应中的字段复制到剪贴板：

~~~yaml
requests:
  - name: 创建任务
    description: 创建任务并提取任务 ID。
    request: |
      curl --request POST "https://api.example.com/tasks" \
        --header "Content-Type: application/json" \
        --data-raw '{"name":"{{task_name}}"}'
    extract:
      task_id: data.taskId
~~~

extract 的键是目标变量名，值是响应路径。路径支持点路径、数组下标和 JSON Pointer：

~~~yaml
extract:
  task_id: data.taskId
  first_file: data.files[0].fileId
  status: /data/status
~~~

请求完成后，在响应区点击「提取」即可把字段值复制到系统剪贴板；再在变量列表点击对应行的「填入」，即可写入当前运行时变量。

解析器只处理静态 curl 参数，不执行 shell 命令替换、管道、重定向或多个命令。无法识别的 curl 参数会在配置加载时给出明确错误。

## 开发检查

~~~bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
~~~
