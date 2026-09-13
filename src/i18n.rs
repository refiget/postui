use crate::settings::Language;

#[derive(Debug, Clone, Copy)]
pub(crate) struct UiText {
    language: Language,
}

impl UiText {
    pub(crate) const fn new(language: Language) -> Self {
        Self { language }
    }

    fn pick(self, english: &'static str, chinese: &'static str) -> &'static str {
        match self.language {
            Language::English => english,
            Language::Chinese => chinese,
        }
    }

    pub(crate) fn workspace(self) -> &'static str {
        self.pick("Workspace", "工作区")
    }

    pub(crate) fn variables(self) -> &'static str {
        self.pick("Variables", "变量")
    }

    pub(crate) fn headers(self) -> &'static str {
        self.pick("Headers", "请求头")
    }

    pub(crate) fn request_editor(self) -> &'static str {
        self.pick("Request", "请求")
    }

    pub(crate) fn params(self) -> &'static str {
        self.pick("Params", "参数")
    }

    pub(crate) fn body(self) -> &'static str {
        self.pick("Body", "请求体")
    }

    pub(crate) fn content(self) -> &'static str {
        self.pick("Content", "内容")
    }

    pub(crate) fn form(self) -> &'static str {
        self.pick("Form", "表单")
    }

    pub(crate) fn files(self) -> &'static str {
        self.pick("Files", "文件")
    }

    pub(crate) fn no_content(self) -> &'static str {
        self.pick("No request content", "无请求内容")
    }

    pub(crate) fn no_requests(self) -> &'static str {
        self.pick("No requests yet", "暂无请求")
    }

    pub(crate) fn enter_url(self) -> &'static str {
        self.pick("Enter URL", "输入地址")
    }

    pub(crate) fn request_url_required(self) -> &'static str {
        self.pick("Enter a request URL first", "请先填写请求地址")
    }

    pub(crate) fn request_selection_required(self) -> &'static str {
        self.pick("Select or create a request first", "请先选择或新建请求")
    }

    pub(crate) fn delete_request(self) -> &'static str {
        self.pick("Delete request", "删除请求")
    }

    pub(crate) fn delete_request_message(self) -> &'static str {
        self.pick(
            "Delete this request and its saved file?",
            "删除此请求及其已保存文件吗？",
        )
    }

    pub(crate) fn delete_request_hint(self) -> &'static str {
        self.pick("Y Delete · N Cancel", "Y 删除 · N 取消")
    }

    pub(crate) fn request_deleted(self) -> &'static str {
        self.pick("Request deleted", "请求已删除")
    }

    pub(crate) fn quit_title(self) -> &'static str {
        self.pick("Discard temporary changes?", "丢弃临时修改？")
    }

    pub(crate) fn quit_modified_message(self) -> &'static str {
        self.pick(
            "Temporary request changes will be lost when PostUI exits.",
            "退出 PostUI 后，当前会话的请求修改将丢失。",
        )
    }

    pub(crate) fn request_delete_failed(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Could not delete request: {error}"),
            Language::Chinese => format!("删除请求失败：{error}"),
        }
    }

    pub(crate) fn request_selector(self) -> &'static str {
        self.pick("Requests", "接口")
    }

    pub(crate) fn ready(self) -> &'static str {
        self.pick("Ready", "就绪")
    }

    pub(crate) fn operation_feedback(self) -> &'static str {
        self.pick("Action", "操作")
    }

    pub(crate) fn theme_loaded(self, name: &str) -> String {
        match self.language {
            Language::English => format!("Theme loaded: {name}"),
            Language::Chinese => format!("主题已加载：{name}"),
        }
    }

    pub(crate) fn theme_load_failed(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Could not load theme: {error}"),
            Language::Chinese => format!("加载主题失败：{error}"),
        }
    }

    pub(crate) fn navigation_hint(self) -> &'static str {
        self.pick(
            "Tab Focus · / Search · r Send/Cancel · R Reload · ? Help · q Quit",
            "Tab 切换 · / 搜索 · r 发送/取消 · R 重载 · ? 帮助 · q 退出",
        )
    }

    pub(crate) fn help_title(self) -> &'static str {
        self.pick("Keyboard help", "快捷键帮助")
    }

    pub(crate) fn help_content(self) -> &'static str {
        self.pick(
            "r   Send or cancel request\nR   Reload and validate YAML\nw   Switch scenario\nv   Open variables\n/   Search requests (name, path, method, URL)\n←/→ Response Raw / Formatted / Headers\no   Response actions\nu   Restore current request\nX   Restore all request changes in this scenario\n?   Open this help\nq   Quit\n\nEdits are temporary for this session. PostUI is not a YAML editor.\nPress any key to close.",
            "r   发送或取消请求\nR   重新加载并校验 YAML\nw   切换场景\nv   打开变量页\n/   搜索请求（名称、路径、方法、URL）\n←/→ 响应区 Raw / Formatted / Headers\no   响应操作\nu   恢复当前请求配置\nX   恢复当前场景的全部请求修改\n?   打开本帮助\nq   退出\n\n所有修改仅在当前会话生效。PostUI 不是 YAML 编辑器。\n按任意键关闭。",
        )
    }

    pub(crate) fn request_restored(self) -> &'static str {
        self.pick("Request restored from YAML", "已恢复当前请求配置")
    }

    pub(crate) fn configuration_restored(self) -> &'static str {
        self.pick(
            "All request changes in this scenario were restored",
            "已恢复当前场景的全部请求修改",
        )
    }

    pub(crate) fn debug_navigation_hint(self) -> &'static str {
        self.pick(
            "Tab Focus · r Send/Cancel · R Reload · o Actions · F5 Theme · q Quit",
            "Tab 切换 · r 发送/取消 · R 重载 · o 操作 · F5 主题 · q 退出",
        )
    }

    pub(crate) fn editing_hint(self) -> &'static str {
        self.pick(
            "Editing · Enter/Tab Confirm · Esc Cancel",
            "编辑中 · Enter/Tab 确认 · Esc 取消",
        )
    }

    pub(crate) fn menu_hint(self) -> &'static str {
        self.pick(
            "↑↓ Select · Enter Apply · Esc Close",
            "↑↓ 选择 · Enter 应用 · Esc 关闭",
        )
    }

    pub(crate) fn variables_page_hint(self) -> &'static str {
        self.pick(
            "↑↓ Select · Enter Edit · Tab Actions · Esc Back",
            "↑↓ 选择 · Enter 编辑 · Tab 切换操作 · Esc 返回",
        )
    }

    pub(crate) fn response_hint(self) -> &'static str {
        self.pick(
            "↑↓ Scroll · ←→ Tabs · / Search · n/N Next/Previous · o Actions · Esc Restore",
            "↑↓ 滚动 · ←→ 页签 · / 搜索 · n/N 下一处/上一处 · o 操作 · Esc 恢复布局",
        )
    }

    pub(crate) fn confirmation_hint(self) -> &'static str {
        self.pick("Y Confirm · N Cancel", "Y 确认 · N 取消")
    }

    pub(crate) fn request_complete(self, status: u16, elapsed_ms: u128) -> String {
        match self.language {
            Language::English => format!("HTTP {status} · {elapsed_ms} ms"),
            Language::Chinese => format!("HTTP {status} · {elapsed_ms} ms"),
        }
    }

    pub(crate) fn response_extract_failures(self, count: usize) -> String {
        match self.language {
            Language::English => format!(
                "{count} field(s) not extracted; check the response JSON and extraction paths"
            ),
            Language::Chinese => format!("{count} 个字段提取失败；请检查响应 JSON 和提取路径"),
        }
    }

    pub(crate) fn request_in_progress(self) -> &'static str {
        self.pick("Request is already in progress", "请求正在发送")
    }

    pub(crate) fn request_cancelled(self) -> &'static str {
        self.pick("Request cancelled", "请求已取消")
    }

    pub(crate) fn workspace_reloaded(self) -> &'static str {
        self.pick("Workspace configuration reloaded", "工作区配置已重新加载")
    }

    pub(crate) fn reload_while_sending(self) -> &'static str {
        self.pick(
            "Cancel running requests before reloading",
            "请先取消正在发送的请求再重新加载",
        )
    }

    pub(crate) fn reload_failed(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Could not reload workspace: {error}"),
            Language::Chinese => format!("重新加载工作区失败：{error}"),
        }
    }

    pub(crate) fn missing_variables(self, names: &[String]) -> String {
        let missing = names
            .iter()
            .map(|name| format!("- {name}"))
            .collect::<Vec<_>>()
            .join("\n");
        match self.language {
            Language::English => {
                format!("Required variables are missing:\n{}", missing)
            }
            Language::Chinese => {
                format!("以下变量缺少值：\n{}", missing)
            }
        }
    }

    pub(crate) fn request_status_not_sent(self) -> &'static str {
        self.pick("Not sent", "未发送")
    }

    pub(crate) fn request_status_sending(self) -> &'static str {
        self.pick("Sending", "请求中")
    }

    pub(crate) fn request_status_success(self) -> &'static str {
        self.pick("Success", "成功")
    }

    pub(crate) fn request_status_failed(self) -> &'static str {
        self.pick("Failed", "失败")
    }

    pub(crate) fn request_status_timeout(self) -> &'static str {
        self.pick("Timeout", "超时")
    }

    pub(crate) fn request_error(self, error: &crate::http::HttpError) -> &'static str {
        use crate::http::HttpError;
        match error {
            HttpError::InvalidRequest(_) => self.pick(
                "Invalid request; check the URL, method, headers and file types",
                "请求参数无效；请检查地址、方法、请求头和文件类型",
            ),
            HttpError::Upload(_) => self.pick(
                "Cannot read upload file; check its path and read permissions",
                "无法读取上传文件；请检查文件路径和读取权限",
            ),
            HttpError::Timeout(_) => self.pick(
                "Request timed out; check the service or increase the timeout",
                "请求超时；请检查服务状态或增加超时时间",
            ),
            HttpError::Connection(_) => self.pick(
                "Cannot connect; check the address, network and proxy settings",
                "无法连接服务；请检查地址、网络和代理设置",
            ),
            HttpError::Transport(_) => self.pick(
                "Request interrupted; check the network and service before retrying",
                "请求传输中断；请检查网络和服务状态后重试",
            ),
            HttpError::ResponseTooLarge(_) => self.pick(
                "Response exceeds max_response_bytes; increase the limit if needed",
                "响应超过 max_response_bytes 限制；如确有需要，可提高配置上限",
            ),
            HttpError::ClientInitialization(_) => self.pick(
                "Cannot initialize the HTTP client; check system and proxy settings",
                "无法初始化 HTTP 客户端；请检查系统和代理设置",
            ),
        }
    }

    pub(crate) fn request_started(self, method: &str, url: &str) -> String {
        match self.language {
            Language::English => format!("Sending {method} {url}"),
            Language::Chinese => format!("正在发送 {method} {url}"),
        }
    }

    pub(crate) fn address(self) -> &'static str {
        self.pick("URL", "地址")
    }

    pub(crate) fn description(self) -> &'static str {
        self.pick("Description", "说明")
    }

    pub(crate) fn current_value(self) -> &'static str {
        self.pick("Current", "当前值")
    }

    pub(crate) fn value(self) -> &'static str {
        self.pick("Value", "值")
    }

    pub(crate) fn name(self) -> &'static str {
        self.pick("Name", "名称")
    }

    pub(crate) fn default_value(self) -> &'static str {
        self.pick("Default", "默认值")
    }

    pub(crate) fn no_variables(self) -> &'static str {
        self.pick("No variables", "暂无变量")
    }

    pub(crate) fn no_headers(self) -> &'static str {
        self.pick("No headers", "暂无请求头")
    }

    pub(crate) fn apply(self) -> &'static str {
        self.pick("Apply", "应用")
    }

    pub(crate) fn close(self) -> &'static str {
        self.pick("Cancel", "取消")
    }

    pub(crate) fn variables_applied(self) -> &'static str {
        self.pick("Variables applied to this session", "变量已应用到当前会话")
    }

    pub(crate) fn configuration_switched(self, configuration: &str) -> String {
        match self.language {
            Language::English => format!(
                "Configuration switched to {configuration}; temporary changes are kept per scenario"
            ),
            Language::Chinese => format!("已切换到配置 {configuration}；临时修改按场景保留"),
        }
    }

    pub(crate) fn no_params(self) -> &'static str {
        self.pick("No params", "暂无参数")
    }

    pub(crate) fn unsupported_method(self, method: &str) -> String {
        match self.language {
            Language::English => format!("{method} is not supported. Use GET or POST."),
            Language::Chinese => format!("不支持 {method}，请使用 GET 或 POST"),
        }
    }

    pub(crate) fn invalid_body_value(self) -> &'static str {
        self.pick(
            "Invalid value for the selected JSON type",
            "输入值不符合当前 JSON 类型",
        )
    }

    pub(crate) fn empty_description(self) -> &'static str {
        self.pick("—", "—")
    }

    pub(crate) fn send_button(self, loading: bool) -> &'static str {
        if loading {
            self.pick("Sending…", "发送中…")
        } else {
            self.pick("Send", "发送")
        }
    }

    pub(crate) fn cancel_request(self) -> &'static str {
        self.pick("Cancel", "取消")
    }

    pub(crate) fn response(self) -> &'static str {
        self.pick("Response", "响应")
    }

    pub(crate) fn response_menu(self) -> &'static str {
        self.pick("Actions", "操作")
    }

    pub(crate) fn response_zoom(self) -> &'static str {
        self.pick("Zoom", "放大")
    }

    pub(crate) fn response_restore(self) -> &'static str {
        self.pick("Restore", "还原")
    }

    pub(crate) fn response_download(self) -> &'static str {
        self.pick("Download", "下载")
    }

    pub(crate) fn response_copy_body(self) -> &'static str {
        self.pick("Copy body", "复制响应体")
    }

    pub(crate) fn response_copy_headers(self) -> &'static str {
        self.pick("Copy headers", "复制响应头")
    }

    pub(crate) fn response_show_raw(self) -> &'static str {
        self.pick("Raw", "原文")
    }

    pub(crate) fn response_show_formatted(self) -> &'static str {
        self.pick("Formatted", "格式化")
    }

    pub(crate) fn response_headers_tab(self) -> &'static str {
        self.pick("Headers", "响应头")
    }

    pub(crate) fn response_preparing_highlight(self) -> &'static str {
        self.pick("Preparing syntax highlighting…", "正在准备语法高亮…")
    }

    pub(crate) fn response_format_note(
        self,
        note: crate::response_format::FormatNote,
    ) -> &'static str {
        use crate::response_format::FormatNote;
        match note {
            FormatNote::Original => self.pick("Original text (format preserved)", "保留原文格式"),
            FormatNote::Limited => self.pick(
                "Formatting limit reached · showing Raw",
                "超出格式化限制 · 显示 Raw",
            ),
            FormatNote::Invalid => self.pick(
                "Invalid or unsupported structure · showing Raw",
                "结构无效或不支持 · 显示 Raw",
            ),
        }
    }

    pub(crate) fn response_headers(self) -> &'static str {
        self.pick("Response headers", "响应头")
    }

    pub(crate) fn response_search_no_match(self, query: &str) -> String {
        match self.language {
            Language::English => format!("No response match for: {query}"),
            Language::Chinese => format!("响应中未找到：{query}"),
        }
    }

    pub(crate) fn response_action_no_response(self) -> &'static str {
        self.pick("No response to act on", "暂无可操作的响应")
    }

    pub(crate) fn binary_response_download(self) -> &'static str {
        self.pick(
            "Binary response cannot be copied as text; download it instead",
            "二进制响应无法按文本复制，请改用下载",
        )
    }

    pub(crate) fn binary_response_summary(self, bytes: usize) -> String {
        match self.language {
            Language::English => {
                format!("Binary response · {bytes} bytes · use Actions → Download")
            }
            Language::Chinese => format!("二进制响应 · {bytes} 字节 · 请使用 操作 → 下载"),
        }
    }

    pub(crate) fn response_copied(self) -> &'static str {
        self.pick("Response copied", "响应已复制")
    }

    pub(crate) fn response_copy_failed(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Could not copy response: {error}"),
            Language::Chinese => format!("复制响应失败：{error}"),
        }
    }

    pub(crate) fn response_downloaded(self, path: &str) -> String {
        match self.language {
            Language::English => format!("Response saved to {path}"),
            Language::Chinese => format!("响应已保存至 {path}"),
        }
    }

    pub(crate) fn response_download_failed(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Could not save response: {error}"),
            Language::Chinese => format!("保存响应失败：{error}"),
        }
    }

    pub(crate) fn waiting_response(self) -> &'static str {
        self.pick("Sending request…", "正在发送请求…")
    }

    pub(crate) fn request_not_sent(self) -> &'static str {
        self.pick("No response yet", "暂无响应")
    }

    pub(crate) fn response_body(self) -> &'static str {
        self.pick("Response body", "响应体")
    }

    pub(crate) fn empty_response(self) -> &'static str {
        self.pick("Empty response", "响应为空")
    }

    pub(crate) fn response_body_limited(self, shown: usize, total: usize) -> String {
        match self.language {
            Language::English => {
                format!("Large response: showing {shown} of {total} bytes; use Actions → Download")
            }
            Language::Chinese => {
                format!("响应过大：仅显示 {shown} / {total} 字节，可在操作中下载")
            }
        }
    }
}
