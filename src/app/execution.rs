use super::Feedback;
use super::{App, RequestStatus, supports_method};
use crate::request_executor::{FinishedRequest, RequestOutcome};

impl App {
    pub(crate) fn poll_request_results(&mut self) -> bool {
        let mut changed = false;
        while let Ok(FinishedRequest {
            request_id,
            operation_id,
            outcome,
        }) = self.request_executor.try_recv()
        {
            tracing::debug!(
                request_id = %request_id,
                operation_id = %operation_id,
                "收到后台请求结果"
            );
            let is_current = self
                .current_request()
                .is_some_and(|request| request.id == request_id);
            let text = self.text();
            let Some(active_operation_id) = self
                .workspace_state
                .request(&request_id)
                .and_then(|session| session.runtime.active_operation_id())
            else {
                tracing::debug!(
                    request_id = %request_id,
                    operation_id = %operation_id,
                    "收到未知接口的后台请求结果"
                );
                continue;
            };
            if active_operation_id != operation_id {
                tracing::debug!(
                    request_id = %request_id,
                    operation_id = %operation_id,
                    active_operation_id = ?active_operation_id,
                    "忽略过期的后台请求结果"
                );
                continue;
            }
            changed = true;
            match outcome {
                RequestOutcome::Response {
                    response,
                    document,
                    extracted_variables,
                    extraction_failure_count,
                } => {
                    let status = response.status;
                    let elapsed_ms = response.elapsed_ms;
                    let request_status = RequestStatus::from_http_status(status);
                    tracing::debug!(
                        request_id = %request_id,
                        operation_id = %operation_id,
                        status,
                        request_status = ?request_status,
                        elapsed_ms,
                        header_count = response.headers.len(),
                        body_bytes = response.body_bytes.len(),
                        "HTTP 响应已接收"
                    );
                    for (variable, value) in extracted_variables {
                        self.workspace_state.variables.insert(variable, value);
                    }
                    let complete = text.request_complete(status, elapsed_ms);
                    let message = if extraction_failure_count == 0 {
                        complete
                    } else {
                        format!(
                            "{complete} · {}",
                            text.response_extract_failures(extraction_failure_count)
                        )
                    };
                    let feedback = if status >= 400 {
                        Feedback::Error(message)
                    } else if extraction_failure_count > 0 {
                        Feedback::Warning(message)
                    } else {
                        Feedback::Success(message)
                    };
                    let Some(session) = self.workspace_state.request_mut(&request_id) else {
                        continue;
                    };
                    session
                        .runtime
                        .receive_response(request_status, response, document, feedback);
                }
                RequestOutcome::Failed(error) => {
                    let request_status = RequestStatus::from_error(&error);
                    let feedback = Feedback::Error(text.request_error(&error).to_string());
                    let error_message = format!("{:#}", anyhow::Error::new(error));
                    tracing::error!(
                        request_id = %request_id,
                        operation_id = %operation_id,
                        request_status = ?request_status,
                        error = %error_message,
                        "后台请求失败"
                    );
                    let error_details = format!("{}\n\n{error_message}", feedback.message());
                    let Some(session) = self.workspace_state.request_mut(&request_id) else {
                        continue;
                    };
                    session
                        .runtime
                        .complete_failure(request_status, error_details, feedback);
                }
            };
            if is_current {
                self.view.response.scroll.reset();
            }
        }
        changed
    }

    pub(crate) fn send_current_request(&mut self) {
        let Some(request_id) = self.current_request().map(|request| request.id.clone()) else {
            self.view.notice = Some(Feedback::Warning(
                self.text().request_selection_required().to_string(),
            ));
            return;
        };
        tracing::debug!(request_id = %request_id, "触发发送当前请求");
        self.cancel_active_editors();
        let Some(effective_request) = self.current_effective_request() else {
            self.view.notice = Some(Feedback::Warning(
                self.text().request_url_required().to_string(),
            ));
            return;
        };
        if effective_request.url.trim().is_empty() {
            self.view.notice = Some(Feedback::Warning(
                self.text().request_url_required().to_string(),
            ));
            return;
        }
        let timeout = effective_request.timeout_seconds;
        let effective_method = effective_request.method;
        if !supports_method(&effective_method) {
            self.view.notice = Some(Feedback::Warning(
                self.text().unsupported_method(&effective_method),
            ));
            tracing::debug!(
                method = %effective_method,
                "忽略不支持的 HTTP 方法"
            );
            return;
        }
        if self.request_status(&request_id) == RequestStatus::Sending {
            tracing::debug!("已有请求执行中，忽略重复发送");
            self.view.notice = Some(Feedback::Warning(
                self.text().request_in_progress().to_string(),
            ));
            return;
        }

        let Some(resolved) = self.current_resolved_request() else {
            self.view.notice = Some(Feedback::Warning(
                self.text().request_url_required().to_string(),
            ));
            return;
        };
        let operation = self.request_executor.prepare(&request_id);
        let operation_id = operation.operation_id.clone();
        let file_directory = self.config.file_directory.clone();
        tracing::debug!(
            request_id = %request_id,
            operation_id = %operation_id,
            method = %resolved.method,
            timeout_seconds = timeout,
            file_directory = %file_directory.display(),
            "开始异步发送请求"
        );
        let feedback = Feedback::Info(self.text().request_started(&resolved.method, &resolved.url));
        if let Some(session) = self.workspace_state.request_mut(&request_id) {
            session.runtime.start(operation_id, feedback);
        } else {
            self.view.notice = Some(Feedback::Warning(
                self.text().request_url_required().to_string(),
            ));
            return;
        }
        self.view.notice = None;
        self.request_executor.start(
            operation,
            resolved,
            timeout,
            file_directory,
            self.global_config.max_response_display_bytes,
        );
    }
}
