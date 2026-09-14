# 配置与请求文件

配置分为四种用途：

| 用途 | 位置 | 内容 |
| --- | --- | --- |
| 个人偏好 | [平台标准配置目录](#个人偏好) | 语言、主题、响应显示上限 |
| 项目默认值 | `.postui/postui.yaml` | 公共变量、Header、超时、文件目录 |
| 场景差异 | `.postui/scenarios/<名称>.yaml` | 当前场景的变量、Header、超时和请求覆盖 |
| 请求定义 | `.postui/requests/**/*.yaml` | URL、方法、参数和请求体 |

配置字段按本文定义读取，未知字段会报错。

## 最小工作区

```text
project/
└── .postui/
    └── requests/
        └── health.yaml
```

```yaml
# .postui/requests/health.yaml
url: https://example.test/health
```

方法默认 `GET`，名称默认取文件名，超时默认 30 秒。`.postui/requests/` 也可以不存在，空工作区仍能打开。

在项目目录或子目录运行 `postui`，会向上查找最近的 `.postui/`，也可指定项目目录或 `.postui` 目录：

```bash
postui /path/to/project
postui /path/to/project/.postui
```

显式指定目录时不向父目录搜索。自动发现一直搜索到文件系统根目录，不以 `.git` 为边界。最近的 `.postui` 如果不是目录、无法访问或是失效的符号链接，会直接报错，不跳过它去加载外层工作区；内部配置有误也不会回退到父级。

未找到工作区时退出，不自动创建 `.postui`。新建空工作区可先执行 `mkdir .postui`，再运行 `postui`。相对路径以启动目录为基准；符号链接按实际目标解析。

## 项目默认值

只有需要共享设置时才创建 `.postui/postui.yaml`，所有字段均可省略：

```yaml
name: 示例接口
default_scenario: dev
timeout: 30
skip_ssl_verification: false

directories:
  uploads: test_files
  downloads: temp

variables:
  host: https://api.example.test
  token:
    value:
    secret: true

headers:
  Accept: application/json
```

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `name` | 项目目录名 | 工作区显示名称 |
| `default_scenario` | 场景名称排序后的第一项 | 启动场景；没有场景文件时为 `default` |
| `timeout` | `30` | 默认超时秒数，必须是正整数 |
| `skip_ssl_verification` | `false` | 跳过 HTTPS 证书校验；可由场景或请求覆盖 |
| `directories.uploads` | `test_files` | 上传文件基准目录 |
| `directories.downloads` | `temp` | 响应下载目录 |
| `variables` | `{}` | 公共变量；未填写时按空字符串展开，可在运行时手动修改 |
| `headers` | `{}` | 公共 Header |

相对目录以项目根目录为基准，也支持绝对路径。下载目录在保存响应时创建；上传文件缺失时发送报错。

变量支持标量简写和带属性的声明。`secret: true` 的变量在变量页中掩码显示，实际值不会写入调试日志；运行时填写的值只保留在当前进程中。

## 场景差异

场景不复制请求，只描述差异。文件名去掉扩展名就是场景名称：

```yaml
# .postui/scenarios/dev.yaml
variables:
  host: https://dev-api.example.test
```

```yaml
# .postui/scenarios/test.yaml
variables:
  host: https://test-api.example.test
timeout: 60
skip_ssl_verification: true
headers:
  X-Debug: "true"
overrides:
  users/detail.yaml:
    url: "{{host}}/staging/users"
    headers:
      Accept: application/json
```

场景仅支持 `variables`、`headers`、`timeout`、`skip_ssl_verification`、`overrides`。覆盖键是相对 `requests/` 的文件路径，例如 `users/detail.yaml`，不加 `requests/` 前缀。请求必须真实存在；路径不能越过请求目录。

启动时可临时指定场景，不回写项目默认值：

```bash
postui /path/to/project --scenario test
```

选择顺序：`--scenario` → `default_scenario` → 名称排序后的第一项。没有场景文件时提供 `default`。场景不存在会报错，不自动换成其他场景。运行中使用 Workspace 下拉菜单切换。

### 合并规则

| 内容 | 生效规则 |
| --- | --- |
| 变量 | 场景同名变量覆盖公共变量，其他变量保留 |
| Header | 项目 → 场景 → 请求；后一级同名 Header 覆盖前一级，名称不区分大小写 |
| 超时 | 项目默认值 → 请求 `timeout` → 场景 `timeout` → 场景请求覆盖的 `timeout` |
| TLS 校验 | 项目默认值 → 请求 `skip_ssl_verification` → 场景同名字段 → 场景请求覆盖的同名字段 |
| HTTP 方法 | 请求 `method`（省略为 `GET`）→ 场景 `overrides` 中该请求的 `method` |
| 请求覆盖 | 只替换声明字段；列表整体替换，不逐项拼接 |

超时省略才表示继承，`0` 无效，不表示无限等待。场景 `timeout` 统一调整该场景请求的超时；个别请求例外写在 `overrides` 中。

覆盖中 `headers: {}` 清除请求自身的 Header，仍继承公共和场景 Header；`params: []`、`form: []`、`files: []`、`extracts: []` 清空对应列表；`body: ""` 清空原始请求体。未声明或 `null` 的可选覆盖字段不做替换。

## 个人偏好

默认位置由 `directories::ProjectDirs` 按平台规范确定：

- Linux：`${XDG_CONFIG_HOME:-$HOME/.config}/postui/config.yaml`
- macOS：`~/Library/Application Support/postui/config.yaml`
- Windows：用户 Roaming AppData 下的 `postui\config\config.yaml`，通常为 `%APPDATA%\postui\config\config.yaml`。

`XDG_CONFIG_HOME` 仅用于 Linux，且必须是绝对路径；空值或相对路径使用平台默认位置。不读取旧路径，不自动迁移配置。默认配置文件不存在时使用内置偏好；显式 `--config` 指定的文件不存在或无法读取时显示配置错误。

```yaml
language: en
theme: catppuccin-mocha
max_response_display_bytes: 16777216
max_response_bytes: 67108864
```

| 字段 | 默认值 | 可选值或约束 |
| --- | --- | --- |
| `language` | `en` | `en`、`zh` |
| `theme` | `gruvbox-dark` | `gruvbox-dark`、`dracula`、`catppuccin-mocha`、`tokyo-night`、`nord`、`one-dark`、`solarized-dark`、`kanagawa`、`rose-pine`、`monokai` |
| `max_response_display_bytes` | `16777216`（16 MiB） | 正整数，只限制格式化及展示，不截断下载内容 |
| `max_response_bytes` | `67108864`（64 MiB） | 正整数，单次响应接收上限；超过后终止请求，不保留截断内容 |

响应区提供 Raw、Formatted、Headers 页签，可点击或聚焦后按左右键切换。JSON 保留原有按视口缩进、即时高亮逻辑，不生成整份美化副本，不另设 1 MiB 美化或 64 KiB 高亮文件上限。Raw 显示原文；Formatted 还支持 XML 缩进和表单解码，其他文本保留排版。非 JSON 常见格式使用 Syntect 后台分页高亮；超长单行或复杂语法局部降级，不因文件大小整份禁用。二进制只显示摘要。接收与显示容量仍由上表两个配置项控制。

复制 Body 和下载始终使用原始响应，不使用美化后的文本。超过显示上限但未超过接收上限的响应仍能通过 Response 的 Actions 下载完整内容。接收上限也适用于未声明 Content-Length 的分块响应；调高上限会增加内存占用。

可以显式指定个人配置：

```bash
postui /path/to/project --config /path/to/ui.yaml --scenario test
```

`--config` 替代自动发现的个人配置，不合并两份文件。相对路径以启动目录为基准；项目根目录的 `config.yaml` 不会自动读取。个人配置仅包含上表选项，请求行为不放在这里。

Debug 构建使用 `--debug` 启动后，可按 `F5` 依次热加载全部内置主题。切换仅影响当前进程，不写回个人配置。

## 请求定义

支持 `.yaml` 和 `.yml`，子目录用于组织请求：

```yaml
name: 查询用户
description: 查询指定用户
method: GET
url: "{{host}}/users/{{item_id}}"
timeout: 10
skip_ssl_verification: false
headers:
  Accept: application/json
  X-Trace-Tag: [one, two]
params:
  - name: include
    value: profile
extracts:
  - variable: user_id
    path: data.id
```

请求支持 `name`、`description`、`method`、`url`、`timeout`、`skip_ssl_verification`、`headers`、`params`、`body`、`form`、`files`、`extracts`。`url` 必填；`method` 默认 `GET`；`timeout` 省略时继承项目默认值，其余内容按需填写。

### HTTP 方法配置

直接在 `.postui/requests/**/*.yaml` 或 `.yml` 的顶层声明 `method`，不需要额外启用方法：

```yaml
# .postui/requests/users/update.yaml
name: 更新用户
method: PUT
url: "{{host}}/users/123"
headers:
  Content-Type: application/json
body: |
  {"name": "Example"}
```

`method` 支持 `GET`、`POST`、`PUT`、`PATCH`、`DELETE`、`HEAD`、`OPTIONS`、`TRACE`、`CONNECT`，也支持合法的扩展方法（如 `PROPFIND`）。各方法使用相同的请求字段；每个文件只定义一个请求。

场景通过 `overrides` 修改已有请求的方法；例如让上述 `users/update.yaml` 在 `dev` 场景使用 `PATCH`：

```yaml
# .postui/scenarios/dev.yaml
variables:
  host: http://127.0.0.1:8080
overrides:
  users/update.yaml:
    method: PATCH
    body: |
      {"name": "Changed"}
```

覆盖键相对 `requests/`，且必须对应真实文件。URL 和 Header 未覆盖时沿用原请求。`method` 不属于个人配置、项目默认值或场景顶层字段，只放在请求定义或场景的请求覆盖中。

| 写法 | 读取行为 |
| --- | --- |
| 请求省略 `method` | 使用 `GET` |
| 覆盖省略 `method` 或写 `method: null` | 保留原请求方法 |
| `method: " patch "` | 去除首尾空白，转为 `PATCH` |
| `method: PROPFIND` | 作为扩展方法加载和发送 |
| `method: ""`、全空白或 `method: "BAD METHOD"` | 配置加载失败，不回退为 `GET` |
| 请求定义写 `method: null` | 配置加载失败；默认值仅适用于省略字段 |

覆盖项必须至少包含一个有效的覆盖字段；只写 `method: null` 会成为空覆盖项并报错，无差异时应删除整个覆盖项。

方法由 HTTP 库校验语法，不展开变量。保存文件后首次启动会读取；应用已打开时按 `R` 重载。可用 `postui /path/to/project --scenario dev` 读取指定场景。

界面点击方法依次切换上述九种标准方法，扩展方法从 `GET` 开始；发送中不可切换。切换保留其他请求字段，仅影响本次会话。持久设置请修改 YAML。

各方法共用请求体和上传流程，服务器是否接受以实际响应为准。`CONNECT` 仅发送请求并展示响应，不提供交互式隧道。

### 参数与请求体

所有层的 Header 都是映射，值为字符串或非空字符串列表；数字、布尔值请加引号。重复 Header 写为同一名称下的列表，列表顺序保留，不重复声明映射键。

`params`、`form` 使用 `name/value` 条目列表，保留重复名称和顺序。Query 的无等号参数使用 `has_equals: false`，例如：

```yaml
params:
  - name: flag
    value: ""
    has_equals: false
```

URL query 和 `params` 在变量展开后统一按 `application/x-www-form-urlencoded` 编码。不带 `=` 的参数保留为 key-only。

`body` 是原始文本，JSON 使用 YAML 块文本：

```yaml
method: POST
url: "{{host}}/users"
headers:
  Content-Type: application/json
body: |
  {"name": "{{user_name}}"}
```

Multipart 使用 `form` 和 `files`，相对文件路径以 `directories.uploads` 为基准：

```yaml
method: POST
url: "{{host}}/files"
files:
  - field: file
    path: "{{upload_file}}"
    filename: report.pdf
    content_type: application/pdf
```

`extracts` 在 HTTP 状态码小于 400 时从 JSON 响应提取变量，支持点路径、数组下标和 JSON Pointer。变量格式为 `{{variable_name}}`；变量编辑只影响当前场景和本次运行，不回写 YAML。

## 读取、保存与错误

- 缺省个人配置和项目配置使用默认值；空文件、只有注释或 `{}` 的个人、项目和场景文件也使用默认值。
- 显式 `--config` 路径缺失、文件不可读、非法值和未知字段直接报错。
- `requests/`、`scenarios/` 缺失时视为空目录；同名路径是文件或目录不可读时会报错。
- 请求文件必须包含非空 URL；新增或手动修改文件后可在界面按 `R` 重新加载。加载失败时保留当前可用配置，并显示出错文件及 YAML 位置。
- 界面中的请求修改只作用于当前运行会话，不写回请求或场景配置文件；退出时直接丢弃。
- 未定义或值为空的变量按空字符串展开，不阻止发送，也不自动打开变量页；展开后的非法 URL、请求头或文件路径由请求执行流程报错。
- 单击接口选择，双击接口直接发送；正在发送的接口不会因双击而取消或重复发送。
- 请求发送过程中再次按 `r` 或点击发送区域可取消当前操作；迟到的后台结果会被丢弃。
  网络等待、上传及响应读取可中断。已开始的后台 JSON 提取或索引计算可能继续完成，但不会更新已取消的请求。最多同时处理 8 个请求，超出时提示稍后重试。
- `R` 在后台扫描、校验并构建工作区；完成前可以浏览旧状态，暂停发送和切换场景。成功后以新配置替换临时编辑，失败则保留旧状态。
- 请求体超过 64 KiB 时不在交互线程执行 JSON 美化和正则高亮，改为普通文本预览；发送内容不变。
- 请求列表聚焦时 `Delete` 经确认删除当前请求文件。
- `.postui/cache/` 只存解析缓存；源内容变化或缓存版本变化即失效，缓存读写失败不影响源文件解析。缓存、日志、下载产物和个人配置不提交。

公共示例见 [双场景 Echo API](../examples/public-api/README.md)。
