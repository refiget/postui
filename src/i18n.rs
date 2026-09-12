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
        self.pick("Enter a URL before sending", "发送前请输入地址")
    }

    pub(crate) fn request_saved(self, path: &str) -> String {
        match self.language {
            Language::English => format!("Request saved to {path}"),
            Language::Chinese => format!("请求已保存至 {path}"),
        }
    }

    pub(crate) fn request_save_failed(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Could not save request: {error}"),
            Language::Chinese => format!("保存请求失败：{error}"),
        }
    }

    pub(crate) fn unsaved_requests(self) -> &'static str {
        self.pick("Unsaved requests", "请求尚未保存")
    }

    pub(crate) fn unsaved_exit_message(self) -> &'static str {
        self.pick("Discard changes and quit?", "要放弃修改并退出吗？")
    }

    pub(crate) fn unsaved_exit_hint(self) -> &'static str {
        self.pick("Y Discard · N Continue", "Y 放弃 · N 继续")
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
        self.pick("Ready", "待发送")
    }

    pub(crate) fn request_complete(self, status: u16, elapsed_ms: u128) -> String {
        match self.language {
            Language::English => format!("HTTP {status} · {elapsed_ms} ms"),
            Language::Chinese => format!("HTTP {status} · {elapsed_ms} ms"),
        }
    }

    pub(crate) fn response_extract_failures(self, count: usize) -> String {
        match self.language {
            Language::English if count == 1 => "1 field not extracted".to_string(),
            Language::English => format!("{count} fields not extracted"),
            Language::Chinese => format!("{count} 个字段提取失败"),
        }
    }

    pub(crate) fn request_in_progress(self) -> &'static str {
        self.pick("Request is already in progress", "请求正在发送")
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

    pub(crate) fn request_failed(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Request failed: {error}"),
            Language::Chinese => format!("请求失败：{error}"),
        }
    }

    pub(crate) fn request_timeout(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Request timed out: {error}"),
            Language::Chinese => format!("请求超时：{error}"),
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
        self.pick("Save", "保存")
    }

    pub(crate) fn close(self) -> &'static str {
        self.pick("Cancel", "取消")
    }

    pub(crate) fn variables_applied(self) -> &'static str {
        self.pick("Variables saved", "变量已保存")
    }

    pub(crate) fn configuration_switched(self, configuration: &str) -> String {
        match self.language {
            Language::English => format!("Configuration switched to {configuration}"),
            Language::Chinese => format!("已切换到配置 {configuration}"),
        }
    }

    pub(crate) fn headers_applied(self) -> &'static str {
        self.pick("Headers saved", "请求头已保存")
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

    pub(crate) fn params_applied(self) -> &'static str {
        self.pick("Params saved", "参数已保存")
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

    pub(crate) fn response(self) -> &'static str {
        self.pick("Response", "响应")
    }

    pub(crate) fn response_menu(self) -> &'static str {
        self.pick("Actions", "操作")
    }

    pub(crate) fn response_download(self) -> &'static str {
        self.pick("Download", "下载")
    }

    pub(crate) fn response_copy(self) -> &'static str {
        self.pick("Copy", "复制")
    }

    pub(crate) fn response_action_no_response(self) -> &'static str {
        self.pick("No response to act on", "暂无可操作的响应")
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

    pub(crate) fn footer_focus(self) -> &'static str {
        self.pick("Focus", "区域")
    }

    pub(crate) fn footer_move(self) -> &'static str {
        self.pick("Move", "移动")
    }

    pub(crate) fn footer_select(self) -> &'static str {
        self.pick("Select", "选择")
    }

    pub(crate) fn footer_send(self) -> &'static str {
        self.pick("Send", "发送")
    }

    pub(crate) fn footer_save(self) -> &'static str {
        self.pick("Save", "保存")
    }

    pub(crate) fn footer_quit(self) -> &'static str {
        self.pick("Quit", "退出")
    }

    pub(crate) fn footer_mouse(self) -> &'static str {
        self.pick("Mouse", "鼠标")
    }

    pub(crate) fn footer_click(self) -> &'static str {
        self.pick("Click", "点击")
    }
}
