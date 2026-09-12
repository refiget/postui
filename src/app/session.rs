use std::collections::BTreeMap;

use crate::{
    config::{
        ApiRequest, DataPart, FileUpload, NameValue, RequestOverride, RequestParam,
        WorkspaceConfig, value_to_string,
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
    environment: String,
}

impl RequestSession {
    pub(super) fn new(source: ApiRequest, environment: &str) -> Self {
        let draft = RequestDraft::from(&source.for_environment(environment));
        Self {
            source,
            draft,
            runtime: RequestRuntimeState::default(),
            dirty: false,
            environment: environment.to_string(),
        }
    }

    pub(super) fn effective_request(&self, collection_headers: &[NameValue]) -> ApiRequest {
        let mut effective = self.source.for_environment(&self.environment);
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

    pub(super) fn activate_environment(&mut self, environment: &str) {
        self.environment = environment.to_string();
        self.draft = RequestDraft::from(&self.source.for_environment(environment));
        self.runtime.reset();
    }

    pub(super) fn commit_draft(&mut self) -> bool {
        let previous = self.source.overrides.get(&self.environment).cloned();
        let existing = previous.clone().unwrap_or_default();
        let next = RequestOverride {
            method: (self.draft.method != self.source.method).then(|| self.draft.method.clone()),
            url: self
                .draft
                .url
                .as_deref()
                .filter(|url| !url.trim().is_empty() && *url != self.source.url)
                .map(str::to_string),
            timeout_seconds: (self.draft.timeout_seconds != self.source.timeout_seconds)
                .then_some(self.draft.timeout_seconds),
            headers: (draft_headers(&self.draft) != self.source.headers)
                .then(|| draft_headers(&self.draft)),
            query_parts: (self.draft.query_parts != self.source.query_parts)
                .then(|| self.draft.query_parts.clone()),
            body_parts: (self.draft.body_parts != self.source.body_parts)
                .then(|| self.draft.body_parts.clone()),
            form: (self.draft.form != self.source.form).then(|| self.draft.form.clone()),
            files: (self.draft.files != self.source.files).then(|| self.draft.files.clone()),
            extracts: existing.extracts,
        };
        let next = (!next.is_empty()).then_some(next);
        if previous == next {
            return false;
        }
        if let Some(request_override) = next {
            self.source
                .overrides
                .insert(self.environment.clone(), request_override);
        } else {
            self.source.overrides.remove(&self.environment);
        }
        self.dirty = true;
        true
    }
}

#[derive(Debug)]
pub(crate) struct WorkspaceSession {
    pub(crate) active_environment: String,
    pub(crate) variables: BTreeMap<String, String>,
    pub(crate) requests: Vec<RequestSession>,
    pub(crate) selected_request: Option<usize>,
    environment_variables: BTreeMap<String, BTreeMap<String, String>>,
}

impl WorkspaceSession {
    pub(super) fn from_config(config: &WorkspaceConfig, requests: Vec<ApiRequest>) -> Self {
        let has_requests = !requests.is_empty();
        let active_environment = config.default_environment.clone();
        let environment_variables = config
            .environments
            .keys()
            .map(|environment| (environment.clone(), initial_variables(config, environment)))
            .collect::<BTreeMap<_, _>>();
        let variables = environment_variables
            .get(&active_environment)
            .cloned()
            .unwrap_or_default();
        Self {
            active_environment: active_environment.clone(),
            variables,
            requests: requests
                .into_iter()
                .map(|request| RequestSession::new(request, &active_environment))
                .collect(),
            selected_request: has_requests.then_some(0),
            environment_variables,
        }
    }

    pub(super) fn switch_environment(
        &mut self,
        config: &WorkspaceConfig,
        environment: &str,
    ) -> bool {
        if environment == self.active_environment || !config.environments.contains_key(environment)
        {
            return false;
        }
        self.commit_environment();
        for session in &mut self.requests {
            session.activate_environment(environment);
        }
        self.active_environment = environment.to_string();
        self.variables = self
            .environment_variables
            .entry(self.active_environment.clone())
            .or_insert_with(|| initial_variables(config, environment))
            .clone();
        true
    }

    pub(super) fn commit_environment(&mut self) -> bool {
        let mut changed = false;
        for session in &mut self.requests {
            changed |= session.commit_draft();
        }
        self.environment_variables
            .insert(self.active_environment.clone(), self.variables.clone());
        changed
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

fn initial_variables(config: &WorkspaceConfig, environment: &str) -> BTreeMap<String, String> {
    let environment_variables = config
        .environments
        .get(environment)
        .map(|config| &config.variables);
    config
        .editable_variables
        .iter()
        .map(|name| {
            let definition = environment_variables
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
