# 仓库结构与提交边界

仓库只提交可复现的程序、公共示例和测试夹具。个人项目的地址、请求、账号、令牌、上传文件和发布产物保留在本机。

## 目录约定

```text
src/                 Rust 源码
mock/                FastAPI mock、公共请求夹具和测试上传文件
test_files/          用户上传目录，只保留占位说明
docs/                使用、配置、开发和发布说明
config.example.yaml  不含个人信息的全局配置模板
install.sh           Linux 安装脚本
install.ps1          Windows 安装脚本
package-linux.sh     Linux amd64 打包脚本
package-windows.ps1  Windows amd64 打包脚本
```

以下内容属于本机数据，已通过 `.gitignore` 排除：

| 路径 | 原因 |
| --- | --- |
| `config.yaml` | 个人全局配置，可能包含项目路径和接口入口 |
| 根目录 `.postui/` | 个人请求集合、变量和项目入口配置 |
| 根目录 `test_files/` 中的真实文件 | 用户上传内容 |
| 根目录 `themes/` | 本地主题配置 |
| `打包区/` | 可能包含个人配置的发布目录和二进制 |
| `target/`、`.venv/`、`.codegraph/`、日志和缓存 | 构建或运行时产物 |

`mock/.postui/` 和 `mock/test_files/` 是公共测试夹具，不属于个人项目；其中的 token、Cookie 和文件内容都是假的测试值。

## 提交前检查

```bash
git status --short
git diff --check
git check-ignore -v config.yaml .postui themes 打包区
```

再检查变更中没有个人地址或凭据。请求示例使用 `example.test`、`127.0.0.1` 和模板变量；不要把真实内网地址、用户目录、密码、Token 或业务文件复制到 `src/`、`mock/`、`docs/` 或配置模板中。

## 公共配置原则

- 新增可复用配置时放入 `config.example.yaml`，使用示例域名和模板变量。
- 个人配置放在根目录 `config.yaml` 或根目录 `.postui/`，不要为了方便把它们强制加入 Git。
- 新增测试请求时使用 `mock/.postui/requests/`，并只依赖 `mock/test_files/` 中的公共夹具。
- 缓存、日志、截图和临时下载文件不应作为源码或测试输入提交。
