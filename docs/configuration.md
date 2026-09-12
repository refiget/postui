# 配置与请求文件

PostUI 把一个包含 `.postui/` 的目录视为一个工作区。项目配置负责请求行为，用户配置只负责个人界面偏好。

## 工作区

推荐结构：

```text
project/
├── .postui/
│   ├── postui.yaml
│   ├── configs/
│   │   ├── dev.yaml
│   │   └── test.yaml
│   ├── cache/                  # 自动生成，不提交
│   └── requests/
│       ├── 01-health.yaml
│       └── users/
│           └── 02-detail.yaml
├── test_files/                 # 默认上传目录
└── temp/                       # 默认下载目录
```

在项目目录或任意子目录运行 `postui`，程序会向上查找最近的 `.postui/`。也可以直接指定项目目录：

```bash
postui /path/to/project
```

`.postui/requests/` 可以不存在或为空，空工作区仍会进入 TUI。PostUI 只读取目录中已有的请求；新增请求请直接创建请求文件，然后重新启动 PostUI。

`.postui/postui.yaml` 可以省略。完整字段如下：

```yaml
name: 示例接口
timeout: 30

directories:
  uploads: test_files
  downloads: temp

headers:
  - name: Accept
    value: application/json

variables:
  token:
  item_id:
```

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `name` | 项目目录名 | 工作区显示名称 |
| `timeout` | `30` | 默认请求超时秒数；请求文件中的 `timeout` 可以覆盖 |
| `directories.uploads` | `test_files` | 相对上传目录 |
| `directories.downloads` | `temp` | 响应下载目录 |
| `headers` | `[]` | 所有请求继承的 Header 条目，按列表顺序发送 |
| `variables` | `{}` | 所有 workspace 配置共享的默认变量；空值表示运行时填写 |
| `default_configuration` | 第一个配置 | 启动时在 Workspace 下拉菜单中选中的配置 |

目录相对路径始终以项目根目录为基准，也支持绝对路径。下载目录在保存响应时自动创建；上传目录或文件不存在时，发送操作会显示错误。

## Workspace 配置

`.postui/configs/` 下的每个 YAML 文件都是 workspace 的一个可切换配置，文件名（去掉扩展名）作为下拉菜单显示名。没有配置文件时，程序自动提供一个名为 `default` 的运行时配置，因此公共请求仍可直接打开。

配置文件只放当前场景和公共请求不同的内容：

~~~yaml
# .postui/configs/dev.yaml
variables:
  host: https://dev-api.example.test
headers:
  - name: X-PostUI-Scenario
    value: dev

# .postui/configs/test.yaml
variables:
  host: https://test-api.example.test
timeout: 60
overrides:
  requests/users.yaml:
    url: "{{host}}/staging/users"
    headers:
      - name: X-Debug
        value: "true"
~~~

配置文件支持 `variables`、`headers`、`timeout` 和 `overrides`。`overrides` 的键是 `.postui/requests/` 下请求文件的稳定路径；覆盖只替换声明的字段。Workspace 下拉菜单切换配置时，公共请求不会复制，运行时按“工作区公共配置 → 当前配置 → 当前接口覆盖”合并。

## 用户界面配置

用户配置位置：

- Linux：`${XDG_CONFIG_HOME:-$HOME/.config}/postui/config.yaml`
- Windows：`%APPDATA%\postui\config.yaml`

```yaml
language: zh
theme: ocean
```

`language` 支持 `en` 和 `zh`；`theme` 支持 `gruvbox-dark`、`ocean`、`nord` 和 `mono`。用户配置不存在时使用内置默认值。语法和变量高亮默认开启并跟随主题。

## 请求文件

请求文件位于 `.postui/requests/`，支持 `.yaml` 和 `.yml`，使用结构化 YAML：

~~~yaml
name: 查询用户
description: 查询指定用户
method: GET
url: "{{host}}/users/{{item_id}}"
timeout: 10
headers:
  - name: Accept
    value: application/json
params:
  - name: include
    value: profile
extracts:
  - variable: user_id
    path: data.id
~~~

子目录用于组织请求。请求文件相对于 `.postui/requests/` 的路径是稳定 ID。

请求文件支持以下字段：`name`、`description`、`method`、`url`、`timeout`、`headers`、`params`、`body`、`form`、`files` 和 `extracts`。`headers`、`params`、`form` 使用条目数组，保留书写顺序和重复名称；`body` 是原始请求体文本。配置差异统一写入对应的 `.postui/configs/<name>.yaml`，请求文件本身不包含环境或配置标签。

文件上传使用 `files` 条目。相对文件名以 `directories.uploads` 为基准：

~~~yaml
method: POST
url: "{{host}}/files"
files:
  - field: file
    path: "{{upload_file}}"
    content_type: application/pdf
~~~

`extracts` 只在 HTTP 状态码小于 400 时从 JSON 响应提取字段。支持点路径、数组下标和 JSON Pointer。

URL query 和 `params` 都按有序参数处理：保留重复名称和书写顺序，变量展开后统一进行 `application/x-www-form-urlencoded` 编码。URL 中的 `+`、百分号编码和空值会在参数编辑器中显示为可编辑的 name/value，保存或发送时重新编码；不带 `=` 的参数仍会保留为 key-only。

`body` 按原始文本发送，不会被强行拆成参数。需要 multipart 时使用 `form` 和 `files`，文件字段单独保留。

Header 使用 `name`/`value` 条目数组，而不是 YAML 映射，因此可以保留重复名称和书写顺序：

```yaml
headers:
  - name: Accept
    value: application/json
  - name: X-Trace-Tag
    value: one
  - name: X-Trace-Tag
    value: two
```

请求文件中的同名 Header 会覆盖工作区默认 Header；请求文件中的重复 Header 会全部保留并按原顺序发送。Form 字段也保留重复名称，参数编辑器不会合并同名条目。

## 编辑、保存与缓存

已加载请求可以在界面中编辑并直接发送，未保存修改以 `●` 标记。按 `Ctrl+S` 将修改写回当前请求文件，并保存已修改的 workspace 配置覆盖。请求列表聚焦时按 `Delete` 会在确认后删除当前请求文件；存在未保存修改时退出会要求确认，避免误操作丢失内容。

`.postui/cache/` 使用 `cacache` 持久化项目配置、workspace 配置和请求文件的解析结果。源内容的 BLAKE3 指纹变化后缓存自动失效；缓存损坏或读写失败时会回退到重新解析，不影响工作区启动。Variables 的修改只在当前配置和本次运行期间生效，不会回写配置 YAML；接口草稿则通过 `Ctrl+S` 写回当前请求 YAML 和对应配置的 `overrides`。
