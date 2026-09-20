use super::{
    App, ConfigurationsDialog, Dialog, ErrorPage, Feedback, Focus, PreviewContentState,
    RequestDraft, RequestStatus, ResponseContentState, WorkspaceSession,
};

pub(super) struct ReloadedWorkspace {
    config: crate::config::WorkspaceConfig,
    baseline_config: crate::config::WorkspaceConfig,
    baseline_requests: std::collections::BTreeMap<String, crate::config::ApiRequest>,
    session: WorkspaceSession,
    warning: Option<ErrorPage>,
}

impl App {
    pub(super) fn has_request_changes(&self) -> bool {
        self.workspace_state.requests.iter().any(|session| {
            session.has_extract_order()
                || self.request_modified(&session.source.id)
                || self
                    .baseline_requests
                    .get(&session.source.id)
                    .is_some_and(|baseline| {
                        session.has_inactive_header_changes(baseline, &self.baseline_config)
                    })
        })
    }

    pub(crate) fn request_modified(&self, request_id: &str) -> bool {
        let Some(session) = self.workspace_state.request(request_id) else {
            return false;
        };
        let Some(source) = self.baseline_requests.get(request_id) else {
            return false;
        };
        let Some(configuration) = self
            .baseline_config
            .configurations
            .get(self.active_configuration())
        else {
            return false;
        };
        !session.draft.matches_configuration(source, configuration)
    }

    pub(super) fn restore_current_request(&mut self) {
        let Some(request_id) = self.current_request().map(|request| request.id.clone()) else {
            return;
        };
        let Some(source) = self.baseline_requests.get(&request_id) else {
            return;
        };
        let Some(configuration) = self
            .baseline_config
            .configurations
            .get(self.active_configuration())
        else {
            return;
        };
        if let Some(session) = self.workspace_state.request_mut(&request_id) {
            let request = source.for_configuration(configuration);
            session.draft = RequestDraft::from(&request);
            session.reset_temporary_variables(&self.baseline_config, configuration, &request);
            session.set_extract_order(None);
        }
        self.view.preview = PreviewContentState::default();
        self.view.dialog = None;
        self.view.notice = Some(Feedback::Success(
            self.text().request_restored().to_string(),
        ));
    }

    pub(super) fn restore_configuration_requests(&mut self) {
        let Some(configuration) = self
            .baseline_config
            .configurations
            .get(self.active_configuration())
            .cloned()
        else {
            return;
        };
        for session in &mut self.workspace_state.requests {
            if let Some(source) = self.baseline_requests.get(&session.source.id) {
                let request = source.for_configuration(&configuration);
                session.draft = RequestDraft::from(&request);
                session.reset_temporary_variables(&self.baseline_config, &configuration, &request);
                session.set_extract_order(None);
            }
        }
        self.config
            .configurations
            .insert(self.active_configuration().to_string(), configuration);
        self.view.preview = PreviewContentState::default();
        self.view.dialog = None;
        self.view.notice = Some(Feedback::Success(
            self.text().configuration_restored().to_string(),
        ));
    }

    pub(crate) fn open_configurations(&mut self) {
        let rows: Vec<String> = self.configuration_names().map(str::to_string).collect();
        let selected = rows
            .iter()
            .position(|configuration| configuration == self.active_configuration())
            .unwrap_or_default();
        let mut state = tui_assets_rust::DropdownState::default();
        state.select(selected, rows.len());
        state.open();
        self.view.dialog = Some(Dialog::Configurations(ConfigurationsDialog { rows, state }));
        self.view.focus = Focus::WorkspaceButton;
        tracing::debug!(
            configuration = %self.active_configuration(),
            configuration_count = self.config.configurations.len(),
            "打开 workspace 配置下拉菜单"
        );
    }

    /// 触发按钮的行为：已打开时关闭，否则打开。
    pub(crate) fn toggle_configurations(&mut self) {
        if matches!(self.view.dialog, Some(Dialog::Configurations(_))) {
            self.close_dialog();
        } else {
            self.open_configurations();
        }
    }

    pub(crate) fn switch_configuration(&mut self, configuration: &str) {
        if self.workspace_reload.is_some() {
            return;
        }
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
        self.view.cancel_active_editors();
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
            self.text().configuration_switched().to_string(),
        ));
        tracing::debug!(configuration, "切换 workspace 配置");
    }

    pub(crate) fn reload_workspace(&mut self) {
        if self.workspace_reload.is_some() {
            return;
        }
        if self
            .workspace_state
            .requests
            .iter()
            .any(|session| session.runtime.status() == RequestStatus::Sending)
        {
            self.view.notice = Some(Feedback::Warning(
                self.text().reload_while_sending().to_string(),
            ));
            return;
        }

        let active_configuration = self.active_configuration().to_string();
        let path = self.workspace_path().to_path_buf();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.workspace_reload = Some(receiver);
        self.view.notice = None;
        std::thread::spawn(move || {
            let loaded = match crate::config::load_tolerant(&path) {
                Ok(loaded) => loaded,
                Err(error) => {
                    let _ =
                        sender.send(Err(ErrorPage::from_error(&error, path.join("postui.yaml"))));
                    return;
                }
            };
            let warning = ErrorPage::from_diagnostics(&loaded.warnings);
            let (mut config, requests) = loaded.config.into_workspace();
            if config.configurations.contains_key(&active_configuration) {
                config.default_configuration = active_configuration;
            }
            let session = WorkspaceSession::from_config(&config, requests);
            let baseline_config = config.clone();
            let baseline_requests = session
                .requests
                .iter()
                .map(|session| (session.source.id.clone(), session.source.clone()))
                .collect();
            let _ = sender.send(Ok(ReloadedWorkspace {
                config,
                baseline_config,
                baseline_requests,
                session,
                warning,
            }));
        });
    }

    pub(crate) fn poll_workspace_reload(&mut self) -> bool {
        let Some(receiver) = self.workspace_reload.as_ref() else {
            return false;
        };
        let loaded = match receiver.try_recv() {
            Ok(loaded) => loaded,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(ErrorPage::from_message(
                self.workspace_path().join("postui.yaml"),
                crate::diagnostics::invalid(
                    &self.workspace_path().join("postui.yaml"),
                    "workspace reload",
                    "Workspace reload ended unexpectedly",
                )
                .to_string(),
            )),
        };
        self.workspace_reload = None;
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(error_page) => {
                self.error_page = Some(error_page);
                return true;
            }
        };
        let selected_request = self.current_request().map(|request| request.id.clone());
        let mut workspace_state = loaded.session;
        if let Some(request_id) = selected_request {
            workspace_state.selected_request = workspace_state
                .requests
                .iter()
                .position(|session| session.source.id == request_id)
                .or(workspace_state.selected_request);
        }
        self.config = loaded.config;
        self.baseline_config = loaded.baseline_config;
        self.baseline_requests = loaded.baseline_requests;
        self.workspace_state = workspace_state;
        self.error_page = loaded.warning;
        self.view = super::ViewState::default();
        self.view.notice = Some(Feedback::Success(
            self.text().workspace_reloaded().to_string(),
        ));
        tracing::debug!("工作区配置重新加载完成");
        true
    }
}
