use std::collections::BTreeMap;

use crate::{
    config::{
        ApiRequest, DataPart, FileUpload, NameValue, RequestOverride, RequestParam,
        WorkspaceConfig, WorkspaceConfiguration, value_to_string,
    },
    http::{HttpError, ResponseData},
    i18n::UiText,
};

use super::dialog::{HeaderRow, HeaderSource};

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
pub(super) struct RequestRuntimeState {
    status: RequestStatus,
    response: Option<ResponseData>,
    error: Option<String>,
    message: Option<String>,
    operation_id: Option<String>,
}

impl RequestRuntimeState {
    pub(super) fn status(&self) -> RequestStatus {
        self.status
    }

    pub(super) fn response(&self) -> Option<&ResponseData> {
        self.response.as_ref()
    }

    pub(super) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(super) fn active_operation_id(&self) -> Option<&str> {
        self.operation_id.as_deref()
    }

    pub(super) fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub(super) fn start(&mut self, operation_id: String, message: String) {
        self.status = RequestStatus::Sending;
        self.response = None;
        self.error = None;
        self.operation_id = Some(operation_id);
        self.message = Some(message);
    }

    pub(super) fn complete_success(
        &mut self,
        status: RequestStatus,
        response: ResponseData,
        message: String,
    ) {
        self.operation_id = None;
        self.status = status;
        self.response = Some(response);
        self.error = None;
        self.message = Some(message);
    }

    pub(super) fn complete_failure(
        &mut self,
        status: RequestStatus,
        error: String,
        message: String,
    ) {
        self.operation_id = None;
        self.status = status;
        self.response = None;
        self.error = Some(error);
        self.message = Some(message);
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug)]
pub(crate) struct RequestSession {
    pub(crate) source: ApiRequest,
    pub(crate) draft: RequestDraft,
    pub(super) runtime: RequestRuntimeState,
    pub(crate) dirty: bool,
}

impl RequestSession {
    pub(super) fn new(source: ApiRequest, configuration: &WorkspaceConfiguration) -> Self {
        let draft = RequestDraft::from(&source.for_configuration(configuration));
        Self {
            source,
            draft,
            runtime: RequestRuntimeState::default(),
            dirty: false,
        }
    }

    pub(super) fn effective_request(
        &self,
        configuration: &WorkspaceConfiguration,
        collection_headers: &[NameValue],
    ) -> ApiRequest {
        let mut effective = self.source.for_configuration(configuration);
        effective.method = self.draft.method.clone();
        if let Some(url) = &self.draft.url {
            effective.url = url.clone();
        }
        effective.timeout_seconds = self.draft.timeout_seconds;
        let headers = collection_headers
            .iter()
            .filter(|header| {
                !self
                    .draft
                    .headers
                    .iter()
                    .any(|row| row.name.eq_ignore_ascii_case(&header.name))
            })
            .cloned()
            .chain(
                self.draft
                    .headers
                    .iter()
                    .filter(|row| row.enabled && !row.name.trim().is_empty())
                    .map(|row| NameValue {
                        name: row.name.trim().to_string(),
                        value: row.value.clone(),
                    }),
            )
            .collect();
        effective.headers = headers;
        effective.query_parts = self.draft.query_parts.clone();
        effective.form = self.draft.form.clone();
        effective.files = self.draft.files.clone();
        effective.body_parts = self.draft.body_parts.clone();
        effective
    }

    pub(super) fn activate_configuration(&mut self, configuration: &WorkspaceConfiguration) {
        self.draft = RequestDraft::from(&self.source.for_configuration(configuration));
        self.runtime.reset();
    }

