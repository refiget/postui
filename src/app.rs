use std::path::{Path, PathBuf};

use crate::{
    config::{ApiRequest, DataPart, RequestConfig, RequestParam, WorkspaceConfig},
    editor::{BodyValueEditor, EditInput},
    i18n::UiText,
    request_executor::RequestExecutor,
    request_file::RequestFileStore,
    response_action::ResponseActionExecutor,
    settings::GlobalConfig,
    template::{self, ResolvedRequest},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

mod dialog;
mod editing;
mod execution;
mod feedback;
mod response;
mod session;
mod variables;
mod view;
use view::ViewState;

use dialog::DialogAction;
pub(crate) use dialog::{
    ConfigurationsDialog, DataPartSource, Dialog, HeaderRow, HeaderSource, HeadersDialog,
    KeyValueField, ParamSource, ParamsDialog, ParamsDialogRow,
};
pub(crate) use feedback::Feedback;
pub(crate) use session::RequestStatus;
pub(crate) use session::{RequestDraft, WorkspaceSession};
use variables::VariablesPageAction;
pub(crate) use variables::{VariablePageFocus, VariableRow, VariablesPage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Header,
    Requests,
    WorkspaceButton,
    Variables,
    Preview,
    SendButton,
    ResponseActions,
    ResponseZoom,
    Response,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ViewMode {
    #[default]
    Standard,
    ResponseZoom {
        return_focus: Focus,
    },
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Header => Self::Requests,
            Self::Requests => Self::WorkspaceButton,
            Self::WorkspaceButton => Self::Variables,
            Self::Variables => Self::Preview,
            Self::Preview => Self::SendButton,
            Self::SendButton => Self::ResponseActions,
            Self::ResponseActions => Self::ResponseZoom,
            Self::ResponseZoom => Self::Response,
            Self::Response => Self::Header,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Header => Self::Response,
            Self::Requests => Self::Header,
            Self::WorkspaceButton => Self::Requests,
            Self::Variables => Self::WorkspaceButton,
            Self::Preview => Self::Variables,
            Self::SendButton => Self::Preview,
            Self::ResponseActions => Self::SendButton,
            Self::ResponseZoom => Self::ResponseActions,
            Self::Response => Self::ResponseZoom,
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

#[derive(Debug, Default)]
pub(crate) struct ResponseScrollState {
    offset: usize,
    pub(crate) drag_anchor: Option<(u16, usize)>,
}

impl ResponseScrollState {
    const STEP: usize = 3;

    pub(crate) fn offset(&self) -> usize {
        self.offset
    }

    fn reset(&mut self) {
        self.offset = 0;
        self.drag_anchor = None;
    }

    pub(crate) fn set_offset(&mut self, offset: usize) {
        self.offset = offset;
    }

    pub(crate) fn move_by(&mut self, direction: isize, max_offset: usize) -> bool {
        let previous = self.offset;
        self.offset = match direction {
            -1 => self.offset.saturating_sub(Self::STEP),
            1 => self.offset.saturating_add(Self::STEP),
            _ => self.offset,
        }
        .min(max_offset);
        self.offset != previous
    }
}

#[derive(Debug, Default)]
pub(crate) struct PreviewContentState {
    pub(crate) active_tab: PreviewTab,
    pub(crate) scroll: ScrollState,
    pub(crate) editor: Option<BodyValueEditor>,
    pub(crate) file_editor: Option<FileValueEditor>,
}

#[derive(Debug)]
pub(crate) enum AppPrompt {
    ConfirmExit,
    ConfirmDelete { request_id: String },
}

#[derive(Debug)]
pub(crate) struct FileValueEditor {
    pub(crate) file_index: usize,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) input: EditInput,
}

#[derive(Debug, Default)]
pub(crate) struct ResponseContentState {
    pub(crate) scroll: ResponseScrollState,
    pub(crate) menu_selection: Option<usize>,
}

pub(crate) struct App {
    pub(crate) view: ViewState,
    pub(crate) config: WorkspaceConfig,
    pub(crate) global_config: GlobalConfig,
    pub(crate) workspace_state: WorkspaceSession,
    pub(crate) should_quit: bool,
    pub(crate) debug_mode: bool,
    request_files: RequestFileStore,
    request_executor: RequestExecutor,
    response_actions: ResponseActionExecutor,
    response_action_running: bool,
}

impl App {
    pub(crate) fn new(
        config: RequestConfig,
        workspace_path: PathBuf,
        global_config: GlobalConfig,
        request_executor: RequestExecutor,
        debug_mode: bool,
    ) -> Self {
        let request_count = config.requests.len();
        let configured_variable_count = config.editable_variables.len();
        let (config, requests) = config.into_workspace();
        let request_files = RequestFileStore::new(workspace_path);
        tracing::debug!(
            config_path = %request_files.workspace_path().display(),
            global_config_path = global_config
                .path
                .as_deref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<内置默认配置>".to_string()),
            theme = %global_config.theme.name,
            request_count,
            configured_variable_count,
            "创建应用状态"
        );
        let workspace_state = WorkspaceSession::from_config(&config, requests);

        Self {
            config,
            global_config,
            view: ViewState::default(),
            workspace_state,
            should_quit: false,
            debug_mode,
            request_files,
            request_executor,
            response_actions: ResponseActionExecutor::new(),
            response_action_running: false,
        }
    }

    pub(crate) fn workspace_path(&self) -> &Path {
        self.request_files.workspace_path()
    }

    pub(crate) fn current_request(&self) -> Option<&ApiRequest> {
        self.workspace_state
            .current()
            .map(|session| &session.source)
    }

    pub(crate) fn has_current_request(&self) -> bool {
        self.workspace_state.current().is_some()
    }

    pub(crate) fn text(&self) -> UiText {
        UiText::new(self.global_config.language)
    }

    pub(crate) fn current_feedback(&self) -> Option<&Feedback> {
        self.view
            .notice
            .as_ref()
            .or_else(|| self.workspace_state.current()?.runtime.feedback())
    }

    pub(crate) fn is_editing(&self) -> bool {
        self.view.preview.editor.is_some()
            || self.view.preview.file_editor.is_some()
            || self
                .view
                .variables
                .as_ref()
                .is_some_and(|page| page.editor.is_some())
            || self.view.dialog.as_ref().is_some_and(Dialog::is_editing)
    }

    pub(crate) fn advance_animation(&mut self) {
        self.view.animation_frame = self.view.animation_frame.wrapping_add(1);
    }

    pub(crate) fn is_animating(&self) -> bool {
        self.workspace_state
            .requests
            .iter()
            .any(|session| session.runtime.status() == RequestStatus::Sending)
    }

    fn mark_current_dirty(&mut self) {
        self.view.notice = None;
        if let Some(session) = self.workspace_state.current_mut() {
            session.dirty = true;
        }
    }

    pub(crate) fn save_current_request(&mut self) {
        self.cancel_active_editors();
        if self
            .current_effective_request()
            .is_none_or(|request| request.url.trim().is_empty())
        {
            self.view.notice = Some(Feedback::Warning(
                self.text().request_url_required().to_string(),
            ));
            return;
        }
        self.workspace_state.commit_configuration(&mut self.config);
        let Some(request) = self.current_request().cloned() else {
            return;
        };
        let request_id = request.id.clone();
        match self.request_files.save(&request) {
            Ok(path) => {
                let configuration_save = self
                    .config
                    .configurations
                    .values()
                    .filter(|configuration| configuration.path.is_some())
                    .try_for_each(|configuration| {
                        self.request_files
                            .save_configuration(configuration)
                            .map(|_| ())
                    });
                if let Err(error) = configuration_save {
                    tracing::error!(request_id = %request_id, error = %format!("{error:#}"), "配置保存失败");
                    self.view.notice = Some(Feedback::Error(
                        self.text().configuration_save_failed(&format!("{error:#}")),
                    ));
                    return;
                }
                if let Some(session) = self.workspace_state.request_mut(&request_id) {
                    session.dirty = false;
                }
                self.view.notice = Some(Feedback::Success(
                    self.text().request_saved(&path.display().to_string()),
                ));
            }
            Err(error) => {
                tracing::error!(request_id = %request_id, error = %format!("{error:#}"), "保存请求失败");
                self.view.notice = Some(Feedback::Error(
                    self.text().request_save_failed(&format!("{error:#}")),
                ));
            }
        }
    }

    fn request_quit(&mut self) {
        self.cancel_active_editors();
        if !self
            .workspace_state
            .requests
            .iter()
            .any(|session| session.dirty)
        {
            self.should_quit = true;
        } else {
            self.view.prompt = Some(AppPrompt::ConfirmExit);
        }
    }

    fn request_delete(&mut self) {
        let Some(request) = self.current_request() else {
            return;
        };
        let request_id = request.id.clone();
        if self.request_status(&request_id) == RequestStatus::Sending {
            self.view.notice = Some(Feedback::Warning(
                self.text().request_in_progress().to_string(),
            ));
            return;
        }
        self.view.prompt = Some(AppPrompt::ConfirmDelete { request_id });
    }

    fn delete_request(&mut self, request_id: &str) {
        let Some(index) = self
            .workspace_state
            .requests
            .iter()
            .position(|session| session.source.id == request_id)
        else {
            self.view.prompt = None;
            return;
        };
        if let Err(error) = self.request_files.delete(request_id) {
            tracing::error!(request_id, error = %format!("{error:#}"), "删除请求失败");
            self.view.notice = Some(Feedback::Error(
                self.text().request_delete_failed(&format!("{error:#}")),
            ));
            return;
        }
        self.workspace_state.requests.remove(index);
        self.workspace_state.selected_request = if self.workspace_state.requests.is_empty() {
            None
        } else {
            Some(index.min(self.workspace_state.requests.len().saturating_sub(1)))
        };
        self.view.preview = PreviewContentState::default();
        self.view.response = ResponseContentState::default();
        self.view.dialog = None;
        self.view.prompt = None;
        self.view.notice = Some(Feedback::Success(self.text().request_deleted().to_string()));
    }

    fn handle_prompt_key(&mut self, key: KeyEvent) {
        match self.view.prompt.as_mut() {
            Some(AppPrompt::ConfirmExit) => match key.code {
                KeyCode::Char('y' | 'Y') => self.should_quit = true,
                KeyCode::Char('n' | 'N') | KeyCode::Esc => self.view.prompt = None,
                _ => {}
            },
            Some(AppPrompt::ConfirmDelete { request_id }) => match key.code {
                KeyCode::Char('y' | 'Y') => {
                    let request_id = request_id.clone();
                    self.delete_request(&request_id);
                }
                KeyCode::Char('n' | 'N') | KeyCode::Esc => self.view.prompt = None,
                _ => {}
            },
            None => {}
        }
    }

    pub(crate) fn select_request(&mut self, index: usize) {
        let request_count = self.workspace_state.requests.len();
        if index >= request_count {
            tracing::debug!(index, request_count, "忽略无效接口索引");
            return;
        }
        let previous = self.workspace_state.selected_request;
        let changed = previous != Some(index);
        if changed {
            self.cancel_active_editors();
            self.workspace_state.commit_configuration(&mut self.config);
            self.close_response_menu();
            if self.editing_preview_tab().is_some() {
                self.view.dialog = None;
            }
            self.workspace_state.selected_request = Some(index);
            self.view.preview.active_tab = PreviewTab::Body;
            self.view.preview.scroll.reset();
            self.view.response.scroll.reset();
            let Some(request_id) = self.current_request().map(|request| request.id.clone()) else {
                return;
            };
            self.view.notice = None;
            tracing::debug!(
                previous_index = ?previous,
                selected_index = index,
                request_id = %request_id,
                "切换当前接口"
            );
        }
    }

    pub(crate) fn current_resolved_request(&self) -> Option<ResolvedRequest> {
        self.current_effective_request()
            .map(|request| template::resolve_request(&request, &self.workspace_state.variables))
    }

    pub(crate) fn current_effective_request(&self) -> Option<ApiRequest> {
        self.workspace_state.current_effective_request(&self.config)
    }

    fn effective_request(&self, request: &ApiRequest) -> ApiRequest {
        self.workspace_state
            .request(&request.id)
            .and_then(|session| {
                self.workspace_state
                    .effective_request(&self.config, session)
            })
            .unwrap_or_else(|| request.clone())
    }

    fn request_draft(&self, request_id: &str) -> Option<&RequestDraft> {
        self.workspace_state
            .request(request_id)
            .map(|session| &session.draft)
    }

    fn request_draft_mut(&mut self, request_id: &str) -> Option<&mut RequestDraft> {
        self.workspace_state
            .request_mut(request_id)
            .map(|session| &mut session.draft)
    }

    pub(crate) fn variable_count(&self) -> usize {
        self.config.editable_variables.len()
    }

    pub(crate) fn active_configuration(&self) -> &str {
        &self.workspace_state.active_configuration
    }

    pub(crate) fn configuration_names(&self) -> impl Iterator<Item = &str> {
        self.config.configurations.keys().map(String::as_str)
    }

    pub(crate) fn variable_default_value(&self, variable: &str) -> String {
        self.config
            .configurations
            .get(self.active_configuration())
            .and_then(|configuration| configuration.variables.get(variable))
            .or_else(|| self.config.variables.get(variable))
            .and_then(|definition| definition.default.as_ref())
            .map(crate::config::value_to_string)
            .unwrap_or_else(|| "—".to_string())
    }

    pub(crate) fn open_configurations(&mut self) {
        let rows: Vec<String> = self.configuration_names().map(str::to_string).collect();
        let selected = rows
            .iter()
            .position(|configuration| configuration == self.active_configuration())
            .unwrap_or_default();
        self.view.dialog = Some(Dialog::Configurations(ConfigurationsDialog {
            rows,
            selected,
        }));
        self.view.focus = Focus::WorkspaceButton;
        tracing::debug!(
            configuration = %self.active_configuration(),
            configuration_count = self.config.configurations.len(),
            "打开 workspace 配置下拉菜单"
        );
    }

    pub(crate) fn switch_configuration(&mut self, configuration: &str) {
        if configuration == self.active_configuration() {
            self.close_dialog();
            return;
        }
        if self
            .workspace_state
            .requests
            .iter()
            .any(|session| session.runtime.status() == RequestStatus::Sending)
        {
            self.view.notice = Some(Feedback::Warning(
                self.text().request_in_progress().to_string(),
            ));
            return;
        }
        self.cancel_active_editors();
        if !self
            .workspace_state
            .switch_configuration(&mut self.config, configuration)
        {
            return;
        }
        self.view.dialog = None;
        self.view.preview = PreviewContentState::default();
        self.view.response = ResponseContentState::default();
        self.view.notice = Some(Feedback::Success(
            self.text().configuration_switched(configuration),
        ));
        tracing::debug!(configuration, "切换 workspace 配置");
    }

    pub(crate) fn current_header_count(&self) -> usize {
        self.current_effective_request()
            .map(|request| request.headers.len())
            .unwrap_or_default()
    }

    pub(crate) fn current_param_count(&self) -> usize {
        let Some(request) = self.current_effective_request() else {
            return 0;
        };
        let url_parts = template::split_url_query(&request.url);
        let url_count = template::parse_query_params(&url_parts.query).len();
        let body_count = self
            .current_request()
            .map(|request| request.id.clone())
            .and_then(|request_id| self.request_draft(&request_id))
            .map(|draft| {
                draft
                    .body_parts
                    .iter()
                    .filter(|part| matches!(part, DataPart::UrlEncoded(_)))
                    .count()
            })
            .unwrap_or_default();
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
        let return_focus = self.view.focus;
        self.view.variables = Some(VariablesPage {
            rows,
            selected: 0,
            focus: VariablePageFocus::Content,
            editor: None,
            return_focus,
        });
        self.view.focus = Focus::Variables;
        tracing::debug!(variable_count = self.variable_count(), "打开工作区变量页面");
    }

    pub(crate) fn open_headers(&mut self) {
        let Some(request) = self.current_request() else {
            return;
        };
        if self.request_status(&request.id) == RequestStatus::Sending {
            tracing::debug!("请求执行中，忽略打开 Header 编辑窗口");
            self.view.notice = Some(Feedback::Warning(
                self.text().request_in_progress().to_string(),
            ));
            return;
        }
        self.view.preview.active_tab = PreviewTab::Headers;
        self.view.dialog = self.preview_dialog(PreviewTab::Headers);
        self.view.focus = Focus::Preview;
        tracing::debug!(
            header_count = self.current_header_count(),
            "打开请求 Header 窗口"
        );
    }

    pub(crate) fn open_params(&mut self) {
        let Some(request_id) = self.current_request().map(|request| request.id.clone()) else {
            return;
        };
        if self.request_status(&request_id) == RequestStatus::Sending {
            tracing::debug!("请求执行中，忽略打开参数编辑窗口");
            self.view.notice = Some(Feedback::Warning(
                self.text().request_in_progress().to_string(),
            ));
            return;
        }
        self.view.preview.active_tab = PreviewTab::Params;
        self.view.dialog = self.preview_dialog(PreviewTab::Params);
        self.view.focus = Focus::Preview;
        let (query_row_count, form_field_count) = self
            .request_draft(&request_id)
            .map(|draft| (draft.query_parts.len(), draft.form.len()))
            .unwrap_or_default();
        tracing::debug!(query_row_count, form_field_count, "打开参数窗口");
    }

    pub(crate) fn preview_dialog(&self, tab: PreviewTab) -> Option<Dialog> {
        let request = self.current_request()?;
        let request_id = request.id.clone();
        let draft = self.request_draft(&request_id)?.clone();
        match tab {
            PreviewTab::Body => None,
            PreviewTab::Headers => {
                let request_rows = draft.headers;
                let mut rows = self
                    .config
                    .headers
                    .iter()
                    .filter(|header| {
                        !request_rows
                            .iter()
                            .any(|row| row.name.eq_ignore_ascii_case(&header.name))
                    })
                    .map(|header| HeaderRow {
                        name: header.name.clone(),
                        value: header.value.clone(),
                        enabled: true,
                        source: HeaderSource::Collection,
                    })
                    .collect::<Vec<_>>();
                rows.extend(request_rows);
                Some(Dialog::Headers(HeadersDialog {
                    request_id,
                    rows,
                    selected: 0,
                    field: KeyValueField::Value,
                    editor: None,
                }))
            }
            PreviewTab::Params => {
                let mut rows = Vec::new();
                let effective_url = draft.url.as_deref().unwrap_or(request.url.as_str());
                let url_parts = template::split_url_query(effective_url);
                for parameter in template::parse_query_params(&url_parts.query) {
                    rows.push(ParamsDialogRow {
                        source: ParamSource::Url,
                        key: parameter.name,
                        value: parameter.value,
                        part_type: None,
                        has_equals: parameter.has_equals,
                    });
                }
                for part in draft.query_parts {
                    let (part_type, parameter) = match part {
                        DataPart::Raw(part) => {
                            (DataPartSource::Raw, RequestParam::from_text(&part))
                        }
                        DataPart::UrlEncoded(parameter) => (DataPartSource::UrlEncoded, parameter),
                    };
                    rows.push(ParamsDialogRow {
                        source: ParamSource::Query,
                        key: parameter.name,
                        value: parameter.value,
                        part_type: Some(part_type),
                        has_equals: parameter.has_equals,
                    });
                }
                for field in &draft.form {
                    rows.push(ParamsDialogRow {
                        source: ParamSource::Form,
                        key: field.name.clone(),
                        value: field.value.clone(),
                        part_type: None,
                        has_equals: true,
                    });
                }
                if draft
                    .body_parts
                    .iter()
                    .all(|part| matches!(part, DataPart::UrlEncoded(_)))
                {
                    for part in &draft.body_parts {
                        let DataPart::UrlEncoded(parameter) = part else {
                            continue;
                        };
                        rows.push(ParamsDialogRow {
                            source: ParamSource::Body,
                            key: parameter.name.clone(),
                            value: parameter.value.clone(),
                            part_type: Some(DataPartSource::UrlEncoded),
                            has_equals: parameter.has_equals,
                        });
                    }
                }

                Some(Dialog::Params(ParamsDialog {
                    request_id,
                    rows,
                    selected: 0,
                    field: KeyValueField::Name,
                    editor: None,
                }))
            }
        }
    }

    pub(crate) fn handle_preview_action(&mut self, action: PreviewAction) {
        match action {
            PreviewAction::Send => self.send_current_request(),
            PreviewAction::Edit(tab) if self.editing_preview_tab() == Some(tab) => {
                if let Some(dialog) = self.view.dialog.as_mut() {
                    dialog.cancel_editor();
                }
                self.apply_dialog();
            }
            PreviewAction::Edit(PreviewTab::Body) => {
                if self.view.preview.editor.is_some() || self.view.preview.file_editor.is_some() {
                    self.view.preview.editor = None;
                    self.view.preview.file_editor = None;
                } else {
                    self.start_body_edit(0, 0);
                }
            }
            PreviewAction::Edit(PreviewTab::Params) => self.open_params(),
            PreviewAction::Edit(PreviewTab::Headers) => self.open_headers(),
        }
    }

    pub(crate) fn editing_preview_tab(&self) -> Option<PreviewTab> {
        self.view.dialog.as_ref().and_then(Dialog::preview_tab)
    }

    pub(crate) fn can_execute_preview_action(&self, action: PreviewAction) -> bool {
        match action {
            PreviewAction::Send => self.current_effective_request().is_some_and(|request| {
                !request.url.trim().is_empty()
                    && supports_method(&request.method)
                    && self.current_request().is_some_and(|current| {
                        self.request_status(&current.id) != RequestStatus::Sending
                    })
                    && !matches!(self.view.dialog, Some(Dialog::Configurations(_)))
                    && self.view.variables.is_none()
                    && self.view.preview.editor.is_none()
                    && self.view.preview.file_editor.is_none()
            }),
            PreviewAction::Edit(tab) => self.current_request().is_some_and(|request| {
                self.editing_preview_tab()
                    .is_none_or(|editing_tab| editing_tab == tab)
                    && self.request_status(&request.id) != RequestStatus::Sending
            }),
        }
    }

    pub(crate) fn focused_preview_action(&self) -> Option<PreviewAction> {
        match self.view.focus {
            Focus::SendButton => Some(PreviewAction::Send),
            _ => None,
        }
    }

    pub(crate) fn close_dialog(&mut self) {
        if self.view.dialog.take().is_some() {
            tracing::debug!("关闭配置编辑窗口");
        }
    }

    fn close_variables(&mut self) {
        let Some(page) = self.view.variables.take() else {
            return;
        };
        self.view.focus = page.return_focus;
        tracing::debug!("关闭工作区变量页面");
    }

    fn apply_variables(&mut self) {
        let Some(mut page) = self.view.variables.take() else {
            return;
        };
        page.cancel_editor();
        for row in page.rows {
            self.workspace_state.variables.insert(row.name, row.value);
        }
        self.view.notice = Some(Feedback::Success(
            self.text().variables_applied().to_string(),
        ));
        tracing::debug!(
            variable_count = self.workspace_state.variables.len(),
            "应用工作区变量修改"
        );
        self.view.focus = page.return_focus;
    }

    fn handle_variables_key(&mut self, key: KeyEvent) {
        let Some(page) = self.view.variables.as_mut() else {
            return;
        };
        match page.handle_key(key) {
            Some(VariablesPageAction::Apply) => self.apply_variables(),
            Some(VariablesPageAction::Close) => self.close_variables(),
            None => {}
        }
    }

    pub(crate) fn apply_dialog(&mut self) {
        if self.view.dialog.is_none() {
            return;
        }
        let changed = self.sync_dialog_draft();

        let Some(dialog) = self.view.dialog.take() else {
            return;
        };
        match dialog {
            Dialog::Configurations(dialog) => {
                if let Some(configuration) = dialog.rows.get(dialog.selected).cloned() {
                    self.switch_configuration(&configuration);
                }
            }
            Dialog::Headers(dialog) => {
                if changed {
                    self.mark_current_dirty();
                }
                let header_count = self
                    .request_draft(&dialog.request_id)
                    .map(|draft| draft.headers.iter())
                    .into_iter()
                    .flatten()
                    .filter(|row| row.enabled)
                    .count();
                self.view.notice =
                    Some(Feedback::Success(self.text().headers_applied().to_string()));
                tracing::debug!(header_count, "应用请求 Header 修改");
            }
            Dialog::Params(dialog) => {
                if changed {
                    self.mark_current_dirty();
                }
                let (query_part_count, form_field_count) = {
                    self.request_draft(&dialog.request_id)
                        .map(|draft| (draft.query_parts.len(), draft.form.len()))
                        .unwrap_or_default()
                };
                self.view.notice =
                    Some(Feedback::Success(self.text().params_applied().to_string()));
                tracing::debug!(query_part_count, form_field_count, "应用请求参数修改");
            }
        }
    }

    pub(crate) fn handle_dialog_key(&mut self, key: KeyEvent) {
        let Some(dialog) = self.view.dialog.as_mut() else {
            return;
        };
        let action = dialog.handle_key(key);
        match action {
            DialogAction::None => {}
            DialogAction::Changed => {
                if self.sync_dialog_draft() {
                    self.mark_current_dirty();
                }
            }
            DialogAction::Apply => self.apply_dialog(),
            DialogAction::Cancel => {
                if self.sync_dialog_draft() {
                    self.mark_current_dirty();
                }
                self.close_dialog();
            }
        }
    }

    fn sync_dialog_draft(&mut self) -> bool {
        let Some(dialog_state) = self.view.dialog.take() else {
            return false;
        };
        let changed = match &dialog_state {
            Dialog::Headers(dialog) => self.sync_header_dialog(dialog),
            Dialog::Params(dialog) => self.sync_params_dialog(dialog),
            Dialog::Configurations(_) => false,
        };
        self.view.dialog = Some(dialog_state);
        changed
    }

    fn sync_header_dialog(&mut self, dialog: &HeadersDialog) -> bool {
        let rows = dialog
            .rows
            .iter()
            .filter(|row| row.source == HeaderSource::Request && !row.name.trim().is_empty())
            .map(|row| HeaderRow {
                name: row.name.trim().to_string(),
                ..row.clone()
            })
            .collect();
        match self.workspace_state.request_mut(&dialog.request_id) {
            Some(session) if session.draft.headers != rows => {
                session.draft.headers = rows;
                true
            }
            _ => false,
        }
    }

    fn sync_params_dialog(&mut self, dialog: &ParamsDialog) -> bool {
        let request_data = self
            .workspace_state
            .request(&dialog.request_id)
            .map(|session| {
                let effective_url = session
                    .draft
                    .url
                    .clone()
                    .unwrap_or_else(|| session.source.url.clone());
                (effective_url, session.draft.body_parts.clone())
            });
        let Some((effective_url, existing_body_parts)) = request_data else {
            return false;
        };

        let url_location = template::split_url_query(&effective_url);
        let mut url_parts = Vec::new();
        let mut query_parts = Vec::new();
        let mut form = Vec::new();
        let mut body_parts = Vec::new();
        for row in &dialog.rows {
            let key = row.key.trim();
            let value = row.value.trim();
            match row.source {
                ParamSource::Url if !key.is_empty() || !value.is_empty() => {
                    url_parts.push(RequestParam::new(
                        key.to_string(),
                        value.to_string(),
                        row.has_equals,
                    ));
                }
                ParamSource::Query if !key.is_empty() || !value.is_empty() => {
                    query_parts.push(row.part_type.unwrap_or(DataPartSource::Raw).to_part(
                        RequestParam::new(key.to_string(), row.value.clone(), row.has_equals),
                    ));
                }
                ParamSource::Form if !key.is_empty() => {
                    form.push(RequestParam::new(key.to_string(), row.value.clone(), true));
                }
                ParamSource::Body if !key.is_empty() || !value.is_empty() => {
                    body_parts.push(row.part_type.unwrap_or(DataPartSource::UrlEncoded).to_part(
                        RequestParam::new(key.to_string(), row.value.clone(), row.has_equals),
                    ));
                }
                _ => {}
            }
        }

        let url = template::rebuild_url(&url_location.base, &url_parts, &url_location.fragment);
        let url = Some(url);
        let replace_body = !body_parts.is_empty()
            || existing_body_parts
                .iter()
                .all(|part| matches!(part, DataPart::UrlEncoded(_)));
        match self.workspace_state.request_mut(&dialog.request_id) {
            Some(session)
                if session.draft.url != url
                    || session.draft.query_parts != query_parts
                    || session.draft.form != form
                    || (replace_body && session.draft.body_parts != body_parts) =>
            {
                session.draft.url = url;
                session.draft.query_parts = query_parts;
                session.draft.form = form;
                if replace_body {
                    session.draft.body_parts = body_parts;
                }
                true
            }
            _ => false,
        }
    }

    pub(crate) fn move_dialog_selection(&mut self, direction: isize) {
        if let Some(dialog) = self.view.dialog.as_mut() {
            dialog.move_selection(direction);
            tracing::debug!(direction, "移动配置窗口列表选择");
        }
        if self.sync_dialog_draft() {
            self.mark_current_dirty();
        }
    }

    pub(crate) fn click_variable_row(&mut self, index: usize, edit: bool, cursor: Option<usize>) {
        if let Some(page) = self.view.variables.as_mut() {
            page.click_row(index, edit, cursor);
        }
    }

    pub(crate) fn click_configuration_row(&mut self, index: usize) {
        if let Some(dialog) = self.view.dialog.as_mut() {
            dialog.click_configuration_row(index);
        }
    }

    pub(crate) fn click_param_row(
        &mut self,
        index: usize,
        field: KeyValueField,
        edit: bool,
        cursor: Option<usize>,
    ) {
        if let Some(dialog) = self.view.dialog.as_mut() {
            dialog.click_param_row(index, field, edit, cursor);
        }
    }

    pub(crate) fn click_header_row(
        &mut self,
        index: usize,
        field: KeyValueField,
        edit: bool,
        cursor: Option<usize>,
    ) {
        if let Some(dialog) = self.view.dialog.as_mut() {
            dialog.click_header_row(index, field, edit, cursor);
        }
    }

    pub(crate) fn toggle_header_row(&mut self, index: usize) {
        if self.sync_dialog_draft() {
            self.mark_current_dirty();
        }
        if let Some(Dialog::Headers(dialog)) = self.view.dialog.as_mut() {
            dialog.selected = index;
            dialog.toggle_selected();
        }
        if self.sync_dialog_draft() {
            self.mark_current_dirty();
        }
    }

    pub(crate) fn focus_variables_page(&mut self, focus: VariablePageFocus) {
        if let Some(page) = self.view.variables.as_mut() {
            page.focus = focus;
        }
    }

    pub(crate) fn click_variables_page_button(&mut self, focus: VariablePageFocus) {
        if let Some(page) = self.view.variables.as_mut() {
            page.cancel_editor();
        }
        self.focus_variables_page(focus);
        self.handle_variables_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }

    pub(crate) fn move_variable_selection(&mut self, direction: isize) {
        if let Some(page) = self.view.variables.as_mut() {
            page.move_selection(direction);
        }
    }

    pub(crate) fn cancel_variable_edit(&mut self) {
        if let Some(page) = self.view.variables.as_mut() {
            page.cancel_editor();
        }
    }

    pub(crate) fn request_status(&self, request_id: &str) -> RequestStatus {
        self.workspace_state
            .request(request_id)
            .map(|session| session.runtime.status())
            .unwrap_or_default()
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        tracing::trace!(
            key_kind = key_kind(key.code),
            modifiers = ?key.modifiers,
            focus = ?self.view.focus,
            "处理键盘操作"
        );

        if key.code == KeyCode::F(5) && self.debug_mode {
            self.load_next_theme();
            return;
        }

        if self.view.prompt.is_some() {
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

        if self.view.variables.is_some() {
            self.handle_variables_key(key);
            return;
        }

        if self.view.dialog.is_some() {
            let inline_table = self
                .view
                .dialog
                .as_ref()
                .is_some_and(|dialog| dialog.preview_tab().is_some());
            let editing_inline_cell = self.view.dialog.as_ref().is_some_and(Dialog::is_editing);
            let handle_as_global = inline_table
                && !editing_inline_cell
                && (self.view.focus != Focus::Preview
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

        if self.view.preview.editor.is_some() || self.view.preview.file_editor.is_some() {
            self.handle_body_editor_key(key);
            return;
        }

        if self.view.response.menu_selection.is_some() {
            match key.code {
                KeyCode::Esc => self.close_response_menu(),
                KeyCode::Char('q') if self.response_zoomed() => self.restore_standard_view(),
                KeyCode::Up | KeyCode::Char('k') => self.move_response_menu_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => self.move_response_menu_selection(1),
                KeyCode::Enter | KeyCode::Char(' ') => self.activate_selected_response_action(),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.response_zoomed() {
                    self.restore_standard_view();
                } else {
                    self.request_quit();
                    tracing::debug!("通过快捷键请求退出");
                }
            }
            KeyCode::Tab => {
                self.view.focus = self.next_focus(key.modifiers.contains(KeyModifiers::SHIFT));
                tracing::debug!(focus = ?self.view.focus, "切换 TUI 区域焦点");
            }
            KeyCode::BackTab => self.view.focus = self.next_focus(true),
            KeyCode::Char('r') => self.handle_preview_action(PreviewAction::Send),
            KeyCode::Char('w') => self.open_configurations(),
            KeyCode::Char('v') => self.open_variables(),
            KeyCode::Char('o') => self.open_response_menu(),
            KeyCode::Delete if self.view.focus == Focus::Requests => self.request_delete(),
            KeyCode::Left if self.view.focus == Focus::Preview => self.move_preview_tab(-1),
            KeyCode::Right if self.view.focus == Focus::Preview => self.move_preview_tab(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_focused(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_focused(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.handle_enter(),
            _ => {}
        }
    }

    fn load_next_theme(&mut self) {
        let current = self.global_config.theme.name.clone();
        match crate::settings::next_theme(&current) {
            Ok(theme) => {
                let name = theme.name.clone();
                self.global_config.theme = theme;
                self.view.notice = Some(Feedback::Info(self.text().theme_loaded(&name)));
                tracing::debug!(previous_theme = %current, theme = %name, "热加载内置主题");
            }
            Err(error) => {
                self.view.notice = Some(Feedback::Error(
                    self.text().theme_load_failed(&error.to_string()),
                ));
                tracing::error!(error = ?error, "热加载内置主题失败");
            }
        }
    }

    fn handle_enter(&mut self) {
        tracing::debug!(focus = ?self.view.focus, "处理 Enter 操作");
        match self.view.focus {
            Focus::Header => {}
            Focus::Requests => {}
            Focus::Variables => self.open_variables(),
            Focus::Preview => {
                let action = PreviewAction::Edit(self.view.preview.active_tab);
                self.handle_preview_action(action);
            }
            Focus::WorkspaceButton => self.open_configurations(),
            Focus::SendButton => self.handle_preview_action(PreviewAction::Send),
            Focus::ResponseActions => self.open_response_menu(),
            Focus::ResponseZoom => self.toggle_response_zoom(),
            Focus::Response => {}
        }
    }

    fn move_focused(&mut self, direction: isize) {
        match self.view.focus {
            Focus::Header => {}
            Focus::Requests => self.move_request(direction),
            Focus::Preview => {
                self.view.preview.scroll.move_by(direction);
            }
            Focus::Response => self.scroll_response(direction),
            Focus::WorkspaceButton
            | Focus::Variables
            | Focus::SendButton
            | Focus::ResponseActions
            | Focus::ResponseZoom => {}
        }
    }

    pub(crate) fn move_request(&mut self, delta: isize) {
        let count = self.workspace_state.requests.len();
        if count == 0 {
            return;
        }
        let current = self.workspace_state.selected_request.unwrap_or_default() % count;
        let next = (current as isize + delta).rem_euclid(count as isize) as usize;
        self.select_request(next);
    }

    pub(crate) fn move_preview_tab(&mut self, direction: isize) {
        if !self.has_current_request() {
            return;
        }
        let tab = match direction {
            value if value < 0 => self.view.preview.active_tab.previous(),
            value if value > 0 => self.view.preview.active_tab.next(),
            _ => self.view.preview.active_tab,
        };
        self.activate_preview_tab(tab);
        self.view.preview.scroll.reset();
        tracing::debug!(
            tab = ?self.view.preview.active_tab,
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
            if let Some(dialog) = self.view.dialog.as_mut() {
                dialog.cancel_editor();
            }
            if self.sync_dialog_draft() {
                self.mark_current_dirty();
            }
            self.view.dialog = None;
        }
        self.view.preview.active_tab = tab;
        match tab {
            PreviewTab::Body => {}
            PreviewTab::Params => self.open_params(),
            PreviewTab::Headers => self.open_headers(),
        }
    }

    pub(crate) fn add_preview_row(&mut self, tab: PreviewTab) {
        self.activate_preview_tab(tab);
        if let Some(dialog) = self.view.dialog.as_mut() {
            dialog.cancel_editor();
        }
        if self.sync_dialog_draft() {
            self.mark_current_dirty();
        }
        match self.view.dialog.as_mut() {
            Some(Dialog::Headers(dialog)) if tab == PreviewTab::Headers => dialog.add_row(),
            Some(Dialog::Params(dialog)) if tab == PreviewTab::Params => dialog.add_row(),
            _ => return,
        }
        self.mark_current_dirty();
        tracing::debug!(tab = ?tab, "通过请求标签新增字段");
    }

    fn next_focus(&self, reverse: bool) -> Focus {
        if !self.response_zoomed() {
            return if reverse {
                self.view.focus.previous()
            } else {
                self.view.focus.next()
            };
        }
        match (self.view.focus, reverse) {
            (Focus::Header, true) => Focus::Response,
            (Focus::Header, false) => Focus::SendButton,
            (Focus::SendButton, true) => Focus::Header,
            (Focus::SendButton, false) => Focus::ResponseActions,
            (Focus::ResponseActions, true) => Focus::SendButton,
            (Focus::ResponseActions, false) => Focus::ResponseZoom,
            (Focus::ResponseZoom, true) => Focus::ResponseActions,
            (Focus::ResponseZoom, false) => Focus::Response,
            (Focus::Response, true) => Focus::ResponseZoom,
            (Focus::Response, false) => Focus::Header,
            (_, _) => Focus::Response,
        }
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
