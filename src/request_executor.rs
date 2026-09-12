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
pub(crate) struct RequestResult {
    pub(crate) request_id: String,
    pub(crate) operation_id: String,
    pub(crate) result: Result<CompletedResponse, HttpError>,
    pub(crate) extracted_variables: Vec<(String, String)>,
    pub(crate) extract_failures: usize,
}

#[derive(Debug)]
pub(crate) struct CompletedResponse {
    pub(crate) response: ResponseData,
    pub(crate) document: ResponseDocument,
}

#[derive(Debug)]
pub(crate) struct RequestExecutor {
    http_client: HttpClient,
    sender: Sender<RequestResult>,
    receiver: Receiver<RequestResult>,
    next_operation: AtomicU64,
}

impl RequestExecutor {
    pub(crate) fn new(http_client: HttpClient) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            http_client,
            sender,
            receiver,
            next_operation: AtomicU64::new(1),
        }
    }

    pub(crate) fn prepare(&self, request_id: &str) -> RequestOperation {
        let sequence = self.next_operation.fetch_add(1, Ordering::Relaxed);
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
            let result = http::send(
                &http_client,
                &request,
                timeout_seconds,
                &file_directory,
                &operation_id,
            );
            let (result, extracted_variables, extract_failures) = match result {
                Ok(response) => {
                    let (extracted_variables, extract_failures) =
                        extract_response_variables(&request, &response);
                    let document = ResponseDocument::new(
                        response.body_bytes.clone(),
                        &response.headers,
                        max_display_bytes,
                    );
                    (
                        Ok(CompletedResponse { response, document }),
                        extracted_variables,
                        extract_failures,
                    )
                }
                Err(error) => (Err(error), Vec::new(), 0),
            };
            match &result {
                Ok(completed) => tracing::debug!(
                    operation_id = %operation_id,
                    status = completed.response.status,
                    elapsed_ms = completed.response.elapsed_ms,
                    body_bytes = completed.response.body_bytes.len(),
                    "HTTP 工作线程完成"
                ),
                Err(error) => tracing::error!(
                    operation_id = %operation_id,
                    error = %error,
                    "HTTP 工作线程失败"
                ),
            }
            if sender
                .send(RequestResult {
                    request_id,
                    operation_id,
                    result,
                    extracted_variables,
                    extract_failures,
                })
                .is_err()
            {
                tracing::debug!("TUI 已退出，丢弃后台请求结果");
            }
        });
    }

    pub(crate) fn try_recv(&self) -> Result<RequestResult, TryRecvError> {
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
    let mut failures = 0;
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
                failures += 1;
                tracing::debug!(
                    variable = %extract.variable,
                    path = %extract.path,
                    error = %error,
                    "响应字段提取失败"
                );
            }
        }
    }
    (values, failures)
}
