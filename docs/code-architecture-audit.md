# PostUI 代码与依赖审计

审计日期：2026-09-12

## 结论

项目当前能够通过：

```sh
cargo clippy --all-targets -- -D warnings
```

核心问题不在基本可用性，而在请求模型、HTTP 客户端生命周期和 `App` 状态组织。阶段一和阶段二已经完成低风险依赖整理，后续重点是请求状态模型和职责拆分：

1. [已完成] 复用 `reqwest::blocking::Client`，并整理缓存 feature 与指纹计算。
2. [已完成] 使用 `url` 和 `form_urlencoded` 处理 query、fragment 与表单编码。
3. [已完成] 用 `serde-saphyr` 替换已经停止维护的 `serde_yaml`。
4. 合并请求配置、编辑态和运行态，消除平行状态容器。
5. 按职责拆分 2077 行的 `App`。
6. 最后评估剪贴板和文本编辑器库。

## 值得使用现成库替换的实现

### URL 和 query 处理

阶段二已将以下标准协议逻辑交给现成库处理：

- `src/template.rs` 中的 URL query/fragment 拼接和 `application/x-www-form-urlencoded` 编解码。
- `src/app.rs` 中的 URL/query 分割与重组现在通过 `template` 的 URL 工具完成。

URL 能够正常解析时使用 `url::Url`，包含 `{{variable}}` 的原始模板保留文本回退路径，变量展开后再由 `Url` 处理。表单参数使用 `form_urlencoded`，因此空格、加号、非 ASCII 字符和保留字符遵循标准表单编码规则。

直接依赖 `url = "2.5"` 和 `form_urlencoded = "1.2"`，使用：

- `Url`
- `query_pairs()` / `query_pairs_mut()`
- `form_urlencoded`

`reqwest` 已间接依赖 `url`，增加直接依赖不会引入另一套 URL 实现。

包含 `{{variable}}` 的原始 URL 在变量展开前可能不是合法 URL，因此应保留以下边界：

- 配置和编辑层继续保存原始字符串。
- 变量解析完成后再构造 `Url`。
- UI 展示原始模板时不强制解析。

阶段二已完成。后续请求模型改造仍需保持原始模板 URL 与已解析 URL 的边界。

### YAML 解析

项目此前直接依赖：

```toml
serde_yaml = "0.9"
```

`serde_yaml 0.9.34+deprecated` 已停止维护。当前 YAML 调用面很小，主要位于：

- `src/config.rs`
- `src/settings.rs`

项目只需要将 YAML 反序列化到明确的数据结构，现已使用固定版本 `serde-saphyr = 1.1.0`，并关闭序列化 feature。固定 1.1.0 是因为项目仍声明支持 Rust 1.85，而 1.2.0 将最低 Rust 版本提高到 1.89。不迁移到 `serde_yml`，因为它也已经弃用。

阶段二已完成。

### 文本编辑器

`src/editor.rs` 中的 `TextEditor` 是轻量单行编辑器，目前能够维护 UTF-8 字节边界，但不处理完整 grapheme，并缺少：

- 组合字符和 emoji 序列级别的移动、删除。
- 选区。
- 撤销和重做。
- 单词跳转。
- 原生粘贴处理。
- 独立横向滚动状态。

可考虑：

- 表格内单行字段使用 `tui-input`。
- 将来出现完整 body 编辑器时使用 `tui-textarea`。
- 当前 JSON 标量点击编辑暂时保留现有实现。

不建议现在直接使用多行编辑组件替换所有单行输入。

优先级：中低。

### 剪贴板

`src/clipboard.rs` 通过外部命令访问剪贴板：

- Windows：PowerShell 或 `clip`。
- Wayland：`wl-copy`。
- X11：`xclip` 或 `xsel`。
- macOS：`pbcopy`。

这意味着 Linux 二进制不是严格的零外部运行依赖。可以使用 `arboard` 统一接口，但会增加平台相关构建依赖。

建议根据交付目标选择：

- 追求较小依赖面和简单构建：保留当前实现。
- 追求下载后剪贴板必然可用：评估 `arboard`。
- 即使保留当前实现，也应考虑启动时探测一次后端，而不是每次复制都依次尝试。

优先级：低。

## 已使用库但用法需要优化

### 复用 reqwest Client

`src/http.rs` 的 `send` 每次请求都会执行 `Client::builder().build()`。`reqwest::Client` 内部维护连接池，官方建议创建后复用。

当前实现无法充分利用：

- TCP keep-alive。
- TLS 会话和连接复用。
- 连接池。
- 部分 DNS 和代理相关资源复用。

建议建立具体类型：

```rust
struct HttpClient {
    regular: reqwest::blocking::Client,
    no_proxy: reqwest::blocking::Client,
}
```

