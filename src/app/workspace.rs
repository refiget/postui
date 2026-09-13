use super::{
    App, ConfigurationsDialog, Dialog, Feedback, Focus, PreviewContentState, RequestDraft,
    RequestStatus, ResponseContentState, WorkspaceSession,
};

pub(super) struct ReloadedWorkspace {
    config: crate::config::WorkspaceConfig,
    baseline_config: crate::config::WorkspaceConfig,
    baseline_requests: std::collections::BTreeMap<String, crate::config::ApiRequest>,
    session: WorkspaceSession,
}

impl App {
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
        session.draft != RequestDraft::from(&source.for_configuration(configuration))
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
            session.draft = RequestDraft::from(&source.for_configuration(configuration));
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
                session.draft = RequestDraft::from(&source.for_configuration(&configuration));
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
            self.text().configuration_switched(configuration),
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
        self.view.notice = Some(Feedback::Info("正在重新加载配置…".to_string()));
        std::thread::spawn(move || {
            let loaded = match crate::config::reload(&path) {
                Ok(config) => config,
                Err(error) => {
                    let _ = sender.send(Err(format!("{error:#}")));
                    return;
                }
            };
            let (mut config, requests) = loaded.into_workspace();
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
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("配置加载任务异常结束".to_string())
            }
        };
        self.workspace_reload = None;
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(details) => {
                self.view.notice = Some(Feedback::Error(self.text().reload_failed(&details)));
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
        self.view = super::ViewState::default();
        self.view.notice = Some(Feedback::Success(
            self.text().workspace_reloaded().to_string(),
        ));
        tracing::debug!("工作区配置重新加载完成");
        true
    }
}
