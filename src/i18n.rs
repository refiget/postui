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

    pub(crate) fn variables(self) -> &'static str {
        self.pick("Variables", "全局变量")
    }

    pub(crate) fn send_actions(self) -> &'static str {
        self.pick("Actions", "发送操作")
    }

    pub(crate) fn ready(self) -> &'static str {
        self.pick("Ready", "就绪")
    }

    pub(crate) fn request_complete(self, status: u16, elapsed_ms: u128) -> String {
        match self.language {
            Language::English => format!("Request complete · HTTP {status} · {elapsed_ms} ms"),
            Language::Chinese => format!("请求完成 · HTTP {status} · {elapsed_ms} ms"),
        }
    }

    pub(crate) fn switched_to(self, focus: &str) -> String {
        match self.language {
            Language::English => format!("Focus: {focus}"),
            Language::Chinese => format!("已切换到 {focus}"),
        }
    }

    pub(crate) fn selected_request(self, name: &str) -> String {
        match self.language {
            Language::English => format!("Selected {name}"),
            Language::Chinese => format!("已选择 {name}"),
        }
    }

    pub(crate) fn request_list_closed(self) -> &'static str {
        self.pick("Request list closed", "已关闭接口列表")
    }

    pub(crate) fn edit_cancelled(self) -> &'static str {
        self.pick("Edit cancelled", "已取消编辑")
    }

    pub(crate) fn no_editable_variables(self) -> &'static str {
        self.pick(
            "No editable variables for this request",
            "当前接口没有可编辑的变量",
        )
    }

    pub(crate) fn edit_variable(self, name: &str) -> String {
        match self.language {
            Language::English => format!("Edit {name} · Enter save · Esc cancel"),
            Language::Chinese => format!("编辑变量 {name} · Enter 保存 · Esc 取消"),
        }
    }

    pub(crate) fn variable_updated(self, name: &str) -> String {
        match self.language {
            Language::English => format!("Updated variable: {name}"),
            Language::Chinese => format!("已更新全局变量: {name}"),
        }
    }

    pub(crate) fn no_clearable_variables(self) -> &'static str {
        self.pick(
            "No variable to clear for this request",
            "当前接口没有可清理的变量",
        )
    }

    pub(crate) fn variable_cleared(self, name: &str) -> String {
        match self.language {
            Language::English => format!("Cleared variable: {name}"),
            Language::Chinese => format!("已清理全局变量: {name}"),
        }
    }

    pub(crate) fn no_extractable_variable(self) -> &'static str {
        self.pick(
            "This variable has no response extraction",
            "当前变量没有响应提取配置",
        )
    }

    pub(crate) fn no_successful_response(self) -> &'static str {
        self.pick(
            "No successful response to extract",
            "当前接口没有可提取的成功响应",
        )
    }

    pub(crate) fn variable_extracted(self, variable: &str) -> String {
        match self.language {
            Language::English => format!("Extracted response value: {variable}"),
            Language::Chinese => format!("已从响应提取变量: {variable}"),
        }
    }

    pub(crate) fn no_pasteable_variables(self) -> &'static str {
        self.pick(
            "No variable to paste into for this request",
            "当前接口没有可粘贴的变量",
        )
    }

    pub(crate) fn clipboard_empty(self) -> &'static str {
        self.pick("Clipboard has no text", "系统剪贴板没有文本")
    }

    pub(crate) fn pasted_to_variable(self, name: &str) -> String {
        match self.language {
            Language::English => format!("Pasted into variable: {name}"),
            Language::Chinese => format!("已从剪贴板粘贴到变量: {name}"),
        }
    }

    pub(crate) fn request_in_progress(self) -> &'static str {
        self.pick("Request in progress, please wait", "请求正在执行，请稍候")
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

    pub(crate) fn method(self) -> &'static str {
        self.pick("Method", "方法")
    }

    pub(crate) fn status(self) -> &'static str {
        self.pick("Status", "状态")
    }

    pub(crate) fn identifier(self) -> &'static str {
        self.pick("ID", "标识")
    }

    pub(crate) fn description(self) -> &'static str {
        self.pick("Description", "说明")
    }

    pub(crate) fn request_config(self) -> &'static str {
        self.pick("Request config", "请求配置")
    }

    pub(crate) fn empty_description(self) -> &'static str {
        self.pick("(not set)", "（未填写）")
    }

    pub(crate) fn send_button(self, loading: bool) -> &'static str {
        if loading {
            self.pick("Sending…", "发送中…")
        } else {
            self.pick("Send", "发送")
        }
    }

    pub(crate) fn no_variables(self) -> &'static str {
        self.pick("This request has no variables", "这个接口没有可替换的变量")
    }

    pub(crate) fn unset(self) -> &'static str {
        self.pick("(unset)", "（未设置）")
    }

    pub(crate) fn clear_button(self) -> &'static str {
        self.pick("Clear", "清理")
    }

    pub(crate) fn request_headers(self) -> &'static str {
        self.pick("Headers", "请求头")
    }

    pub(crate) fn request_body(self) -> &'static str {
        self.pick("Body", "请求体")
    }

    pub(crate) fn form(self) -> &'static str {
        self.pick("Form", "表单")
    }

    pub(crate) fn files(self) -> &'static str {
        self.pick("Files", "文件")
    }

    pub(crate) fn download(self) -> &'static str {
        self.pick("Download", "下载")
    }

    pub(crate) fn directory(self) -> &'static str {
        self.pick("Directory", "目录")
    }

    pub(crate) fn auto_filename(self) -> &'static str {
        self.pick("auto filename", "自动取文件名")
    }

    pub(crate) fn remote_filename(self) -> &'static str {
        self.pick("remote filename", "远程文件名")
    }

    pub(crate) fn remote_filename_from_header(self) -> &'static str {
        self.pick(
            "remote filename (Content-Disposition)",
            "远程文件名（Content-Disposition）",
        )
    }

    pub(crate) fn unresolved(self, names: &str) -> String {
        match self.language {
            Language::English => format!("Unresolved: {names}"),
            Language::Chinese => format!("未替换: {names}"),
        }
    }

    pub(crate) fn preview(self) -> &'static str {
        self.pick("Preview", "预览")
    }

    pub(crate) fn response(self) -> &'static str {
        self.pick("Response", "响应")
    }

    pub(crate) fn waiting_response(self) -> &'static str {
        self.pick("Waiting for response…", "正在等待响应…")
    }

    pub(crate) fn request_not_sent(self) -> &'static str {
        self.pick("This request has not been sent", "还没有发送这个请求")
    }

    pub(crate) fn extract_button(self) -> &'static str {
        self.pick("Extract", "提取")
    }

    pub(crate) fn paste_button(self) -> &'static str {
        self.pick("Paste", "粘贴")
    }

    pub(crate) fn response_headers(self) -> &'static str {
        self.pick("Response headers", "响应头")
    }

    pub(crate) fn response_body(self) -> &'static str {
        self.pick("Response body", "响应体")
    }

    pub(crate) fn download_saved(self, path: &str) -> String {
        match self.language {
            Language::English => format!("Saved file: {path}"),
            Language::Chinese => format!("文件已保存: {path}"),
        }
    }

    pub(crate) fn empty_response(self) -> &'static str {
        self.pick("(empty)", "（空）")
    }

    pub(crate) fn send_hint(self) -> &'static str {
        self.pick("Press r to send", "按 r 发送请求")
    }

    pub(crate) fn choose_request(self) -> &'static str {
        self.pick("Choose request", "选择接口")
    }

    pub(crate) fn footer_focus(self) -> &'static str {
        self.pick("Focus", "区域")
    }

    pub(crate) fn footer_move(self) -> &'static str {
        self.pick("Move", "移动")
    }

    pub(crate) fn footer_select_edit(self) -> &'static str {
        self.pick("Select/Edit", "选择/编辑")
    }

    pub(crate) fn footer_edit(self) -> &'static str {
        self.pick("Edit", "编辑")
    }

    pub(crate) fn footer_send(self) -> &'static str {
        self.pick("Send", "发送")
    }

    pub(crate) fn footer_clear(self) -> &'static str {
        self.pick("Clear", "清理")
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

    pub(crate) fn footer_action(self) -> &'static str {
        self.pick("Act", "操作")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_english_copy() {
        let text = UiText::new(Language::English);
        assert_eq!(text.preview(), "Preview");
        assert_eq!(text.send_button(false), "Send");
    }

    #[test]
    fn chinese_copy_is_available() {
        let text = UiText::new(Language::Chinese);
        assert_eq!(text.preview(), "预览");
        assert_eq!(text.send_button(false), "发送");
    }
}