请求超时通过 `RequestBuilder::timeout()` 设置，不需要为不同 timeout 重建 Client。本地地址使用 `no_proxy` Client，其他地址使用普通 Client。

该类型应由 `App` 或独立请求执行器长期持有。当前没有多种传输实现，不需要额外创建 `HttpTransport` trait。

优先级：最高。

### cacache feature 和 fingerprint

当前配置为：

```toml
cacache = { version = "13.1", default-features = false, features = ["tokio-runtime"] }
```

`src/cache.rs` 实际只使用：

- `cacache::read_sync`
- `cacache::write_sync`

同步 API 不需要 `tokio-runtime`。但 `cacache 13.1.0` 在关闭所有异步 feature 时会因其内部 `update_state` 的条件编译缺陷而无法编译，因此实际采用 `async-std` 作为编译所需的异步实现，并保留 `mmap`：

```toml
cacache = { version = "13.1", default-features = false, features = ["async-std", "mmap"] }
```

PostUI 仍只调用 `read_sync` 和 `write_sync`，不会启动 async-std runtime。这样会移除 `cacache` 对 Tokio 和 tokio-stream 的依赖；`Cargo.lock` 中若仍有 Tokio，是 `reqwest` 等其他依赖带入的。变更后重新生成 `Cargo.lock`，并验证 Linux 和 Windows 构建。

另外，`src/config.rs` 当前先将全部配置和请求内容拼接进一个 `Vec<u8>`，再计算 BLAKE3。更合理的方式是逐段写入 `blake3::Hasher`，直接得到 digest，避免工作区较大时额外复制全部内容。

对只有一个 workspace 配置缓存对象的场景，`cacache` 功能略重。但项目要求使用正式持久缓存机制，因此当前可以保留，不建议再次更换。

优先级：高。

### ratatui-interact 使用范围过窄

`ratatui-interact` 当前基本只用于 Button。按钮的鼠标命中、focus、disabled 判断和 action 分发仍主要由项目自己维护。

可选择：

1. 扩大组件库的使用范围，让它统一管理交互状态。
2. 删除该依赖，使用 Ratatui 的 `Block`/`Paragraph` 渲染按钮，并保留现有命中处理。

考虑当前项目规模，更倾向第二种，但这不是近期高优先级事项。

## 抽象不足

### App 承担过多职责

`src/app.rs` 共 2077 行，当前同时负责：

- 当前请求选择。
- 编辑器生命周期。
- dialog 行为。
- dirty 状态。
- 文件保存和删除。
- HTTP 请求调度。
- 后台线程消息。
- response extract。
- 下载与复制。
- 键盘快捷键。
- 请求序列化。

建议逐步形成以下结构：

```text
App
├── WorkspaceSession
│   ├── requests
│   ├── variables
│   └── selected_request
├── EditorState
│   ├── inline editors
│   └── dialogs
├── RequestExecutor
│   ├── reusable HttpClient
│   ├── active operations
│   └── result channel
└── UiState
    ├── focus
    ├── tabs
    ├── scroll
    └── response menu
```

这里需要的是职责划分，不是动态多态。不要先创建大量 trait。

### 请求状态由多个平行容器维护

当前 `WorkspaceState` 同时包含：

```rust
request_edits: HashMap<String, RequestEdits>
request_states: HashMap<String, RequestRuntimeState>
dirty_requests: HashSet<String>
```

请求配置本体又位于 `App.config.requests`。这依赖隐含不变量：所有容器必须永远与请求列表同步。代码中多处 `expect("every configured request...")` 是该问题的表现。

建议改为：

```rust
struct RequestSession {
    source: ApiRequest,
    draft: RequestDraft,
    runtime: RequestRuntimeState,
    dirty: bool,
}

struct WorkspaceSession {
    requests: Vec<RequestSession>,
    selected: Option<usize>,
    variables: BTreeMap<String, String>,
}
```

这样可以消除：

- 大量 ID 查找和 clone。
- 多个容器的同步风险。
- 删除请求后的残留状态。
- 多处不变量 `expect`。
- 到处临时拼装 effective request。

优先级：最高。

### 使用空请求 sentinel

`App` 保存 ID 为 `__empty__` 的 `empty_request`，无请求时 `current_request()` 返回该伪对象。调用方必须同时记住先调用 `has_current_request()`，否则伪请求可能进入编辑、状态查询或发送流程。

建议改为：

```rust
fn current_request(&self) -> Option<&RequestSession>
```

UI 已有空列表分支，没有继续维护 Null Object 的必要。

### 持久化逻辑位于 App

以下行为不应由 `App` 直接承担：

