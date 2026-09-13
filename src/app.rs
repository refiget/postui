use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{
    config::{ApiRequest, DataPart, RequestConfig, RequestParam, WorkspaceConfig},
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
mod input;
mod response;
mod search;
mod session;
mod variables;
mod view;
mod workspace;
use view::ViewState;
pub(crate) use view::{AppPrompt, FileValueEditor, Focus};
use view::{PreviewContentState, ResponseContentState, ViewMode};

use dialog::DialogAction;
pub(crate) use dialog::{
    ConfigurationsDialog, DataPartSource, Dialog, HeaderRow, HeaderSource, HeadersDialog,
    KeyValueField, ParamSource, ParamsDialog, ParamsDialogRow,
};
pub(crate) use feedback::Feedback;
pub(crate) use session::RequestStatus;
pub(crate) use session::{RequestDraft, WorkspaceSession};
use variables::VariablesPageAction;
pub(crate) use variables::{VariablePageFocus, VariablesPage};

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
    CopyBody,
    CopyHeaders,
}

impl ResponseMenuAction {
    pub(crate) const fn all() -> [Self; 3] {
        [Self::Download, Self::CopyBody, Self::CopyHeaders]
    }

    pub(crate) fn from_index(index: usize) -> Option<Self> {
        Self::all().get(index).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ResponseTab {
    Raw,
    #[default]
    Formatted,
    Headers,
}

impl ResponseTab {
    fn next(self) -> Self {
        match self {
            Self::Raw => Self::Formatted,
            Self::Formatted => Self::Headers,
            Self::Headers => Self::Raw,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Raw => Self::Headers,
            Self::Formatted => Self::Raw,
            Self::Headers => Self::Formatted,
        }
    }

    fn toggle_format(self) -> Self {
        match self {
            Self::Raw => Self::Formatted,
            Self::Formatted => Self::Raw,
            Self::Headers => Self::Formatted,
        }
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
    response_search_task: Option<response::ResponseSearchTask>,
    workspace_reload:
        Option<std::sync::mpsc::Receiver<Result<workspace::ReloadedWorkspace, String>>>,
    baseline_config: WorkspaceConfig,
    baseline_requests: BTreeMap<String, ApiRequest>,
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
        let baseline_config = config.clone();
        let baseline_requests = requests
            .iter()
            .map(|request| (request.id.clone(), request.clone()))
            .collect();
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
            response_search_task: None,
            workspace_reload: None,
            baseline_config,
            baseline_requests,
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

    pub(crate) fn advance_animation(&mut self) {
        self.view.animation_frame = self.view.animation_frame.wrapping_add(1);
    }

    pub(crate) fn is_animating(&self) -> bool {
        self.workspace_state
            .requests
            .iter()
            .any(|session| session.runtime.status() == RequestStatus::Sending)
    }

    pub(crate) fn has_pending_background_work(&self) -> bool {
        self.is_animating()
            || self.workspace_reload.is_some()
            || self.response_search_task.is_some()
            || self.response_actions.is_running()
    }

    fn register_request_change(&mut self) {
        self.view.notice = None;
    }

    fn request_quit(&mut self) {
        self.view.cancel_active_editors();
        if self
            .workspace_state
            .requests
            .iter()
            .any(|session| self.request_modified(&session.source.id))
            || self.config != self.baseline_config
        {
            self.view.prompt = Some(AppPrompt::ConfirmQuit);
        } else {
            self.should_quit = true;
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
            Some(AppPrompt::ConfirmDelete { request_id }) => match key.code {
                KeyCode::Char('y' | 'Y') => {
                    let request_id = request_id.clone();
                    self.delete_request(&request_id);
                }
                KeyCode::Char('n' | 'N') | KeyCode::Esc => self.view.prompt = None,
                _ => {}
            },
            Some(AppPrompt::ConfirmQuit) => match key.code {
                KeyCode::Char('y' | 'Y') => {
                    self.view.prompt = None;
                    self.should_quit = true;
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
            self.view.cancel_active_editors();
            self.workspace_state.commit_configuration(&mut self.config);
            self.close_response_menu();
            if self.editing_preview_tab().is_some() {
                self.view.dialog = None;
            }
            self.workspace_state.selected_request = Some(index);
            self.view.preview.active_tab = PreviewTab::Body;
            self.view.preview.scroll.reset();
            self.view.response.scroll.reset();
            self.view.response.search_match_line = None;
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

    pub(crate) fn current_request_is_insecure(&self) -> bool {
        let Some(request) = self.current_request() else {
            return false;
        };
        let configuration = self.config.configurations.get(self.active_configuration());
        configuration
            .and_then(|configuration| {
                configuration
                    .request_overrides
                    .get(&request.id)
                    .and_then(|request| request.skip_ssl_verification)
                    .or(configuration.skip_ssl_verification)
            })
            .unwrap_or(request.skip_ssl_verification)
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
                if self.view.preview.is_editing() {
                    self.view.preview.cancel_editor();
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
                    && !matches!(self.view.dialog, Some(Dialog::Configurations(_)))
                    && self.view.variables.is_none()
                    && !self.view.preview.is_editing()
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
                    self.register_request_change();
                }
                let header_count = self
                    .request_draft(&dialog.request_id)
                    .map(|draft| draft.headers.iter())
                    .into_iter()
                    .flatten()
                    .filter(|row| row.enabled)
                    .count();
                tracing::debug!(header_count, "应用请求 Header 修改");
            }
            Dialog::Params(dialog) => {
                if changed {
                    self.register_request_change();
                }
                let (query_part_count, form_field_count) = {
                    self.request_draft(&dialog.request_id)
                        .map(|draft| (draft.query_parts.len(), draft.form.len()))
                        .unwrap_or_default()
                };
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
                    self.register_request_change();
                }
            }
            DialogAction::Apply => self.apply_dialog(),
            DialogAction::Cancel => {
                if self.sync_dialog_draft() {
                    self.register_request_change();
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
            self.register_request_change();
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
            self.register_request_change();
        }
        if let Some(Dialog::Headers(dialog)) = self.view.dialog.as_mut() {
            dialog.selected = index;
            dialog.toggle_selected();
        }
        if self.sync_dialog_draft() {
            self.register_request_change();
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

    pub(crate) fn move_request(&mut self, delta: isize) {
        let visible = self.visible_request_indices();
        let count = visible.len();
        if count == 0 {
            return;
        }
        let current = self
            .workspace_state
            .selected_request
            .and_then(|selected| visible.iter().position(|index| *index == selected))
            .unwrap_or_default();
        let next = (current as isize + delta).rem_euclid(count as isize) as usize;
        self.select_request(visible[next]);
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
                self.register_request_change();
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
            self.register_request_change();
        }
        match self.view.dialog.as_mut() {
            Some(Dialog::Headers(dialog)) if tab == PreviewTab::Headers => dialog.add_row(),
            Some(Dialog::Params(dialog)) if tab == PreviewTab::Params => dialog.add_row(),
            _ => return,
        }
        self.register_request_change();
        tracing::debug!(tab = ?tab, "通过请求标签新增字段");
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
