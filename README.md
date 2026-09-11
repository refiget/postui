# PostUI

PostUI 是一个配置驱动的终端接口测试工具。它读取 `.http`、`.rest` 或 `.curl` 请求文件，在同一个终端工作区中编辑请求、发送请求并查看响应。

界面以鼠标操作为主，同时保留 Vim 风格快捷键作为辅助。请求文件、项目配置和本地上传文件彼此分离，个人项目配置不会随源码提交。

## 快速开始

### 从源码运行

先准备一个项目配置。`config.example.yaml` 只包含公共示例，可复制为本机使用的 `config.yaml`：

```bash
cp config.example.yaml config.yaml
```

如果只是运行仓库中的 mock 请求，可以直接覆盖请求集合路径：

```bash
cargo run -- --requests mock/.postui
```

程序启动时无需显式传入 `--config`。它会自动寻找当前项目的 `.postui/config.yaml`；找不到时再查找程序目录和用户级配置。完整规则见[配置说明](docs/configuration.md)。

### 从发布目录运行

Linux 发布目录通常包含：

```text
postui                 # 启动脚本
postui.bin             # Linux amd64 二进制
config.yaml            # 发布目录配置
.postui/config.yaml    # 项目入口配置
.postui/collections/   # 请求集合
docs/                  # 使用和开发说明
test_files/            # 默认上传目录
temp/                  # 默认下载目录，按需创建
```

运行：

```bash
./postui
```

Windows amd64 发布目录使用 `postui.exe`，在 PowerShell 中运行：

```powershell
.\postui.exe
```

安装后可直接执行 `postui`。`postui init` 只修改当前用户的 PATH 或 shell 配置，不写入系统级配置。

## 功能概览

- 请求列表、Collection 切换和会话变量管理。
- Request 与 Response 并排显示，内容在面板内直接编辑。
- Params 和 Headers 使用原生两列表格；最后一行下方的 `+` 方框用于新增，内容过长自动截断。
- `Send` 发送当前请求；当前支持发送 `GET` 和 `POST`，其他方法可以查看但不会发送。
- Response 的 `Actions` 始终可点击并使用焦点高亮；有缓存响应时可以 Download 或 Copy。
- JSON 响应语法高亮，`@extract` 可把成功响应中的字段写入当前集合的会话变量。
- Debug 日志会脱敏 Authorization、Cookie、Token、Secret、Password 和 API key 等字段。

详细内容：

- [配置、请求文件与变量](docs/configuration.md)
- [开发、测试与发布](docs/development.md)
- [仓库结构与提交边界](docs/repository.md)

## 常用检查

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Linux amd64 发布包：

```bash
./package-linux.sh
```

Windows amd64 发布包：

```powershell
.\package-windows.ps1
```

打包脚本读取本机的 `config.yaml` 和 `.postui/`，输出目录被 Git 忽略；发布前请确认没有把个人项目配置复制到公共仓库。
