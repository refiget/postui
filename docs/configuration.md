# 配置与请求文件

配置只分四种用途，按需增加，不需要先准备完整目录：

| 用途 | 位置 | 内容 |
| --- | --- | --- |
| 个人偏好 | 用户配置目录的 `postui/config.yaml` | 语言、主题、响应显示上限 |
| 项目默认值 | `.postui/postui.yaml` | 公共变量、Header、超时、文件目录 |
| 场景差异 | `.postui/scenarios/<名称>.yaml` | 当前场景的变量、Header、超时和请求覆盖 |
| 请求定义 | `.postui/requests/**/*.yaml` | URL、方法、参数和请求体 |

只读取新规范，不提供旧字段、旧目录或旧 Header 条目数组的兼容解析。

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

在项目目录或子目录运行 `postui`，会向上查找最近的 `.postui/`，也可指定项目目录：

```bash
postui /path/to/project
```

## 项目默认值

只有需要共享设置时才创建 `.postui/postui.yaml`，所有字段均可省略：

```yaml
name: 示例接口
default_scenario: dev
timeout: 30

directories:
  uploads: test_files
  downloads: temp

variables:
  host: https://api.example.test
  token:

headers:
  Accept: application/json
```

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `name` | 项目目录名 | 工作区显示名称 |
| `default_scenario` | 场景名称排序后的第一项 | 启动场景；没有场景文件时为 `default` |
| `timeout` | `30` | 默认超时秒数，必须是正整数 |
| `directories.uploads` | `test_files` | 上传文件基准目录 |
| `directories.downloads` | `temp` | 响应下载目录 |
| `variables` | `{}` | 公共变量；空值表示运行时填写 |
| `headers` | `{}` | 公共 Header |

相对目录以项目根目录为基准，也支持绝对路径。下载目录在保存响应时创建；上传文件缺失时发送报错。

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
headers:
  X-Debug: "true"
overrides:
  users/detail.yaml:
    url: "{{host}}/staging/users"
    headers:
      Accept: application/json
```

场景仅支持 `variables`、`headers`、`timeout`、`overrides`。覆盖键是相对 `requests/` 的文件路径，例如 `users/detail.yaml`，不加 `requests/` 前缀。请求必须真实存在；路径不能越过请求目录。

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
| 请求覆盖 | 只替换声明字段；列表整体替换，不逐项拼接 |

超时省略才表示继承，`0` 无效，不表示无限等待。场景 `timeout` 统一调整该场景请求的超时；个别请求例外写在 `overrides` 中。

覆盖中 `headers: {}` 清除请求自身的 Header，仍继承公共和场景 Header；`params: []`、`form: []`、`files: []`、`extracts: []` 清空对应列表；`body: ""` 清空原始请求体。未声明或 `null` 的可选覆盖字段不做替换。

## 个人偏好

默认位置：

- Linux：`${XDG_CONFIG_HOME:-$HOME/.config}/postui/config.yaml`
- Windows：`%APPDATA%\postui\config.yaml`；显式设置 `XDG_CONFIG_HOME` 时优先使用该目录。

```yaml
language: zh
theme: catppuccin-mocha
max_response_display_bytes: 16777216
```

| 字段 | 默认值 | 可选值或约束 |
| --- | --- | --- |
| `language` | `en` | `en`、`zh` |
| `theme` | `gruvbox-dark` | `gruvbox-dark`、`dracula`、`catppuccin-mocha`、`tokyo-night`、`nord`、`one-dark`、`solarized-dark`、`kanagawa`、`rose-pine`、`monokai` |
| `max_response_display_bytes` | `16777216`（16 MiB） | 正整数，只限制格式化及展示，不截断下载内容 |

JSON 按当前视口生成文本和高亮。超过显示上限的响应仍能通过 Response 的 Actions 下载完整内容。

可以显式指定个人配置：

```bash
postui /path/to/project --config /path/to/ui.yaml --scenario test
```

`--config` 替代自动发现的个人配置，不合并两份文件。相对路径以启动目录为基准；项目根目录的 `config.yaml` 不会自动读取。个人配置仅包含上述三个选项，请求行为不放在这里。

Debug 构建使用 `--debug` 启动后，可按 `F5` 依次热加载全部内置主题。切换仅影响当前进程，不写回个人配置，适合演示和录屏。

## 请求定义

支持 `.yaml` 和 `.yml`，子目录用于组织请求：

```yaml
name: 查询用户
description: 查询指定用户
method: GET
url: "{{host}}/users/{{item_id}}"
timeout: 10
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

请求支持 `name`、`description`、`method`、`url`、`timeout`、`headers`、`params`、`body`、`form`、`files`、`extracts`。`url` 必填；`method` 默认 `GET`；`timeout` 省略时继承项目默认值，其余内容按需填写。

所有层的 Header 都是映射，值为字符串或非空字符串列表；数字、布尔值请加引号。重复 Header 写为同一名称下的列表，不重复声明映射键。保存时同名 Header 归组，保留该名称下各值的顺序。

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
- 请求文件必须包含非空 URL；新增或手动修改文件后重新启动加载。
- `Ctrl+S` 保存当前请求及已修改场景覆盖，写出的 Header 只采用新映射格式；有未保存修改时退出需要确认。
- 请求列表聚焦时 `Delete` 经确认删除当前请求文件。
- `.postui/cache/` 只存解析缓存；源内容变化或缓存版本变化即失效，缓存读写失败不影响源文件解析。缓存、日志、下载产物和个人配置不提交。

公共示例见 [双场景 Echo API](../examples/public-api/README.md)。已有本地配置需按本规范手动整理，程序不会迁移或改写旧配置。
