use std::collections::BTreeMap;

use crate::{
    config::{
        ApiRequest, DataPart, FileUpload, NameValue, RequestOverride, RequestParam,
        WorkspaceConfig, WorkspaceConfiguration, value_to_string,
    },
    http::{HttpError, ResponseData},
    i18n::UiText,
    response_document::ResponseDocument,
};

use super::Feedback;
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
}

#[derive(Debug, Default)]
pub(super) struct RequestRuntimeState {
    phase: RequestPhase,
    feedback: Option<Feedback>,
}

#[derive(Debug, Default)]
enum RequestPhase {
    #[default]
    Idle,
    Sending {
        operation_id: String,
    },
    Received {
        response: ResponseData,
        document: ResponseDocument,
    },
    Failed {
        status: RequestStatus,
        error: String,
    },
}

impl RequestRuntimeState {
    pub(super) fn status(&self) -> RequestStatus {
        match &self.phase {
            RequestPhase::Idle => RequestStatus::NotSent,
            RequestPhase::Sending { .. } => RequestStatus::Sending,
            RequestPhase::Received { response, .. } => {
                RequestStatus::from_http_status(response.status)
            }
            RequestPhase::Failed { status, .. } => *status,
        }
    }

    pub(super) fn response(&self) -> Option<&ResponseData> {
        match &self.phase {
            RequestPhase::Received { response, .. } => Some(response),
            _ => None,
        }
    }

    pub(super) fn document(&self) -> Option<&ResponseDocument> {
        match &self.phase {
            RequestPhase::Received { document, .. } => Some(document),
            _ => None,
        }
    }