    pub(super) fn commit_draft(&mut self, configuration: &mut WorkspaceConfiguration) -> bool {
        if configuration.path.is_none() {
            return self.commit_default_draft();
        }

        let mut base = self.source.clone();
        if let Some(timeout_seconds) = configuration.timeout_seconds {
            base.timeout_seconds = timeout_seconds;
        }
        let previous = configuration
            .request_overrides
            .get(&self.source.id)
            .cloned();
        let existing = previous.clone().unwrap_or_default();
        let next = RequestOverride {
            method: (self.draft.method != base.method).then(|| self.draft.method.clone()),
            url: self
                .draft
                .url
                .as_deref()
                .filter(|url| !url.trim().is_empty() && *url != base.url)
                .map(str::to_string),
            timeout_seconds: (self.draft.timeout_seconds != base.timeout_seconds)
                .then_some(self.draft.timeout_seconds),
            headers: (draft_headers(&self.draft) != base.headers)
                .then(|| draft_headers(&self.draft)),
            query_parts: (self.draft.query_parts != base.query_parts)
                .then(|| self.draft.query_parts.clone()),
            body_parts: (self.draft.body_parts != base.body_parts)
                .then(|| self.draft.body_parts.clone()),
            form: (self.draft.form != base.form).then(|| self.draft.form.clone()),
            files: (self.draft.files != base.files).then(|| self.draft.files.clone()),
            extracts: existing.extracts,
        };
        let next = (!next.is_empty()).then_some(next);
        if previous == next {
            return false;
        }
        if let Some(request_override) = next {
            configuration
                .request_overrides
                .insert(self.source.id.clone(), request_override);
        } else {
            configuration.request_overrides.remove(&self.source.id);
        }
        self.dirty = true;
        true
    }

    fn commit_default_draft(&mut self) -> bool {
        let before = self.source.clone();
        self.source.method = self.draft.method.clone();
        self.source.url = self
            .draft
            .url
            .clone()
            .unwrap_or_else(|| self.source.url.clone());
        self.source.timeout_seconds = self.draft.timeout_seconds;
        self.source.headers = draft_headers(&self.draft);
        self.source.query_parts = self.draft.query_parts.clone();
        self.source.body_parts = self.draft.body_parts.clone();
        self.source.form = self.draft.form.clone();
        self.source.files = self.draft.files.clone();
        if before == self.source {
            return false;
        }
        self.dirty = true;
        true
    }
}

#[derive(Debug)]
pub(crate) struct WorkspaceSession {
    pub(crate) active_configuration: String,
    pub(crate) variables: BTreeMap<String, String>,
    pub(crate) requests: Vec<RequestSession>,
    pub(crate) selected_request: Option<usize>,
    configuration_variables: BTreeMap<String, BTreeMap<String, String>>,
}

