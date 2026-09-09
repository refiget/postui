use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    clipboard::SystemClipboard,
    config::{ApiRequest, RequestConfig, value_to_string},
    http::{self, ResponseData},
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

    fn label(self) -> &'static str {
        match self {
            Self::Requests => "接口列表",
            Self::Variables => "全局变量",
            Self::Actions => "发送操作",
        }
    }
}

enum AppMessage {
    RequestFinished {
        request_id: String,
        operation_id: String,
        result: Result<ResponseData, String>,
    },
}

static NEXT_REQUEST_OPERATION: AtomicU64 = AtomicU64::new(1);

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
    pub(crate) responses: HashMap<String, ResponseData>,
    pub(crate) loading_request: Option<String>,
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
            responses: HashMap::new(),
            loading_request: None,
            status: "就绪".to_string(),
            should_quit: false,
            clipboard: SystemClipboard::default(),
            sender,
            receiver,
        }
    }

    pub(crate) fn current_request(&self) -> &ApiRequest {
        &self.config.requests[self.selected_request]
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
        if self.selected_request != index {
            self.stop_editing();
        }
        self.selected_request = index;
        self.variable_index = self
            .variable_index
            .min(self.current_variable_names().len().saturating_sub(1));
        if previous != index {
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

    pub(crate) fn current_resolved_request(&self) -> ResolvedRequest {
        template::resolve_request(self.current_request(), &self.global_variables)
    }

    pub(crate) fn current_response(&self) -> Option<&ResponseData> {
        self.responses.get(&self.current_request().id)
    }

    pub(crate) fn is_request_loading(&self, request_id: &str) -> bool {
        self.loading_request.as_deref() == Some(request_id)
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
                    if self.loading_request.as_deref() == Some(request_id.as_str()) {
                        self.loading_request = None;
                    }
                    match result {
                        Ok(response) => {
                            let status = response.status;
                            let elapsed = response.elapsed_ms;
                            tracing::debug!(
                                request_id = %request_id,
                                operation_id = %operation_id,
                                status,
                                elapsed_ms = elapsed,
                                header_count = response.headers.len(),
                                body_bytes = response.body.len(),
                                "后台请求成功"
                            );
                            self.responses.insert(request_id, response);
                            self.status = format!("请求完成 · HTTP {status} · {elapsed} ms");
                        }
                        Err(error) => {
                            tracing::error!(
                                request_id = %request_id,
                                operation_id = %operation_id,
                                error = %error,
                                "后台请求失败"
                            );
                            self.status = error;
                        }
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
                self.status = format!("已切换到 {}", self.focus.label());
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
                    format!("已选择 {name}")
                } else {
                    tracing::debug!("关闭接口下拉列表");
                    "已关闭接口列表".to_string()
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
                self.status = "已取消编辑".to_string();
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

    pub(crate) fn edit_current_variable(&mut self) {
        let Some(name) = self.current_variable_name() else {
            tracing::debug!("当前接口没有可编辑变量");
            self.status = "当前接口没有可编辑的变量".to_string();
            return;
        };
        self.edit_buffer = self
            .global_variables
            .get(&name)
            .cloned()
            .unwrap_or_default();
        tracing::debug!(variable = %name, value_bytes = self.edit_buffer.len(), "开始编辑全局变量");
        self.editing = true;
        self.status = format!("编辑变量 {name} · Enter 保存 · Esc 取消");
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
        self.status = format!("已更新全局变量: {name}");
    }

    pub(crate) fn clear_current_variable(&mut self) {
        self.clear_variable(self.variable_index);
    }

    pub(crate) fn clear_variable(&mut self, index: usize) {
        let Some(name) = self.current_variable_names().get(index).cloned() else {
            tracing::debug!(index, "清空变量时索引无效");
            self.status = "当前接口没有可清空的变量".to_string();
            return;
        };
        tracing::debug!(variable = %name, index, "清空全局变量");
        self.global_variables.insert(name.clone(), String::new());
        if self.variable_index == index {
            self.stop_editing();
        }
        self.status = format!("已清空全局变量: {name}");
    }

    pub(crate) fn extract_response(&mut self, index: usize) {
        let request_id = self.current_request().id.clone();
        let Some(extract) = self.current_request().extracts.get(index) else {
            tracing::debug!(request_id = %request_id, index, "响应提取索引无效");
            self.status = "当前接口没有可提取的字段".to_string();
            return;
        };
        let variable = extract.variable.clone();
        let path = extract.path.clone();
        let Some(body) = self
            .responses
            .get(&request_id)
            .map(|response| response.body.clone())
        else {
            tracing::debug!(request_id = %request_id, index, "当前接口还没有响应，无法提取");
            self.status = "当前接口还没有响应".to_string();
            return;
        };
        tracing::debug!(
            request_id = %request_id,
            index,
            variable = %variable,
            path = %path,
            response_body_bytes = body.len(),
            "提取响应字段"
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
            variable = %variable,
            value_bytes = value.len(),
            "响应字段提取成功，准备写入剪贴板"
        );
        match self.clipboard.set_text(&value) {
            Ok(()) => {
                tracing::debug!(request_id = %request_id, variable = %variable, "响应字段已复制到剪贴板");
                self.status = format!("已提取并复制: {variable}");
            }
            Err(error) => {
                tracing::error!(request_id = %request_id, variable = %variable, error = %error, "写入剪贴板失败");
                self.status = error;
            }
        }
    }

    pub(crate) fn paste_variable(&mut self, index: usize) {
        tracing::debug!(index, "从剪贴板填入变量");
        if self.editing {
            self.commit_edit();
        }
        let Some(name) = self.current_variable_names().get(index).cloned() else {
            tracing::debug!(index, "填入变量时索引无效");
            self.status = "当前接口没有可填入的变量".to_string();
            return;
        };
        let value = match self.clipboard.get_text() {
            Ok(value) if !value.is_empty() => value,
            Ok(_) => {
                tracing::debug!(variable = %name, "系统剪贴板为空");
                self.status = "系统剪贴板没有文本".to_string();
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
        self.status = format!("已从剪贴板填入全局变量: {name}");
    }

    pub(crate) fn send_current_request(&mut self) {
        tracing::debug!(request_id = %self.current_request().id, "触发发送当前请求");
        if self.editing {
            self.commit_edit();
        }
        if self.loading_request.is_some() {
            tracing::debug!("已有请求执行中，忽略重复发送");
            self.status = "请求正在执行，请稍候".to_string();
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
        let timeout = self.config.timeout_seconds;
        let file_directory = self.file_directory();
        let sender = self.sender.clone();
        tracing::debug!(
            request_id = %request_id,
            operation_id = %operation_id,
            method = %resolved.method,
            timeout_seconds = timeout,
            file_directory = %file_directory.display(),
            "开始异步发送请求"
        );
        self.loading_request = Some(request_id.clone());
        self.status = format!("请求中 · {} {}", resolved.method, display_url);

        thread::spawn(move || {
            tracing::debug!(operation_id = %operation_id, "HTTP 工作线程开始");
            let result = http::send(&resolved, timeout, &file_directory, &operation_id);
            match &result {
                Ok(response) => tracing::debug!(
                    operation_id = %operation_id,
                    status = response.status,
                    elapsed_ms = response.elapsed_ms,
                    body_bytes = response.body.len(),
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

    fn file_directory(&self) -> PathBuf {
        if self.config.file_directory.is_absolute() {
            return self.config.file_directory.clone();
        }
        self.config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&self.config.file_directory)
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
