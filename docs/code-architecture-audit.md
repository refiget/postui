# PostUI 代码与依赖审计

审计日期：2026-09-12

## 结论

项目当前能够通过：

```sh
cargo clippy --all-targets -- -D warnings
```

核心问题不在基本可用性，而在请求模型、HTTP 客户端生命周期和 `App` 状态组织。阶段一至阶段八已经完成依赖整理、请求状态收敛、第一轮职责拆分、请求字段模型整理、输入基础设施收敛、参数语义统一和按钮渲染收敛：

1. [已完成] 复用 `reqwest::blocking::Client`，并整理缓存 feature 与指纹计算。
2. [已完成] 使用 `url` 和 `form_urlencoded` 处理 query、fragment 与表单编码。
3. [已完成] 用 `serde-saphyr` 替换已经停止维护的 `serde_yaml`。
4. [已完成] 合并请求配置、编辑态和运行态，消除平行状态容器。
5. [阶段 4 已完成第一轮] 按职责拆分 1980 行的 `App`。
6. [阶段 5 已完成] 将 Header/Form 字段统一为有序、可重复的条目模型。
7. [阶段 6 已完成] 收敛单行编辑器的 Unicode 边界，并使用正式剪贴板库。
8. [阶段 7 已完成] 统一 URL query、curl data 和 form 的参数表示与编码路径。
9. [阶段 8 已完成] 删除仅用于 Button 渲染的 `ratatui-interact`，改用 Ratatui 原生组件。
10. [阶段 9 已完成] 将请求会话模型和运行态状态转换移出 `App`，集中到会话模块。

## 值得使用现成库替换的实现

### URL 和 query 处理

阶段二已将以下标准协议逻辑交给现成库处理：

- `src/template.rs` 中的 URL query/fragment 拼接和 `application/x-www-form-urlencoded` 编解码。
- `src/app.rs` 中的 URL/query 分割与重组现在通过 `template` 的 URL 工具完成。

URL 能够正常解析时使用 `url::Url`，包含 `{{variable}}` 的原始模板保留文本回退路径，变量展开后再由 `Url` 处理。表单参数使用 `form_urlencoded`，因此空格、加号、非 ASCII 字符和保留字符遵循标准表单编码规则。

直接依赖 `url = "2.5"` 和 `form_urlencoded = "1.2"`，使用：

- `Url`
- `Url` 的 URL 结构解析和 query 写回。
- `form_urlencoded` 的参数组件编码与解码。

阶段七将 URL 中的 query、`--get` 携带的 data 和 URL 编码 data 统一为 `RequestParam`；原始 data 仍由 `DataPart::Raw` 保留。参数编辑器现在使用同一套 name/value/等号语义，保存时集中编码，发送时集中展开变量并编码。

`reqwest` 已间接依赖 `url`，增加直接依赖不会引入另一套 URL 实现。

包含 `{{variable}}` 的原始 URL 在变量展开前可能不是合法 URL，因此应保留以下边界：

- 配置和编辑层继续保存原始字符串。
- 变量解析完成后再构造 `Url`。
- UI 展示原始模板时不强制解析。

阶段二和阶段七已完成。请求模型仍保持原始模板 URL 与已解析 URL 的边界，但不会再在 App、curl 解析器和编辑器之间重复拆分参数。

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

`src/editor.rs` 中的 `TextEditor` 仍是轻量单行编辑器，但编辑边界已经交给 `unicode-segmentation`：

- 方向移动和删除按 grapheme 处理，不会把组合字符或 emoji 序列拆开。
- Ctrl/Alt 单词移动和删除使用 Unicode word boundary。
- 光标仍以 UTF-8 字节偏移保存，渲染时单独计算终端显示列。

`tui-input` 已完成评估但没有接入：0.15.x 要求 `unicode-width >= 0.2.2`，而项目当前的 `ratatui 0.29` 固定使用 `unicode-width = 0.2.0`；0.14.x 又不提供完整 grapheme 编辑。为此不升级整套 Ratatui 依赖，而是在本地编辑器适配层直接使用 `unicode-segmentation`。当前仍未实现选区、撤销/重做和多行编辑，这些不属于配置表单的必要能力。

当前 JSON 标量、URL、Header、Params 和文件路径继续共用这一单行编辑器；将来出现完整 body 编辑器时再单独评估 `tui-textarea`。

阶段六已完成。优先级：中低。

### 剪贴板

`src/clipboard.rs` 现在通过 `arboard 3.6.1` 访问系统剪贴板，并关闭图像 feature、启用 Linux Wayland data-control feature：

- Windows 使用原生剪贴板 API。
- Linux 使用 X11，并在可用时使用 Wayland data-control。
- 正常桌面环境不依赖命令搜索路径；无图形剪贴板环境时保留 `wl-copy`、`xclip`、`xsel` 或 Windows `clip` 作为兼容 fallback，适配 SSH/终端转发场景。

