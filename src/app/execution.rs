use super::Feedback;
use super::{App, RequestStatus, http_method};
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
                RequestOutcome::Response { response, document } => {
                    self.view.response.selection = None;
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
                    let feedback = if status >= 400 {
                        Feedback::Error(text.request_status_failed().to_string())
                    } else {
                        Feedback::Success(text.request_status_success().to_string())
                    };
                    let Some(session) = self.workspace_state.request_mut(&request_id) else {
                        continue;
                    };
                    session
                        .runtime
                        .receive_response(*response, document, feedback);
                }
                RequestOutcome::Failed(error) => {
                    let request_status = RequestStatus::from_error(&error);
                    let feedback = Feedback::Error(request_status.label(text).to_string());
                    let error_detail = text.request_error(&error);
                    let error_message = format!("{:#}", anyhow::Error::new(error));
                    tracing::error!(
                        request_id = %request_id,
                        operation_id = %operation_id,
                        request_status = ?request_status,
                        error = %error_message,
                        "后台请求失败"
                    );
                    let error_details = format!("{error_detail}\n\n{error_message}");
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
                self.view.response.search_match_line = None;
            }
        }
        changed
    }

    pub(crate) fn send_current_request(&mut self) {
        if self.workspace_reload.is_some() {
            return;
        }
        let Some(request_id) = self.current_request().map(|request| request.id.clone()) else {
            self.view.notice = Some(Feedback::Warning(
                self.text().request_selection_required().to_string(),
            ));
            return;
        };
        tracing::debug!(request_id = %request_id, "触发发送当前请求");
        if self.request_status(&request_id) == RequestStatus::Sending {
            let operation_id = self
                .workspace_state
                .request(&request_id)
                .and_then(|session| {
                    session
                        .runtime
                        .active_operation_id()
                        .map(|operation_id| operation_id.to_string())
                });
            if let Some(operation_id) = operation_id {
                self.request_executor.cancel(&operation_id);
            }
            let feedback = Feedback::Warning(self.text().request_cancelled().to_string());
            if let Some(session) = self.workspace_state.request_mut(&request_id) {
                session.runtime.cancel(feedback.clone());
            }
            self.view.notice = Some(feedback);
            tracing::debug!(request_id = %request_id, "取消当前请求");
            return;
        }
        self.view.cancel_active_editors();
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
        if http_method::parse(&effective_request.method).is_err() {
            self.view.notice = Some(Feedback::Warning(
                self.text().invalid_method(&effective_request.method),
            ));
            tracing::debug!(
                method = %effective_request.method,
                "HTTP 方法无效"
            );
            return;
        }
        let resolved =
            crate::template::resolve_request(&effective_request, self.current_request_variables());
        let operation = self.request_executor.prepare(&request_id);
        let operation_id = operation.operation_id.clone();
        let file_directory = self.config.file_directory.clone();
        let skip_ssl_verification = effective_request.skip_ssl_verification;
        let secret_values = self.secret_variable_values();
        tracing::debug!(
            request_id = %request_id,
            operation_id = %operation_id,
            method = %resolved.method,
            timeout_seconds = timeout,
            file_directory = %file_directory.display(),
            skip_ssl_verification,
            "开始异步发送请求"
        );
        if let Some(session) = self.workspace_state.request_mut(&request_id) {
            session.runtime.start(operation_id);
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
            crate::request_executor::RequestExecutionOptions {
                timeout_seconds: timeout,
                file_directory,
                skip_ssl_verification,
                secret_values,
                max_display_bytes: self.global_config.max_response_display_bytes,
                max_response_bytes: self.global_config.max_response_bytes,
            },
        );
    }
}
