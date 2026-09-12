use std::collections::BTreeMap;

use crate::{
    config::{
        ApiRequest, DataPart, FileUpload, NameValue, RequestParam, WorkspaceConfig, value_to_string,
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
}

#[derive(Debug)]
pub(crate) struct RequestSession {
    pub(crate) source: ApiRequest,
    pub(crate) draft: RequestDraft,
    pub(super) runtime: RequestRuntimeState,
    pub(crate) dirty: bool,
}

impl RequestSession {
    pub(super) fn new(source: ApiRequest) -> Self {
        let draft = RequestDraft::from(&source);
        Self {
            source,
            draft,
            runtime: RequestRuntimeState::default(),
            dirty: false,
        }
    }

    pub(super) fn effective_request(&self, collection_headers: &[NameValue]) -> ApiRequest {
        let mut effective = self.source.clone();
        if let Some(url) = &self.draft.url {
            effective.url = url.clone();
        }
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
}

#[derive(Debug)]
pub(crate) struct WorkspaceSession {
    pub(crate) variables: BTreeMap<String, String>,
    pub(crate) requests: Vec<RequestSession>,
    pub(crate) selected_request: Option<usize>,
}

impl WorkspaceSession {
    pub(super) fn from_config(config: &WorkspaceConfig, requests: Vec<ApiRequest>) -> Self {
        let has_requests = !requests.is_empty();
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
            requests: requests.into_iter().map(RequestSession::new).collect(),
            selected_request: has_requests.then_some(0),
        }
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

#[derive(Debug, Clone)]
pub(crate) struct RequestDraft {
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
            url: None,
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