`ClipboardService` 在首次复制时初始化，并由 `App` 持有到退出。这样符合 Linux 剪贴板由写入进程托管的生命周期要求；没有图形剪贴板环境时，复制会返回明确错误，不影响请求和 TUI 启动。

代价是增加 X11/Wayland 和 Windows 平台构建依赖，但换来了统一 API；`arboard` 失败时的 fallback 保留了无图形环境下的终端剪贴板能力。阶段六已完成。优先级：低。

## 已使用库但用法需要优化

### 复用 reqwest Client

`src/http.rs` 的 `HttpClient` 长期持有普通代理和 no-proxy 两个 `reqwest::blocking::Client`。`RequestExecutor` 持有这个客户端，并将它的 clone 交给后台请求线程。`reqwest::Client` 内部维护连接池，因而可以复用：

请求超时继续通过 `RequestBuilder::timeout()` 设置，不需要为不同 timeout 重建 Client。本地地址使用 no-proxy Client，其他地址使用普通 Client。

阶段一和阶段四已完成。现在仍然保留两个客户端，是因为本地 mock/API 与普通外部 API 的代理策略不同。这样可以继续利用：

- TCP keep-alive。
- TLS 会话和连接复用。
- 连接池。
- 部分 DNS 和代理相关资源复用。

```text
App
└── RequestExecutor
    ├── HttpClient
    ├── operation id
    └── result channel
```

当前实现不再需要让 `App` 直接管理线程和 channel。

历史上的建议类型如下，已经落地为 `src/http.rs` 中的具体实现：

```rust
struct HttpClient {
    regular: reqwest::blocking::Client,
    no_proxy: reqwest::blocking::Client,
}
```

该类型由独立请求执行器长期持有。当前没有多种传输实现，不需要额外创建 `HttpTransport` trait。

优先级：最高，已完成。

### cacache feature 和 fingerprint

当前配置为：

```toml
cacache = { version = "13.1", default-features = false, features = ["async-std", "mmap"] }
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

### ratatui-interact（阶段 8 已移除）

`ratatui-interact` 之前基本只用于 Button。按钮的鼠标命中、focus、disabled 判断和 action 分发仍主要由项目自己维护，因此这层依赖没有提供完整的交互抽象。

阶段八已完成：

- 使用 Ratatui 的 `Block`/`Paragraph` 渲染单行和块按钮。
- 将 enabled、focused 和按钮样式直接传给渲染函数。
- 保留现有 `UiLayout` 命中区域和 `App` 动作分发，不引入新的组件状态机。

这样减少了一层只有渲染用途的依赖，同时不会改变键盘、鼠标、焦点和禁用态逻辑。优先级：已完成。

## 抽象不足

### App 承担过多职责

`src/app.rs` 当前约 1800 行，仍负责：

- 当前请求选择。
- 编辑器生命周期。
- dialog 行为。
- dirty 状态。
- response extract。
- 下载与复制。
- 键盘快捷键。
- 请求发送前的校验与结果应用。

阶段四完成后的职责边界如下：

```text
App
├── WorkspaceSession
│   ├── requests
│   ├── variables
│   └── selected_request
├── RequestFileStore
│   ├── request path validation
│   ├── save/delete
│   └── curl serialization
├── RequestExecutor
│   ├── reusable HttpClient
│   ├── operation id
│   └── result channel
└── UI and business state
    ├── editors and dialogs
    ├── response state
    └── keyboard/mouse orchestration
```

阶段九继续收拢了会话层，但没有把界面流程泛化成 trait 或状态机：

- `src/app/session.rs` 负责 `RequestDraft`、`RequestSession` 和 `WorkspaceSession`。
- `RequestStatus` 和 `RequestRuntimeState` 与请求会话放在同一模块，保持运行态字段的所有权集中。
- 有效请求的 Header 合并由会话模型完成，请求运行态的开始、成功结束和失败结束由运行态对象统一迁移。
- `App` 仍负责变量提取、状态提示和事件编排，这些行为需要同时协调工作区与 UI，因此没有继续下沉。

这里需要的是职责划分，不是动态多态。不要先创建大量 trait。阶段四已经将 `RequestFileStore` 和 `RequestExecutor` 接入 `App`；结果如何写入 `RequestSession` 仍由 `App` 编排。

### 请求状态由多个平行容器维护

阶段 3 已完成。运行时现在由 `WorkspaceSession` 管理请求集合和选中索引，每个 `RequestSession` 同时持有源配置、草稿、运行态和 dirty 标记：

```rust
struct RequestSession {
    source: ApiRequest,
    draft: RequestDraft,
    runtime: RequestRuntimeState,
    dirty: bool,
}

