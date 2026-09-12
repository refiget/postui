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
    pub(crate) result: Result<ResponseData, HttpError>,
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
            match &result {
                Ok(response) => tracing::debug!(
                    operation_id = %operation_id,
                    status = response.status,
                    elapsed_ms = response.elapsed_ms,
                    body_bytes = response.body_bytes.len(),
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
