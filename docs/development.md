# 开发指南

本文面向修改 PostUI 的维护者。用户配置见[配置规范](configuration.md)，模块边界和设计取舍见[架构说明](code-architecture-audit.md)，提交范围见[仓库约定](repository.md)。仓库根目录的 [AGENTS.md](../AGENTS.md) 是开发与验证约束的依据。

## 环境

项目使用 Rust 2024 edition。`Cargo.toml` 声明 `rust-version = "1.85"`，但当前源码包含 let-chain 语法，不能把该声明当成已验证的最低版本保证；发布前需单独核对工具链和锁定依赖。日常使用可构建当前代码的 Rust stable。Linux amd64 静态构建需要 musl 工具链；Windows 构建需要 MSVC、Visual C++ Build Tools 和 Windows SDK；macOS 构建使用 Apple 的系统 SDK。

## 从哪里修改

PostUI 由两个独立的 Cargo target 组成：

- `postui-core`（`src/lib.rs`）负责配置、模板、HTTP 请求执行和响应文档处理。
- `postui`（`src/main.rs`）负责命令行入口、应用状态、终端事件和 TUI 绘制。

只改核心逻辑时可单独检查库目标，只改交互和界面时可单独检查二进制目标：

```bash
cargo check --lib
cargo check --bin postui
```

二进制通过 `postui_core` 使用核心代码；不要在 `src/main.rs` 重新声明核心模块，否则同一份源码会被重复编译成两套类型。完整启动命令保持不变：

```bash
cargo run -- examples/public-api --scenario dev
```

| 修改内容 | 首先阅读 | 同步核对 |
| --- | --- | --- |
| YAML 字段、默认值和场景覆盖 | `src/config.rs`、`src/config/headers.rs` | `template.rs`、`cache.rs`、配置文档和公共示例 |
| 请求草稿与运行状态 | `src/app/session.rs` | `app/editing.rs`、`app/execution.rs`、`app/workspace.rs` |
| 快捷键与请求过滤 | `src/app/input.rs`、`src/app/search.rs` | `app/view.rs`、`ui.rs`、`i18n.rs` 的帮助文案 |
| 容器布局、焦点和鼠标命中 | `src/ui/layout.rs`、`src/ui/focus.rs`、`src/ui.rs` | `app/view.rs`、普通模式与响应放大模式 |
| 请求构造、取消和上传 | `src/template.rs`、`src/http.rs`、`src/request_executor.rs` | `app/execution.rs` 的过期结果校验 |
| 响应展示、搜索、复制和下载 | `src/response_document.rs`、`src/app/response.rs`、`src/ui/response.rs` | `response_action.rs`、`response_output.rs` |
| 变量与敏感信息 | `src/app/variables.rs`、`src/ui/dialog.rs` | `http.rs` 日志脱敏、`config.rs`、`cache.rs` |

## 实现规范

- 先沿调用链确认需求、状态所有者和失败行为，再直接实现；不采用 TDD，不为测试增加接口、状态或抽象层。
- 复用现有职责模块，优先使用私有或 `pub(super)` 接口。只有确实跨模块使用的接口才扩大可见性；不按文件长度机械拆分，不为单一实现引入通用框架。
- 请求列表、预览、响应的局部视图状态分别放在 `ViewState.requests/preview/response`；业务草稿和执行结果留在 `WorkspaceSession`。不要在 UI 再存一份选中请求或响应副本。
- 请求状态只能通过 `RequestRuntimeState` 转换。发送态持有操作 ID，响应态同时持有原始响应和文档，取消回到未发送态；HTTP 成败由响应状态码推导。
- 键盘和鼠标调用同一业务操作。修改焦点时同时检查 Tab、反向 Tab、响应放大和变量页；修改内容高度时使用相同模型更新滚动边界。
- `commit_configuration` 只提交到进程内的场景配置，不写 YAML。恢复、场景切换、重载和退出的语义必须与配置文档一致。文件删除是单独的、需要确认的磁盘操作。
- 校验错误保留文件、字段和可获得的 YAML 行列上下文；不得用静默默认值掩盖非法输入，也不要在日志或错误中新增敏感值回显。
- 改动公共配置结构时核对缓存格式版本；变更依赖时说明实际用途并核对 `Cargo.lock`、支持平台及最低工具链，不顺带升级无关依赖。

