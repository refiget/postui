use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread,
};

use crate::{
    http::{self, HttpClient, HttpError, ResponseData},
    response_document::ResponseDocument,
    template::ResolvedRequest,
};

#[derive(Debug)]
pub(crate) struct RequestOperation {
    pub(crate) request_id: String,
    pub(crate) operation_id: String,
}

#[derive(Debug)]
pub(crate) struct FinishedRequest {
    pub(crate) request_id: String,
    pub(crate) operation_id: String,
    pub(crate) outcome: RequestOutcome,
}

#[derive(Debug)]
pub(crate) enum RequestOutcome {
    Response {
        response: ResponseData,
        document: ResponseDocument,
        extracted_variables: Vec<(String, String)>,
        extraction_failure_count: usize,
    },
    Failed(HttpError),
}

#[derive(Debug)]
pub(crate) struct RequestExecutor {
    http_client: HttpClient,
    sender: Sender<FinishedRequest>,
    receiver: Receiver<FinishedRequest>,
    next_operation_sequence: AtomicU64,
}

impl RequestExecutor {
    pub(crate) fn new(http_client: HttpClient) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            http_client,
            sender,
            receiver,
            next_operation_sequence: AtomicU64::new(1),
        }
    }

    pub(crate) fn prepare(&self, request_id: &str) -> RequestOperation {
        let sequence = self.next_operation_sequence.fetch_add(1, Ordering::Relaxed);
        RequestOperation {
            request_id: request_id.to_string(),
            operation_id: format!("{request_id}-{sequence}"),
        }
    }

    pub(crate) fn start(
        &self,
        operation: RequestOperation,
        request: ResolvedRequest,
        timeout_seconds: u64,
        file_directory: PathBuf,
        max_display_bytes: usize,
    ) {
        let http_client = self.http_client.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let RequestOperation {
                request_id,
                operation_id,
            } = operation;
            tracing::debug!(operation_id = %operation_id, "HTTP 工作线程开始");
            let outcome = match http::send(
                &http_client,
                &request,
                timeout_seconds,
                &file_directory,
                &operation_id,
            ) {
                Ok(response) => {
                    let (extracted_variables, extraction_failure_count) =
                        extract_response_variables(&request, &response);
                    let document = ResponseDocument::new(
                        response.body_bytes.clone(),
                        &response.headers,
                        max_display_bytes,
                    );
                    RequestOutcome::Response {
                        response,
                        document,
                        extracted_variables,
                        extraction_failure_count,
                    }
                }
                Err(error) => RequestOutcome::Failed(error),
            };
            if sender
                .send(FinishedRequest {
                    request_id,
                    operation_id,
                    outcome,
                })
                .is_err()
            {
                tracing::debug!("TUI 已退出，丢弃后台请求结果");
            }
        });
    }

    pub(crate) fn try_recv(&self) -> Result<FinishedRequest, TryRecvError> {
        self.receiver.try_recv()
    }
}

fn extract_response_variables(
    request: &ResolvedRequest,
    response: &ResponseData,
) -> (Vec<(String, String)>, usize) {
    if response.status >= 400 || request.extracts.is_empty() {
        return (Vec::new(), 0);
    }

    let root = match serde_json::from_slice::<serde_json::Value>(&response.body_bytes) {
        Ok(root) => root,
        Err(error) => {
            tracing::debug!(error = %error, "响应不是有效 JSON，无法提取字段");
            return (Vec::new(), request.extracts.len());
        }
    };
    let mut values = Vec::new();
    let mut failure_count = 0;
    for extract in &request.extracts {
        match crate::template::extract_json_value(&root, &extract.path) {
            Ok(value) => {
                tracing::debug!(
                    variable = %extract.variable,
                    path = %extract.path,
                    "响应字段已在 HTTP 工作线程提取"
                );
                values.push((extract.variable.clone(), value));
            }
            Err(error) => {
                failure_count += 1;
                tracing::debug!(
                    variable = %extract.variable,
                    path = %extract.path,
                    error = %error,
                    "响应字段提取失败"
                );
            }
        }
    }
    (values, failure_count)
}
