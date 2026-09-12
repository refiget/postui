# 配置与请求文件

PostUI 把一个包含 `.postui/` 的目录视为一个工作区。项目配置负责请求行为，用户配置只负责个人界面偏好。

## 工作区

推荐结构：

```text
project/
├── .postui/
│   ├── postui.yaml
│   ├── cache/                  # 自动生成，不提交
│   └── requests/
│       ├── 01-health.http
│       └── users/
│           └── 02-detail.http
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
  host: https://api.example.test
  token:
  item_id:
```

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `name` | 项目目录名 | 工作区显示名称 |
| `timeout` | `30` | 默认请求超时秒数；请求文件中的 `@timeout` 可以覆盖 |
| `directories.uploads` | `test_files` | 相对上传目录 |
| `directories.downloads` | `temp` | 响应下载目录 |
| `headers` | `[]` | 所有请求继承的 Header 条目，按列表顺序发送 |
| `variables` | `{}` | 工作区变量；空值表示启动后填写 |

目录相对路径始终以项目根目录为基准，也支持绝对路径。下载目录在保存响应时自动创建；上传目录或文件不存在时，发送操作会显示错误。

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

请求文件位于 `.postui/requests/`，支持 `.http`、`.rest` 和 `.curl`，并使用静态 curl 文本：

~~~text
# @name 查询用户
# @description 查询指定用户
# @timeout 10
# @extract user_id = data.id
curl --request GET "{{host}}/users/{{item_id}}"
~~~

子目录用于组织请求。请求文件相对于 `.postui/requests/` 的路径是稳定 ID。

支持常用的 curl 请求参数，包括 `--request`、`--url`、`--header`、`--data`、`--json`、`--data-urlencode`、`--form`、`--form-string` 和 `--get`。输出参数不会改变 PostUI 的行为，响应保存由 Response 的 Actions 菜单负责。

文件上传使用 `--form` 的 `@` 写法。相对文件名以 `directories.uploads` 为基准：

~~~text
curl --request POST "{{host}}/files" \
  --form "file=@{{upload_file}};type=application/pdf"
~~~

`@extract` 只在 HTTP 状态码小于 400 时从 JSON 响应提取字段。支持点路径、数组下标和 JSON Pointer。

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

已加载请求可以在界面中编辑并直接发送，未保存修改以 `●` 标记。按 `Ctrl+S` 将修改写回当前请求文件。请求列表聚焦时按 `Delete` 会在确认后删除当前请求文件；存在未保存修改时退出会要求确认，避免误操作丢失内容。

`.postui/cache/` 使用 `cacache` 持久化项目配置和请求文件的解析结果。源内容的 BLAKE3 指纹变化后缓存自动失效；缓存损坏或读写失败时会回退到重新解析，不影响工作区启动。Variables 的修改仍只在当前运行期间生效，不会回写 `postui.yaml`。
