# 公共手工接口示例

这些接口只用于手工体验 PostUI，不是自动化测试或真实业务服务。请只监听本机地址，不上传个人文件或填入真实令牌。

在仓库根目录运行：

```bash
python3 -m venv .venv
.venv/bin/python -m pip install -r mock/requirements.txt
.venv/bin/python -m uvicorn main:app --app-dir mock --host 127.0.0.1 --port 18080
```

另一终端运行 `cargo run -- mock --scenario test`。接口参数说明可在本机浏览器打开 `http://127.0.0.1:18080/docs`。

## 响应展示

请求列表的 `responses/` 分组对应 `GET /v1/responses/{response_format}`：

| 参数 | 用途 |
| --- | --- |
| `json` | 紧凑 JSON、中文、嵌套结构、布尔值、null、大整数和高精度小数 |
| `invalid-json` | 无效 JSON 的容错展示；Raw 查看原始内容，展示扫描器不代替严格校验 |
| `xml` | XML 缩进与语法高亮 |
| `mixed-xml` | 混合文本保持原样，避免插入空白改变含义 |
| `invalid-xml` | 标签不匹配时回退原文 |
| `html` | HTML 高亮、跨行注释及 pre 空白保留；不执行网页脚本 |
| `form` | 表单解码，保留重复键、空值、中文；控制字符使用 JSON 转义 |
| `yaml`、`javascript`、`css`、`markdown` | 原文排版与 Syntect 语法高亮 |
| `plain` | 保留空格、中文和看起来像 JSON 的纯文本 |
| `binary` | 二进制只展示摘要，通过操作菜单下载原始字节 |

发送后点击页签，或聚焦响应区按 `←` / `→`，在 Raw、Formatted、Headers 间切换。Raw 和 Formatted 均可用 `/` 搜索、`n` / `N` 跳转；切换页签清除旧的匹配位置。使用 `o` 下载或复制，得到的始终是原始 Body，不是美化结果。

JSON 使用原有按视口格式化和即时高亮，不因超过 1 MiB 或 64 KiB 而降级。接收上限默认 64 MiB，显示上限默认 16 MiB，均可配置。其他格式使用后台分页高亮，约 64 KiB 一页；超长单行或复杂语法可局部降级。

## 其他接口

顶层请求示例涵盖变量提取、请求头、表单、文件上传、重定向、HTTP 错误、空响应和超时。文件上传需要自行准备非敏感示例文件，不要提交上传文件或下载产物。

`GET /v1/large-response` 用于大响应与慢速分块：`format=json|plain|xml|html`，`count=1..100000`，`size_kb=1..65536`，`delay_ms=0..1000`。JSON 仍按 `count` 生成，忽略 `size_kb`；纯文本、XML 和 HTML 按 `size_kb` 输出精确字节数，忽略 `count`。XML 和 HTML 流式重复完整元素，不截断标签或 UTF-8 字符，服务端无需构造整个响应。

`responses/14-large-xml.yaml` 和 `15-large-html.yaml` 默认请求 64 MiB，超时 120 秒。可将 `size_kb` 改为 `1024`（1 MiB）、`16384`（16 MiB）或 `65536`（64 MiB），再按 `R` 重载。`delay_ms=50` 可体验慢速接收并按 `r` 取消。服务端代码修改后需重启 mock API。

手工检查接收大小、Raw / Formatted 切换、放大、滚动、搜索及取消。XML 支持缩进，HTML 保持原有排版并高亮；默认显示上限仍为 16 MiB，收到 64 MiB 不代表会显示全部内容，XML 缩进后的大小也可能更大。大 JSON 的原有缩进和即时颜色不变。`GET /v1/delay/{seconds}` 也可用于发送中取消。

结束后停止服务；`mock/.postui/cache/`、`mock/temp/`、`.venv/` 和日志均不应提交。