struct WorkspaceSession {
    requests: Vec<RequestSession>,
    selected_request: Option<usize>,
    variables: BTreeMap<String, String>,
}
```

这样已经消除了：

- 大量 ID 查找和 clone。
- 多个容器的同步风险。
- 删除请求后的残留状态。
- 多处不变量 `expect`。
- 到处临时拼装 effective request。

优先级：最高，已完成。

### 使用空请求 sentinel

阶段 3 已移除 ID 为 `__empty__` 的 `empty_request`。当前请求返回 `Option`，无请求时由 UI 空列表分支处理，编辑、发送、响应状态和下载操作都会安全退出。

```rust
fn current_request(&self) -> Option<&ApiRequest>
```

UI 已有空列表分支，不再维护 Null Object。

### 持久化逻辑位于 App

以下行为不应由 `App` 直接承担，阶段四已完成：

- 请求文件定位。
- 写入请求文件。
- 删除请求文件。
- 序列化 curl 请求。

具体实现位于：

```text
src/request_file.rs
```

`RequestFileStore` 负责：

```rust
RequestFileStore::save(...)
RequestFileStore::delete(...)
```

保存使用临时文件再替换目标文件，并对请求 ID 做相对路径校验；Windows 下会先移除目标文件，保证已有请求可以再次保存。序列化函数保持在该模块内部，不向 `App` 暴露格式细节。当前没有多种存储实现，不需要 Repository trait。

### 请求执行职责位于 RequestExecutor

阶段四已将以下内容移到 `src/request_executor.rs`：

- HTTP 客户端生命周期。
- 操作 ID 生成。
- 后台线程创建。
- 请求结果 channel。

`App` 只准备已解析请求、更新 `RequestSession.runtime`，并在主循环中消费 `RequestResult`。过期结果校验、response extract 和状态消息仍属于界面业务流程，因此保留在 `App`。

### Header 和参数模型（阶段 5/7 已完成）

此前请求头、form 字段和解析后的请求大量使用 `BTreeMap<String, String>`，无法完整表达：

- 重复 Header。
- 重复 form key。
- 顺序敏感的参数。
- 大小写不同但语义相同的 Header。

UI 编辑层已经使用 `Vec<HeaderRow>`，但保存和发送时又压回 Map，造成了信息损失。

当前领域模型已经统一为有序列表：

```rust
struct NameValue {
    name: String,
    value: String,
}
```

Header 继续使用 `NameValue`，而 URL query、URL 编码 data 和 Form 使用有序的 `RequestParam`；`DataPart` 只表示原始文本或 URL 编码参数。请求头发送阶段逐条交给 reqwest，Form 逐条加入 multipart，URL query 和 data 由同一套参数编码器生成；因此重复字段和原始顺序不会在中间层丢失。工作区配置的 `headers` 现在是条目数组，请求级同名 Header 覆盖工作区默认项，缓存格式同步升版。

阶段七已完成。优先级：高。

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

1. [已完成] 建立 `RequestSession`。
2. [已完成] 消除三张 `HashMap`/`HashSet` 平行状态。
3. [已完成] 删除 `empty_request`。
4. [已完成] 将请求文件读写和序列化移出 `App`。
5. [已完成] 将请求执行器移出 `App`。

### 第三批：模型完善

1. [已完成] Header 改为有序、可重复结构。
2. [已完成] Form 参数改为有序、可重复结构。
3. [已完成] 统一 URL query、curl data 和 form 的数据语义。
4. [已完成] 评估 `tui-input`，并接入 `unicode-segmentation` 和 `arboard`。

### 第四批：界面依赖收敛

1. [阶段 8 已完成] 删除仅用于 Button 渲染的 `ratatui-interact`，使用 Ratatui 原生 `Block`/`Paragraph`。

### 第五批：会话模型收敛

1. [阶段 9 已完成] 将请求草稿、请求会话、工作区会话和运行态转换移到 `src/app/session.rs`。
2. [阶段 9 已完成] 让 `App` 通过运行态对象更新请求状态，避免直接维护多个相互关联的字段。

不建议一次性进行框架化重写。先解决 HTTP Client 生命周期和请求状态模型，项目复杂度会自然下降，再进行模块拆分。

## 参考资料

- [url crate](https://docs.rs/url/latest/url/)
- [form_urlencoded crate](https://docs.rs/form_urlencoded/latest/form_urlencoded/)
- [reqwest blocking Client](https://docs.rs/reqwest/latest/reqwest/blocking/struct.Client.html)
- [serde_yaml 维护状态](https://docs.rs/serde_yaml/latest/serde_yaml/)
- [serde-saphyr 1.1.0](https://docs.rs/serde-saphyr/1.1.0/serde_saphyr/)
- [serde_yml 迁移说明](https://docs.rs/serde_yml/latest/serde_yml/)
- [unicode-segmentation](https://docs.rs/unicode-segmentation/latest/unicode_segmentation/)
- [arboard](https://docs.rs/arboard/latest/arboard/)
- [tui-input](https://docs.rs/tui-input/latest/tui_input/)
- [tui-textarea](https://docs.rs/tui-textarea/latest/tui_textarea/)
- [Ratatui user input 示例](https://ratatui.rs/examples/apps/user_input/)
