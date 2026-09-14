use crate::settings::Language;
use crate::shortcuts::{self, Command, Context};

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

    pub(crate) fn workspace_theme_hint(self) -> &'static str {
        self.pick("F5 Theme", "F5 主题")
    }

    pub(crate) fn workspace_picker_title(self) -> &'static str {
        self.pick("Select workspace", "选择工作区")
    }

    pub(crate) fn workspace_recent(self) -> &'static str {
        self.pick("Recent workspaces", "最近工作区")
    }

    pub(crate) fn workspace_not_found(self) -> &'static str {
        self.pick("No .postui found", "未找到 .postui")
    }

    pub(crate) fn workspace_path_missing(self) -> &'static str {
        self.pick(" [missing]", " [不存在]")
    }

    pub(crate) fn workspace_other(self) -> &'static str {
        self.pick("Open another directory…", "打开其他目录…")
    }

    pub(crate) fn workspace_no_recent(self) -> &'static str {
        self.pick("No recent workspaces", "无最近工作区")
    }

    pub(crate) fn workspace_no_match(self) -> &'static str {
        self.pick("No matches", "没有匹配项")
    }

    pub(crate) fn workspace_directory(self) -> &'static str {
        self.pick("Directory", "目录")
    }

    pub(crate) fn workspace_empty_path(self) -> &'static str {
        self.pick("Directory is required", "请输入目录")
    }

    pub(crate) fn workspace_min_size(self) -> &'static str {
        self.pick("Minimum terminal size: 48 × 20", "窗口最小尺寸：48 × 20")
    }

    pub(crate) fn workspace_input_hint(self) -> &'static str {
        self.pick("Enter Confirm  Esc Back", "Enter 确认  Esc 返回")
    }

    pub(crate) fn workspace_picker_hint(self) -> &'static str {
        self.pick(
            "j/k Select  Enter Open  / Filter  o Directory  d Remove  q Quit",
            "j/k 选择  Enter 打开  / 筛选  o 输入目录  d 移除记录  q 退出",
        )
    }

    pub(crate) fn workspace(self) -> &'static str {
        self.pick("Workspace", "工作区")
    }

    pub(crate) fn curl_import_name(self) -> &'static str {
        self.pick("* Name", "* 名称")
    }

    pub(crate) fn curl_import_title(self) -> &'static str {
        self.pick("cURL import", "cURL 导入")
    }

    pub(crate) fn curl_import_details(self) -> &'static str {
        self.pick("REQUEST DETAILS", "请求信息")
    }

    pub(crate) fn curl_import_request(self) -> &'static str {
        self.pick("Request", "请求")
    }

    pub(crate) fn curl_import_description(self) -> &'static str {
        self.pick("Description", "描述")
    }

    pub(crate) fn curl_import_variables(self) -> &'static str {
        self.pick("Register variables", "注册变量")
    }

    pub(crate) fn curl_import_variables_hint(self) -> &'static str {
        self.pick(
            "{{example1}}\n{{example2}}\n{{example3}}=\"hello, world!\"",
            "{{example1}}\n{{example2}}\n{{example3}}=\"hello, world!\"",
        )
    }

    pub(crate) fn curl_import_workspace(self) -> &'static str {
        self.pick("Workspace", "工作空间")
    }

    pub(crate) fn curl_import_command(self) -> &'static str {
        self.pick("Command", "命令")
    }

    pub(crate) fn curl_import_confirm(self) -> &'static str {
        self.pick("Import request", "导入请求")
    }

    pub(crate) fn curl_import_cancel(self) -> &'static str {
        self.pick("Cancel", "取消")
    }

    pub(crate) fn curl_import_parsing(self) -> &'static str {
        self.pick("Parsing", "解析中")
    }

    pub(crate) fn curl_import_saving(self) -> &'static str {
        self.pick("Saving", "保存中")
    }

    pub(crate) fn curl_import_stopped(self) -> &'static str {
        self.pick("Import stopped", "解析任务已停止")
    }

    pub(crate) fn curl_import_invalid_variable(self, value: &str) -> String {
        match self.language {
            Language::English => format!("Invalid variable: {value}"),
            Language::Chinese => format!("变量格式无效: {value}"),
        }
    }

    pub(crate) fn curl_import_duplicate_variable(self, name: &str) -> String {
        match self.language {
            Language::English => format!("Duplicate variable: {name}"),
            Language::Chinese => format!("变量重复: {name}"),
        }
    }

    pub(crate) fn curl_import_empty_variable(self) -> &'static str {
        self.pick("Variable list contains an empty item", "变量列表存在空项")
    }

    pub(crate) fn curl_import_unclosed_default(self) -> &'static str {
        self.pick(
            "Variable default has an unclosed quote",
            "变量默认值引号未闭合",
        )
    }

    pub(crate) fn curl_import_invalid_default(self, value: &str) -> String {
        match self.language {
            Language::English => format!("Invalid variable default: {value}"),
            Language::Chinese => format!("变量默认值无效: {value}"),
        }
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

    pub(crate) fn add_entry(self) -> &'static str {
        self.pick("Add row", "添加行")
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
        self.pick("Request body: empty", "请求体：空")
    }

    pub(crate) fn no_requests(self) -> &'static str {
        self.pick("Request list: empty", "接口列表：空")
    }

    pub(crate) fn enter_url(self) -> &'static str {
        self.pick("URL required", "地址未设置")
    }

    pub(crate) fn request_url_required(self) -> &'static str {
        self.pick("URL required", "地址未设置")
    }

    pub(crate) fn request_selection_required(self) -> &'static str {
        self.pick("No request selected", "未选择接口")
    }

    pub(crate) fn delete_request(self) -> &'static str {
        self.pick("Delete request", "删除请求")
    }

    pub(crate) fn delete_request_message(self) -> &'static str {
        self.pick("Delete request and source file?", "删除接口及其源文件？")
    }

    pub(crate) fn delete_request_hint(self) -> String {
        self.confirmation_hint()
    }

    pub(crate) fn request_deleted(self) -> &'static str {
        self.pick("Request deleted", "请求已删除")
    }

    pub(crate) fn quit_title(self) -> &'static str {
        self.pick("Discard session changes?", "丢弃会话修改？")
    }

    pub(crate) fn quit_modified_message(self) -> &'static str {
        self.pick(
            "Exit clears all unsaved request changes.",
            "退出后清除全部未保存的请求修改。",
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

    pub(crate) fn theme_load_failed(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Could not load theme: {error}"),
            Language::Chinese => format!("加载主题失败：{error}"),
        }
    }

    pub(crate) fn shortcut_hint(self, context: Context, debug: bool) -> String {
        use Command::*;
        let commands: &[Command] = match context {
            Context::Editor => &[Confirm, Back, SelectAll, Help],
            Context::Confirm => &[Confirm, Back],
            Context::Help => &[Up, Down, Back],
            Context::Menu => &[Up, Down, Activate, Back, Help],
            Context::Variables => &[Up, Down, Activate, Back, Help],
            Context::Headers => &[Activate, Add, Delete, Toggle, FocusNext, Help],
            Context::Params => &[Activate, Add, Delete, FocusNext, Help],
            Context::Response => &[
                Up,
                Down,
                Search,
                NextMatch,
                PreviousMatch,
                ResponseMenu,
                Help,
            ],
            _ => &[FocusNext, Search, Send, Reload, Help, Back],
        };
        shortcuts::hint(context, self.language, commands, debug)
    }

    pub(crate) fn help_title(self) -> &'static str {
        self.pick("Key map", "按键表")
    }

    pub(crate) fn help_content(self, context: Context, debug: bool) -> String {
        shortcuts::help(context, self.language, debug)
    }

    pub(crate) fn request_restored(self) -> &'static str {
        self.pick("Reset", "已重置")
    }

    pub(crate) fn configuration_restored(self) -> &'static str {
        self.pick("Reset", "已重置")
    }

    pub(crate) fn confirmation_hint(self) -> String {
        self.shortcut_hint(Context::Confirm, false)
    }

    pub(crate) fn response_extract_failures(self, count: usize) -> String {
        match self.language {
            Language::English => format!("{count} field(s) failed"),
            Language::Chinese => format!("{count} 个字段提取失败"),
        }
    }

    pub(crate) fn request_in_progress(self) -> &'static str {
        self.pick("Request in progress", "请求处理中")
    }

    pub(crate) fn request_cancelled(self) -> &'static str {
        self.pick("Cancelled", "已取消")
    }

    pub(crate) fn workspace_reloaded(self) -> &'static str {
        self.pick("Reloaded", "已重载")
    }

    pub(crate) fn reload_while_sending(self) -> &'static str {
        self.pick("Request in progress", "请求处理中")
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
            HttpError::InvalidRequest(_) => self.pick("Invalid request", "请求参数无效"),
            HttpError::Upload(_) => self.pick("Upload file could not be read", "上传文件读取失败"),
            HttpError::Timeout(_) => self.pick("Request timed out", "请求超时"),
            HttpError::Connection(_) => self.pick("Connection failed", "连接失败"),
            HttpError::Transport(_) => self.pick("Request interrupted", "请求传输中断"),
            HttpError::ResponseTooLarge(_) => self.pick(
                "Response exceeds max_response_bytes",
                "响应超过 max_response_bytes 限制",
            ),
            HttpError::ClientInitialization(_) => {
                self.pick("HTTP client initialization failed", "HTTP 客户端初始化失败")
            }
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
        self.pick("Variables: empty", "变量：空")
    }

    pub(crate) fn no_headers(self) -> &'static str {
        self.pick("Headers: empty", "请求头：空")
    }

    pub(crate) fn configuration_switched(self) -> &'static str {
        self.pick("Switched", "已切换")
    }

    pub(crate) fn no_params(self) -> &'static str {
        self.pick("Params: empty", "参数：空")
    }

    pub(crate) fn invalid_method(self, method: &str) -> String {
        match self.language {
            Language::English => format!("Invalid HTTP method: {method}"),
            Language::Chinese => format!("无效的 HTTP 方法：{method}"),
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
        self.pick("INDEXING SYNTAX…", "语法索引中…")
    }

    pub(crate) fn response_format_note(
        self,
        note: crate::response_format::FormatNote,
    ) -> &'static str {
        use crate::response_format::FormatNote;
        match note {
            FormatNote::Original => self.pick("Original text", "原文"),
            FormatNote::Limited => {
                self.pick("Formatting limit reached · Raw", "超出格式化限制 · Raw")
            }
            FormatNote::Invalid => self.pick(
                "Invalid or unsupported structure · Raw",
                "结构无效或不支持 · Raw",
            ),
        }
    }

    pub(crate) fn response_headers(self) -> &'static str {
        self.pick("Response headers", "响应头")
    }

    pub(crate) fn response_search_no_match(self) -> &'static str {
        self.pick("No match", "未找到匹配项")
    }

    pub(crate) fn response_action_no_response(self) -> &'static str {
        self.pick("No response", "无响应")
    }

    pub(crate) fn binary_response_download(self) -> &'static str {
        self.pick(
            "Binary response cannot be copied as text",
            "二进制响应无法按文本复制",
        )
    }

    pub(crate) fn binary_response_summary(self, bytes: usize) -> String {
        match self.language {
            Language::English => format!("Binary response · {bytes} bytes"),
            Language::Chinese => format!("二进制响应 · {bytes} 字节"),
        }
    }

    pub(crate) fn response_copied(self) -> &'static str {
        self.pick("Copied", "已复制")
    }

    pub(crate) fn response_copy_failed(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Could not copy response: {error}"),
            Language::Chinese => format!("复制响应失败：{error}"),
        }
    }

    pub(crate) fn response_downloaded(self) -> &'static str {
        self.pick("Saved", "已保存")
    }

    pub(crate) fn response_download_failed(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Could not save response: {error}"),
            Language::Chinese => format!("保存响应失败：{error}"),
        }
    }

    pub(crate) fn waiting_response(self) -> &'static str {
        self.pick("REQUEST IN FLIGHT…", "请求传输中…")
    }

    pub(crate) fn request_not_sent(self) -> &'static str {
        self.pick("Response: not available", "响应：不可用")
    }

    pub(crate) fn response_body(self) -> &'static str {
        self.pick("Response body", "响应体")
    }

    pub(crate) fn empty_response(self) -> &'static str {
        self.pick("Empty response", "响应为空")
    }

    pub(crate) fn response_body_limited(self, shown: usize, total: usize) -> String {
        match self.language {
            Language::English => format!("Response: {shown} of {total} bytes displayed"),
            Language::Chinese => format!("响应：显示 {shown} / {total} 字节"),
        }
    }
}