## 本地验证

禁止新增函数级自动化测试、测试模块、独立测试文件、runner、快照、Mock 框架和测试结果报告。按改动风险执行最少验证：

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build
```

涉及交互、请求发送或文件操作时，使用独立临时工作区进行一次真实人工流程，不直接修改个人工作区。按本次改动选择流程：

- 状态与请求：发送成功、HTTP 错误、连接失败、发送中取消再发送；切换请求后结果仍归属原请求，已取消或被替代的操作结果不得覆盖新状态。
- 视图：搜索后清除并恢复选择，Tab/反向 Tab，放大后返回，编辑取消，变量页返回。
- 配置：修改 YAML 后 `R`，再输入非法配置并重载，确认旧状态仍可用；恢复临时草稿并切换场景。
- 响应：Raw/Formatted/Headers、鼠标与左右键切换、搜索下一处与上一处、复制或下载；大响应操作期间仍能处理输入。

验证结束后停止自己启动的服务，并清理临时配置、下载、日志和缓存，不新增验证脚本或报告。只在实际检查过的平台上声明验证通过；发布构建另用 `cargo build --release` 或下述打包脚本。

## 公共手工示例

仓库提供可直接使用的[公共 API 双场景示例](../examples/public-api/README.md)，不依赖本地服务：

```bash
cargo run -- examples/public-api --scenario dev
```

需要验证配置编辑、文件上传、大响应或取消请求时，另建临时工作区和本地服务，使用虚构数据。`example-api/` 是 Git 忽略的本地目录，不随仓库分发，也不是构建或发布的前置条件。

## 性能边界

网络请求复用一个 Tokio 运行时及 HTTP 连接池；取消时中止异步任务，不等待网络超时。并发最多 8 个，JSON 提取和响应索引放在阻塞任务池中，避免占用网络执行线程。已开始的阻塞计算不能靠 abort 立即停止，执行槽位保留到计算结束。

JSON 保留原有渲染链路：`response_document/json.rs` 基于原始共享字节建立稀疏索引，只为当前视口生成缩进和 token 样式，首帧即带颜色。它不构造完整 pretty 文本、不经过 serde_json Value 或 Syntect 异步高亮，因此不会改变数字、重复键与转义写法。Raw 使用相同字节的普通文本索引，不重排。JSON 展示扫描器不是严格校验器，无效内容以 Raw 为准。

其他格式独立扩展：`response_format.rs` 使用 quick-xml 和 form_urlencoded；美化输出只保留 `max_response_display_bytes` 允许显示的前缀，不再设置额外的 1 MiB 输入限制。XML 混合文本、CDATA、实体引用、DTD 和 xml:space 保留原文，不做外部实体加载。

XML 和 HTML 不经过 Syntect。`response_document/markup.rs` 参考 Lexilla/Xi 的成熟结构，在线性扫描中建立“虚拟行偏移 + 紧凑词法状态”的稀疏检查点。显示任意位置时从前一个检查点恢复，只为当前视口生成标签、属性、字符串、注释、实体、CDATA 和处理指令的样式。HTML 的 script/style 内容保持原文颜色并寻找对应结束标签，避免其中的 `<` 被识别成 HTML 标签；代码内容本身不做 JavaScript/CSS 深层语法分析。状态机支持标记序列跨越虚拟行边界，索引在响应后台准备线程创建，绘制和随机跳转不等待高亮页。

其余具有成熟语法定义的非 JSON 文本（YAML、JavaScript、CSS、Markdown）使用 Syntect，在后台准备文档后交付界面，首个约 64 KiB 页面提前生成样式。`ResponseHighlightCache::new` 必须在后台调用，不能移入绘制路径。每份文档保留首屏及最多三个最近使用的滚动页面；滚动页按 64 KiB 对齐，通常覆盖 128 KiB，避免视口跨页时频繁等待。这是缓存粒度，不是文件上限，高亮范围受配置的展示字节数约束。未命中时显示高亮准备提示，不先显示无色正文；成功或明确降级后整页呈现。缓存保存的是字节范围与样式，搜索、复制仍使用文档内容，不会搜索或复制等待提示。

Syntect 后续页面由一个后台线程处理，有界任务队列最多 8 项；完成当前页后预取下一页，新需求会中断预取。视口变化使旧任务失效，队列满时让下一次主循环重试，不阻塞 UI。每个活动解析器保留最多 64 个稀疏检查点及起点，以及最近页末逻辑行的解析状态，空闲 5 秒释放解析器。单个逻辑行超过 8192 字节时跳过该行并重置语法状态；片段数超过每 64 KiB 页预算 32768 个（128 KiB 页为 65536 个）或超过 64 层作用域、无效 UTF-8 或语法库错误会降级为原文并缓存该结果，避免永久等待。日志记录降级原因。JSON、XML、HTML 不经过这条链路。YAML、JavaScript、CSS、Markdown 只高亮，不重新排版；Content-Type 优先，仅缺失时尝试识别 JSON。

默认最多展示 16 MiB，接收上限为 64 MiB，分别由 `max_response_display_bytes` 和 `max_response_bytes` 控制。响应搜索在后台分批读取当前页签文档，不构造高亮样式；同一时间只执行一次扫描，连续操作只保留最新待执行搜索。切换文档、页签后丢弃过期搜索结果并重置匹配位置；重载同样在后台构建状态。复制和下载使用原始响应字节，美化副本不参与提取或发送。

大响应不会在 debug 日志中执行完整 Body 脱敏解析。调试日志经有界队列异步写入；队列满、写入失败或退出时可能丢失尾部日志，不保证审计完整性。

维护时不要在绘制函数或输入处理路径新增网络等待、文件读取、完整响应格式化和无界任务创建。共享响应字节，不为展示复制整份 Body；后台结果由主循环应用，不在线程中修改 App。当前仍存在大请求预览克隆、列表过滤扫描等同步工作，不能把“后台执行网络请求”等同于主线程绝不会卡顿；优化应先确认具体输入规模和耗时路径。

## Linux 打包

安装目标并构建发布包：

```bash
rustup target add x86_64-unknown-linux-musl
./package-linux.sh
```

脚本构建二进制并输出到 `打包区/postui-linux-amd64.tar.gz`。项目配置和个人配置不进入软件包。可以指定输出目录：

```bash
./package-linux.sh \
  --output-dir /path/to/output
