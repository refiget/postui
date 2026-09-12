# 公开 Echo API 双场景示例

这是一个不包含账号、令牌或个人地址的临时项目，用来直观看到同一请求在多个环境之间复用。

## 场景

- `dev`：读取 `.postui/configs/dev.yaml`，使用 `https://httpbin.org`
- `test`：读取 `.postui/configs/test.yaml`，使用 `https://postman-echo.com`

两个场景都提供 `/get` 和 `/post` Echo 接口，因此请求文件不需要复制。场景只改变 `api_base`、`scenario` 等变量；`02-json-echo.yaml` 还演示了 `test` 专属的接口级 Header 覆盖。

## 启动

在仓库根目录执行：

```bash
cargo run -- examples/public-api
```

也可以进入本目录后直接启动已安装的 `postui`：

```bash
cd examples/public-api
postui
```

启动后点击左侧 `Workspace` 下拉框（或按 `w`）切换 `dev`/`test`，选择请求并按 `Enter` 发送。该示例需要网络访问公开 API；服务响应、网络代理或限流可能影响结果。
