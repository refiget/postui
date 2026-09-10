use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    clipboard::SystemClipboard,
    config::{ApiRequest, RequestConfig, value_to_string},
    http::{self, HttpError, ResponseData},
    i18n::UiText,
    settings::GlobalConfig,
    template::{self, ResolvedRequest},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Requests,
    Variables,
    Actions,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Requests => Self::Variables,
            Self::Variables => Self::Actions,
            Self::Actions => Self::Requests,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Requests => Self::Actions,
            Self::Variables => Self::Requests,
            Self::Actions => Self::Variables,
        }
    }

    fn label(self, text: UiText) -> &'static str {
        match self {
            Self::Requests => text.requests(),
            Self::Variables => text.variables(),
            Self::Actions => text.send_actions(),
        }
    }
}

enum AppMessage {
    RequestFinished {
        request_id: String,
        operation_id: String,
        result: Result<ResponseData, HttpError>,
    },
}

static NEXT_REQUEST_OPERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Default)]
pub(crate) struct ScrollState {
    offset: u16,
}

impl ScrollState {
    const STEP: u16 = 3;

    pub(crate) fn offset(&self) -> u16 {
        self.offset
    }

    fn reset(&mut self) {
        self.offset = 0;
    }

    fn move_by(&mut self, direction: isize) {
        self.offset = match direction {
            -1 => self.offset.saturating_sub(Self::STEP),
            1 => self.offset.saturating_add(Self::STEP),
            _ => self.offset,
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RequestStatus {
    #[default]
    NotSent,
    Sending,
    Success,
    Failed,
    Timeout,
}

impl RequestStatus {
    pub(crate) fn from_http_status(status: u16) -> Self {
        if status < 400 {
            Self::Success
        } else {
            Self::Failed
        }
    }

    pub(crate) fn from_error(error: &HttpError) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else {
            Self::Failed
        }
    }

    pub(crate) fn tag(self) -> &'static str {
        match self {
            Self::NotSent => "--",
            Self::Sending => "..",
            Self::Success => "OK",
            Self::Failed => "ERR",
            Self::Timeout => "TO",
        }
    }

    pub(crate) fn label(self, text: UiText) -> &'static str {
        match self {
            Self::NotSent => text.request_status_not_sent(),
            Self::Sending => text.request_status_sending(),
            Self::Success => text.request_status_success(),
            Self::Failed => text.request_status_failed(),
            Self::Timeout => text.request_status_timeout(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RequestRuntimeState {
    pub(crate) status: RequestStatus,
    pub(crate) response: Option<ResponseData>,
    pub(crate) error: Option<String>,
    operation_id: Option<String>,
}

impl RequestRuntimeState {
    #[cfg(test)]
    pub(crate) fn from_response(response: ResponseData) -> Self {
        Self {
            status: RequestStatus::from_http_status(response.status),
            response: Some(response),
            ..Default::default()
        }
    }
}

pub(crate) struct App {
    pub(crate) config: RequestConfig,
    pub(crate) config_path: PathBuf,
    pub(crate) global_config: GlobalConfig,
    pub(crate) selected_request: usize,
    pub(crate) dropdown_open: bool,
    pub(crate) focus: Focus,
    pub(crate) variable_index: usize,
    pub(crate) editing: bool,
    pub(crate) edit_buffer: String,
    pub(crate) global_variables: BTreeMap<String, String>,
    pub(crate) request_states: HashMap<String, RequestRuntimeState>,
    pub(crate) response_headers_expanded: bool,
    pub(crate) response_scroll: ScrollState,
    pub(crate) status: String,
    pub(crate) should_quit: bool,
    clipboard: SystemClipboard,
    sender: Sender<AppMessage>,
    receiver: Receiver<AppMessage>,
}

impl App {
    pub(crate) fn new(
        config: RequestConfig,
        config_path: PathBuf,
        global_config: GlobalConfig,
    ) -> Self {
        let text = UiText::new(global_config.language);
        tracing::debug!(
            config_path = %config_path.display(),
            global_config_path = global_config
                .path
                .as_deref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<内置默认配置>".to_string()),
            theme = %global_config.theme.name,
            request_count = config.requests.len(),
            configured_variable_count = config.variables.len(),
            "创建应用状态"
        );
        let global_variables = config
            .variables
            .iter()
            .map(|(key, definition)| {
                let value = definition
                    .default
                    .as_ref()
                    .map(value_to_string)
                    .unwrap_or_default();
                (key.clone(), value)
            })
            .collect::<BTreeMap<_, _>>();
        tracing::debug!(
            global_variable_count = global_variables.len(),
            "初始化全局变量"
        );
        let request_states = config
            .requests
            .iter()
            .map(|request| (request.id.clone(), RequestRuntimeState::default()))
            .collect::<HashMap<_, _>>();
        tracing::debug!(
            request_state_count = request_states.len(),
            "初始化接口运行状态"
        );

        let (sender, receiver) = mpsc::channel();
        Self {
            config,
            config_path,
            global_config,
            selected_request: 0,
            dropdown_open: false,
            focus: Focus::Requests,
            variable_index: 0,
            editing: false,
            edit_buffer: String::new(),
            global_variables,
            request_states,
            response_headers_expanded: false,
            response_scroll: ScrollState::default(),
            status: text.ready().to_string(),
            should_quit: false,
            clipboard: SystemClipboard::default(),
            sender,
            receiver,
        }
    }

    pub(crate) fn current_request(&self) -> &ApiRequest {
        &self.config.requests[self.selected_request]
    }

    pub(crate) fn text(&self) -> UiText {
        UiText::new(self.global_config.language)
    }

    pub(crate) fn select_request(&mut self, index: usize) {
        if index >= self.config.requests.len() {
            tracing::debug!(
                index,
                request_count = self.config.requests.len(),
                "忽略无效接口索引"
            );
            return;
        }
        let previous = self.selected_request;
        let changed = previous != index;
        if changed {
            self.stop_editing();
        }
        self.selected_request = index;
        if changed {
            self.response_headers_expanded = false;
            self.response_scroll.reset();
        }
        self.variable_index = self
            .variable_index
            .min(self.current_variable_names().len().saturating_sub(1));
        if changed {
            tracing::debug!(
                previous_index = previous,
                selected_index = index,
                request_id = %self.current_request().id,
                "切换当前接口"
            );
        }
    }

    pub(crate) fn current_variable_names(&self) -> Vec<String> {
        template::variable_names(self.current_request())
    }

    pub(crate) fn current_variables(&self) -> Vec<(String, String)> {
        self.current_variable_names()
            .into_iter()
            .map(|name| {
                let value = self
                    .global_variables
                    .get(&name)
                    .cloned()
                    .unwrap_or_default();
                (name, value)
            })
            .collect()
    }

    pub(crate) fn variable_has_extract(&self, index: usize) -> bool {
        let names = self.current_variable_names();
        let Some(name) = names.get(index) else {
            return false;
        };
        self.extract_path(name).is_some()
    }

    pub(crate) fn can_extract_variable(&self, index: usize) -> bool {
        self.variable_has_extract(index)
            && self.request_status(&self.current_request().id) == RequestStatus::Success
            && self.current_response().is_some()
    }

    fn extract_path(&self, variable: &str) -> Option<String> {
        self.current_request()
            .extracts
            .iter()
            .find(|extract| extract.variable == variable)
            .map(|extract| extract.path.clone())
    }

    pub(crate) fn current_resolved_request(&self) -> ResolvedRequest {
        template::resolve_request(self.current_request(), &self.global_variables)
    }

    pub(crate) fn current_request_state(&self) -> Option<&RequestRuntimeState> {
        self.request_states.get(&self.current_request().id)
    }

    pub(crate) fn request_status(&self, request_id: &str) -> RequestStatus {
        self.request_states
            .get(request_id)
            .map(|state| state.status)
            .unwrap_or_default()
    }

    pub(crate) fn current_response(&self) -> Option<&ResponseData> {
        self.current_request_state()
            .and_then(|state| state.response.as_ref())
    }

    pub(crate) fn current_error(&self) -> Option<&str> {
        self.current_request_state()
            .and_then(|state| state.error.as_deref())
    }

    pub(crate) fn poll_messages(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                AppMessage::RequestFinished {
                    request_id,
                    operation_id,
                    result,
                } => {
                    tracing::debug!(
                        request_id = %request_id,
                        operation_id = %operation_id,
                        "收到后台请求结果"
                    );
                    let is_current = self.current_request().id == request_id;
                    let text = self.text();
                    let Some(state) = self.request_states.get_mut(&request_id) else {
                        tracing::debug!(
                            request_id = %request_id,
                            operation_id = %operation_id,
                            "收到未知接口的后台请求结果"
                        );
                        continue;
                    };
                    if state.operation_id.as_deref() != Some(operation_id.as_str()) {
                        tracing::debug!(
                            request_id = %request_id,
                            operation_id = %operation_id,
                            active_operation_id = ?state.operation_id,
                            "忽略过期的后台请求结果"
                        );
                        continue;
                    }
                    state.operation_id = None;
                    let status_message = match result {
                        Ok(response) => {
                            let status = response.status;
                            let elapsed = response.elapsed_ms;
                            let request_status = RequestStatus::from_http_status(status);
                            tracing::debug!(
                                request_id = %request_id,
                                operation_id = %operation_id,
                                status,
                                request_status = ?request_status,
                                elapsed_ms = elapsed,
                                header_count = response.headers.len(),
                                body_bytes = response.body.len(),
                                "后台请求成功"
                            );
                            state.status = request_status;
                            state.response = Some(response);
                            state.error = None;
                            is_current.then(|| text.request_complete(status, elapsed))
                        }
                        Err(error) => {
                            let request_status = RequestStatus::from_error(&error);
                            let error_message = error.to_string();
                            tracing::error!(
                                request_id = %request_id,
                                operation_id = %operation_id,
                                request_status = ?request_status,
                                error = %error_message,
                                "后台请求失败"
                            );
                            state.status = request_status;
                            state.response = None;
                            state.error = Some(error_message.clone());
                            is_current.then(|| {
                                if request_status == RequestStatus::Timeout {
                                    text.request_timeout(&error_message)
                                } else {
                                    text.request_failed(&error_message)
                                }
                            })
                        }
                    };
                    if let Some(status) = status_message {
                        self.response_headers_expanded = false;
                        self.response_scroll.reset();
                        self.status = status;
                    }
                }
            }
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        tracing::debug!(
            key_kind = key_kind(key.code),
            modifiers = ?key.modifiers,
            focus = ?self.focus,
            editing = self.editing,
            dropdown_open = self.dropdown_open,
            "处理键盘操作"
        );
        if self.editing {
            self.handle_editing_key(key);
            return;
        }

        if self.dropdown_open {
            self.handle_dropdown_key(key);
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            tracing::debug!("通过 Ctrl+C 请求退出");
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
                tracing::debug!("通过快捷键请求退出");
            }
            KeyCode::Tab => {
                self.focus = if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.focus.previous()
                } else {
                    self.focus.next()
                };
                tracing::debug!(focus = ?self.focus, "切换 TUI 区域焦点");
                let text = self.text();
                self.status = text.switched_to(self.focus.label(text));
            }
            KeyCode::Char('r') => self.send_current_request(),
            KeyCode::Up | KeyCode::Char('k') => self.move_focused(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_focused(1),
            KeyCode::Enter => self.handle_enter(),
            KeyCode::Char('c') | KeyCode::Char('x') | KeyCode::Delete
                if self.focus == Focus::Variables =>
            {
                self.clear_current_variable()
            }
            _ => {}
        }
    }

    fn handle_dropdown_key(&mut self, key: KeyEvent) {
        tracing::debug!(key_kind = key_kind(key.code), "处理接口下拉列表操作");
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_request(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_request(1),
            KeyCode::Enter | KeyCode::Esc => {
                self.dropdown_open = false;
                self.focus = Focus::Requests;
                self.status = if key.code == KeyCode::Enter {
                    let name = self.current_request().name.clone();
                    tracing::debug!(request_id = %self.current_request().id, "确认接口选择");
                    self.text().selected_request(&name)
                } else {
                    tracing::debug!("关闭接口下拉列表");
                    self.text().request_list_closed().to_string()
                };
            }
            _ => {}
        }
    }

    fn handle_editing_key(&mut self, key: KeyEvent) {
        tracing::debug!(key_kind = key_kind(key.code), "处理变量编辑操作");
        match key.code {
            KeyCode::Enter => self.commit_edit(),
            KeyCode::Esc => {
                self.stop_editing();
                self.status = self.text().edit_cancelled().to_string();
            }
            KeyCode::Backspace => {
                self.edit_buffer.pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.edit_buffer.clear();
            }
            KeyCode::Char(character) => self.edit_buffer.push(character),
            _ => {}
        }
    }

    fn handle_enter(&mut self) {
        tracing::debug!(focus = ?self.focus, "处理 Enter 操作");
        match self.focus {
            Focus::Requests => {
                self.dropdown_open = true;
                tracing::debug!("打开接口下拉列表");
            }
            Focus::Variables => self.edit_current_variable(),
            Focus::Actions => self.send_current_request(),
        }
    }

    fn move_focused(&mut self, direction: isize) {
        match self.focus {
            Focus::Requests => self.move_request(direction),
            Focus::Variables => self.move_variable(direction),
            Focus::Actions => {}
        }
    }

    pub(crate) fn move_variable(&mut self, direction: isize) {
        let previous = self.variable_index;
        let last = self.current_variable_names().len().saturating_sub(1);
        self.variable_index = match direction {
            -1 => self.variable_index.saturating_sub(1),
            1 => (self.variable_index + 1).min(last),
            _ => self.variable_index.min(last),
        };
        if previous != self.variable_index {
            tracing::debug!(
                previous_index = previous,
                selected_index = self.variable_index,
                direction,
                "移动变量选择"
            );
        }
    }

    pub(crate) fn move_request(&mut self, delta: isize) {
        let count = self.config.requests.len();
        if count == 0 {
            tracing::debug!("接口列表为空，忽略移动操作");
            return;
        }
        let current = self.selected_request % count;
        let next = (current as isize + delta).rem_euclid(count as isize) as usize;
        self.select_request(next);
    }

    pub(crate) fn toggle_response_headers(&mut self) {
        let Some(response) = self.current_response() else {
            tracing::debug!("当前接口没有响应，忽略响应头展开操作");
            return;
        };
        if response.headers.is_empty() {
            tracing::debug!("当前响应没有响应头，忽略展开操作");
            return;
        }
        self.response_headers_expanded = !self.response_headers_expanded;
        self.response_scroll.reset();
        tracing::debug!(
            expanded = self.response_headers_expanded,
            "切换响应头展开状态"
        );
    }

    pub(crate) fn scroll_response(&mut self, direction: isize) {
        if self.current_response().is_none() {
            return;
        }
        let previous = self.response_scroll.offset();
        self.response_scroll.move_by(direction);
        if previous != self.response_scroll.offset() {
            tracing::debug!(
                previous_offset = previous,
                offset = self.response_scroll.offset(),
                direction,
                "滚动响应内容"
            );
        }
    }

    pub(crate) fn edit_current_variable(&mut self) {
        let Some(name) = self.current_variable_name() else {
            tracing::debug!("当前接口没有可编辑变量");
            self.status = self.text().no_editable_variables().to_string();
            return;
        };
        self.edit_buffer = self
            .global_variables
            .get(&name)
            .cloned()
            .unwrap_or_default();
        tracing::debug!(variable = %name, value_bytes = self.edit_buffer.len(), "开始编辑全局变量");
        self.editing = true;
        self.status = self.text().edit_variable(&name);
    }

    fn commit_edit(&mut self) {
        let Some(name) = self.current_variable_name() else {
            tracing::debug!("编辑提交时没有当前变量");
            self.stop_editing();
            return;
        };
        let value = std::mem::take(&mut self.edit_buffer);
        tracing::debug!(variable = %name, value_bytes = value.len(), "提交全局变量");
        self.global_variables.insert(name.clone(), value);
        self.stop_editing();
        self.status = self.text().variable_updated(&name);
    }

    pub(crate) fn clear_current_variable(&mut self) {
        self.clear_variable(self.variable_index);
    }

    pub(crate) fn clear_variable(&mut self, index: usize) {
        let Some(name) = self.current_variable_names().get(index).cloned() else {
            tracing::debug!(index, "清理变量时索引无效");
            self.status = self.text().no_clearable_variables().to_string();
            return;
        };
        tracing::debug!(variable = %name, index, "清理全局变量");
        self.global_variables.insert(name.clone(), String::new());
        if self.variable_index == index {
            self.stop_editing();
        }
        self.status = self.text().variable_cleared(&name);
    }

    pub(crate) fn extract_variable(&mut self, index: usize) {
        if self.editing {
            self.commit_edit();
        }

        let request_id = self.current_request().id.clone();
        let Some(name) = self.current_variable_names().get(index).cloned() else {
            tracing::debug!(request_id = %request_id, index, "响应提取变量索引无效");
            self.status = self.text().no_extractable_variable().to_string();
            return;
        };

        let Some(path) = self.extract_path(&name) else {
            tracing::debug!(
                request_id = %request_id,
                index,
                variable = %name,
                "变量没有响应提取配置"
            );
            self.status = self.text().no_extractable_variable().to_string();
            return;
        };

        let request_status = self.request_status(&request_id);
        if request_status != RequestStatus::Success {
            tracing::debug!(
                request_id = %request_id,
                index,
                variable = %name,
                request_status = ?request_status,
                "当前接口没有成功响应，无法提取变量"
            );
            self.status = self.text().no_successful_response().to_string();
            return;
        }

        let Some(body) = self
            .current_response()
            .map(|response| response.body.clone())
        else {
            tracing::debug!(
                request_id = %request_id,
                index,
                variable = %name,
                "当前接口没有响应，无法提取变量"
            );
            self.status = self.text().no_successful_response().to_string();
            return;
        };
        tracing::debug!(
            request_id = %request_id,
            index,
            variable = %name,
            path = %path,
            response_body_bytes = body.len(),
            "从响应提取变量"
        );
        let value = match template::extract_json_value(&body, &path) {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(request_id = %request_id, path = %path, error = %error, "响应字段提取失败");
                self.status = error;
                return;
            }
        };
        tracing::debug!(
            request_id = %request_id,
            variable = %name,
            value_bytes = value.len(),
            "响应字段提取成功，写入全局变量"
        );
        self.global_variables.insert(name.clone(), value);
        self.variable_index = index;
        self.status = self.text().variable_extracted(&name);
    }

    pub(crate) fn paste_variable(&mut self, index: usize) {
        tracing::debug!(index, "从剪贴板粘贴变量");
        if self.editing {
            self.commit_edit();
        }
        let Some(name) = self.current_variable_names().get(index).cloned() else {
            tracing::debug!(index, "粘贴变量时索引无效");
            self.status = self.text().no_pasteable_variables().to_string();
            return;
        };
        let value = match self.clipboard.get_text() {
            Ok(value) if !value.is_empty() => value,
            Ok(_) => {
                tracing::debug!(variable = %name, "系统剪贴板为空");
                self.status = self.text().clipboard_empty().to_string();
                return;
            }
            Err(error) => {
                tracing::error!(variable = %name, error = %error, "读取剪贴板失败");
                self.status = error;
                return;
            }
        };
        tracing::debug!(variable = %name, value_bytes = value.len(), "从剪贴板读取变量值");
        self.global_variables.insert(name.clone(), value);
        self.variable_index = index;
        self.status = self.text().pasted_to_variable(&name);
    }

    pub(crate) fn send_current_request(&mut self) {
        tracing::debug!(request_id = %self.current_request().id, "触发发送当前请求");
        if self.editing {
            self.commit_edit();
        }
        if self.request_status(&self.current_request().id) == RequestStatus::Sending {
            tracing::debug!("已有请求执行中，忽略重复发送");
            self.status = self.text().request_in_progress().to_string();
            return;
        }

        let request_id = self.current_request().id.clone();
        let operation_id = format!(
            "{}-{}",
            request_id,
            NEXT_REQUEST_OPERATION.fetch_add(1, Ordering::Relaxed)
        );
        let resolved = self.current_resolved_request();
        let display_url = template::display_url(self.current_request());
        let timeout = self.current_request().timeout_seconds;
        let file_directory = self.config.file_directory.clone();
        let download_directory = self.config.download_directory.clone();
        let sender = self.sender.clone();
        tracing::debug!(
            request_id = %request_id,
            operation_id = %operation_id,
            method = %resolved.method,
            timeout_seconds = timeout,
            file_directory = %file_directory.display(),
            download_directory = %download_directory.display(),
            "开始异步发送请求"
        );
        let state = self.request_states.entry(request_id.clone()).or_default();
        state.status = RequestStatus::Sending;
        state.response = None;
        state.error = None;
        state.operation_id = Some(operation_id.clone());
        self.status = self.text().request_started(&resolved.method, &display_url);

        thread::spawn(move || {
            tracing::debug!(operation_id = %operation_id, "HTTP 工作线程开始");
            let result = http::send(
                &resolved,
                timeout,
                &file_directory,
                &download_directory,
                &operation_id,
            );
            match &result {
                Ok(response) => tracing::debug!(
                    operation_id = %operation_id,
                    status = response.status,
                    elapsed_ms = response.elapsed_ms,
                    body_bytes = response.body.len(),
                    download_path = response
                        .download_path
                        .as_deref()
                        .map(|path| path.display().to_string()),
                    "HTTP 工作线程完成"
                ),
                Err(error) => tracing::error!(
                    operation_id = %operation_id,
                    error = %error,
                    "HTTP 工作线程失败"
                ),
            }
            if sender
                .send(AppMessage::RequestFinished {
                    request_id,
                    operation_id,
                    result,
                })
                .is_err()
            {
                tracing::debug!("TUI 已退出，丢弃后台请求结果");
            }
        });
    }

    fn current_variable_name(&self) -> Option<String> {
        self.current_variable_names()
            .get(self.variable_index)
            .cloned()
    }

    fn stop_editing(&mut self) {
        if self.editing {
            tracing::debug!("结束变量编辑状态");
        }
        self.editing = false;
        self.edit_buffer.clear();
    }
}

pub(crate) fn key_kind(code: KeyCode) -> &'static str {
    match code {
        KeyCode::Char(_) => "字符键",
        KeyCode::F(_) => "功能键",
        KeyCode::Enter => "回车",
        KeyCode::Esc => "Esc",
        KeyCode::Tab | KeyCode::BackTab => "Tab",
        KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => "方向键",
        KeyCode::Backspace => "退格",
        KeyCode::Delete => "删除",
        _ => "其他按键",
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::{config::load, http::ResponseData, settings::GlobalConfig};

    #[test]
    fn keeps_the_latest_response_for_each_request_during_the_session() {
        let config = load(Path::new(".postui/requests.yaml")).expect("示例请求配置应当可以加载");
        let first_id = config.requests[0].id.clone();
        let second_id = config.requests[1].id.clone();
        let mut app = App::new(
            config,
            PathBuf::from(".postui/requests.yaml"),
            GlobalConfig::default(),
        );

        assert_eq!(app.request_status(&first_id), RequestStatus::NotSent);
        assert_eq!(app.request_status(&second_id), RequestStatus::NotSent);

        let first_state = RequestRuntimeState::from_response(ResponseData {
            status: 200,
            reason: "OK".to_string(),
            headers: Vec::new(),
            body: "first".to_string(),
            download_path: None,
            elapsed_ms: 1,
        });
        app.request_states.insert(first_id.clone(), first_state);

        app.select_request(1);
        assert!(app.current_response().is_none());
        assert_eq!(app.request_status(&first_id), RequestStatus::Success);

        app.select_request(0);
        assert_eq!(
            app.current_response()
                .map(|response| response.body.as_str()),
            Some("first")
        );

        let latest_state = RequestRuntimeState {
            status: RequestStatus::Failed,
            error: Some("latest failure".to_string()),
            ..Default::default()
        };
        app.request_states.insert(first_id.clone(), latest_state);
        assert!(app.current_response().is_none());
        assert_eq!(app.current_error(), Some("latest failure"));
        assert_eq!(app.request_status(&first_id), RequestStatus::Failed);
    }

    #[test]
    fn extracts_response_value_directly_into_the_variable() {
        let config = load(Path::new(".postui/requests.yaml")).expect("示例请求配置应当可以加载");
        let mut app = App::new(
            config,
            PathBuf::from(".postui/requests.yaml"),
            GlobalConfig::default(),
        );
        app.select_request(1);
        let variable_index = app
            .current_variable_names()
            .iter()
            .position(|name| name == "posted_message")
            .expect("POST JSON 接口应声明提取变量");
        assert!(app.variable_has_extract(variable_index));
        assert!(!app.can_extract_variable(variable_index));
        app.request_states.insert(
            app.current_request().id.clone(),
            RequestRuntimeState::from_response(ResponseData {
                status: 200,
                reason: "OK".to_string(),
                headers: Vec::new(),
                body: r#"{"json":{"message":"from-response"}}"#.to_string(),
                download_path: None,
                elapsed_ms: 1,
            }),
        );
        assert!(app.can_extract_variable(variable_index));

        app.extract_variable(variable_index);

        assert_eq!(
            app.global_variables
                .get("posted_message")
                .map(String::as_str),
            Some("from-response")
        );
        assert_eq!(app.variable_index, variable_index);
    }
}
