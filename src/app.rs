use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use crate::{
    config::{ApiRequest, BodyPart, FileUpload, RequestConfig, value_to_string},
    editor::{
        BodyValueEditor, EditorAction, TextEditor, convert_json_scalar, json_scalar_at,
        merge_json_edit, terminal_width, text_position,
    },
    http::{self, HttpClient, HttpError, ResponseData},
    i18n::UiText,
    settings::GlobalConfig,
    template::{self, ResolvedRequest},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

mod dialog;

pub(crate) use dialog::{
    BodyPartSource, Dialog, DialogFocus, HeaderField, HeaderRow, HeaderSource, HeadersDialog,
    ParamSource, ParamsDialog, ParamsDialogRow, VariableRow, VariablesDialog,
};
use dialog::{DialogAction, remove_header, remove_header_map, split_key_value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Requests,
    Variables,
    Preview,
    Actions,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Requests => Self::Variables,
            Self::Variables => Self::Preview,
            Self::Preview => Self::Actions,
            Self::Actions => Self::Requests,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Requests => Self::Actions,
            Self::Variables => Self::Requests,
            Self::Preview => Self::Variables,
            Self::Actions => Self::Preview,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PreviewTab {
    #[default]
    Body,
    Params,
    Headers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreviewAction {
    Edit(PreviewTab),
    Send,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponseMenuAction {
    Download,
    Copy,
}

impl ResponseMenuAction {
    pub(crate) const fn all() -> [Self; 2] {
        [Self::Download, Self::Copy]
    }

    pub(crate) fn from_index(index: usize) -> Option<Self> {
        Self::all().get(index).copied()
    }
}

impl PreviewTab {
    pub(crate) const fn all() -> [Self; 3] {
        [Self::Body, Self::Params, Self::Headers]
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Body => Self::Params,
            Self::Params => Self::Headers,
            Self::Headers => Self::Body,
        }
    }

    pub(crate) fn previous(self) -> Self {
        match self {
            Self::Body => Self::Headers,
            Self::Params => Self::Body,
            Self::Headers => Self::Params,
        }
    }

    pub(crate) fn label(self, text: UiText) -> &'static str {
        match self {
            Self::Body => text.body(),
            Self::Params => text.params(),
            Self::Headers => text.headers(),
        }
    }
}

#[derive(Debug)]
enum AppMessage {
    RequestFinished {
        request_id: String,
        operation_id: String,
        result: Result<ResponseData, HttpError>,
    },
}

static NEXT_REQUEST_OPERATION: AtomicU64 = AtomicU64::new(1);
fn blank_request(id: String, timeout_seconds: u64) -> ApiRequest {
    ApiRequest {
        id,
        name: "Untitled request".to_string(),
        method: "GET".to_string(),
        url: String::new(),
        timeout_seconds,
        description: String::new(),
        headers: BTreeMap::new(),
        body_parts: Vec::new(),
        query_parts: Vec::new(),
        form: BTreeMap::new(),
        files: Vec::new(),
        extracts: Vec::new(),
    }
}

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

    pub(crate) fn move_by(&mut self, direction: isize) -> bool {
        let previous = self.offset;
        self.offset = match direction {
            -1 => self.offset.saturating_sub(Self::STEP),
            1 => self.offset.saturating_add(Self::STEP),
            _ => self.offset,
        };
        self.offset != previous
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

    pub(crate) fn label(self, text: UiText) -> &'static str {
        match self {
            Self::NotSent => text.request_status_not_sent(),
            Self::Sending => text.request_status_sending(),
            Self::Success => text.request_status_success(),
            Self::Failed => text.request_status_failed(),
            Self::Timeout => text.request_status_timeout(),
        }
    }

    pub(crate) fn error_message(self, text: UiText, error: &str) -> String {
        if self == Self::Timeout {
            text.request_timeout(error)
        } else {
            text.request_failed(error)
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RequestRuntimeState {
    pub(crate) status: RequestStatus,
    pub(crate) response: Option<ResponseData>,
    pub(crate) error: Option<String>,
    message: Option<String>,
    operation_id: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct RequestsContentState {
    pub(crate) selected_request: usize,
}

#[derive(Debug, Default)]
pub(crate) struct PreviewContentState {
    pub(crate) active_tab: PreviewTab,
    pub(crate) scroll: ScrollState,
    pub(crate) editor: Option<BodyValueEditor>,
    pub(crate) file_editor: Option<FileValueEditor>,
    pub(crate) variable_editor: Option<RequestVariableEditor>,
    pub(crate) url_editor: Option<TextEditor>,
}

#[derive(Debug, Clone)]
pub(crate) enum AppPrompt {
    ConfirmExit,
    ConfirmDelete { request_id: String },
}

#[derive(Debug)]
pub(crate) struct FileValueEditor {
    pub(crate) file_index: usize,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) input: TextEditor,
}

#[derive(Debug)]
pub(crate) struct RequestVariableEditor {
    pub(crate) variable: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) input: TextEditor,
}

#[derive(Debug, Default)]
pub(crate) struct ResponseContentState {
    pub(crate) scroll: ScrollState,
    pub(crate) menu_open: bool,
    pub(crate) menu_selected: usize,
}

#[derive(Debug, Default)]
pub(crate) struct WorkspaceState {
    pub(crate) variables: BTreeMap<String, String>,
    pub(crate) request_edits: HashMap<String, RequestEdits>,
    pub(crate) request_states: HashMap<String, RequestRuntimeState>,
    pub(crate) dirty_requests: HashSet<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct RequestEdits {
    pub(crate) url: Option<String>,
    pub(crate) headers: Vec<HeaderRow>,
    pub(crate) query_parts: Vec<BodyPart>,
    pub(crate) form: BTreeMap<String, String>,
    pub(crate) files: Vec<FileUpload>,
    pub(crate) body_parts: Vec<BodyPart>,
}

impl From<&ApiRequest> for RequestEdits {
    fn from(request: &ApiRequest) -> Self {
        Self {
            url: None,
            headers: request
                .headers
                .iter()
                .map(|(name, value)| HeaderRow {
                    name: name.clone(),
                    value: value.clone(),
                    enabled: true,
                    source: HeaderSource::Request,
                })
                .collect(),
            query_parts: request.query_parts.clone(),
            form: request.form.clone(),
            files: request.files.clone(),
            body_parts: request.body_parts.clone(),
        }
    }
}

impl WorkspaceState {
    fn from_config(config: &RequestConfig) -> Self {
        let variables = config
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
            .collect();
        Self {
            variables,
            request_edits: config
                .requests
                .iter()
                .map(|request| (request.id.clone(), RequestEdits::from(request)))
                .collect(),
            request_states: config
                .requests
                .iter()
                .map(|request| (request.id.clone(), RequestRuntimeState::default()))
                .collect(),
            dirty_requests: HashSet::new(),
        }
    }
}

pub(crate) struct App {
    pub(crate) config: RequestConfig,
    pub(crate) config_path: PathBuf,
    pub(crate) global_config: GlobalConfig,
    pub(crate) focus: Focus,
    pub(crate) requests_state: RequestsContentState,
    pub(crate) preview_state: PreviewContentState,
    pub(crate) response_state: ResponseContentState,
    pub(crate) workspace_state: WorkspaceState,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) prompt: Option<AppPrompt>,
    pub(crate) status: String,
    pub(crate) animation_frame: usize,
    pub(crate) should_quit: bool,
    empty_request: ApiRequest,
    http_client: HttpClient,
    sender: Sender<AppMessage>,
    receiver: Receiver<AppMessage>,
}

impl App {
    pub(crate) fn new(
        config: RequestConfig,
        config_path: PathBuf,
        global_config: GlobalConfig,
        http_client: HttpClient,
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
            configured_variable_count = config.editable_variables.len(),
            "创建应用状态"
        );
        let workspace_state = WorkspaceState::from_config(&config);
        let empty_request = blank_request("__empty__".to_string(), config.timeout_seconds);

        let (sender, receiver) = mpsc::channel();
        Self {
            config,
            config_path,
            global_config,
            focus: Focus::Requests,
            requests_state: RequestsContentState {
                selected_request: 0,
            },
            preview_state: PreviewContentState {
                active_tab: PreviewTab::Body,
                scroll: ScrollState::default(),
                editor: None,
                file_editor: None,
                variable_editor: None,
                url_editor: None,
            },
            response_state: ResponseContentState::default(),
            workspace_state,
            dialog: None,
            prompt: None,
            status: text.ready().to_string(),
            animation_frame: 0,
            should_quit: false,
            empty_request,
            http_client,
            sender,
            receiver,
        }
    }

    pub(crate) fn current_request(&self) -> &ApiRequest {
        self.config
            .requests
            .get(self.requests_state.selected_request)
            .unwrap_or(&self.empty_request)
    }

    pub(crate) fn has_current_request(&self) -> bool {
        self.requests_state.selected_request < self.config.requests.len()
    }

    pub(crate) fn text(&self) -> UiText {
        UiText::new(self.global_config.language)
    }

    pub(crate) fn advance_animation(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
    }

    pub(crate) fn is_request_dirty(&self, request_id: &str) -> bool {
        self.workspace_state.dirty_requests.contains(request_id)
    }

    fn mark_current_dirty(&mut self) {
        if self.has_current_request() {
            self.workspace_state
                .dirty_requests
                .insert(self.current_request().id.clone());
        }
    }

    pub(crate) fn start_url_edit(&mut self) {
        if !self.has_current_request()
            || self.request_status(&self.current_request().id) == RequestStatus::Sending
        {
            return;
        }
        let url = self.current_effective_request().url;
        self.preview_state.url_editor = Some(TextEditor::new(url));
        self.focus = Focus::Preview;
    }

    pub(crate) fn cycle_method(&mut self) {
        if !self.has_current_request()
            || self.request_status(&self.current_request().id) == RequestStatus::Sending
        {
            return;
        }
        const METHODS: [&str; 2] = ["GET", "POST"];
        let current = self.current_request().method.as_str();
        let index = METHODS
            .iter()
            .position(|method| *method == current)
            .unwrap_or(0);
        self.config.requests[self.requests_state.selected_request].method =
            METHODS[(index + 1) % METHODS.len()].to_string();
        self.mark_current_dirty();
    }

    fn commit_url_edit(&mut self) {
        let Some(editor) = self.preview_state.url_editor.take() else {
            return;
        };
        if !self.has_current_request() {
            return;
        }
        let request_id = self.current_request().id.clone();
        let configured = self.current_request().url.clone();
        let value = editor.value.trim().to_string();
        self.request_edits_mut(&request_id).url = (value != configured).then_some(value);
        self.mark_current_dirty();
    }

    fn handle_url_editor_key(&mut self, key: KeyEvent) {
        let Some(editor) = self.preview_state.url_editor.as_mut() else {
            return;
        };
        match editor.handle_key(key) {
            EditorAction::Continue => {}
            EditorAction::Commit => self.commit_url_edit(),
            EditorAction::Cancel => self.preview_state.url_editor = None,
        }
    }

    pub(crate) fn save_current_request(&mut self) {
        self.commit_active_editors();
        if !self.has_current_request() {
            return;
        }
        let id = self.current_request().id.clone();
        match self.write_request_file(&id) {
            Ok(path) => {
                self.workspace_state.dirty_requests.remove(&id);
                self.status = self.text().request_saved(&path.display().to_string());
            }
            Err(error) => self.status = self.text().request_save_failed(&error.to_string()),
        }
    }

    fn write_request_file(&self, request_id: &str) -> anyhow::Result<PathBuf> {
        let relative = request_id
            .strip_prefix("requests/")
            .ok_or_else(|| anyhow::anyhow!("请求没有可写入的源文件"))?;
        let path = self.config_path.join("requests").join(relative);
        self.write_request_to_path(&path)?;
        Ok(path)
    }

    fn write_request_to_path(&self, path: &Path) -> anyhow::Result<()> {
        let mut request = self.current_effective_request();
        request.headers = self
            .request_edits(&request.id)
            .headers
            .iter()
            .filter(|row| row.enabled && !row.name.trim().is_empty())
            .map(|row| (row.name.trim().to_string(), row.value.clone()))
            .collect();
        let text = serialize_request(&request);
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("请求路径缺少父目录"))?;
        fs::create_dir_all(parent)?;
        let temporary = path.with_extension("http.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, path)?;
        Ok(())
    }

    fn request_quit(&mut self) {
        self.commit_active_editors();
        if self.workspace_state.dirty_requests.is_empty() {
            self.should_quit = true;
        } else {
            self.prompt = Some(AppPrompt::ConfirmExit);
        }
    }

    fn request_delete(&mut self) {
        if !self.has_current_request() {
            return;
        }
        let request_id = self.current_request().id.clone();
        if self.request_status(&request_id) == RequestStatus::Sending {
            self.status = self.text().request_in_progress().to_string();
            return;
        }
        self.prompt = Some(AppPrompt::ConfirmDelete { request_id });
    }

    fn delete_request(&mut self, request_id: &str) {
        let Some(index) = self
            .config
            .requests
            .iter()
            .position(|request| request.id == request_id)
        else {
            self.prompt = None;
            return;
        };
        let Some(relative) = request_id.strip_prefix("requests/") else {
            self.status = self.text().request_delete_failed("请求源路径无效");
            return;
        };
        let path = self.config_path.join("requests").join(relative);
        if let Err(error) = fs::remove_file(&path) {
            self.status = self.text().request_delete_failed(&error.to_string());
            return;
        }
        self.config.requests.remove(index);
        self.workspace_state.request_edits.remove(request_id);
        self.workspace_state.request_states.remove(request_id);
        self.workspace_state.dirty_requests.remove(request_id);
        self.requests_state.selected_request =
            index.min(self.config.requests.len().saturating_sub(1));
        self.preview_state = PreviewContentState::default();
        self.response_state = ResponseContentState::default();
        self.dialog = None;
        self.prompt = None;
        self.status = self.text().request_deleted().to_string();
    }

    fn handle_prompt_key(&mut self, key: KeyEvent) {
        match self.prompt.as_mut() {
            Some(AppPrompt::ConfirmExit) => match key.code {
                KeyCode::Char('y' | 'Y') => self.should_quit = true,
                KeyCode::Char('n' | 'N') | KeyCode::Esc => self.prompt = None,
                _ => {}
            },
            Some(AppPrompt::ConfirmDelete { request_id }) => match key.code {
                KeyCode::Char('y' | 'Y') => {
                    let request_id = request_id.clone();
                    self.delete_request(&request_id);
                }
                KeyCode::Char('n' | 'N') | KeyCode::Esc => self.prompt = None,
                _ => {}
            },
            None => {}
        }
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
        let previous = self.requests_state.selected_request;
        let changed = previous != index;
        if changed {
            self.commit_active_editors();
            self.close_response_menu();
            if self.editing_preview_tab().is_some() {
                self.dialog = None;
            }
            self.requests_state.selected_request = index;
            self.preview_state.active_tab = PreviewTab::Body;
            self.preview_state.scroll.reset();
            self.response_state.scroll.reset();
            self.status = self
                .workspace_state
                .request_states
                .get(&self.current_request().id)
                .and_then(|state| state.message.clone())
                .unwrap_or_else(|| self.text().ready().to_string());
            tracing::debug!(
                previous_index = previous,
                selected_index = index,
                request_id = %self.current_request().id,
                "切换当前接口"
            );
        }
    }

    pub(crate) fn current_resolved_request(&self) -> ResolvedRequest {
        template::resolve_request(
            &self.current_effective_request(),
            &self.workspace_state.variables,
        )
    }

    pub(crate) fn current_effective_request(&self) -> ApiRequest {
        self.effective_request(self.current_request())
    }

    fn effective_request(&self, request: &ApiRequest) -> ApiRequest {
        let edits = self.request_edits(&request.id);
        let mut effective = request.clone();
        if let Some(url) = &edits.url {
            effective.url = url.clone();
        }
        effective.headers = self.effective_request_headers(&request.id);
        effective.query_parts = edits.query_parts.clone();
        effective.form = edits.form.clone();
        effective.files = edits.files.clone();
        effective.body_parts = edits.body_parts.clone();
        effective
    }

    fn request_edits(&self, request_id: &str) -> &RequestEdits {
        self.workspace_state
            .request_edits
            .get(request_id)
            .expect("every configured request has session edits")
    }

    fn request_edits_mut(&mut self, request_id: &str) -> &mut RequestEdits {
        self.workspace_state
            .request_edits
            .get_mut(request_id)
            .expect("every configured request has session edits")
    }

    pub(crate) fn current_url_variables(&self) -> Vec<String> {
        let request = self.current_effective_request();
        template::url_variable_names(&request.url)
    }

    pub(crate) fn request_variable_value(&self, variable: &str) -> String {
        self.workspace_state
            .variables
            .get(variable)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn resolved_url(&self, request: &ApiRequest) -> String {
        let request = self.effective_request(request);
        let url = template::display_url(&request);
        template::display_text_parts(&url, &self.workspace_state.variables)
            .into_iter()
            .map(|part| part.text)
            .collect()
    }

    pub(crate) fn body_json(&self) -> String {
        let body = self
            .current_resolved_request()
            .raw_body
            .unwrap_or_else(|| "{}".to_string());
        serde_json::from_str::<serde_json::Value>(&body).map_or(body, |value| {
            serde_json::to_string_pretty(&value).expect("JSON 请求体应可序列化")
        })
    }

    pub(crate) fn body_preview(&self) -> String {
        let request = self.current_effective_request();
        if !request.body_parts.is_empty()
            && request
                .body_parts
                .iter()
                .all(|part| matches!(part, BodyPart::UrlEncoded(_)))
        {
            return self
                .current_resolved_request()
                .raw_body
                .unwrap_or_default()
                .split('&')
                .map(template::decode_urlencoded_data)
                .collect::<Vec<_>>()
                .join("\n");
        }
        self.body_json()
    }

    pub(crate) fn start_body_edit(&mut self, line: usize, column: usize) {
        if self.request_status(&self.current_request().id) == RequestStatus::Sending {
            return;
        }
        if self.preview_state.editor.is_some()
            || self.preview_state.file_editor.is_some()
            || self.preview_state.variable_editor.is_some()
        {
            return;
        }
        let Some(body) = self.current_resolved_request().raw_body else {
            self.start_file_edit(line, column);
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) else {
            return;
        };
        let Ok(document) = serde_json::to_string_pretty(&value) else {
            return;
        };
        let offset = text_position(&document, line, column);
        let Some((span, kind, input)) = json_scalar_at(&document, offset) else {
            return;
        };
        self.preview_state.editor = Some(BodyValueEditor {
            document,
            span,
            kind,
            input: TextEditor::new(input),
        });
        self.focus = Focus::Preview;
    }

    pub(crate) fn body_editor(&self) -> Option<&BodyValueEditor> {
        self.preview_state.editor.as_ref()
    }

    pub(crate) fn file_editor(&self) -> Option<&FileValueEditor> {
        self.preview_state.file_editor.as_ref()
    }

    pub(crate) fn variable_editor(&self) -> Option<&RequestVariableEditor> {
        self.preview_state.variable_editor.as_ref()
    }

    pub(crate) fn start_request_variable_edit(&mut self, variable: String, line: usize) {
        if self.request_status(&self.current_request().id) == RequestStatus::Sending
            || self.preview_state.editor.is_some()
            || self.preview_state.file_editor.is_some()
            || self.preview_state.variable_editor.is_some()
        {
            return;
        }
        let value = self.request_variable_value(&variable);
        self.preview_state.variable_editor = Some(RequestVariableEditor {
            column: terminal_width(&variable).saturating_add(2),
            variable,
            line,
            input: TextEditor::new(value),
        });
        self.focus = Focus::Preview;
    }

    fn start_file_edit(&mut self, line: usize, column: usize) {
        let request = self.current_resolved_request();
        let variable_lines = self.current_url_variables().len();
        let has_other_content = !request.form.is_empty() || !request.files.is_empty();
        let content_offset =
            variable_lines.saturating_add(usize::from(variable_lines > 0 && has_other_content));
        let mut file_line = content_offset.saturating_add(if request.form.is_empty() {
            1
        } else {
            request.form.len().saturating_add(3)
        });
        let Some((file_index, file)) = request
            .files
            .iter()
            .enumerate()
            .find(|(index, _)| file_line.saturating_add(*index) == line)
        else {
            return;
        };
        file_line = file_line.saturating_add(file_index);
        let value_column = terminal_width(&file.field).saturating_add(2);
        let value_columns =
            value_column..value_column.saturating_add(terminal_width(&file.path).max(1));
        if !value_columns.contains(&column) {
            return;
        }
        let Some(configured_path) = self
            .request_edits(&self.current_request().id)
            .files
            .get(file_index)
            .map(|file| file.path.clone())
        else {
            return;
        };
        self.preview_state.file_editor = Some(FileValueEditor {
            file_index,
            line: file_line,
            column: value_column,
            input: TextEditor::new(configured_path),
        });
        self.focus = Focus::Preview;
    }

    pub(crate) fn commit_active_editors(&mut self) {
        if self.preview_state.url_editor.is_some() {
            self.commit_url_edit();
        }
        if self.preview_state.editor.is_some() {
            self.commit_body_value();
        }
        if self.preview_state.file_editor.is_some() {
            self.commit_file_value();
        }
        if self.preview_state.variable_editor.is_some() {
            self.commit_request_variable();
        }
        if self.editing_preview_tab().is_some() {
            self.persist_request_edits();
        }
    }

    fn handle_body_editor_key(&mut self, key: KeyEvent) {
        if self.preview_state.variable_editor.is_some() {
            self.handle_request_variable_key(key);
            return;
        }
        if self.preview_state.file_editor.is_some() {
            self.handle_file_editor_key(key);
            return;
        }
        if key.code == KeyCode::Esc {
            self.preview_state.editor = None;
            return;
        }
        if key.code == KeyCode::Enter {
            self.commit_body_value();
            return;
        }
        let Some(editor) = self.preview_state.editor.as_mut() else {
            return;
        };
        let _ = editor.input.handle_key(key);
    }

    fn handle_file_editor_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.preview_state.file_editor = None;
            return;
        }
        if key.code == KeyCode::Enter {
            self.commit_file_value();
            return;
        }
        if let Some(editor) = self.preview_state.file_editor.as_mut() {
            let _ = editor.input.handle_key(key);
        }
    }

    fn commit_file_value(&mut self) {
        let Some(editor) = self.preview_state.file_editor.take() else {
            return;
        };
        let request_id = self.current_request().id.clone();
        let default = self
            .current_request()
            .files
            .get(editor.file_index)
            .map(|file| file.path.clone())
            .unwrap_or_default();
        let value = editor.input.value.trim();
        let path = if value.is_empty() {
            default
        } else {
            value.to_string()
        };
        if let Some(file) = self
            .request_edits_mut(&request_id)
            .files
            .get_mut(editor.file_index)
        {
            file.path = path;
            self.mark_current_dirty();
        }
    }

    fn handle_request_variable_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.preview_state.variable_editor = None;
            return;
        }
        if key.code == KeyCode::Enter {
            self.commit_request_variable();
            return;
        }
        if let Some(editor) = self.preview_state.variable_editor.as_mut() {
            let _ = editor.input.handle_key(key);
        }
    }

    fn commit_request_variable(&mut self) {
        let Some(editor) = self.preview_state.variable_editor.take() else {
            return;
        };
        self.workspace_state
            .variables
            .insert(editor.variable, editor.input.value);
    }

    fn commit_body_value(&mut self) {
        let Some(editor) = self.preview_state.editor.take() else {
            return;
        };
        let Some(replacement) = convert_json_scalar(editor.kind, &editor.input.value) else {
            self.status = self.text().invalid_body_value().to_string();
            return;
        };
        let rendered_document = editor.document;
        let mut document = rendered_document.clone();
        document.replace_range(editor.span, &replacement);
        let request_id = self.current_request().id.clone();
        let source_document = self
            .request_edits(&request_id)
            .body_parts
            .iter()
            .map(template::body_part_value)
            .collect::<Vec<_>>()
            .join("&");
        let document =
            merge_json_edit(&source_document, &rendered_document, &document).unwrap_or(document);
        self.request_edits_mut(&request_id).body_parts = vec![BodyPart::Raw(document)];
        self.mark_current_dirty();
    }

    pub(crate) fn variable_count(&self) -> usize {
        self.config.editable_variables.len()
    }

    pub(crate) fn current_header_count(&self) -> usize {
        self.effective_request_headers(&self.current_request().id)
            .len()
    }

    pub(crate) fn current_param_count(&self) -> usize {
        let request = self.current_effective_request();
        let url_parts = template::split_url_query(&request.url);
        let url_count = split_query_parts(&url_parts.query).count();
        let body_count = self
            .request_edits(&self.current_request().id)
            .body_parts
            .iter()
            .filter(|part| matches!(part, BodyPart::UrlEncoded(_)))
            .count();
        url_count + request.query_parts.len() + request.form.len() + body_count
    }

    pub(crate) fn open_variables(&mut self) {
        let rows = self
            .config
            .editable_variables
            .iter()
            .map(|name| VariableRow {
                name: name.clone(),
                value: self
                    .workspace_state
                    .variables
                    .get(name)
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        self.dialog = Some(Dialog::Variables(VariablesDialog {
            rows,
            selected: 0,
            focus: DialogFocus::Content,
            editor: None,
        }));
        self.focus = Focus::Variables;
        tracing::debug!(variable_count = self.variable_count(), "打开工作区变量窗口");
    }

    pub(crate) fn open_headers(&mut self) {
        if !self.has_current_request() {
            return;
        }
        if self.request_status(&self.current_request().id) == RequestStatus::Sending {
            tracing::debug!("请求执行中，忽略打开 Header 编辑窗口");
            self.status = self.text().request_in_progress().to_string();
            return;
        }
        self.preview_state.active_tab = PreviewTab::Headers;
        self.dialog = self.preview_dialog(PreviewTab::Headers);
        self.focus = Focus::Preview;
        tracing::debug!(
            header_count = self.current_header_count(),
            "打开请求 Header 窗口"
        );
    }

    pub(crate) fn open_params(&mut self) {
        if !self.has_current_request() {
            return;
        }
        if self.request_status(&self.current_request().id) == RequestStatus::Sending {
            tracing::debug!("请求执行中，忽略打开参数编辑窗口");
            self.status = self.text().request_in_progress().to_string();
            return;
        }
        self.preview_state.active_tab = PreviewTab::Params;
        self.dialog = self.preview_dialog(PreviewTab::Params);
        self.focus = Focus::Preview;
        tracing::debug!(
            query_row_count = self
                .request_edits(&self.current_request().id)
                .query_parts
                .len(),
            form_field_count = self.request_edits(&self.current_request().id).form.len(),
            "打开参数窗口"
        );
    }

    pub(crate) fn preview_dialog(&self, tab: PreviewTab) -> Option<Dialog> {
        match tab {
            PreviewTab::Body => None,
            PreviewTab::Headers => {
                let request_id = self.current_request().id.clone();
                let request_rows = self.request_edits(&request_id).headers.clone();
                let mut rows = self
                    .config
                    .headers
                    .iter()
                    .filter(|(name, _)| {
                        !request_rows
                            .iter()
                            .any(|row| row.name.eq_ignore_ascii_case(name))
                    })
                    .map(|(name, value)| HeaderRow {
                        name: name.clone(),
                        value: value.clone(),
                        enabled: true,
                        source: HeaderSource::Collection,
                    })
                    .collect::<Vec<_>>();
                rows.extend(request_rows);
                Some(Dialog::Headers(HeadersDialog {
                    request_id,
                    rows,
                    selected: 0,
                    field: HeaderField::Value,
                    editor: None,
                }))
            }
            PreviewTab::Params => {
                let request_id = self.current_request().id.clone();
                let edits = self.request_edits(&request_id);
                let query_parts = self.request_edits(&request_id).query_parts.clone();
                let mut rows = Vec::new();
                let effective_url = edits.url.as_deref().unwrap_or(&self.current_request().url);
                let url_parts = template::split_url_query(effective_url);
                for part in split_query_parts(&url_parts.query) {
                    let (key, value, has_equals) = split_key_value(part);
                    rows.push(ParamsDialogRow {
                        source: ParamSource::Url,
                        key,
                        value,
                        part_type: None,
                        has_equals,
                    });
                }
                for part in query_parts {
                    let (part_type, part) = match part {
                        BodyPart::Raw(part) => (BodyPartSource::Raw, part),
                        BodyPart::UrlEncoded(part) => (BodyPartSource::UrlEncoded, part),
                    };
                    let (key, value, has_equals) = split_key_value(&part);
                    rows.push(ParamsDialogRow {
                        source: ParamSource::Query,
                        key,
                        value,
                        part_type: Some(part_type),
                        has_equals,
                    });
                }
                for (key, value) in &self.request_edits(&request_id).form {
                    rows.push(ParamsDialogRow {
                        source: ParamSource::Form,
                        key: key.clone(),
                        value: value.clone(),
                        part_type: None,
                        has_equals: true,
                    });
                }
                if edits
                    .body_parts
                    .iter()
                    .all(|part| matches!(part, BodyPart::UrlEncoded(_)))
                {
                    for part in &edits.body_parts {
                        let BodyPart::UrlEncoded(part) = part else {
                            unreachable!();
                        };
                        let (key, value, has_equals) = split_key_value(part);
                        rows.push(ParamsDialogRow {
                            source: ParamSource::Body,
                            key,
                            value,
                            part_type: Some(BodyPartSource::UrlEncoded),
                            has_equals,
                        });
                    }
                }

                Some(Dialog::Params(ParamsDialog {
                    request_id,
                    rows,
                    selected: 0,
                    field: HeaderField::Name,
                    editor: None,
                }))
            }
        }
    }

    pub(crate) fn handle_preview_action(&mut self, action: PreviewAction) {
        match action {
            PreviewAction::Send => self.send_current_request(),
            PreviewAction::Edit(tab) if self.editing_preview_tab() == Some(tab) => {
                if let Some(dialog) = self.dialog.as_mut() {
                    dialog.commit_editor();
                }
                self.apply_dialog();
            }
            PreviewAction::Edit(PreviewTab::Body) => {
                if self.preview_state.editor.is_some()
                    || self.preview_state.file_editor.is_some()
                    || self.preview_state.variable_editor.is_some()
                {
                    self.preview_state.editor = None;
                    self.preview_state.file_editor = None;
                    self.preview_state.variable_editor = None;
                } else {
                    self.start_body_edit(0, 0);
                }
            }
            PreviewAction::Edit(PreviewTab::Params) => self.open_params(),
            PreviewAction::Edit(PreviewTab::Headers) => self.open_headers(),
        }
    }

    pub(crate) fn editing_preview_tab(&self) -> Option<PreviewTab> {
        self.dialog.as_ref().and_then(Dialog::preview_tab)
    }

    pub(crate) fn can_execute_preview_action(&self, action: PreviewAction) -> bool {
        match action {
            PreviewAction::Send => {
                self.has_current_request()
                    && !self.current_effective_request().url.trim().is_empty()
                    && supports_method(&self.current_effective_request().method)
                    && self.request_status(&self.current_request().id) != RequestStatus::Sending
                    && !matches!(self.dialog, Some(Dialog::Variables(_)))
                    && self.preview_state.editor.is_none()
                    && self.preview_state.file_editor.is_none()
                    && self.preview_state.variable_editor.is_none()
            }
            PreviewAction::Edit(tab) => {
                self.has_current_request()
                    && self
                        .editing_preview_tab()
                        .is_none_or(|editing_tab| editing_tab == tab)
                    && self.request_status(&self.current_request().id) != RequestStatus::Sending
            }
        }
    }

    pub(crate) fn focused_preview_action(&self) -> Option<PreviewAction> {
        match self.focus {
            Focus::Actions => Some(PreviewAction::Send),
            _ => None,
        }
    }

    pub(crate) fn close_dialog(&mut self) {
        if self.dialog.take().is_some() {
            tracing::debug!("关闭配置编辑窗口");
        }
    }

    pub(crate) fn apply_dialog(&mut self) {
        if self.dialog.is_none() {
            return;
        }
        self.persist_request_edits();

        let Some(dialog) = self.dialog.take() else {
            return;
        };
        match dialog {
            Dialog::Variables(dialog) => {
                for row in dialog.rows {
                    self.workspace_state.variables.insert(row.name, row.value);
                }
                self.status = self.text().variables_applied().to_string();
                tracing::debug!(
                    variable_count = self.workspace_state.variables.len(),
                    "应用工作区变量修改"
                );
            }
            Dialog::Headers(dialog) => {
                let header_count = self
                    .request_edits(&dialog.request_id)
                    .headers
                    .iter()
                    .filter(|row| row.enabled)
                    .count();
                self.status = self.text().headers_applied().to_string();
                tracing::debug!(header_count, "应用请求 Header 修改");
            }
            Dialog::Params(dialog) => {
                let (query_part_count, form_field_count) = {
                    let edits = self.request_edits(&dialog.request_id);
                    (edits.query_parts.len(), edits.form.len())
                };
                self.status = self.text().params_applied().to_string();
                tracing::debug!(query_part_count, form_field_count, "应用请求参数修改");
            }
        }
    }

    pub(crate) fn handle_dialog_key(&mut self, key: KeyEvent) {
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        let action = dialog.handle_key(key);
        match action {
            DialogAction::None => {}
            DialogAction::Changed => {
                self.persist_request_edits();
                self.mark_current_dirty();
            }
            DialogAction::Apply => self.apply_dialog(),
            DialogAction::Cancel => {
                self.persist_request_edits();
                self.close_dialog();
            }
        }
    }

    fn persist_request_edits(&mut self) {
        let (dialog, request_edits) = (&mut self.dialog, &mut self.workspace_state.request_edits);
        let Some(dialog) = dialog.as_mut() else {
            return;
        };
        dialog.commit_editor();
        match dialog {
            Dialog::Headers(dialog) => {
                let rows = dialog
                    .rows
                    .iter()
                    .filter(|row| {
                        row.source == HeaderSource::Request && !row.name.trim().is_empty()
                    })
                    .fold(Vec::new(), |mut rows, row| {
                        remove_header(&mut rows, &row.name);
                        rows.push(HeaderRow {
                            name: row.name.trim().to_string(),
                            ..row.clone()
                        });
                        rows
                    });
                request_edits
                    .get_mut(&dialog.request_id)
                    .expect("every configured request has session edits")
                    .headers = rows;
            }
            Dialog::Params(dialog) => {
                let edits = request_edits
                    .get_mut(&dialog.request_id)
                    .expect("every configured request has session edits");
                let configured_url = self
                    .config
                    .requests
                    .iter()
                    .find(|request| request.id == dialog.request_id)
                    .map(|request| request.url.as_str())
                    .unwrap_or_default();
                let effective_url = edits.url.as_deref().unwrap_or(configured_url);
                let url_location = template::split_url_query(effective_url);
                let mut url_parts = Vec::new();
                let mut query_parts = Vec::new();
                let mut form = BTreeMap::new();
                let mut body_parts = Vec::new();
                for row in &dialog.rows {
                    let key = row.key.trim();
                    let value = row.value.trim();
                    match row.source {
                        ParamSource::Url if !key.is_empty() || !value.is_empty() => {
                            url_parts.push(join_param_row(key, value, row.has_equals));
                        }
                        ParamSource::Query if !key.is_empty() || !value.is_empty() => {
                            let part = join_param_row(key, value, row.has_equals);
                            query_parts
                                .push(row.part_type.unwrap_or(BodyPartSource::Raw).to_part(part));
                        }
                        ParamSource::Form if !key.is_empty() => {
                            form.insert(key.to_string(), row.value.clone());
                        }
                        ParamSource::Body if !key.is_empty() || !value.is_empty() => {
                            let part = join_param_row(key, value, row.has_equals);
                            body_parts.push(
                                row.part_type
                                    .unwrap_or(BodyPartSource::UrlEncoded)
                                    .to_part(part),
                            );
                        }
                        _ => {}
                    }
                }
                edits.url = Some(template::rebuild_url(
                    &url_location.base,
                    &url_parts,
                    &url_location.fragment,
                ));
                edits.query_parts = query_parts;
                edits.form = form;
                if !body_parts.is_empty()
                    || edits
                        .body_parts
                        .iter()
                        .all(|part| matches!(part, BodyPart::UrlEncoded(_)))
                {
                    edits.body_parts = body_parts;
                }
            }
            Dialog::Variables(_) => {}
        }
    }

    pub(crate) fn move_dialog_selection(&mut self, direction: isize) {
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.move_selection(direction);
            tracing::debug!(direction, "移动配置窗口列表选择");
        }
        self.persist_request_edits();
    }

    pub(crate) fn click_variable_row(&mut self, index: usize, edit: bool) {
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.commit_editor();
            dialog.click_variable_row(index, edit);
        }
    }

    pub(crate) fn click_param_row(&mut self, index: usize, field: HeaderField, edit: bool) {
        let changed = self.dialog.as_ref().is_some_and(Dialog::is_editing);
        self.persist_request_edits();
        if changed {
            self.mark_current_dirty();
        }
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.click_param_row(index, field, edit);
        }
    }

    pub(crate) fn click_header_row(&mut self, index: usize, field: HeaderField, edit: bool) {
        let changed = self.dialog.as_ref().is_some_and(Dialog::is_editing);
        self.persist_request_edits();
        if changed {
            self.mark_current_dirty();
        }
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.click_header_row(index, field, edit);
        }
    }

    pub(crate) fn toggle_header_row(&mut self, index: usize) {
        self.persist_request_edits();
        if let Some(Dialog::Headers(dialog)) = self.dialog.as_mut() {
            dialog.selected = index;
            dialog.toggle_selected();
        }
        self.persist_request_edits();
        self.mark_current_dirty();
    }

    pub(crate) fn focus_dialog(&mut self, focus: DialogFocus) {
        if let Some(Dialog::Variables(dialog)) = self.dialog.as_mut() {
            dialog.focus = focus;
        }
    }

    pub(crate) fn click_dialog_button(&mut self, focus: DialogFocus) {
        if self.dialog.as_ref().is_some_and(Dialog::is_editing) {
            self.handle_dialog_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        }
        self.focus_dialog(focus);
        self.handle_dialog_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }

    fn effective_request_headers(&self, request_id: &str) -> BTreeMap<String, String> {
        let mut headers = self.config.headers.clone();
        for row in &self.request_edits(request_id).headers {
            remove_header_map(&mut headers, &row.name);
            if row.enabled && !row.name.trim().is_empty() {
                headers.insert(row.name.clone(), row.value.clone());
            }
        }
        headers
    }

    pub(crate) fn current_request_state(&self) -> Option<&RequestRuntimeState> {
        self.workspace_state
            .request_states
            .get(&self.current_request().id)
    }

    pub(crate) fn request_status(&self, request_id: &str) -> RequestStatus {
        self.workspace_state
            .request_states
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

    fn apply_response_extracts(&mut self, request_id: &str, body: &str) -> usize {
        let (requests, variables) = (&self.config.requests, &mut self.workspace_state.variables);
        let Some(extracts) = requests
            .iter()
            .find(|request| request.id == request_id)
            .map(|request| request.extracts.as_slice())
        else {
            return 0;
        };

        let mut failures = 0;
        for extract in extracts {
            match template::extract_json_value(body, &extract.path) {
                Ok(value) => {
                    variables.insert(extract.variable.clone(), value);
                    tracing::debug!(
                        request_id,
                        variable = %extract.variable,
                        path = %extract.path,
                        "响应字段已写入会话变量"
                    );
                }
                Err(error) => {
                    failures += 1;
                    tracing::debug!(
                        request_id,
                        variable = %extract.variable,
                        path = %extract.path,
                        error = %error,
                        "响应字段提取失败"
                    );
                }
            }
        }
        failures
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
                    let Some(state) = self.workspace_state.request_states.get(&request_id) else {
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
                                body_bytes = response.body_bytes.len(),
                                "后台请求成功"
                            );
                            let extract_failures = if request_status == RequestStatus::Success {
                                self.apply_response_extracts(&request_id, &response.body)
                            } else {
                                0
                            };
                            let state = self
                                .workspace_state
                                .request_states
                                .get_mut(&request_id)
                                .expect("请求状态已在处理消息前确认存在");
                            state.operation_id = None;
                            state.status = request_status;
                            state.response = Some(response);
                            state.error = None;
                            let complete = text.request_complete(status, elapsed);
                            let message = if extract_failures == 0 {
                                complete
                            } else {
                                format!(
                                    "{complete} · {}",
                                    text.response_extract_failures(extract_failures)
                                )
                            };
                            state.message = Some(message.clone());
                            is_current.then_some(message)
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
                            let state = self
                                .workspace_state
                                .request_states
                                .get_mut(&request_id)
                                .expect("请求状态已在处理消息前确认存在");
                            state.operation_id = None;
                            state.status = request_status;
                            state.response = None;
                            state.error = Some(error_message.clone());
                            let message = request_status.error_message(text, &error_message);
                            state.message = Some(message.clone());
                            is_current.then_some(message)
                        }
                    };
                    if let Some(status) = status_message {
                        if let Some(state) =
                            self.workspace_state.request_states.get_mut(&request_id)
                        {
                            state.message = Some(status.clone());
                        }
                        self.response_state.scroll.reset();
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
            "处理键盘操作"
        );

        if self.prompt.is_some() {
            self.handle_prompt_key(key);
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.request_quit();
            tracing::debug!("通过 Ctrl+C 请求退出");
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            self.save_current_request();
            return;
        }

        if self.dialog.is_some() {
            let inline_table = self
                .dialog
                .as_ref()
                .is_some_and(|dialog| dialog.preview_tab().is_some());
            let editing_inline_cell = self.dialog.as_ref().is_some_and(Dialog::is_editing);
            let handle_as_global = inline_table
                && !editing_inline_cell
                && (self.focus != Focus::Preview
                    || matches!(
                        key.code,
                        KeyCode::Tab
                            | KeyCode::BackTab
                            | KeyCode::Char('r' | 'v' | 'c' | 'o' | 'q')
                    ));
            if !handle_as_global {
                self.handle_dialog_key(key);
                return;
            }
        }

        if self.preview_state.editor.is_some()
            || self.preview_state.file_editor.is_some()
            || self.preview_state.variable_editor.is_some()
            || self.preview_state.url_editor.is_some()
        {
            if self.preview_state.url_editor.is_some() {
                self.handle_url_editor_key(key);
            } else {
                self.handle_body_editor_key(key);
            }
            return;
        }

        if self.response_state.menu_open {
            match key.code {
                KeyCode::Esc => self.close_response_menu(),
                KeyCode::Up | KeyCode::Char('k') => self.move_response_menu_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => self.move_response_menu_selection(1),
                KeyCode::Enter | KeyCode::Char(' ') => self.activate_selected_response_action(),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.request_quit();
                tracing::debug!("通过快捷键请求退出");
            }
            KeyCode::Tab => {
                self.focus = if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.focus.previous()
                } else {
                    self.focus.next()
                };
                tracing::debug!(focus = ?self.focus, "切换 TUI 区域焦点");
            }
            KeyCode::Char('r') => self.handle_preview_action(PreviewAction::Send),
            KeyCode::Char('v') => self.open_variables(),
            KeyCode::Char('o') => self.open_response_menu(),
            KeyCode::Delete if self.focus == Focus::Requests => self.request_delete(),
            KeyCode::Left if self.focus == Focus::Preview => self.move_preview_tab(-1),
            KeyCode::Right if self.focus == Focus::Preview => self.move_preview_tab(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_focused(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_focused(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.handle_enter(),
            _ => {}
        }
    }

    fn handle_enter(&mut self) {
        tracing::debug!(focus = ?self.focus, "处理 Enter 操作");
        match self.focus {
            Focus::Requests => {}
            Focus::Variables => self.open_variables(),
            Focus::Preview => {
                let action = PreviewAction::Edit(self.preview_state.active_tab);
                self.handle_preview_action(action);
            }
            Focus::Actions => self.handle_preview_action(PreviewAction::Send),
        }
    }

    fn move_focused(&mut self, direction: isize) {
        match self.focus {
            Focus::Requests => self.move_request(direction),
            Focus::Preview => {
                self.preview_state.scroll.move_by(direction);
            }
            Focus::Variables | Focus::Actions => {}
        }
    }

    pub(crate) fn move_request(&mut self, delta: isize) {
        let count = self.config.requests.len();
        if count == 0 {
            return;
        }
        let current = self.requests_state.selected_request % count;
        let next = (current as isize + delta).rem_euclid(count as isize) as usize;
        self.select_request(next);
    }

    pub(crate) fn move_preview_tab(&mut self, direction: isize) {
        if !self.has_current_request() {
            return;
        }
        let tab = match direction {
            value if value < 0 => self.preview_state.active_tab.previous(),
            value if value > 0 => self.preview_state.active_tab.next(),
            _ => self.preview_state.active_tab,
        };
        self.activate_preview_tab(tab);
        self.preview_state.scroll.reset();
        tracing::debug!(
            tab = ?self.preview_state.active_tab,
            direction,
            "切换请求预览标签"
        );
    }

    pub(crate) fn activate_preview_tab(&mut self, tab: PreviewTab) {
        if !self.has_current_request() {
            return;
        }
        if self.editing_preview_tab() == Some(tab) {
            return;
        }
        if self.editing_preview_tab().is_some() {
            self.persist_request_edits();
            self.dialog = None;
        }
        self.preview_state.active_tab = tab;
        match tab {
            PreviewTab::Body => {}
            PreviewTab::Params => self.open_params(),
            PreviewTab::Headers => self.open_headers(),
        }
    }

    pub(crate) fn add_preview_row(&mut self, tab: PreviewTab) {
        self.activate_preview_tab(tab);
        self.persist_request_edits();
        match self.dialog.as_mut() {
            Some(Dialog::Headers(dialog)) if tab == PreviewTab::Headers => dialog.add_row(),
            Some(Dialog::Params(dialog)) if tab == PreviewTab::Params => dialog.add_row(),
            _ => return,
        }
        self.mark_current_dirty();
        tracing::debug!(tab = ?tab, "通过请求标签新增字段");
    }

    pub(crate) fn scroll_response(&mut self, direction: isize) {
        if self.current_response().is_none() {
            return;
        }
        if self.response_state.scroll.move_by(direction) {
            tracing::debug!(
                offset = self.response_state.scroll.offset(),
                direction,
                "滚动响应内容"
            );
        }
    }

    pub(crate) fn open_response_menu(&mut self) {
        self.commit_active_editors();
        if self.editing_preview_tab().is_some() {
            self.dialog = None;
        }
        self.response_state.menu_open = true;
        self.response_state.menu_selected = 0;
    }

    pub(crate) fn close_response_menu(&mut self) {
        self.response_state.menu_open = false;
    }

    pub(crate) fn move_response_menu_selection(&mut self, direction: isize) {
        let action_count = ResponseMenuAction::all().len();
        self.response_state.menu_selected = (self.response_state.menu_selected as isize + direction)
            .rem_euclid(action_count as isize) as usize;
    }

    pub(crate) fn choose_response_action(&mut self, index: usize) {
        self.response_state.menu_selected = index;
        self.activate_selected_response_action();
    }

    pub(crate) fn activate_selected_response_action(&mut self) {
        let action = ResponseMenuAction::from_index(self.response_state.menu_selected);
        self.close_response_menu();
        if let Some(action) = action {
            self.activate_response_action(action);
        }
    }

    fn activate_response_action(&mut self, action: ResponseMenuAction) {
        match action {
            ResponseMenuAction::Download => self.download_current_response(),
            ResponseMenuAction::Copy => self.copy_current_response(),
        }
    }

    fn copy_current_response(&mut self) {
        let Some(body) = self
            .current_response()
            .map(|response| response.body.clone())
        else {
            self.status = self.text().response_action_no_response().to_string();
            return;
        };
        match crate::clipboard::copy_text(&body) {
            Ok(()) => self.status = self.text().response_copied().to_string(),
            Err(error) => self.status = self.text().response_copy_failed(&error),
        }
    }

    fn download_current_response(&mut self) {
        let Some(response) = self.current_response() else {
            self.status = self.text().response_action_no_response().to_string();
            return;
        };
        let body = response.body_bytes.clone();
        let headers = response.headers.clone();
        let request_id = self.current_request().id.clone();
        let directory = self.config.download_directory.clone();
        match crate::response_output::save_response(&body, &headers, &request_id, &directory) {
            Ok(path) => self.status = self.text().response_downloaded(&path.display().to_string()),
            Err(error) => self.status = self.text().response_download_failed(&error),
        }
    }

    pub(crate) fn send_current_request(&mut self) {
        if !self.has_current_request() || self.current_effective_request().url.trim().is_empty() {
            self.status = self.text().request_url_required().to_string();
            return;
        }
        tracing::debug!(request_id = %self.current_request().id, "触发发送当前请求");
        self.commit_active_editors();
        let effective_method = self.current_effective_request().method;
        if !supports_method(&effective_method) {
            self.status = self.text().unsupported_method(&effective_method);
            tracing::debug!(
                method = %effective_method,
                "忽略不支持的 HTTP 方法"
            );
            return;
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
        let display_url = resolved.url.clone();
        let timeout = self.current_request().timeout_seconds;
        let file_directory = self.config.file_directory.clone();
        let http_client = self.http_client.clone();
        let sender = self.sender.clone();
        tracing::debug!(
            request_id = %request_id,
            operation_id = %operation_id,
            method = %resolved.method,
            timeout_seconds = timeout,
            file_directory = %file_directory.display(),
            "开始异步发送请求"
        );
        let message = self.text().request_started(&resolved.method, &display_url);
        let state = self
            .workspace_state
            .request_states
            .entry(request_id.clone())
            .or_default();
        state.status = RequestStatus::Sending;
        state.response = None;
        state.error = None;
        state.operation_id = Some(operation_id.clone());
        state.message = Some(message.clone());
        self.status = message;

        thread::spawn(move || {
            tracing::debug!(operation_id = %operation_id, "HTTP 工作线程开始");
            let result = http::send(
                &http_client,
                &resolved,
                timeout,
                &file_directory,
                &operation_id,
            );
            match &result {
                Ok(response) => tracing::debug!(
                    operation_id = %operation_id,
                    status = response.status,
                    elapsed_ms = response.elapsed_ms,
                    body_bytes = response.body_bytes.len(),
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
}

pub(crate) fn supports_method(method: &str) -> bool {
    let method = method.trim();
    method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("POST")
}

fn split_query_parts(query: &str) -> impl Iterator<Item = &str> {
    query.split('&').filter(|part| !part.is_empty())
}

fn join_param_row(key: &str, value: &str, has_equals: bool) -> String {
    if has_equals || !value.is_empty() {
        format!("{key}={value}")
    } else {
        key.to_string()
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn serialize_request(request: &ApiRequest) -> String {
    let mut metadata = vec![format!("# @name {}", request.name)];
    if !request.description.is_empty() {
        metadata.push(format!("# @description {}", request.description));
    }
    let mut command = vec![format!(
        "curl --request {} --url {}",
        request.method,
        shell_quote(&request.url)
    )];
    for (name, value) in &request.headers {
        command.push(format!(
            "  --header {}",
            shell_quote(&format!("{name}: {value}"))
        ));
    }
    for part in &request.query_parts {
        let option = match part {
            BodyPart::Raw(_) => "--data-raw",
            BodyPart::UrlEncoded(_) => "--data-urlencode",
        };
        command.push(format!(
            "  {option} {}",
            shell_quote(crate::template::body_part_value(part))
        ));
    }
    if !request.query_parts.is_empty() {
        command.push("  --get".to_string());
    }
    for part in &request.body_parts {
        let option = match part {
            BodyPart::Raw(_) => "--data-raw",
            BodyPart::UrlEncoded(_) => "--data-urlencode",
        };
        command.push(format!(
            "  {option} {}",
            shell_quote(crate::template::body_part_value(part))
        ));
    }
    for (name, value) in &request.form {
        command.push(format!(
            "  --form-string {}",
            shell_quote(&format!("{name}={value}"))
        ));
    }
    for file in &request.files {
        let mut value = format!("{}=@{}", file.field, file.path);
        if let Some(content_type) = &file.content_type {
            value.push_str(&format!(";type={content_type}"));
        }
        if let Some(filename) = &file.filename {
            value.push_str(&format!(";filename={filename}"));
        }
        command.push(format!("  --form {}", shell_quote(&value)));
    }
    format!("{}\n{}\n", metadata.join("\n"), command.join(" \\\n"))
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
