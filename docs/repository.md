# 仓库结构与提交边界

仓库只提交可复现的程序和公共示例。个人项目的地址、请求、账号、令牌、上传文件和发布产物保留在本机。

## 目录约定

```text
src/                 Rust 源码
example-api/         本地接口服务与请求集合（Git 忽略）
test_files/          用户上传目录，只保留占位说明
docs/                使用、配置、开发和发布说明
config.example.yaml  不含个人信息的全局配置模板
install.sh           Linux/macOS 安装脚本
install.ps1          Windows 安装脚本
package-linux.sh     Linux amd64 打包脚本
package-windows.ps1  Windows amd64 打包脚本
package-macos.sh     macOS Intel/Apple Silicon 打包脚本
```

以下内容属于本机数据，已通过 `.gitignore` 排除：

| 路径 | 原因 |
| --- | --- |
| [平台标准配置目录](configuration.md#个人偏好)中的个人配置 | 个人语言和主题偏好 |
| 根目录 `.postui/` | 个人工作区、变量和请求配置 |
| 根目录 `test_files/` 中的真实文件 | 用户上传内容 |
| 根目录 `themes/` | 本地主题配置 |
| `打包区/` | 可能包含个人配置的发布目录和二进制 |
| `target/`、`.venv/`、`.codegraph/`、日志和缓存 | 构建或运行时产物 |

`example-api/` 是本地手工验证区，整个目录由 Git 忽略。需要共享的示例不能依赖该目录，应放入明确受版本控制的文档或模板中。

## 提交前检查

```bash
git status --short
git diff --check
git check-ignore -v .postui 打包区
```

再检查变更中没有个人地址或凭据。受版本控制的请求示例使用 `example.test`、`127.0.0.1` 和模板变量；不要把真实内网地址、用户目录、密码、Token 或业务文件复制到 `src/`、`docs/` 或配置模板中。

## 公共配置原则

- 新增可复用配置时放入 `config.example.yaml`，使用示例域名和模板变量。
- 个人界面配置放在系统用户配置目录，不要加入仓库。
- 本地请求放在 `example-api/.postui/requests/`；需要纳入仓库的示例应另建明确的公共模板，不能通过取消忽略规则提交本地目录。
- 缓存、日志、截图和临时下载文件不应提交。