    pub(super) fn error(&self) -> Option<&str> {
        match &self.phase {
            RequestPhase::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    pub(super) fn active_operation_id(&self) -> Option<&str> {
        match &self.phase {
            RequestPhase::Sending { operation_id } => Some(operation_id),
            _ => None,
        }
    }

    pub(super) fn feedback(&self) -> Option<&Feedback> {
        self.feedback.as_ref()
    }

    pub(super) fn start(&mut self, operation_id: String) {
        self.phase = RequestPhase::Sending { operation_id };
        self.feedback = None;
    }

    pub(super) fn receive_response(
        &mut self,
        response: ResponseData,
        document: ResponseDocument,
        feedback: Feedback,
    ) {
        self.phase = RequestPhase::Received { response, document };
        self.feedback = Some(feedback);
    }

    pub(super) fn complete_failure(
        &mut self,
        status: RequestStatus,
        error: String,
        feedback: Feedback,
    ) {
        self.phase = RequestPhase::Failed { status, error };
        self.feedback = Some(feedback);
    }

    pub(super) fn cancel(&mut self, feedback: Feedback) {
        self.phase = RequestPhase::Idle;
        self.feedback = Some(feedback);
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug)]
pub(crate) struct RequestSession {
    pub(crate) source: ApiRequest,
    pub(crate) draft: RequestDraft,
    pub(crate) temporary_variables: BTreeMap<String, String>,
    pub(super) runtime: RequestRuntimeState,
    inactive_headers: BTreeMap<String, Vec<HeaderRow>>,
    inactive_temporary_variables: BTreeMap<String, BTreeMap<String, String>>,
}

impl RequestSession {
    pub(crate) fn status(&self) -> RequestStatus {
        self.runtime.status()
    }

    pub(super) fn new(
        source: ApiRequest,
        configuration: &WorkspaceConfiguration,
        config: &WorkspaceConfig,
    ) -> Self {
        let effective = source.for_configuration(configuration);
        let temporary_variables = initial_temporary_variables(config, configuration, &effective);
        let draft = RequestDraft::from(&effective);
        Self {
            source,
            draft,
            temporary_variables,
            runtime: RequestRuntimeState::default(),
            inactive_headers: BTreeMap::new(),
            inactive_temporary_variables: BTreeMap::new(),
        }
    }

    pub(super) fn effective_request(
        &self,
        configuration: &WorkspaceConfiguration,
        config: &WorkspaceConfig,
    ) -> ApiRequest {
        let mut effective = self.source.for_configuration(configuration);
        effective.method = self.draft.method.clone();
        if let Some(url) = &self.draft.url {
            effective.url = url.clone();
        }
        effective.timeout_seconds = self.draft.timeout_seconds;
        let headers = self
            .draft
            .inherited_headers(config, configuration)
            .cloned()
            .chain(self.draft.enabled_headers())
            .collect();
        effective.headers = headers;
        effective.query_parts = self.draft.query_parts.clone();
        effective.form = self.draft.form.clone();
        effective.files = self.draft.files.clone();
        effective.body_parts = self.draft.body_parts.clone();
        effective
    }

    fn activate_configuration(
        &mut self,
        previous_name: &str,
        name: &str,
        configuration: &WorkspaceConfiguration,
        config: &WorkspaceConfig,
    ) {
        let effective = self.source.for_configuration(configuration);
        let mut draft = RequestDraft::from(&effective);
        if let Some(headers) = self.inactive_headers.remove(name) {
            draft.headers = headers;
        }
        let previous = std::mem::replace(&mut self.draft, draft);
        self.inactive_headers
            .insert(previous_name.to_string(), previous.headers);
        let next_temporary_variables = self
            .inactive_temporary_variables
            .remove(name)
            .unwrap_or_else(|| initial_temporary_variables(config, configuration, &effective));
        let previous_temporary_variables =
            std::mem::replace(&mut self.temporary_variables, next_temporary_variables);
        self.inactive_temporary_variables
            .insert(previous_name.to_string(), previous_temporary_variables);
        self.runtime.reset();
    }

    fn sync_temporary_variables(
        &mut self,
        configuration: &WorkspaceConfiguration,
        config: &WorkspaceConfig,
    ) {
        let request = self.effective_request(configuration, config);
        let initial = initial_temporary_variables(config, configuration, &request);
        self.temporary_variables
            .retain(|name, _| initial.contains_key(name));
        for (name, value) in initial {
            self.temporary_variables.entry(name).or_insert(value);
        }
    }

    pub(super) fn reset_temporary_variables(
        &mut self,
        config: &WorkspaceConfig,
        configuration: &WorkspaceConfiguration,
        request: &ApiRequest,
    ) {
        self.temporary_variables = initial_temporary_variables(config, configuration, request);
    }

    pub(super) fn has_inactive_header_changes(
        &self,
        baseline: &ApiRequest,
        config: &WorkspaceConfig,
    ) -> bool {
        self.inactive_headers.iter().any(|(name, headers)| {
            config.configurations.get(name).is_none_or(|configuration| {
                let overrides = configuration.request_overrides.get(&baseline.id);
                !headers_match(
                    headers,
                    overrides
                        .and_then(|value| value.headers.as_deref())
                        .unwrap_or(&baseline.headers),
                )
            })
        })
    }

    fn commit_draft(&mut self, configuration: &mut WorkspaceConfiguration) {
        if configuration.path.is_none() {
            return self.commit_default_draft();
        }

        let mut base = self.source.clone();
        if let Some(timeout_seconds) = configuration.timeout_seconds {
            base.timeout_seconds = timeout_seconds;
        }
        if let Some(skip_ssl_verification) = configuration.skip_ssl_verification {
            base.skip_ssl_verification = skip_ssl_verification;
        }
        let previous = configuration
            .request_overrides
            .get(&self.source.id)
            .cloned();
        let existing_extracts = previous
            .as_ref()
            .and_then(|request_override| request_override.extracts.clone());
        let existing_skip_ssl_verification = previous
            .as_ref()
            .and_then(|request_override| request_override.skip_ssl_verification);
        let headers: Vec<_> = self.draft.enabled_headers().collect();
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
            skip_ssl_verification: existing_skip_ssl_verification,
            headers: (headers != base.headers).then_some(headers),
            query_parts: (self.draft.query_parts != base.query_parts)
                .then(|| self.draft.query_parts.clone()),
            body_parts: (self.draft.body_parts != base.body_parts)
                .then(|| self.draft.body_parts.clone()),
            form: (self.draft.form != base.form).then(|| self.draft.form.clone()),
            files: (self.draft.files != base.files).then(|| self.draft.files.clone()),
            extracts: existing_extracts,
        };
        let next = (!next.is_empty()).then_some(next);
        if previous == next {
            return;
        }
        if let Some(request_override) = next {
            configuration
                .request_overrides
                .insert(self.source.id.clone(), request_override);
        } else {
            configuration.request_overrides.remove(&self.source.id);
        }
    }

    fn commit_default_draft(&mut self) {
        self.source.method = self.draft.method.clone();
        self.source.url = self
            .draft
            .url
            .clone()
            .unwrap_or_else(|| self.source.url.clone());
        self.source.timeout_seconds = self.draft.timeout_seconds;
        self.source.headers = self.draft.enabled_headers().collect();
        self.source.query_parts = self.draft.query_parts.clone();
        self.source.body_parts = self.draft.body_parts.clone();
        self.source.form = self.draft.form.clone();
        self.source.files = self.draft.files.clone();
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
            .expect("default configuration must be normalized during loading");
        Self {
            active_configuration: active_configuration.clone(),
            variables,
            requests: requests
                .into_iter()
                .map(|request| RequestSession::new(request, configuration, config))
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
        let target_configuration = config
            .configurations
            .get(configuration)
            .expect("configuration existence checked above")
            .clone();
        for session in &mut self.requests {
            session.activate_configuration(
                &self.active_configuration,
                configuration,
                &target_configuration,
                config,
            );
        }
        self.active_configuration = configuration.to_string();
        self.variables = self
            .configuration_variables
            .entry(self.active_configuration.clone())
            .or_insert_with(|| initial_variables(config, configuration))
            .clone();
        true
    }

    pub(super) fn commit_configuration(&mut self, config: &mut WorkspaceConfig) {
        let Some(configuration) = config.configurations.get_mut(&self.active_configuration) else {
            return;
        };
        for session in &mut self.requests {
            session.commit_draft(configuration);
        }
        self.configuration_variables
            .insert(self.active_configuration.clone(), self.variables.clone());
    }

    pub(super) fn current_effective_request(&self, config: &WorkspaceConfig) -> Option<ApiRequest> {
        self.effective_request(config, self.current()?)
    }

    pub(super) fn sync_current_temporary_variables(&mut self, config: &WorkspaceConfig) {
        let Some(configuration) = config.configurations.get(&self.active_configuration) else {
            return;
        };
        if let Some(session) = self.current_mut() {
            session.sync_temporary_variables(configuration, config);
        }
    }

    pub(super) fn effective_request(
        &self,
        config: &WorkspaceConfig,
        session: &RequestSession,
    ) -> Option<ApiRequest> {
        let configuration = config.configurations.get(&self.active_configuration)?;
        Some(session.effective_request(configuration, config))
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

fn initial_temporary_variables(
    config: &WorkspaceConfig,
    configuration: &WorkspaceConfiguration,
    request: &ApiRequest,
) -> BTreeMap<String, String> {
    crate::template::input_variable_names(request)
        .into_iter()
        .filter_map(|name| {
            let definition = configuration
                .variables
                .get(&name)
                .or_else(|| config.variables.get(&name))?;
            definition.temporary.then(|| {
                let value = definition
                    .default
                    .as_ref()
                    .map(value_to_string)
                    .unwrap_or_default();
                (name, value)
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl RequestDraft {
    pub(super) fn inherited_headers<'a>(
        &'a self,
        config: &'a WorkspaceConfig,
        configuration: &'a WorkspaceConfiguration,
    ) -> impl Iterator<Item = &'a NameValue> {
        config
            .headers
            .iter()
            .filter(|existing| {
                !configuration
                    .headers
                    .iter()
                    .any(|header| existing.name.eq_ignore_ascii_case(&header.name))
            })
            .chain(&configuration.headers)
            .filter(|header| {
                !self
                    .headers
                    .iter()
                    .any(|row| row.name.eq_ignore_ascii_case(&header.name))
            })
    }

    pub(super) fn matches_configuration(
        &self,
        source: &ApiRequest,
        configuration: &WorkspaceConfiguration,
    ) -> bool {
        let overrides = configuration.request_overrides.get(&source.id);
        self.method
            == *overrides
                .and_then(|value| value.method.as_ref())
                .unwrap_or(&source.method)
            && self.url.as_ref()
                == Some(
                    overrides
                        .and_then(|value| value.url.as_ref())
                        .unwrap_or(&source.url),
                )
            && self.timeout_seconds
                == overrides
                    .and_then(|value| value.timeout_seconds)
                    .or(configuration.timeout_seconds)
                    .unwrap_or(source.timeout_seconds)
            && headers_match(
                &self.headers,
                overrides
                    .and_then(|value| value.headers.as_deref())
                    .unwrap_or(&source.headers),
            )
            && self.query_parts
                == *overrides
                    .and_then(|value| value.query_parts.as_ref())
                    .unwrap_or(&source.query_parts)
            && self.body_parts
                == *overrides
                    .and_then(|value| value.body_parts.as_ref())
                    .unwrap_or(&source.body_parts)
            && self.form
                == *overrides
                    .and_then(|value| value.form.as_ref())
                    .unwrap_or(&source.form)
            && self.files
                == *overrides
                    .and_then(|value| value.files.as_ref())
                    .unwrap_or(&source.files)
    }

    fn enabled_headers(&self) -> impl Iterator<Item = NameValue> + '_ {
        self.headers
            .iter()
            .filter(|row| row.enabled && !row.name.trim().is_empty())
            .map(|row| NameValue {
                name: row.name.trim().to_string(),
                value: row.value.clone(),
            })
    }
}

fn headers_match(rows: &[HeaderRow], headers: &[NameValue]) -> bool {
    rows.len() == headers.len()
        && rows.iter().zip(headers).all(|(row, header)| {
            row.enabled
                && row.source == HeaderSource::Request
                && row.name == header.name
                && row.value == header.value
        })
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
