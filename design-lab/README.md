# Design Lab

## Frontend preview

```bash
cargo run --example frontend_preview
```

完整 PostUI 界面使用中文与 `gruvbox-dark` 主题。界面支持键盘、鼠标、粘贴、
请求编辑、场景切换、cURL 导入、删除、重载、上传与响应下载。

内置 8 个示例请求：创建项目、项目列表、服务状态、更新项目、HTTP 422、
延迟响应、上传文件、导出项目。启动时发送创建项目请求。
示例 HTTP 服务监听 `127.0.0.1` 的临时端口，最多同时处理 8 个连接，
单次请求体上限为 1 MiB。

工作区、上传文件、下载文件与界面配置位于本次运行的临时目录。
退出后删除该目录。预览不读取个人配置，不记录最近工作区，不生成日志。
新增的请求使用填写的 URL。

```bash
cargo run --example frontend_preview -- --theme postui --language en
```

`Tab` / `Shift+Tab` 切换焦点，`j` / `k` 或方向键移动，`s` 发送或停止请求，
`c` 选择场景，`v` 打开变量，`m` 打开响应菜单，`?` 显示按键，`q` / `Esc` 返回或退出。

## Button gallery

`button_gallery` renders response toolbar button variants and records focus and
activation events.

```bash
cargo run --example button_gallery
```

Use the arrow keys or `j`/`k` to move, `Tab` or `h`/`l` to switch between
Formatted and Raw, and `Enter` or Space to activate a style. Mouse clicks are
supported. Press `q` or `Esc` to exit.

## UI studies

The following examples are stored in `design-lab/ui-studies/`:

- `button_showcase`: button states and interaction behavior
- `add_entry_gallery`: add-entry controls in table context
- `delete_icon_gallery`: delete icon and compact action variants

Each can still be started with its original example name:

```bash
cargo run --example button_showcase
cargo run --example add_entry_gallery
cargo run --example delete_icon_gallery
```
