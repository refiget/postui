use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::shortcuts::{self, Command, Context};
use crate::{
    config::{ApiRequest, DataPart, RequestConfig, RequestParam, WorkspaceConfig},
    i18n::UiText,
    request_executor::RequestExecutor,
    request_file::RequestFileStore,
    response_action::ResponseActionExecutor,
    settings::GlobalConfig,
    template::{self, ResolvedRequest},
};
use crossterm::event::{KeyCode, KeyEvent};

mod dialog;
mod editing;
mod error_page;
mod execution;
mod preview;
pub(crate) use error_page::ErrorPage;
mod feedback;
mod input;
mod response;
mod search;
mod session;
mod variables;
mod view;
mod workspace;
use view::ViewState;
pub(crate) use view::{AppPrompt, FileValueEditor, Focus, ListScrollState};
use view::{PreviewContentState, ResponseContentState, ViewMode};

use dialog::DialogAction;
pub(crate) use dialog::{
    ConfigurationsDialog, DataPartSource, Dialog, HeaderRow, HeaderSource, HeadersDialog,
    KeyValueField, ParamSource, ParamsDialog, ParamsDialogRow,
};
pub(crate) use feedback::Feedback;
pub(crate) use session::RequestStatus;
pub(crate) use session::{RequestDraft, WorkspaceSession};
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
    error_page: Option<ErrorPage>,
    request_files: RequestFileStore,
    request_executor: RequestExecutor,
    response_actions: ResponseActionExecutor,
    response_search_task: Option<response::ResponseSearchTask>,
    workspace_reload:
        Option<std::sync::mpsc::Receiver<Result<workspace::ReloadedWorkspace, ErrorPage>>>,
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
        error_page: Option<ErrorPage>,
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
            error_page,
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
        if let Some(notice) = self.view.notice.as_ref() {
            return Some(notice);
        }
        if self.workspace_reload.is_some() {
            return None;
        }
        self.workspace_state.current()?.runtime.feedback()
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
        if self.has_request_changes() || self.config != self.baseline_config {
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
        match shortcuts::resolve(Context::Confirm, key, false) {
            Some(Command::Back) => self.view.prompt = None,
            Some(Command::Confirm) => match self.view.prompt.take() {
                Some(AppPrompt::ConfirmDelete { request_id }) => self.delete_request(&request_id),
                Some(AppPrompt::ConfirmQuit) => self.should_quit = true,
                None => {}
            },
            _ => {}
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

    pub(crate) fn request_draft(&self, request_id: &str) -> Option<&RequestDraft> {
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