- 请求文件定位。
- 写入请求文件。
- 删除请求文件。
- 序列化 curl 请求。

建议建立具体模块：

```text
src/workspace.rs
src/request_file.rs
```

对外提供：

```rust
load_workspace(...)
save_request(...)
delete_request(...)
serialize_request(...)
```

当前没有多种存储实现，不需要 Repository trait。

### Header 和 Form 模型不能表达重复字段

请求头、form 字段和解析后的请求大量使用 `BTreeMap<String, String>`，无法完整表达：

- 重复 Header。
- 重复 form key。
- 顺序敏感的参数。
- 大小写不同但语义相同的 Header。

UI 编辑层已经使用 `Vec<HeaderRow>`，但保存和发送时又压回 Map，可能造成信息损失。

建议领域模型统一为有序列表，例如：

```rust
struct NameValue {
    name: String,
    value: String,
}
```

请求头发送阶段再转换为 `HeaderMap`，并明确哪些 Header 允许重复。该修改会影响配置格式，应作为单独任务实施。

## 可能的过度抽象或过度设计

### 不要把 HeadersDialog 和 ParamsDialog 合并成泛型框架

两者在 selected、field、editor、add/remove 和左右切换方面存在重复，但业务规则不同：

- Header 有 collection/request source 和 enabled。
- Params 有 URL/query/form/body source。
- Params 还维护 `part_type` 和 `has_equals`。

可以抽取小型编辑原语，例如：

```rust
struct CellSelection {
    row: usize,
    column: Column,
}
```

不建议创建 `EditableTable<T, Adapter, Validator, Renderer>` 一类泛型系统，否则明确的领域分支会变成间接调用。

### HttpError 不需要为少量样板引入 thiserror

`HttpError` 只有两个 variant，当前手动 `Display` 和 `Error` 实现简单明确。仅为了减少十几行代码将 `thiserror` 变成直接依赖，收益有限。

### CLI 不值得引入 clap

当前命令只有：

- `init`
- `--debug`
- `--log-file`
- 可选项目路径

现有参数解析较清晰，错误信息经过中文定制。引入 `clap` 会增加编译量和宏依赖。应拆分 `main.rs` 的 shell 安装逻辑，而不是更换 CLI 库。

### 目录递归不必换 walkdir

当前递归实现很短，并且明确只读取普通文件和目录、不跟随符号链接。换用 `walkdir` 只能减少少量代码，同时还需重新确认 symlink、排序和错误上下文语义。

### 响应文件名清洗暂时不必引库

`src/response_output.rs` 包含扩展名判断、Content-Disposition 提取和 Windows 保留名过滤。当前 Content-Disposition 解析并不完整，但功能范围有限。

仅为文件名清洗引入完整 MIME/Content-Disposition 依赖暂时不划算。将来需要支持 RFC 5987 `filename*=` 时，再引入正式 parser。

## 推荐实施顺序

### 第一批：低风险、高收益

1. [已完成] 复用普通和 no-proxy 两个 `reqwest::Client`。
2. [已完成] 删除 `cacache` 的 `tokio-runtime` feature。
3. [已完成] fingerprint 改为流式 BLAKE3。
4. [已完成] 迁移 `serde_yaml`。
5. [已完成] 引入 `url`，替换 URL/query 编解码。

### 第二批：核心结构调整

1. 建立 `RequestSession`。
2. 消除三张 `HashMap`/`HashSet` 平行状态。
3. 删除 `empty_request`。
4. 将请求文件读写和序列化移出 `App`。
5. 将请求执行器移出 `App`。

### 第三批：模型完善

1. Header 改为有序、可重复结构。
2. Form 参数改为有序、可重复结构。
3. 统一 URL query、curl data 和 form 的数据语义。
4. 再评估 `tui-input` 和剪贴板库。

不建议一次性进行框架化重写。先解决 HTTP Client 生命周期和请求状态模型，项目复杂度会自然下降，再进行模块拆分。

## 参考资料

- [url crate](https://docs.rs/url/latest/url/)
- [form_urlencoded crate](https://docs.rs/form_urlencoded/latest/form_urlencoded/)
- [reqwest blocking Client](https://docs.rs/reqwest/latest/reqwest/blocking/struct.Client.html)
- [serde_yaml 维护状态](https://docs.rs/serde_yaml/latest/serde_yaml/)
- [serde-saphyr 1.1.0](https://docs.rs/serde-saphyr/1.1.0/serde_saphyr/)
- [serde_yml 迁移说明](https://docs.rs/serde_yml/latest/serde_yml/)
- [tui-textarea](https://docs.rs/tui-textarea/latest/tui_textarea/)
- [Ratatui user input 示例](https://ratatui.rs/examples/apps/user_input/)
