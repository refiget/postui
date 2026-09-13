# PostUI

这个软件是我在某企畸形的开发环境使用的个人工具, 因为只能用到 `GET` 和 `POST`, 所以我只有这两个功能. Linux 个 windows 都支持(虽然我是Mac)

界面以鼠标操作为主

PostUI 不是 YAML 编辑器。请求与场景始终由配置文件定义；界面中的请求调整只在当前会话生效，不会写回 YAML。按 `R` 可重新加载配置，按 `?` 查看完整快捷键帮助。

## 界面预览

![PostUI 终端界面预览](assets/screenshot.png)

## 构建

### 本地构建

环境：

- 能构建当前源码和锁定依赖的 Rust stable（最低版本声明的限制见[开发指南](docs/development.md#环境)）
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

最小工作区只需要一个请求文件：

```yaml
# .postui/requests/health.yaml
url: https://example.test/health
```

运行 `postui /path/to/project`，或在项目目录及子目录中直接运行 `postui`。

需要多个场景时，共享请求保持一份：

```text
.postui/
├── postui.yaml          # 可选：项目默认值
├── scenarios/           # 可选：场景差异
│   ├── dev.yaml
│   └── test.yaml
└── requests/
    └── health.yaml
```

```yaml
# .postui/postui.yaml
default_scenario: dev
headers:
  Accept: application/json
variables:
  token:
    value:
    secret: true

# 场景变量分别写在 .postui/scenarios/dev.yaml、test.yaml
# variables:
#   host: https://dev-api.example.test
```

```bash
postui /path/to/project --scenario test
```

个人界面偏好与项目分开，完整示例见 [config.example.yaml](config.example.yaml)：

```yaml
language: zh
theme: catppuccin-mocha
max_response_display_bytes: 16777216
```

默认从用户配置目录读取 `postui/config.yaml`，也可使用 `--config <路径>`。显示上限不影响完整响应下载。

响应区支持点击或用左右键切换 Raw / Formatted / Headers。JSON 保留按视口格式化与即时高亮；XML、表单支持美化，其他常见代码格式使用 Syntect 后台分页高亮，HTML 等保留原文排版。复制与下载始终保留原始 Body。可运行 [mock 手工示例](mock/README.md) 体验各类响应。

常用快捷键：`/` 搜索请求（响应区聚焦时搜索响应体），`n`/`N` 跳转响应匹配项，`r` 发送或取消，`R` 重载配置，`w` 切换场景，`v` 打开变量，`o` 打开响应操作，`u` 恢复当前请求，`X` 恢复当前场景全部请求修改，`?` 查看完整帮助。

仅支持新配置规范，不兼容旧 `configs/`、`default_configuration` 或 Header 条目数组。Header 使用映射，重复值使用字符串列表。详细字段、场景合并和保存规则见[配置与请求文件](docs/configuration.md)，可运行示例见[公共 API 双场景示例](examples/public-api/README.md)。

## 参与开发

核心逻辑与 TUI 可分别使用 `cargo check --lib`、`cargo check --bin postui` 检查。完整的模块边界、实现规范和人工验证流程见[开发指南](docs/development.md)；状态所有权与已知限制见[架构说明](docs/code-architecture-audit.md)，提交范围见[仓库约定](docs/repository.md)。开发遵循 [AGENTS.md](AGENTS.md)，不新增自动化测试设施，不提交个人配置或运行产物。
