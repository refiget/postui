use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use crate::{
    config::{ApiRequest, BodyPart, FileUpload, RequestConfig, value_to_string},
    editor::{
        BodyValueEditor, TextEditor, convert_json_scalar, json_scalar_at, merge_json_edit,
        terminal_width, text_position,
    },
    http::{self, HttpError, ResponseData},
    i18n::UiText,
    settings::GlobalConfig,
    template::{self, ResolvedRequest},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[cfg(test)]
use crate::editor::JsonScalarKind;

mod dialog;

pub(crate) use dialog::{
    BodyPartSource, Dialog, DialogFocus, HeaderField, HeaderRow, HeaderSource, HeadersDialog,
    ParamSource, ParamsDialog, ParamsDialogRow, VariableRow, VariablesDialog,
};
use dialog::{DialogAction, remove_header, remove_header_map, split_key_value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Collection,
    Requests,
    Variables,
    Preview,
    Actions,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Collection => Self::Requests,
            Self::Requests => Self::Variables,
            Self::Variables => Self::Preview,
            Self::Preview => Self::Actions,
            Self::Actions => Self::Collection,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Collection => Self::Actions,
            Self::Requests => Self::Collection,
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
pub(crate) struct RequestCollectionState {
    pub(crate) variables: BTreeMap<String, String>,
    pub(crate) request_edits: HashMap<String, RequestEdits>,
    pub(crate) request_states: HashMap<String, RequestRuntimeState>,
}

#[derive(Debug, Clone)]
pub(crate) struct RequestEdits {
    pub(crate) headers: Vec<HeaderRow>,
    pub(crate) query_parts: Vec<BodyPart>,
    pub(crate) form: BTreeMap<String, String>,
    pub(crate) files: Vec<FileUpload>,
    pub(crate) body_parts: Vec<BodyPart>,
}

impl From<&ApiRequest> for RequestEdits {
    fn from(request: &ApiRequest) -> Self {
        Self {
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

#[derive(Debug)]
struct CollectionSession {
    config: RequestConfig,
    config_path: PathBuf,
    requests_state: RequestsContentState,
    preview_state: PreviewContentState,
    response_state: ResponseContentState,
    collection_state: RequestCollectionState,
    status: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CollectionChoice {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

impl RequestCollectionState {
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
        }
    }
}

impl CollectionSession {
    fn new(config: RequestConfig, config_path: PathBuf, text: UiText) -> Self {
        let collection_state = RequestCollectionState::from_config(&config);
        Self {
            config,
            config_path,
            requests_state: RequestsContentState::default(),
            preview_state: PreviewContentState::default(),
            response_state: ResponseContentState::default(),
            collection_state,
            status: text.ready().to_string(),
        }
    }
}

fn discover_collections(current: &PathBuf) -> Vec<CollectionChoice> {
    let mut paths = current
        .parent()
        .and_then(|parent| fs::read_dir(parent).ok())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join("requests").is_dir())
        .collect::<Vec<_>>();
    if !paths.iter().any(|path| path == current) {
        paths.push(current.clone());
    }
    paths.sort_by(|left, right| {
        left.file_name()
            .unwrap_or_default()
            .cmp(right.file_name().unwrap_or_default())
    });
    paths.dedup();
    paths
        .into_iter()
        .map(|path| CollectionChoice {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Collection")
                .to_string(),
            path,
        })
        .collect()
}

pub(crate) struct App {
    pub(crate) config: RequestConfig,
    pub(crate) config_path: PathBuf,
    pub(crate) global_config: GlobalConfig,
    pub(crate) focus: Focus,
    pub(crate) requests_state: RequestsContentState,
    pub(crate) preview_state: PreviewContentState,
    pub(crate) response_state: ResponseContentState,
    pub(crate) collection_state: RequestCollectionState,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) status: String,
    pub(crate) animation_frame: usize,
    pub(crate) should_quit: bool,
    pub(crate) collections: Vec<CollectionChoice>,
    pub(crate) collection_menu_open: bool,
    pub(crate) selected_collection: usize,
    collection_sessions: HashMap<PathBuf, CollectionSession>,
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
            configured_variable_count = config.editable_variables.len(),
            "创建应用状态"
        );
        let collection_state = RequestCollectionState::from_config(&config);
        let collections = discover_collections(&config_path);
        let selected_collection = collections
            .iter()
            .position(|choice| choice.path == config_path)
            .unwrap_or(0);

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
            },
            response_state: ResponseContentState::default(),
            collection_state,
            dialog: None,
            status: text.ready().to_string(),
            animation_frame: 0,
            should_quit: false,
            collections,
            collection_menu_open: false,
            selected_collection,
            collection_sessions: HashMap::new(),
            sender,
            receiver,
        }
    }

    pub(crate) fn current_request(&self) -> &ApiRequest {
        &self.config.requests[self.requests_state.selected_request]
    }

    pub(crate) fn text(&self) -> UiText {
        UiText::new(self.global_config.language)
    }

    pub(crate) fn advance_animation(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
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
            &self.collection_state.variables,
        )
    }

    pub(crate) fn current_effective_request(&self) -> ApiRequest {
        self.effective_request(self.current_request())
    }

    fn effective_request(&self, request: &ApiRequest) -> ApiRequest {
        let edits = self.request_edits(&request.id);
        let mut effective = request.clone();
        effective.headers = self.effective_request_headers(&request.id);
        effective.query_parts = edits.query_parts.clone();
        effective.form = edits.form.clone();
        effective.files = edits.files.clone();
        effective.body_parts = edits.body_parts.clone();
        effective
    }

    fn request_edits(&self, request_id: &str) -> &RequestEdits {
        self.collection_state
            .request_edits
            .get(request_id)
            .expect("every configured request has session edits")
    }

    fn request_edits_mut(&mut self, request_id: &str) -> &mut RequestEdits {
        self.collection_state
            .request_edits
            .get_mut(request_id)
            .expect("every configured request has session edits")
    }

    pub(crate) fn current_url_variables(&self) -> Vec<String> {
        let request = self.current_effective_request();
        template::url_variable_names(&request.url)
    }

    pub(crate) fn request_variable_value(&self, variable: &str) -> String {
        self.collection_state
            .variables
            .get(variable)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn resolved_url(&self, request: &ApiRequest) -> String {
        let request = self.effective_request(request);
        let url = template::display_url(&request);
        template::display_text_parts(&url, &self.collection_state.variables)
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
        self.collection_state
            .variables
            .insert(editor.variable, editor.input.value);
    }

    fn commit_body_value(&mut self) {
        let Some(editor) = self.preview_state.editor.take() else {
            return;
        };
        let Some(replacement) = convert_json_scalar(editor.kind, &editor.input.value) else {
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
    }

    pub(crate) fn variable_count(&self) -> usize {
        self.config.editable_variables.len()
    }

    pub(crate) fn current_header_count(&self) -> usize {
        self.effective_request_headers(&self.current_request().id)
            .len()
    }

    pub(crate) fn current_param_count(&self) -> usize {
        let request = self.current_resolved_request();
        request.query_parts.len() + request.form.len()
    }

    pub(crate) fn open_variables(&mut self) {
        let rows = self
            .config
            .editable_variables
            .iter()
            .map(|name| VariableRow {
                name: name.clone(),
                value: self
                    .collection_state
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
        tracing::debug!(variable_count = self.variable_count(), "打开集合变量窗口");
    }

    pub(crate) fn open_headers(&mut self) {
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
                let query_parts = self.request_edits(&request_id).query_parts.clone();
                let mut rows = Vec::new();
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
                supports_method(&self.current_effective_request().method)
                    && self.request_status(&self.current_request().id) != RequestStatus::Sending
                    && !matches!(self.dialog, Some(Dialog::Variables(_)))
                    && self.preview_state.editor.is_none()
                    && self.preview_state.file_editor.is_none()
                    && self.preview_state.variable_editor.is_none()
            }
            PreviewAction::Edit(tab) => {
                self.editing_preview_tab()
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
                    self.collection_state.variables.insert(row.name, row.value);
                }
                self.status = self.text().variables_applied().to_string();
                tracing::debug!(
                    variable_count = self.collection_state.variables.len(),
                    "应用集合变量修改"
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
            DialogAction::Changed => self.persist_request_edits(),
            DialogAction::Apply => self.apply_dialog(),
            DialogAction::Cancel => {
                self.persist_request_edits();
                self.close_dialog();
            }
        }
    }

    fn persist_request_edits(&mut self) {
        let (dialog, request_edits) = (&mut self.dialog, &mut self.collection_state.request_edits);
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
                let mut query_parts = Vec::new();
                let mut form = BTreeMap::new();
                for row in &dialog.rows {
                    let key = row.key.trim();
                    let value = row.value.trim();
                    match row.source {
                        ParamSource::Query if !key.is_empty() || !value.is_empty() => {
                            let part = if row.has_equals || !value.is_empty() {
                                format!("{key}={value}")
                            } else {
                                key.to_string()
                            };
                            query_parts
                                .push(row.part_type.unwrap_or(BodyPartSource::Raw).to_part(part));
                        }
                        ParamSource::Form if !key.is_empty() => {
                            form.insert(key.to_string(), row.value.clone());
                        }
                        _ => {}
                    }
                }
                let edits = request_edits
                    .get_mut(&dialog.request_id)
                    .expect("every configured request has session edits");
                edits.query_parts = query_parts;
                edits.form = form;
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
        self.persist_request_edits();
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.click_param_row(index, field, edit);
        }
    }

    pub(crate) fn click_header_row(&mut self, index: usize, field: HeaderField, edit: bool) {
        self.persist_request_edits();
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
        self.collection_state
            .request_states
            .get(&self.current_request().id)
    }

    pub(crate) fn request_status(&self, request_id: &str) -> RequestStatus {
        self.collection_state
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
        let (requests, variables) = (&self.config.requests, &mut self.collection_state.variables);
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
                    let Some(state) = self.collection_state.request_states.get(&request_id) else {
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
                                .collection_state
                                .request_states
                                .get_mut(&request_id)
                                .expect("请求状态已在处理消息前确认存在");
                            state.operation_id = None;
                            state.status = request_status;
                            state.response = Some(response);
                            state.error = None;
                            is_current.then(|| {
                                let complete = text.request_complete(status, elapsed);
                                if extract_failures == 0 {
                                    complete
                                } else {
                                    format!(
                                        "{complete} · {}",
                                        text.response_extract_failures(extract_failures)
                                    )
                                }
                            })
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
                                .collection_state
                                .request_states
                                .get_mut(&request_id)
                                .expect("请求状态已在处理消息前确认存在");
                            state.operation_id = None;
                            state.status = request_status;
                            state.response = None;
                            state.error = Some(error_message.clone());
                            is_current.then(|| request_status.error_message(text, &error_message))
                        }
                    };
                    if let Some(status) = status_message {
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

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            tracing::debug!("通过 Ctrl+C 请求退出");
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
        {
            self.handle_body_editor_key(key);
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

        if self.collection_menu_open {
            match key.code {
                KeyCode::Esc => self.close_collection_menu(),
                KeyCode::Up | KeyCode::Char('k') => self.move_collection_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => self.move_collection_selection(1),
                KeyCode::Enter | KeyCode::Char(' ') => self.activate_selected_collection(),
                _ => {}
            }
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
            }
            KeyCode::Char('r') => self.handle_preview_action(PreviewAction::Send),
            KeyCode::Char('v') => self.open_variables(),
            KeyCode::Char('c') => self.open_collection_menu(),
            KeyCode::Char('o') => self.open_response_menu(),
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
            Focus::Collection => self.open_collection_menu(),
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
            Focus::Collection => self.move_collection_selection(direction),
            Focus::Requests => self.move_request(direction),
            Focus::Preview => {
                self.preview_state.scroll.move_by(direction);
            }
            Focus::Variables | Focus::Actions => {}
        }
    }

    pub(crate) fn open_collection_menu(&mut self) {
        self.commit_active_editors();
        self.close_response_menu();
        self.dialog = None;
        self.focus = Focus::Collection;
        self.selected_collection = self
            .collections
            .iter()
            .position(|choice| choice.path == self.config_path)
            .unwrap_or(0);
        self.collection_menu_open = true;
    }

    pub(crate) fn close_collection_menu(&mut self) {
        self.collection_menu_open = false;
    }

    pub(crate) fn move_collection_selection(&mut self, direction: isize) {
        let count = self.collections.len();
        if count == 0 {
            return;
        }
        self.selected_collection =
            (self.selected_collection as isize + direction).rem_euclid(count as isize) as usize;
    }

    pub(crate) fn choose_collection(&mut self, index: usize) {
        if index >= self.collections.len() {
            return;
        }
        self.selected_collection = index;
        self.activate_selected_collection();
    }

    fn activate_selected_collection(&mut self) {
        self.collection_menu_open = false;
        let Some(choice) = self.collections.get(self.selected_collection).cloned() else {
            return;
        };
        if choice.path == self.config_path {
            return;
        }
        if self
            .collection_state
            .request_states
            .values()
            .any(|state| state.status == RequestStatus::Sending)
        {
            self.status = self.text().collection_switch_blocked().to_string();
            return;
        }

        self.persist_request_edits();
        let target = if let Some(session) = self.collection_sessions.remove(&choice.path) {
            session
        } else {
            match crate::config::load(&choice.path) {
                Ok(config) => CollectionSession::new(config, choice.path.clone(), self.text()),
                Err(error) => {
                    self.status = self.text().collection_load_failed(&error.to_string());
                    return;
                }
            }
        };
        let previous_path = self.config_path.clone();
        let previous = CollectionSession {
            config: std::mem::replace(&mut self.config, target.config),
            config_path: std::mem::replace(&mut self.config_path, target.config_path),
            requests_state: std::mem::replace(&mut self.requests_state, target.requests_state),
            preview_state: std::mem::replace(&mut self.preview_state, target.preview_state),
            response_state: std::mem::replace(&mut self.response_state, target.response_state),
            collection_state: std::mem::replace(
                &mut self.collection_state,
                target.collection_state,
            ),
            status: std::mem::replace(&mut self.status, target.status),
        };
        self.collection_sessions.insert(previous_path, previous);
        self.dialog = None;
        self.focus = Focus::Requests;
    }

    pub(crate) fn move_request(&mut self, delta: isize) {
        let count = self.config.requests.len();
        if count == 0 {
            tracing::debug!("接口列表为空，忽略移动操作");
            return;
        }
        let current = self.requests_state.selected_request % count;
        let next = (current as isize + delta).rem_euclid(count as isize) as usize;
        self.select_request(next);
    }

    pub(crate) fn move_preview_tab(&mut self, direction: isize) {
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
        let display_url = template::display_url(self.current_request());
        let timeout = self.current_request().timeout_seconds;
        let file_directory = self.config.file_directory.clone();
        let sender = self.sender.clone();
        tracing::debug!(
            request_id = %request_id,
            operation_id = %operation_id,
            method = %resolved.method,
            timeout_seconds = timeout,
            file_directory = %file_directory.display(),
            "开始异步发送请求"
        );
        let state = self
            .collection_state
            .request_states
            .entry(request_id.clone())
            .or_default();
        state.status = RequestStatus::Sending;
        state.response = None;
        state.error = None;
        state.operation_id = Some(operation_id.clone());
        self.status = self.text().request_started(&resolved.method, &display_url);

        thread::spawn(move || {
            tracing::debug!(operation_id = %operation_id, "HTTP 工作线程开始");
            let result = http::send(&resolved, timeout, &file_directory, &operation_id);
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
    use crate::{
        config::{ResponseExtract, load},
        http::ResponseData,
        settings::GlobalConfig,
    };

    #[test]
    fn successful_response_extracts_session_variables_without_losing_old_values() {
        let mut config = load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载");
        let request_id = config.requests[0].id.clone();
        config.requests[0].extracts = vec![
            ResponseExtract {
                variable: "task_id".to_string(),
                path: "data.taskId".to_string(),
            },
            ResponseExtract {
                variable: "unchanged".to_string(),
                path: "data.missing".to_string(),
            },
        ];
        let mut app = App::new(
            config,
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        app.collection_state
            .variables
            .insert("unchanged".to_string(), "old-value".to_string());
        let state = app
            .collection_state
            .request_states
            .get_mut(&request_id)
            .expect("请求状态应存在");
        state.status = RequestStatus::Sending;
        state.operation_id = Some("extract-test".to_string());
        app.sender
            .send(AppMessage::RequestFinished {
                request_id: request_id.clone(),
                operation_id: "extract-test".to_string(),
                result: Ok(ResponseData {
                    status: 200,
                    reason: "OK".to_string(),
                    headers: Vec::new(),
                    body: r#"{"data":{"taskId":"task-001"}}"#.to_string(),
                    body_bytes: br#"{"data":{"taskId":"task-001"}}"#.to_vec(),
                    elapsed_ms: 8,
                }),
            })
            .expect("测试消息应可发送");

        app.poll_messages();

        assert_eq!(app.collection_state.variables["task_id"], "task-001");
        assert_eq!(app.collection_state.variables["unchanged"], "old-value");
        assert_eq!(app.request_status(&request_id), RequestStatus::Success);
        assert!(app.status.contains("1 field not extracted"));
    }

    #[test]
    fn failed_http_response_does_not_extract_variables() {
        let mut config = load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载");
        let request_id = config.requests[0].id.clone();
        config.requests[0].extracts = vec![ResponseExtract {
            variable: "task_id".to_string(),
            path: "data.taskId".to_string(),
        }];
        let mut app = App::new(
            config,
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        app.collection_state
            .variables
            .insert("task_id".to_string(), "old-value".to_string());
        let state = app
            .collection_state
            .request_states
            .get_mut(&request_id)
            .expect("请求状态应存在");
        state.status = RequestStatus::Sending;
        state.operation_id = Some("failed-extract-test".to_string());
        app.sender
            .send(AppMessage::RequestFinished {
                request_id: request_id.clone(),
                operation_id: "failed-extract-test".to_string(),
                result: Ok(ResponseData {
                    status: 500,
                    reason: "Internal Server Error".to_string(),
                    headers: Vec::new(),
                    body: r#"{"data":{"taskId":"new-value"}}"#.to_string(),
                    body_bytes: br#"{"data":{"taskId":"new-value"}}"#.to_vec(),
                    elapsed_ms: 8,
                }),
            })
            .expect("测试消息应可发送");

        app.poll_messages();

        assert_eq!(app.collection_state.variables["task_id"], "old-value");
        assert_eq!(app.request_status(&request_id), RequestStatus::Failed);
    }

    #[test]
    fn keeps_the_latest_response_for_each_request_during_the_session() {
        let config = load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载");
        let first_id = config.requests[0].id.clone();
        let second_id = config.requests[1].id.clone();
        let mut app = App::new(
            config,
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );

        assert_eq!(app.request_status(&first_id), RequestStatus::NotSent);
        assert_eq!(app.request_status(&second_id), RequestStatus::NotSent);

        let first_state = RequestRuntimeState::from_response(ResponseData {
            status: 200,
            reason: "OK".to_string(),
            headers: Vec::new(),
            body: "first".to_string(),
            body_bytes: b"first".to_vec(),
            elapsed_ms: 1,
        });
        app.collection_state
            .request_states
            .insert(first_id.clone(), first_state);

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
        app.collection_state
            .request_states
            .insert(first_id.clone(), latest_state);
        assert!(app.current_response().is_none());
        assert_eq!(app.current_error(), Some("latest failure"));
        assert_eq!(app.request_status(&first_id), RequestStatus::Failed);
    }

    #[test]
    fn tab_cycles_through_collection_actions_and_send() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.focus, Focus::Variables);
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.focus, Focus::Preview);
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.focus, Focus::Actions);
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.focus, Focus::Collection);
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.focus, Focus::Requests);
    }

    #[test]
    fn switching_collections_restores_each_session_state() {
        let path = PathBuf::from("mock/.postui");
        let config = load(&path).expect("mock 请求配置应当可以加载");
        let mut app = App::new(config.clone(), path.clone(), GlobalConfig::default());
        let second_path = PathBuf::from("mock/second-collection");
        let mut second_config = config;
        second_config.name = "Second".to_string();
        app.collections.push(CollectionChoice {
            name: "second-collection".to_string(),
            path: second_path.clone(),
        });
        app.collection_sessions.insert(
            second_path.clone(),
            CollectionSession::new(second_config, second_path, app.text()),
        );
        app.collection_state
            .variables
            .insert("session_value".to_string(), "first".to_string());

        app.choose_collection(1);
        assert_eq!(app.config.name, "Second");
        app.collection_state
            .variables
            .insert("session_value".to_string(), "second".to_string());
        app.choose_collection(0);

        assert_eq!(app.config_path, path);
        assert_eq!(
            app.collection_state.variables.get("session_value"),
            Some(&"first".to_string())
        );
    }

    #[test]
    fn only_get_and_post_requests_can_be_sent() {
        assert!(supports_method("GET"));
        assert!(supports_method("post"));
        assert!(!supports_method("PUT"));
        assert!(!supports_method("DELETE"));

        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        app.select_request(4);
        app.send_current_request();

        assert_eq!(
            app.request_status(&app.current_request().id),
            RequestStatus::NotSent
        );
        assert!(app.status.contains("GET") && app.status.contains("POST"));
    }

    #[test]
    fn variable_dialog_applies_session_values_without_changing_defaults() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let default = app.config.variables["host"].default.clone();

        app.open_variables();
        let Dialog::Variables(dialog) = app.dialog.as_mut().expect("变量窗口应当打开")
        else {
            panic!("应打开变量窗口")
        };
        let row = dialog
            .rows
            .iter_mut()
            .find(|row| row.name == "host")
            .expect("host 变量应存在");
        row.value = "override.example.test".to_string();
        app.apply_dialog();

        assert_eq!(
            app.collection_state.variables["host"],
            "override.example.test"
        );
        assert_eq!(app.config.variables["host"].default, default);
    }

    #[test]
    fn request_editor_changes_resolved_values_without_rewriting_raw_config() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let raw = app.config.headers["X-PostUI-Collection"].clone();

        app.open_headers();
        let index = match app.dialog.as_ref().expect("Header 编辑器应当打开") {
            Dialog::Headers(dialog) => {
                let index = dialog
                    .rows
                    .iter()
                    .position(|row| row.name == "X-PostUI-Collection")
                    .expect("应显示集合 Header");
                assert_eq!(dialog.rows[index].value, "{{collection_name}}");
                assert_eq!(dialog.rows[index].source, HeaderSource::Collection);
                index
            }
            _ => panic!("应打开 Header 编辑器"),
        };
        app.click_header_row(index, HeaderField::Value, true);
        app.handle_dialog_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        for character in "session-value".chars() {
            app.handle_dialog_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
        app.handle_dialog_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(
            app.current_resolved_request().headers["X-PostUI-Collection"],
            "session-value"
        );
        assert_eq!(raw, "{{collection_name}}");
        assert_eq!(app.config.headers["X-PostUI-Collection"], raw);
        assert!(
            app.collection_state.request_edits[&app.current_request().id]
                .headers
                .iter()
                .any(|row| row.name == "X-PostUI-Collection"
                    && row.source == HeaderSource::Request)
        );
    }

    #[test]
    fn body_canvas_edits_the_rendered_json_without_rewriting_raw_config() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        app.select_request(2);
        let raw = app.current_request().body_parts.clone();

        let preview = app.body_json();
        assert!(preview.contains("\"taskId\": \"task-from-config\""));
        assert!(!preview.contains("{{task_id}}"));

        let value_offset = preview.find("task-from-config").unwrap();
        let before = &preview[..value_offset];
        let line = before.bytes().filter(|byte| *byte == b'\n').count();
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        let column = before[line_start..].chars().count();
        app.start_body_edit(line, column);
        app.preview_state.editor.as_mut().unwrap().input.value = "edited".to_string();
        app.commit_body_value();

        assert!(
            app.current_resolved_request()
                .raw_body
                .unwrap()
                .contains("\"taskId\": \"edited\"")
        );
        assert_eq!(app.current_request().body_parts, raw);
    }

    #[test]
    fn body_canvas_preserves_unedited_template_values() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let request_id = app.current_request().id.clone();
        app.collection_state
            .variables
            .insert("first".to_string(), "one".to_string());
        app.collection_state
            .variables
            .insert("second".to_string(), "two".to_string());
        app.collection_state
            .request_edits
            .get_mut(&request_id)
            .expect("请求编辑状态应存在")
            .body_parts = vec![BodyPart::Raw(
            r#"{"changed":"{{first}}","kept":"{{second}}"}"#.to_string(),
        )];

        let preview = app.body_json();
        let offset = preview.find("one").expect("预览应包含已解析变量");
        let before = &preview[..offset];
        let line = before.bytes().filter(|byte| *byte == b'\n').count();
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        let column = terminal_width(&before[line_start..]);
        app.start_body_edit(line, column);
        app.preview_state
            .editor
            .as_mut()
            .expect("标量编辑器应打开")
            .input
            .value = "updated".to_string();

        app.commit_body_value();

        let BodyPart::Raw(stored) = &app.collection_state.request_edits[&request_id].body_parts[0]
        else {
            panic!("编辑后的 JSON 应保存为原始请求体")
        };
        assert!(stored.contains(r#""changed": "updated""#));
        assert!(stored.contains(r#""kept": "{{second}}""#));
    }

    #[test]
    fn switching_requests_commits_the_active_body_editor() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        app.select_request(2);

        let preview = app.body_json();
        let value_offset = preview.find("task-from-config").unwrap();
        let before = &preview[..value_offset];
        let line = before.bytes().filter(|byte| *byte == b'\n').count();
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        let column = before[line_start..].chars().count();
        app.start_body_edit(line, column);
        app.preview_state.editor.as_mut().unwrap().input.value = "temporary-body".to_string();

        app.select_request(0);
        app.select_request(2);

        assert!(app.body_json().contains("\"taskId\": \"temporary-body\""));
    }

    #[test]
    fn switching_requests_commits_active_header_and_param_inputs() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        app.select_request(1);

        app.open_headers();
        let header_index = match app.dialog.as_ref().unwrap() {
            Dialog::Headers(dialog) => dialog
                .rows
                .iter()
                .position(|row| row.name == "X-Debug-Token")
                .unwrap(),
            _ => unreachable!(),
        };
        app.click_header_row(header_index, HeaderField::Value, true);
        let Dialog::Headers(dialog) = app.dialog.as_mut().unwrap() else {
            unreachable!()
        };
        dialog.editor.as_mut().unwrap().value = "temporary-header".to_string();

        app.select_request(0);
        app.select_request(1);
        assert_eq!(
            app.current_resolved_request().headers["X-Debug-Token"],
            "temporary-header"
        );

        app.open_params();
        app.click_param_row(0, HeaderField::Value, true);
        let Dialog::Params(dialog) = app.dialog.as_mut().unwrap() else {
            unreachable!()
        };
        dialog.editor.as_mut().unwrap().value = "temporary-param".to_string();

        app.select_request(0);
        app.select_request(1);
        app.open_params();
        let Dialog::Params(dialog) = app.dialog.as_ref().unwrap() else {
            unreachable!()
        };
        assert_eq!(dialog.rows[0].value, "temporary-param");
    }

    #[test]
    fn urlencoded_body_preview_is_human_readable() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let index = app
            .config
            .requests
            .iter()
            .position(|request| request.id.ends_with("08-form.http"))
            .expect("应包含 URL 编码表单请求");
        app.select_request(index);

        let preview = app.body_preview();
        assert!(preview.contains("name=文档接口测试"));
        assert!(preview.contains("note=multipart note"));
        assert!(!preview.contains("%E6"));
        assert!(
            app.current_resolved_request()
                .raw_body
                .unwrap()
                .contains("name=%E6%96%87")
        );
    }

    #[test]
    fn urlencoded_query_editor_uses_decoded_values_without_losing_its_type() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let index = app
            .config
            .requests
            .iter()
            .position(|request| request.id.ends_with("02-search.http"))
            .expect("应包含查询请求");
        app.select_request(index);
        app.open_params();

        let Dialog::Params(dialog) = app.dialog.as_ref().expect("参数编辑器应当打开")
        else {
            panic!("应打开参数编辑器");
        };
        assert_eq!(dialog.rows[0].value, "{{search_term}}");
        assert_eq!(
            template::resolve_text(&dialog.rows[0].value, &app.collection_state.variables),
            "文档审查 & edge"
        );
        assert_eq!(dialog.rows[0].part_type, Some(BodyPartSource::UrlEncoded));
    }

    #[test]
    fn params_editor_reopens_from_the_current_session_state() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let index = app
            .config
            .requests
            .iter()
            .position(|request| request.id.ends_with("02-search.http"))
            .expect("应包含查询请求");
        app.select_request(index);
        let request_id = app.current_request().id.clone();
        app.collection_state
            .request_edits
            .get_mut(&request_id)
            .expect("请求编辑状态应存在")
            .query_parts = vec![BodyPart::Raw("term=plain value".to_string())];

        app.open_params();

        let Dialog::Params(dialog) = app.dialog.as_ref().expect("参数编辑器应当打开")
        else {
            panic!("应打开参数编辑器");
        };
        assert_eq!(dialog.rows[0].value, "plain value");
        assert_eq!(dialog.rows[0].part_type, Some(BodyPartSource::Raw));
    }

    #[test]
    fn opening_inline_tables_preserves_templates_and_query_flags() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let request_id = app.current_request().id.clone();
        let edits = app
            .collection_state
            .request_edits
            .get_mut(&request_id)
            .expect("请求编辑状态应存在");
        edits.headers.push(HeaderRow {
            name: "X-Template".to_string(),
            value: "{{host}}".to_string(),
            enabled: true,
            source: HeaderSource::Request,
        });
        edits.query_parts = vec![
            BodyPart::Raw("flag".to_string()),
            BodyPart::Raw("empty=".to_string()),
            BodyPart::Raw("value={{host}}".to_string()),
        ];

        app.open_headers();
        app.select_request(1);
        app.select_request(0);
        assert!(
            app.collection_state.request_edits[&request_id]
                .headers
                .iter()
                .any(|row| row.name == "X-Template" && row.value == "{{host}}")
        );

        app.open_params();
        app.select_request(1);
        assert_eq!(
            app.collection_state.request_edits[&request_id].query_parts,
            vec![
                BodyPart::Raw("flag".to_string()),
                BodyPart::Raw("empty=".to_string()),
                BodyPart::Raw("value={{host}}".to_string()),
            ]
        );
    }

    #[test]
    fn opening_a_query_flag_value_without_changes_preserves_the_flag() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let request_id = app.current_request().id.clone();
        app.collection_state
            .request_edits
            .get_mut(&request_id)
            .expect("请求编辑状态应存在")
            .query_parts = vec![BodyPart::Raw("flag".to_string())];

        app.open_params();
        app.click_param_row(0, HeaderField::Value, true);
        app.commit_active_editors();

        assert_eq!(
            app.collection_state.request_edits[&request_id].query_parts,
            vec![BodyPart::Raw("flag".to_string())]
        );
    }

    #[test]
    fn unchanged_collection_header_does_not_become_a_request_override() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let request_id = app.current_request().id.clone();
        app.open_headers();

        app.click_header_row(0, HeaderField::Value, true);
        app.commit_active_editors();

        assert!(
            app.collection_state.request_edits[&request_id]
                .headers
                .is_empty()
        );
    }

    #[test]
    fn clicking_an_inline_cell_keeps_its_editor_open() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        app.open_headers();
        app.click_header_row(0, HeaderField::Value, true);

        let Dialog::Headers(dialog) = app.dialog.as_ref().expect("Header 表格应当打开")
        else {
            panic!("应打开 Header 表格")
        };
        assert!(dialog.editor.is_some());
    }

    #[test]
    fn response_menu_commits_and_releases_an_inline_editor() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let request_id = app.current_request().id.clone();
        app.collection_state.request_states.insert(
            request_id.clone(),
            RequestRuntimeState::from_response(ResponseData {
                status: 200,
                reason: "OK".to_string(),
                headers: Vec::new(),
                body: "ok".to_string(),
                body_bytes: b"ok".to_vec(),
                elapsed_ms: 1,
            }),
        );
        app.open_headers();
        let row = 0;
        app.click_header_row(row, HeaderField::Value, true);
        let Dialog::Headers(dialog) = app.dialog.as_mut().expect("Header 表格应当打开")
        else {
            panic!("应打开 Header 表格")
        };
        dialog.editor.as_mut().expect("单元格编辑器应打开").value =
            "committed-before-menu".to_string();

        app.open_response_menu();

        assert!(app.response_state.menu_open);
        assert!(app.dialog.is_none());
        assert!(
            app.collection_state.request_edits[&request_id]
                .headers
                .iter()
                .any(|row| row.value == "committed-before-menu")
        );
    }

    #[test]
    fn non_json_body_does_not_open_the_scalar_editor() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let request_id = app.current_request().id.clone();
        app.collection_state
            .request_edits
            .get_mut(&request_id)
            .expect("请求编辑状态应存在")
            .body_parts = vec![BodyPart::Raw("plain text".to_string())];

        app.start_body_edit(0, 0);

        assert!(app.body_editor().is_none());
        assert_eq!(
            app.collection_state.request_edits[&request_id].body_parts,
            vec![BodyPart::Raw("plain text".to_string())]
        );
    }

    #[test]
    fn body_value_conversion_preserves_the_original_json_type() {
        assert_eq!(
            convert_json_scalar(JsonScalarKind::String, "42").as_deref(),
            Some("\"42\"")
        );
        assert_eq!(
            convert_json_scalar(JsonScalarKind::Number, "42.5").as_deref(),
            Some("42.5")
        );
        assert_eq!(
            convert_json_scalar(JsonScalarKind::Boolean, "false").as_deref(),
            Some("false")
        );
        assert!(convert_json_scalar(JsonScalarKind::Number, "nope").is_none());
        assert!(convert_json_scalar(JsonScalarKind::Boolean, "yes").is_none());
    }

    #[test]
    fn blurring_an_invalid_body_value_restores_the_previous_body() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let request_id = app.current_request().id.clone();
        app.collection_state
            .request_edits
            .get_mut(&request_id)
            .expect("请求编辑状态应存在")
            .body_parts = vec![BodyPart::Raw(r#"{"count": 1}"#.to_string())];
        let before = app.body_json();
        let offset = before.find('1').unwrap();
        let prefix = &before[..offset];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
        let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
        let column = prefix[line_start..].chars().count();
        app.start_body_edit(line, column);
        app.preview_state.editor.as_mut().unwrap().input.value = "invalid".to_string();

        app.commit_active_editors();

        assert!(app.body_editor().is_none());
        assert_eq!(app.body_json(), before);
    }

    #[test]
    fn json_scalar_hit_testing_excludes_keys_and_containers() {
        let document = r#"{"name": 1, "enabled": true}"#;
        assert!(json_scalar_at(document, 2).is_none());
        assert!(json_scalar_at(document, 0).is_none());
        assert_eq!(
            json_scalar_at(document, document.find('1').unwrap()).map(|(_, kind, _)| kind),
            Some(JsonScalarKind::Number)
        );
    }

    #[test]
    fn resolved_request_merges_collection_and_request_headers_case_insensitively() {
        let mut app = App::new(
            load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
            PathBuf::from("mock/.postui"),
            GlobalConfig::default(),
        );
        let request_id = app.current_request().id.clone();
        app.config
            .headers
            .insert("X-Collection-Test".to_string(), "collection".to_string());
        app.collection_state
            .request_edits
            .get_mut(&request_id)
            .expect("请求编辑状态应存在")
            .headers
            .push(HeaderRow {
                name: "x-collection-test".to_string(),
                value: "request".to_string(),
                enabled: true,
                source: HeaderSource::Request,
            });

        let resolved = app.current_resolved_request();
        assert_eq!(resolved.headers.len(), app.current_header_count());
        assert_eq!(
            resolved
                .headers
                .get("x-collection-test")
                .map(String::as_str),
            Some("request")
        );
    }
}
