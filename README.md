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

一个包含 `.postui/` 的目录就是一个工作区：

```text
.postui/
├── postui.yaml
├── configs/
│   ├── dev.yaml
│   └── test.yaml
└── requests/
    ├── 01-list.yaml
    └── 02-detail.yaml
test_files/
temp/
```

项目配置 `.postui/postui.yaml`：

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

所有字段都可省略。相对上传和下载目录以项目根目录为基准。在项目目录或子目录运行 `postui` 会自动发现工作区，也可以运行 `postui /path/to/project`。

`requests/` 目录也可以省略，空工作区仍会正常打开。PostUI 只加载配置中已有的请求；请在 `.postui/requests/` 中维护请求文件。

workspace 的配置选项位于 `.postui/configs/`，文件名就是下拉菜单中的配置名：

```yaml
# .postui/configs/dev.yaml
variables:
  host: https://dev-api.example.test

# .postui/configs/test.yaml
variables:
  host: https://test-api.example.test
```

在左侧 Workspace 下拉菜单中选择 `dev` 或 `test`。公共请求只保留一份，切换配置只更换变量、Header、超时和该配置的接口覆盖。`postui.yaml` 中的 `default_configuration` 指定启动时的选项；省略时使用配置文件名排序后的第一个配置。

个人界面配置位于 `${XDG_CONFIG_HOME:-$HOME/.config}/postui/config.yaml`，Windows 位于 `%APPDATA%\postui\config.yaml`：

```yaml
language: zh
theme: ocean
max_response_display_bytes: 16777216
```

`max_response_display_bytes` 默认是 `16777216`（16 MiB），控制响应体最多交给格式化和界面展示的字节数，可直接覆盖 10 MiB 级响应。界面只为当前视口生成文本行，不会等待完整响应完成格式化；超过上限时原始响应仍可通过 Response 的 Actions 下载。

### 请求文件

请求文件使用结构化 YAML，支持 `.yaml` 和 `.yml`：

```yaml
name: 查询用户
description: 查询指定用户
method: GET
url: "{{host}}/users/{{item_id}}"
headers:
  - name: Accept
    value: application/json
params:
  - name: include
    value: profile
extracts:
  - variable: user_id
    path: data.id

```

变量格式：`{{variable_name}}`。

工作区 `variables` 是所有配置共享的默认变量；`.postui/configs/<name>.yaml` 只声明该配置的变量、Header、timeout 和接口 `overrides`。发送时按“工作区公共配置 → 当前 workspace 配置 → 当前接口覆盖 → 当前编辑草稿”的顺序合并，因此同一接口可以被多个配置复用。

支持的 HTTP 方法：`GET`、`POST`。

完整配置字段见[配置与请求文件](docs/configuration.md)。