```

发布包包含二进制、启动脚本和文档。工作区由用户独立维护。

## Windows 打包

在 Windows PowerShell 5.1 或更高版本中运行：

```powershell
rustup target add x86_64-pc-windows-msvc
.\package-windows.ps1
```

脚本会优先使用 PATH 中的 `cargo`，并自动为 MSVC 构建启用静态 CRT。默认输出到 `打包区\postui-windows-amd64.zip`；`-OutputDir` 可以覆盖输出位置。

## macOS 打包

在对应架构的 macOS 上运行：

```bash
rustup target add x86_64-apple-darwin    # Intel
rustup target add aarch64-apple-darwin   # Apple Silicon
./package-macos.sh
```

也可以显式指定 target：

```bash
./package-macos.sh --target x86_64-apple-darwin
./package-macos.sh --target aarch64-apple-darwin
```

脚本默认输出到 `打包区/postui-macos-amd64.tar.gz` 或 `打包区/postui-macos-arm64.tar.gz`；`--output-dir` 可以覆盖输出位置。

## 安装脚本

Linux：

```bash
./install.sh --skip-init
```

macOS：

```bash
./install.sh --skip-init
```

Windows：

```powershell
.\install.ps1 -SkipInit
```

安装脚本只安装程序，不复制或覆盖工作区和用户配置。

## Debug 日志

Debug 构建可以使用：

```bash
cargo run -- --debug
cargo run -- --debug --log-file ./logs/postui-debug.log
```

日志文件默认位于工作区的 `.postui/logs/postui-debug.log`。日志字段限制为 64 KiB，文件达到 8 MiB 后轮转。敏感请求头、查询参数、表单字段和 JSON 值会脱敏，剪贴板内容不会写入日志。

### 手工性能采集

定位大响应卡顿时使用独立的性能模式（debug 构建），不要为了采集耗时开启包含业务内容的完整调试日志：

```bash
cargo run -- /path/to/temporary-workspace --perf --log-file /tmp/postui-perf.log
```

`--perf` 只允许 `postui::perf` 目标的指标，不记录 URL、Header、正文、变量、搜索词或具体按键字符。若同时传入 `--debug`，仍以性能过滤为准。请使用新的日志路径，模式切换不会清除该路径已有的普通 debug 日志。默认日志路径与 `--debug` 相同；日志达到 8 MiB 时保留上一份 `.1` 轮转文件。日志通过容量 256 的队列异步写入，队列溢出会输出 `log_queue_overflow dropped_chunks=...`；日志可能丢失尾部，不是审计记录。不开启日志时不进行磁盘写入。

推荐操作：发送大 XML/HTML → 连续滚动 → 快速拖到远处 → 返回顶部 → 切换 Raw/Formatted → 切换请求。记录大致操作时间，然后查看当前日志及轮转文件。只有同一工作环境中可访问的日志才能由协作者读取；其他机器需要提供日志文件。

| 事件 | 数据与用途 |
| --- | --- |
| `response_prepared` | `response_id`、网络毫秒数、后台排队/提取/文档准备微秒数、响应与展示字节数、行数；区分网络与响应处理耗时 |
| `highlight_first_page` | `cache_id`、语法、首屏准备微秒数、片段数、失败原因；在 `response_prepare` span 中关联响应编号 |
| `markup_index` | XML/HTML 展示字节数、虚拟行数、状态检查点数和完整索引耗时 |
| `highlight_request` | 页范围、命中状态、累计命中/未命中次数、缓存页数；定位预取不足和缓存抖动，次数按视图查询计，不按用户动作计 |
| `highlight_scan` | 扫描起点、实际扫描字节、检查点数、跳过的长行数；判断远跳是否重复扫描 |
| `highlight_page` | 页范围、排队/解析微秒数、是否预取/过期、片段数、降级原因；区分调度等待与解析成本 |
| `highlight_visible` / `highlight_progress` | 当前页面从缓存未命中到 UI 取得完整页面的等待微秒数；长扫描每秒记录一次字节进度 |
| `highlight_stale_queue` / `highlight_queue_full` | 旧任务丢弃、队列拥塞重试 |
| `highlight_cache_released` | 文档缓存释放时的累计命中/未命中次数 |
| `ui_frames` / `ui_slow_frame` | 有绘制活动时约每秒汇总帧数、平均/最大绘制微秒数；单帧达到 16 ms 单独记录，含终端输出耗时，不等同于屏幕实际呈现延迟 |
| `ui_slow_event` | 输入处理达到 4 ms 的耗时及事件类别，不记录输入内容；不包含事件进入操作系统队列前后的等待 |

先观察慢帧是否出现，再结合解析、预取和排队指标定位。不通过调低接收上限掩盖问题，也不在 UI 中等待解析。debug 日志和未优化构建会影响绝对耗时，最终性能结论还需要 release 构建的真实操作确认。

Debug 模式中按 `F5` 会按内置主题顺序即时切换。该操作只修改运行时状态，不写入个人配置；切换不会产生额外提示，重新启动即可恢复配置主题。

## 发布前检查

发布或提交前，确认根目录的个人配置没有进入变更：

```bash
git status --short
git diff --check
git check-ignore -v .postui 打包区
```

工作区可能包含内部地址或凭据，提交前应单独检查 `.postui/postui.yaml` 和请求文件。
