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

    pub(crate) fn requests(self) -> &'static str {
        self.pick("Requests", "接口列表")
    }

    pub(crate) fn collection(self) -> &'static str {
        self.pick("Collection", "集合")
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

    pub(crate) fn query(self) -> &'static str {
        self.pick("Query", "查询")
    }

    pub(crate) fn raw(self) -> &'static str {
        self.pick("Raw", "原始")
    }

    pub(crate) fn url_encoded(self) -> &'static str {
        self.pick("URL Encoded", "URL 编码")
    }

    pub(crate) fn form(self) -> &'static str {
        self.pick("Form", "表单")
    }

    pub(crate) fn request_selector(self) -> &'static str {
        self.pick("Requests/", "接口/")
    }

    pub(crate) fn send_actions(self) -> &'static str {
        self.pick("Send", "发送")
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

    pub(crate) fn switched_to(self, focus: &str) -> String {
        match self.language {
            Language::English => format!("Focus · {focus}"),
            Language::Chinese => format!("当前区域 · {focus}"),
        }
    }

    pub(crate) fn selected_request(self, name: &str) -> String {
        match self.language {
            Language::English => format!("Request · {name}"),
            Language::Chinese => format!("当前接口 · {name}"),
        }
    }

    pub(crate) fn request_in_progress(self) -> &'static str {
        self.pick("Request still running", "请求尚未完成")
    }

    pub(crate) fn request_status_not_sent(self) -> &'static str {
        self.pick("Not sent", "未请求")
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
            Language::English => format!("Request failed · {error}"),
            Language::Chinese => format!("请求失败 · {error}"),
        }
    }

    pub(crate) fn request_timeout(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Request timed out · {error}"),
            Language::Chinese => format!("请求超时 · {error}"),
        }
    }

    pub(crate) fn request_started(self, method: &str, url: &str) -> String {
        match self.language {
            Language::English => format!("Sending · {method} {url}"),
            Language::Chinese => format!("请求中 · {method} {url}"),
        }
    }

    pub(crate) fn address(self) -> &'static str {
        self.pick("URL", "地址")
    }

    pub(crate) fn description(self) -> &'static str {
        self.pick("Description", "说明")
    }

    pub(crate) fn request_config(self) -> &'static str {
        self.pick("Config", "配置")
    }

    pub(crate) fn current_value(self) -> &'static str {
        self.pick("Current", "当前值")
    }

    pub(crate) fn value(self) -> &'static str {
        self.pick("Value", "值")
    }

    pub(crate) fn default_value(self) -> &'static str {
        self.pick("Default", "默认值")
    }

    pub(crate) fn source(self) -> &'static str {
        self.pick("Source", "来源")
    }

    pub(crate) fn value_type(self) -> &'static str {
        self.pick("Type", "类型")
    }

    pub(crate) fn inherited(self) -> &'static str {
        self.pick("Collection", "集合")
    }

    pub(crate) fn request_scope(self) -> &'static str {
        self.pick("Request", "请求")
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
        self.pick("Variables saved for this run", "变量已保存（本次运行）")
    }

    pub(crate) fn headers_applied(self) -> &'static str {
        self.pick("Headers saved for this run", "请求头已保存（本次运行）")
    }

    pub(crate) fn no_params(self) -> &'static str {
        self.pick("No params", "暂无参数")
    }

    pub(crate) fn editable_value_hint(self) -> &'static str {
        self.pick("Click an underlined value to edit", "点击下划线值修改")
    }

    pub(crate) fn json_value_hint(self) -> &'static str {
        self.pick("Click a value to edit", "点击值修改")
    }

    pub(crate) fn unsupported_method(self, method: &str) -> String {
        match self.language {
            Language::English => {
                format!("Cannot send {method} · GET and POST only")
            }
            Language::Chinese => format!("无法发送 {method} · 仅支持 GET / POST"),
        }
    }

    pub(crate) fn edit_headers(self) -> &'static str {
        self.pick("Headers", "请求头")
    }

    pub(crate) fn edit_params(self) -> &'static str {
        self.pick("Params", "参数")
    }

    pub(crate) fn edit_body(self) -> &'static str {
        self.pick("Body", "请求体")
    }

    pub(crate) fn params_applied(self) -> &'static str {
        self.pick("Params saved for this run", "参数已保存（本次运行）")
    }

    pub(crate) fn add_row(self) -> &'static str {
        self.pick("Add", "新增")
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

    pub(crate) fn waiting_response(self) -> &'static str {
        self.pick("Waiting…", "等待响应…")
    }

    pub(crate) fn request_not_sent(self) -> &'static str {
        self.pick("Not sent", "尚未发送")
    }

    pub(crate) fn response_body(self) -> &'static str {
        self.pick("Response body", "响应体")
    }

    pub(crate) fn download_saved(self, path: &str) -> String {
        match self.language {
            Language::English => format!("Saved to {path}"),
            Language::Chinese => format!("已保存至 {path}"),
        }
    }

    pub(crate) fn empty_response(self) -> &'static str {
        self.pick("(empty)", "（空）")
    }

    pub(crate) fn remaining_items(self, count: usize) -> String {
        match self.language {
            Language::English => format!("+ {count} more"),
            Language::Chinese => format!("另有 {count} 项"),
        }
    }

    pub(crate) fn send_hint(self) -> &'static str {
        self.pick("Click Send or press r", "点击发送，或按 r")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_english_copy() {
        let text = UiText::new(Language::English);
        assert_eq!(text.request_selector(), "Requests/");
        assert_eq!(text.send_button(false), "Send");
    }

    #[test]
    fn chinese_copy_is_available() {
        let text = UiText::new(Language::Chinese);
        assert_eq!(text.request_selector(), "接口/");
        assert_eq!(text.send_button(false), "发送");
    }
}