impl WorkspaceSession {
    pub(super) fn from_config(config: &WorkspaceConfig, requests: Vec<ApiRequest>) -> Self {
        let has_requests = !requests.is_empty();
        let active_configuration = config.default_configuration.clone();
        let configuration_variables = config
            .configurations
            .keys()
            .map(|configuration| {
                (
                    configuration.clone(),
                    initial_variables(config, configuration),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let variables = configuration_variables
            .get(&active_configuration)
            .cloned()
            .unwrap_or_default();
        let configuration = config
            .configurations
            .get(&active_configuration)
            .expect("默认配置应已在加载时规范化");
        Self {
            active_configuration: active_configuration.clone(),
            variables,
            requests: requests
                .into_iter()
                .map(|request| RequestSession::new(request, configuration))
                .collect(),
            selected_request: has_requests.then_some(0),
            configuration_variables,
        }
    }

    pub(super) fn switch_configuration(
        &mut self,
        config: &mut WorkspaceConfig,
        configuration: &str,
    ) -> bool {
        if configuration == self.active_configuration
            || !config.configurations.contains_key(configuration)
        {
            return false;
        }
        self.commit_configuration(config);
        let configuration_config = config
            .configurations
            .get(configuration)
            .expect("已验证配置存在")
            .clone();
        for session in &mut self.requests {
            session.activate_configuration(&configuration_config);
        }
        self.active_configuration = configuration.to_string();
        self.variables = self
            .configuration_variables
            .entry(self.active_configuration.clone())
            .or_insert_with(|| initial_variables(config, configuration))
            .clone();
        true
    }

    pub(super) fn commit_configuration(&mut self, config: &mut WorkspaceConfig) -> bool {
        let mut changed = false;
        let Some(configuration) = config.configurations.get_mut(&self.active_configuration) else {
            return false;
        };
        for session in &mut self.requests {
            changed |= session.commit_draft(configuration);
        }
        self.configuration_variables
            .insert(self.active_configuration.clone(), self.variables.clone());
        changed
    }

    pub(super) fn current_effective_request(&self, config: &WorkspaceConfig) -> Option<ApiRequest> {
        self.effective_request(config, self.current()?)
    }

    pub(super) fn effective_request(
        &self,
        config: &WorkspaceConfig,
        session: &RequestSession,
    ) -> Option<ApiRequest> {
        let configuration = config.configurations.get(&self.active_configuration)?;
        let headers = collection_headers(config, configuration);
        Some(session.effective_request(configuration, &headers))
    }

    pub(super) fn current(&self) -> Option<&RequestSession> {
        self.selected_request
            .and_then(|index| self.requests.get(index))
    }

    pub(super) fn current_mut(&mut self) -> Option<&mut RequestSession> {
        self.selected_request
            .and_then(|index| self.requests.get_mut(index))
    }

    pub(super) fn request(&self, request_id: &str) -> Option<&RequestSession> {
        self.requests
            .iter()
            .find(|session| session.source.id == request_id)
    }

    pub(super) fn request_mut(&mut self, request_id: &str) -> Option<&mut RequestSession> {
        self.requests
            .iter_mut()
            .find(|session| session.source.id == request_id)
    }
}

fn collection_headers(
    config: &WorkspaceConfig,
    configuration: &WorkspaceConfiguration,
) -> Vec<NameValue> {
    let mut headers = config.headers.clone();
    headers.retain(|existing| {
        !configuration
            .headers
            .iter()
            .any(|header| existing.name.eq_ignore_ascii_case(&header.name))
    });
    headers.extend(configuration.headers.iter().cloned());
    headers
}

fn initial_variables(config: &WorkspaceConfig, configuration: &str) -> BTreeMap<String, String> {
    let configuration_variables = config
        .configurations
        .get(configuration)
        .map(|config| &config.variables);
    config
        .editable_variables
        .iter()
        .map(|name| {
            let definition = configuration_variables
                .and_then(|variables| variables.get(name))
                .or_else(|| config.variables.get(name));
            let value = definition
                .and_then(|definition| definition.default.as_ref())
                .map(value_to_string)
                .unwrap_or_default();
            (name.clone(), value)
        })
        .collect()
}

fn draft_headers(draft: &RequestDraft) -> Vec<NameValue> {
    draft
        .headers
        .iter()
        .filter(|row| row.enabled && !row.name.trim().is_empty())
        .map(|row| NameValue {
            name: row.name.trim().to_string(),
            value: row.value.clone(),
        })
        .collect()
}

#[derive(Debug, Clone)]
pub(crate) struct RequestDraft {
    pub(crate) method: String,
    pub(crate) timeout_seconds: u64,
    pub(crate) url: Option<String>,
    pub(crate) headers: Vec<HeaderRow>,
    pub(crate) query_parts: Vec<DataPart>,
    pub(crate) form: Vec<RequestParam>,
    pub(crate) files: Vec<FileUpload>,
    pub(crate) body_parts: Vec<DataPart>,
}

impl From<&ApiRequest> for RequestDraft {
    fn from(request: &ApiRequest) -> Self {
        Self {
            method: request.method.clone(),
            timeout_seconds: request.timeout_seconds,
            url: Some(request.url.clone()),
            headers: request
                .headers
                .iter()
                .map(|header| HeaderRow {
                    name: header.name.clone(),
                    value: header.value.clone(),
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
