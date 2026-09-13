use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
};

static NEXT_RESPONSE_ID: AtomicU64 = AtomicU64::new(1);

use crate::{
    http::{self, HttpClient, HttpError, RequestOptions, ResponseData},
    response_document::ResponseDocument,
    template::ResolvedRequest,
};

#[derive(Debug)]
pub struct RequestOperation {
    pub request_id: String,
    pub operation_id: String,
}

pub struct RequestExecutionOptions {
    pub timeout_seconds: u64,
    pub file_directory: PathBuf,
    pub skip_ssl_verification: bool,
    pub secret_values: Vec<String>,
    pub max_display_bytes: usize,
    pub max_response_bytes: usize,
}

#[derive(Debug)]
pub struct FinishedRequest {
    pub request_id: String,
    pub operation_id: String,
    pub outcome: RequestOutcome,
}

#[derive(Debug)]
pub enum RequestOutcome {
    Response {
        response: Box<ResponseData>,
        document: ResponseDocument,
        extracted_variables: Vec<(String, String)>,
        extraction_failure_count: usize,
    },
    Failed(HttpError),
}

#[derive(Debug)]
pub struct RequestExecutor {
    http_client: HttpClient,
    sender: Sender<FinishedRequest>,
    receiver: Receiver<FinishedRequest>,
    next_operation_sequence: u64,
    active_operations: BTreeMap<String, tokio::task::AbortHandle>,
    runtime: Option<tokio::runtime::Runtime>,
    capacity: Arc<tokio::sync::Semaphore>,
}

impl RequestExecutor {
    pub fn new(http_client: HttpClient) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(4)
            .enable_all()
            .build()?;
        Ok(Self {
            http_client,
            sender,
            receiver,
            next_operation_sequence: 1,
            active_operations: BTreeMap::new(),
            runtime: Some(runtime),
            capacity: Arc::new(tokio::sync::Semaphore::new(8)),
        })
    }

    pub fn prepare(&mut self, request_id: &str) -> RequestOperation {
        let sequence = self.next_operation_sequence;
        self.next_operation_sequence = sequence.wrapping_add(1);
        RequestOperation {
            request_id: request_id.to_string(),
            operation_id: format!("{request_id}-{sequence}"),
        }
    }

    pub fn start(
        &mut self,
        operation: RequestOperation,
        request: ResolvedRequest,
        options: RequestExecutionOptions,
    ) {
        let RequestOperation {
            request_id,
            operation_id,
        } = operation;
        let http_client = self.http_client.clone();
        let sender = self.sender.clone();
        let permit = match self.capacity.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                let _ = sender.send(FinishedRequest {
                    request_id,
                    operation_id,
                    outcome: RequestOutcome::Failed(HttpError::InvalidRequest(
                        "最多同时处理 8 个请求，请稍后重试".to_string(),
                    )),
                });
                return;
            }
        };
        let task_id = operation_id.clone();
        let task = self
            .runtime
            .as_ref()
            .expect("request runtime")
            .spawn(async move {
                tracing::debug!(operation_id = %operation_id, "HTTP 工作线程开始");
                let outcome = match http::send(
                    &http_client,
                    &request,
                    RequestOptions {
                        timeout_seconds: options.timeout_seconds,
                        file_directory: &options.file_directory,
                        skip_ssl_verification: options.skip_ssl_verification,
                        secret_values: &options.secret_values,
                        max_response_bytes: options.max_response_bytes,
                    },
                    &operation_id,
                )
                .await
                {
                    Ok(response) => {
                        let response_id = NEXT_RESPONSE_ID.fetch_add(1, Ordering::Relaxed);
                        let queued = std::time::Instant::now();
                        let prepared = tokio::task::spawn_blocking(move || {
                            let span = tracing::debug_span!(target: "postui::perf", "response_prepare", response_id);
                            let _entered = span.enter();
                            let started = std::time::Instant::now();
                            let queue_us = queued.elapsed().as_micros() as u64;
                            let _permit = permit;
                            let (extracted_variables, extraction_failure_count) =
                                extract_response_variables(&request, &response);
                            let extraction_us = started.elapsed().as_micros() as u64;
                            let document_started = std::time::Instant::now();
                            let document = ResponseDocument::new(
                                response.body_bytes.clone(),
                                &response.headers,
                                options.max_display_bytes,
                            );
                            tracing::debug!(target: "postui::perf", queue_us, extraction_us,
                                document_us = document_started.elapsed().as_micros() as u64,
                                network_ms = response.elapsed_ms as u64,
                                response_bytes = response.body_bytes.len(),
                                displayed_bytes = document.displayed_bytes(),
                                lines = document.line_count(), status = response.status,
                                "response_prepared");
                            RequestOutcome::Response {
                                response: Box::new(response),
                                document,
                                extracted_variables,
                                extraction_failure_count,
                            }
                        })
                        .await;
                        match prepared {
                            Ok(outcome) => outcome,
                            Err(_) => RequestOutcome::Failed(HttpError::InvalidRequest(
                                "响应处理任务异常结束".to_string(),
                            )),
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
        self.active_operations.insert(task_id, task.abort_handle());
    }

    pub fn try_recv(&mut self) -> Result<FinishedRequest, TryRecvError> {
        let result = self.receiver.try_recv()?;
        self.active_operations.remove(&result.operation_id);
        Ok(result)
    }

    pub fn cancel(&mut self, operation_id: &str) -> bool {
        let Some(token) = self.active_operations.remove(operation_id) else {
            return false;
        };
        token.abort();
        true
    }
}

impl Drop for RequestExecutor {
    fn drop(&mut self) {
        for task in self.active_operations.values() {
            task.abort();
        }
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
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
