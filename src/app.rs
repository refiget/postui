use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use crate::{
    config::{ApiRequest, BodyPart, RequestConfig, value_to_string},
    editor::{BodyValueEditor, TextEditor, convert_json_scalar, json_scalar_at, text_position},
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
use dialog::{
    DialogAction, remove_header, remove_header_map, resolved_header_value, split_key_value,
};

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

    fn label(self, text: UiText) -> &'static str {
        match self {
            Self::Requests => text.requests(),
            Self::Variables => text.variables(),
            Self::Preview => text.request_editor(),
            Self::Actions => text.send_actions(),
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
}

#[derive(Debug, Default)]
pub(crate) struct ResponseContentState {
    pub(crate) scroll: ScrollState,
}

#[derive(Debug, Default)]
pub(crate) struct RequestCollectionState {
    pub(crate) variables: BTreeMap<String, String>,
    pub(crate) request_headers: HashMap<String, Vec<HeaderRow>>,
    pub(crate) request_query_parts: HashMap<String, Vec<BodyPart>>,
    pub(crate) request_form: HashMap<String, BTreeMap<String, String>>,
    pub(crate) request_body_parts: HashMap<String, Vec<BodyPart>>,
    pub(crate) request_states: HashMap<String, RequestRuntimeState>,
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
        let request_headers = config
            .requests
            .iter()
            .map(|request| {
                let headers = request
                    .headers
                    .iter()
                    .map(|(name, value)| HeaderRow {
                        name: name.clone(),
                        value: value.clone(),
                        enabled: true,
                        source: HeaderSource::Request,
                    })
                    .collect();
                (request.id.clone(), headers)
            })
            .collect();

        Self {
            variables,
            request_headers,
            request_query_parts: config
                .requests
                .iter()
                .map(|request| (request.id.clone(), request.query_parts.clone()))
                .collect(),
            request_form: config
                .requests
                .iter()
                .map(|request| (request.id.clone(), request.form.clone()))
                .collect(),
            request_body_parts: config
                .requests
                .iter()
                .map(|request| (request.id.clone(), request.body_parts.clone()))
                .collect(),
            request_states: config
                .requests
                .iter()
                .map(|request| (request.id.clone(), RequestRuntimeState::default()))
                .collect(),
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
    pub(crate) collection_state: RequestCollectionState,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) status: String,
    pub(crate) should_quit: bool,
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
        let collection_state = RequestCollectionState::from_config(&config);

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
            },
            response_state: ResponseContentState::default(),
            collection_state,
            dialog: None,
            status: text.ready().to_string(),
            should_quit: false,
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
        self.requests_state.selected_request = index;
        if changed {
            self.preview_state.editor = None;
            if self.editing_preview_tab().is_some() {
                self.dialog = None;
            }
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
        let mut request = self.current_request().clone();
        request.headers = self.effective_request_headers(&request.id);
        request.query_parts = self
            .collection_state
            .request_query_parts
            .get(&request.id)
            .cloned()
            .unwrap_or_default();
        request.form = self
            .collection_state
            .request_form
            .get(&request.id)
            .cloned()
            .unwrap_or_default();
        request.body_parts = self
            .collection_state
            .request_body_parts
            .get(&request.id)
            .cloned()
            .unwrap_or_default();
        request
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
        if self.preview_state.editor.is_some() {
            return;
        }
        let document = self.body_json();
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

    pub(crate) fn blur_body_editor(&mut self) {
        if self.preview_state.editor.is_some() {
            self.commit_body_value();
        }
    }

    fn handle_body_editor_key(&mut self, key: KeyEvent) {
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

    fn commit_body_value(&mut self) {
        let Some(editor) = self.preview_state.editor.take() else {
            return;
        };
        let Some(replacement) = convert_json_scalar(editor.kind, &editor.input.value) else {
            return;
        };
        let mut document = editor.document;
        document.replace_range(editor.span, &replacement);
        let request_id = self.current_request().id.clone();
        self.collection_state
            .request_body_parts
            .insert(request_id, vec![BodyPart::Raw(document)]);
    }

    pub(crate) fn variable_count(&self) -> usize {
        self.config.variables.len()
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
            .variables
            .keys()
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
        let request_id = self.current_request().id.clone();
        self.preview_state.active_tab = PreviewTab::Headers;
        let request_rows = self
            .collection_state
            .request_headers
            .get(&request_id)
            .cloned()
            .unwrap_or_default();
        let mut rows = self
            .resolved_collection_headers()
            .into_iter()
            .filter(|(name, _)| {
                !request_rows
                    .iter()
                    .any(|row| row.name.eq_ignore_ascii_case(name))
            })
            .map(|(name, value)| HeaderRow {
                name,
                value,
                enabled: true,
                source: HeaderSource::Collection,
            })
            .collect::<Vec<_>>();
        rows.extend(request_rows.iter().map(|row| HeaderRow {
            value: self.resolved_request_header_value(row),
            ..row.clone()
        }));
        self.dialog = Some(Dialog::Headers(HeadersDialog {
            request_id,
            rows,
            selected: 0,
            field: HeaderField::Value,
            focus: DialogFocus::Content,
            editor: None,
        }));
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
        let request_id = self.current_request().id.clone();
        self.preview_state.active_tab = PreviewTab::Params;
        let resolved = self.current_resolved_request();
        let query_parts = self
            .collection_state
            .request_query_parts
            .get(&request_id)
            .cloned()
            .unwrap_or_default();
        let mut rows = Vec::new();
        for (index, part) in resolved.query_parts.into_iter().enumerate() {
            let part_type = query_parts
                .get(index)
                .map(|part| match part {
                    BodyPart::Raw(_) => BodyPartSource::Raw,
                    BodyPart::UrlEncoded(_) => BodyPartSource::UrlEncoded,
                })
                .unwrap_or(BodyPartSource::Raw);
            let part = match part_type {
                BodyPartSource::Raw => part,
                BodyPartSource::UrlEncoded => template::decode_urlencoded_data(&part),
            };
            let (key, value) = split_key_value(&part);
            rows.push(ParamsDialogRow {
                source: ParamSource::Query,
                key,
                value,
                part_type: Some(part_type),
            });
        }
        for (key, value) in resolved.form {
            rows.push(ParamsDialogRow {
                source: ParamSource::Form,
                key,
                value,
                part_type: None,
            });
        }

        self.dialog = Some(Dialog::Params(ParamsDialog {
            request_id,
            rows,
            selected: 0,
            field: HeaderField::Name,
            focus: DialogFocus::Content,
            editor: None,
            add_source: ParamSource::Query,
        }));
        self.focus = Focus::Preview;
        tracing::debug!(
            query_row_count = self
                .collection_state
                .request_query_parts
                .get(&self.current_request().id)
                .map_or(0, Vec::len),
            form_field_count = self
                .collection_state
                .request_form
                .get(&self.current_request().id)
                .map_or(0, BTreeMap::len),
            "打开参数窗口"
        );
    }

    pub(crate) fn handle_preview_action(&mut self, action: PreviewAction) {
        match action {
            PreviewAction::Send => {
                self.focus = Focus::Actions;
                self.send_current_request();
            }
            PreviewAction::Edit(tab) if self.editing_preview_tab() == Some(tab) => {
                if let Some(dialog) = self.dialog.as_mut() {
                    dialog.commit_editor();
                }
                self.apply_dialog();
            }
            PreviewAction::Edit(PreviewTab::Body) => {
                if self.preview_state.editor.is_some() {
                    self.preview_state.editor = None;
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
                    && self.editing_preview_tab().is_none()
                    && self.preview_state.editor.is_none()
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
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        dialog.commit_editor();
        self.persist_request_edits();

        let dialog = self.dialog.take().expect("dialog exists");
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
                    .collection_state
                    .request_headers
                    .get(&dialog.request_id)
                    .into_iter()
                    .flatten()
                    .filter(|row| row.enabled)
                    .count();
                self.status = self.text().headers_applied().to_string();
                tracing::debug!(header_count, "应用请求 Header 修改");
            }
            Dialog::Params(_) => {
                self.status = self.text().params_applied().to_string();
                tracing::debug!(
                    query_part_count = self
                        .collection_state
                        .request_query_parts
                        .get(&self.current_request().id)
                        .map_or(0, Vec::len),
                    form_field_count = self
                        .collection_state
                        .request_form
                        .get(&self.current_request().id)
                        .map_or(0, BTreeMap::len),
                    "应用请求参数修改"
                );
            }
        }
    }

    pub(crate) fn handle_dialog_key(&mut self, key: KeyEvent) {
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        let was_editing = dialog.is_editing();
        let previous_focus = dialog.focus();
        let action = dialog.handle_key(key);
        match action {
            DialogAction::None => {
                let committed_editor = was_editing && key.code == KeyCode::Enter;
                let changed_row = !was_editing
                    && (matches!(key.code, KeyCode::Char('a' | 'd' | ' '))
                        || key.code == KeyCode::Enter && previous_focus == DialogFocus::Add);
                if committed_editor || changed_row {
                    self.persist_request_edits();
                }
            }
            DialogAction::Apply => self.apply_dialog(),
            DialogAction::Cancel => {
                self.persist_request_edits();
                self.close_dialog();
            }
        }
    }

    fn persist_request_edits(&mut self) {
        let Some(mut dialog) = self.dialog.clone() else {
            return;
        };
        dialog.commit_editor();
        match dialog {
            Dialog::Headers(dialog) => {
                let rows = dialog
                    .rows
                    .into_iter()
                    .filter(|row| {
                        row.source == HeaderSource::Request && !row.name.trim().is_empty()
                    })
                    .fold(Vec::new(), |mut rows, row| {
                        remove_header(&mut rows, &row.name);
                        rows.push(HeaderRow {
                            name: row.name.trim().to_string(),
                            ..row
                        });
                        rows
                    });
                self.collection_state
                    .request_headers
                    .insert(dialog.request_id, rows);
            }
            Dialog::Params(dialog) => {
                let mut query_parts = Vec::new();
                let mut form = BTreeMap::new();
                for row in dialog.rows {
                    match row.source {
                        ParamSource::Query if !row.key.is_empty() || !row.value.is_empty() => {
                            let value = if row.value.is_empty() && !row.key.contains('=') {
                                row.key.trim().to_string()
                            } else {
                                format!("{}={}", row.key.trim(), row.value.trim())
                            };
                            query_parts
                                .push(row.part_type.unwrap_or(BodyPartSource::Raw).to_part(value));
                        }
                        ParamSource::Form if !row.key.trim().is_empty() => {
                            form.insert(row.key.trim().to_string(), row.value);
                        }
                        _ => {}
                    }
                }
                self.collection_state
                    .request_query_parts
                    .insert(dialog.request_id.clone(), query_parts);
                self.collection_state
                    .request_form
                    .insert(dialog.request_id, form);
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
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.commit_editor();
            dialog.click_param_row(index, field, edit);
        }
        self.persist_request_edits();
    }

    pub(crate) fn click_header_row(&mut self, index: usize, field: HeaderField, edit: bool) {
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.commit_editor();
            dialog.click_header_row(index, field, edit);
        }
        self.persist_request_edits();
    }

    pub(crate) fn toggle_header_row(&mut self, index: usize) {
        if let Some(Dialog::Headers(dialog)) = self.dialog.as_mut() {
            dialog.editor = None;
            dialog.selected = index;
            dialog.focus = DialogFocus::Content;
            dialog.toggle_selected();
        }
        self.persist_request_edits();
    }

    pub(crate) fn focus_dialog(&mut self, focus: DialogFocus) {
        if let Some(dialog) = self.dialog.as_mut() {
            match dialog {
                Dialog::Variables(dialog) => dialog.focus = focus,
                Dialog::Headers(dialog) => dialog.focus = focus,
                Dialog::Params(dialog) => dialog.focus = focus,
            }
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
        for row in self
            .collection_state
            .request_headers
            .get(request_id)
            .into_iter()
            .flatten()
        {
            remove_header_map(&mut headers, &row.name);
            if row.enabled && !row.name.trim().is_empty() {
                headers.insert(row.name.clone(), row.value.clone());
            }
        }
        headers
    }

    fn resolved_collection_headers(&self) -> BTreeMap<String, String> {
        let mut request = self.current_request().clone();
        request.headers = self.config.headers.clone();
        template::resolve_request(&request, &self.collection_state.variables).headers
    }

    fn resolved_request_header_value(&self, row: &HeaderRow) -> String {
        let mut request = self.current_request().clone();
        request.headers.clear();
        request.headers.insert(row.name.clone(), row.value.clone());
        let headers = template::resolve_request(&request, &self.collection_state.variables).headers;
        resolved_header_value(&headers, &row.name)
            .unwrap_or(&row.value)
            .to_string()
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
                    let Some(state) = self.collection_state.request_states.get_mut(&request_id)
                    else {
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
            self.handle_dialog_key(key);
            return;
        }

        if self.preview_state.editor.is_some() {
            self.handle_body_editor_key(key);
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
            KeyCode::Char('r') => self.handle_preview_action(PreviewAction::Send),
            KeyCode::Char('v') => self.open_variables(),
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
            Focus::Requests => {
                let name = self.current_request().name.clone();
                self.status = self.text().selected_request(&name);
            }
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

    pub(crate) fn send_current_request(&mut self) {
        tracing::debug!(request_id = %self.current_request().id, "触发发送当前请求");
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
    use crate::{config::load, http::ResponseData, settings::GlobalConfig};

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
            download_path: None,
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
        assert_eq!(app.focus, Focus::Requests);
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
                assert_eq!(dialog.rows[index].value, "fastapi-mock");
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
            app.collection_state.request_headers[&app.current_request().id]
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
        assert_eq!(dialog.rows[0].value, "文档审查 & edge");
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
        app.collection_state.request_query_parts.insert(
            request_id,
            vec![BodyPart::Raw("term=plain value".to_string())],
        );

        app.open_params();

        let Dialog::Params(dialog) = app.dialog.as_ref().expect("参数编辑器应当打开")
        else {
            panic!("应打开参数编辑器");
        };
        assert_eq!(dialog.rows[0].value, "plain value");
        assert_eq!(dialog.rows[0].part_type, Some(BodyPartSource::Raw));
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
        app.collection_state.request_body_parts.insert(
            app.current_request().id.clone(),
            vec![BodyPart::Raw(r#"{"count": 1}"#.to_string())],
        );
        let before = app.body_json();
        let offset = before.find('1').unwrap();
        let prefix = &before[..offset];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
        let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
        let column = prefix[line_start..].chars().count();
        app.start_body_edit(line, column);
        app.preview_state.editor.as_mut().unwrap().input.value = "invalid".to_string();

        app.blur_body_editor();

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
            .request_headers
            .get_mut(&request_id)
            .expect("请求 Header 状态应存在")
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
