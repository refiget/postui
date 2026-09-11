# 配置与请求文件

PostUI 使用两层配置：全局配置决定请求集合入口和界面样式，集合配置决定请求、变量、请求头和文件目录。

## 配置文件

| 文件 | 用途 | 是否提交 |
| --- | --- | --- |
| `config.example.yaml` | 公共配置模板 | 是 |
| `config.yaml` | 本机全局配置 | 否 |
| `.postui/config.yaml` | 项目入口配置 | 根目录项目配置不提交；`mock/.postui` 是公共测试夹具 |
| `.postui/collections/<name>/config.yaml` | 某个请求集合的配置 | 按项目性质决定 |

全局配置示例：

```yaml
request_config: .postui/collections/example
language: en
theme: gruvbox-dark

highlight:
  enabled: true
  syntax: base16-mocha.dark
  variable: "#d3869b"
```

`request_config` 和 `theme_file` 的相对路径以全局配置文件所在目录为基准。`language` 支持 `en` 和 `zh`；内置主题包括 `gruvbox-dark`、`ocean`、`nord` 和 `mono`。

## 自动发现顺序

未指定 `--config` 时，程序按以下顺序寻找配置：

1. 当前目录及父目录中的 `.postui/config.yaml`。
2. 当前可执行文件同目录的 `config.yaml`。
3. Linux 用户目录下的 `postui.yaml` 或 `.postui.yaml`；Windows 用户目录下的同名文件。
4. Linux 的 `${XDG_CONFIG_HOME:-$HOME/.config}/postui/config.yaml`；Windows 的 `%APPDATA%\postui\config.yaml`。

显式指定的配置文件读取失败会直接报错，不会回退到其他位置。未指定 `--requests` 时使用全局配置中的 `request_config`；`--requests` 可以覆盖它：

```bash
postui --requests ./.postui/collections/other
```

## 目录布局

推荐的项目结构如下：

```text
.postui/
├── config.yaml
└── collections/
    └── example/
        ├── config.yaml
        ├── requests.cache.json   # 运行时生成，不提交
        └── requests/
            ├── 01-list.http
            └── 02-detail.http
test_files/                       # 默认上传目录
temp/                             # 默认下载目录，首次保存时创建
```

集合配置只保存集合级设置：

```yaml
name: 示例接口
timeout_seconds: 30

headers:
  Accept: application/json

variables:
  host: https://api.example.test
  token:
  item_id:
```

`file_directory` 和 `download_directory` 省略时，分别默认为 `.postui/` 同级的 `test_files/` 和 `temp/`。显式相对路径则以集合配置文件所在目录为基准。`timeout_seconds: 0` 等同于默认值 30 秒。

集合 Header 会被所有请求继承；请求文件中的同名 Header 覆盖集合默认值。变量值省略或写成 `null` 时没有默认值，运行期间仍可在 Variables 窗口中编辑。

## 请求文件

每个请求文件包含一段静态 curl 文本，可以通过注释指令提供名称、说明、超时和响应字段提取：

~~~text
# @name 查询用户
# @description 查询指定用户。
# @timeout 10
# @extract user_id = data.id
curl --request GET "{{host}}/users/{{item_id}}"
~~~

没有 `@name` 时使用文件名。文件名前的数字序号和连接符会被去掉，例如 `02-user-detail.http` 显示为 `user-detail`。相对路径作为请求的稳定 id，可以使用子目录组织请求。

支持的 curl 参数：

- `-X`、`--request`、`--url`。
- `-H`、`--header`。
- `-d`、`--data`、`--data-raw`、`--data-binary`、`--json`、`--data-urlencode`。
- `-F`、`--form`、`--form-string`。
- `-G`、`--get`，把 data 参数放入查询字符串。
- `-b`、`--cookie`、`-A`、`--user-agent`、`-e`、`--referer`，会转换成请求头。

`--location`、`--compressed`、`--silent` 等不影响请求内容的选项会被忽略。输出选项（`-o`、`--output`、`-O`、`--remote-name`、`-J`、`--remote-header-name`）不会改变 PostUI 的请求行为；响应保存由 Response 的 `Actions` 菜单负责。

PowerShell 请求请使用 `curl.exe`，避免 `curl` 别名被解析为 `Invoke-WebRequest`：

~~~powershell
curl.exe --request GET "https://example.test/items/{{item_id}}" `
  --header "Accept: application/json"
~~~

文件上传使用 `--form` 的 `@` 写法。相对路径以 `file_directory` 为基准，也可以使用绝对路径：

~~~text
curl --request POST "{{host}}/files" \
  --form "file=@{{upload_file}};type=application/pdf"
~~~

解析器会合并反斜杠换行和 PowerShell 反引号换行，但不会执行命令替换、管道、重定向或多个命令。

## 返回字段提取

`@extract` 只在 HTTP 成功响应（状态码小于 400）后执行，从 JSON 响应体读取字段并写入当前集合的会话变量。路径支持：

- `data.taskId`
- `data.items[0].id`
- `/data/taskId`（JSON Pointer）

某个字段不存在或响应不是 JSON 时，其他字段仍会更新，已有变量值不会被清空；状态栏会显示失败数量。HTTP 失败响应和传输错误不会执行提取。

## 缓存与编辑

首次成功解析集合后，会在集合目录生成 `requests.cache.json`。缓存指纹覆盖集合 `config.yaml` 和 `requests/` 下的全部请求文件，新增、修改或删除请求后会自动失效。

界面中的 Params、Headers、Body 和 Variables 修改只在本次运行生效，不会回写请求文件。请求切换、集合切换、发送请求或打开 Response 菜单前，当前编辑内容会先提交到会话状态。
