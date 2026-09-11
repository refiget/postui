# PostUI

这个软件是我在某企畸形的开发环境使用的个人工具, 因为只能用到 `GET` 和 `POST`, 所以我只有这两个功能. Linux 个 windows 都支持(虽然我是Mac)

界面以鼠标操作为主

## 界面预览

![PostUI 终端界面预览](assets/screenshot.png)

## 构建

### 本地构建

环境：

- Rust 1.85 或更高版本
- Rust 2024 edition
- Linux amd64：`x86_64-unknown-linux-musl`
- Windows amd64：`x86_64-pc-windows-msvc`

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

## 配置

### 全局配置

复制配置模板：

```bash
cp config.example.yaml config.yaml
```

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

字段：

| 字段 | 值 |
| --- | --- |
| `request_config` | 请求集合目录 |
| `language` | `en` 或 `zh` |
| `theme` | `gruvbox-dark`、`ocean`、`nord`、`mono` |
| `theme_file` | 自定义主题文件路径 |
| `highlight.enabled` | 是否启用语法高亮 |
| `highlight.syntax` | 语法主题名称 |
| `highlight.variable` | 变量颜色 |

未指定 `--config` 时，配置查找顺序如下：

1. 当前项目及父目录中的 `.postui/config.yaml`。
2. 程序目录中的 `config.yaml`。
3. 用户级配置文件。

命令行参数：

```bash
postui --config ./config.yaml --requests ./.postui/collections/example
```

### 请求集合

请求集合包含 `config.yaml` 和 `requests/`：

```text
.postui/
├── config.yaml
└── collections/
    └── example/
        ├── config.yaml
        └── requests/
            ├── 01-list.http
            └── 02-detail.http
```

集合配置示例：

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

集合字段：`name`、`timeout_seconds`、`headers`、`variables`、`file_directory`、`download_directory`。

### 请求文件

支持 `.http`、`.rest` 和 `.curl` 文件。请求文件使用 curl 格式：

```text
# @name 查询用户
# @description 查询指定用户
# @timeout 10
# @extract user_id = data.id
curl --request GET "{{host}}/users/{{item_id}}"
```

变量格式：`{{variable_name}}`。

支持的 HTTP 方法：`GET`、`POST`。

完整配置字段见[配置与请求文件](docs/configuration.md)。
