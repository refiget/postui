use super::Feedback;
use super::{App, Focus, ResponseMenuAction, ViewMode};
use crate::response_action::FinishedResponseAction;
use crate::{http::ResponseData, response_document::ResponseDocument};

impl App {
    pub(crate) fn poll_response_actions(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.response_actions.try_recv() {
            changed = true;
            self.response_action_running = false;
            match result {
                FinishedResponseAction::Copied(Ok(())) => {
                    self.view.notice =
                        Some(Feedback::Success(self.text().response_copied().to_string()));
                }
                FinishedResponseAction::Copied(Err(error)) => {
                    tracing::error!(error = %error, "复制响应失败");
                    self.view.notice =
                        Some(Feedback::Error(self.text().response_copy_failed(&error)));
                }
                FinishedResponseAction::Downloaded(Ok(path)) => {
                    self.view.notice = Some(Feedback::Success(
                        self.text().response_downloaded(&path.display().to_string()),
                    ));
                }
                FinishedResponseAction::Downloaded(Err(error)) => {
                    tracing::error!(error = %error, "下载响应失败");
                    self.view.notice = Some(Feedback::Error(
                        self.text().response_download_failed(&error),
                    ));
                }
            }
        }
        changed
    }

    pub(crate) fn current_response(&self) -> Option<&ResponseData> {
        self.workspace_state
            .current()
            .and_then(|session| session.runtime.response())
    }

    pub(crate) fn current_response_document(&self) -> Option<&ResponseDocument> {
        self.workspace_state
            .current()
            .and_then(|session| session.runtime.document())
    }

    pub(crate) fn current_error(&self) -> Option<&str> {
        self.workspace_state
            .current()
            .and_then(|session| session.runtime.error())
    }

    pub(crate) fn scroll_response(&mut self, direction: isize) {
        let Some(max_offset) = self.current_response_document().map(|document| {
            1usize
                .saturating_add(document.line_count())
                .saturating_add(usize::from(document.limited()))
                .saturating_sub(1)
        }) else {
            return;
        };
        if self.view.response.scroll.move_by(direction, max_offset) {
            tracing::trace!(
                offset = self.view.response.scroll.offset(),
                direction,
                "滚动响应内容"
            );
        }
    }

    pub(crate) fn open_response_menu(&mut self) {
        self.cancel_active_editors();
        if self.editing_preview_tab().is_some() {
            self.view.dialog = None;
        }
        self.view.response.menu_selection = Some(0);
        self.view.focus = Focus::ResponseActions;
    }

    pub(crate) fn close_response_menu(&mut self) {
        self.view.response.menu_selection = None;
    }

    pub(crate) fn move_response_menu_selection(&mut self, direction: isize) {
        let action_count = ResponseMenuAction::all().len();
        if let Some(selected) = self.view.response.menu_selection.as_mut() {
            *selected = (*selected as isize + direction).rem_euclid(action_count as isize) as usize;
        }
    }

    pub(crate) fn choose_response_action(&mut self, index: usize) {
        self.view.response.menu_selection = Some(index);
        self.activate_selected_response_action();
    }

    pub(crate) fn activate_selected_response_action(&mut self) {
        let action = self
            .view
            .response
            .menu_selection
            .and_then(ResponseMenuAction::from_index);
        self.close_response_menu();
        if let Some(action) = action {
            self.activate_response_action(action);
        }
    }

    fn activate_response_action(&mut self, action: ResponseMenuAction) {
        match action {
            ResponseMenuAction::Download => self.download_current_response(),
            ResponseMenuAction::Copy => self.copy_current_response(),
        }
    }

    pub(crate) fn response_zoomed(&self) -> bool {
        matches!(self.view.mode, ViewMode::ResponseZoom { .. })
    }

    pub(crate) fn toggle_response_zoom(&mut self) {
        if self.response_zoomed() {
            self.restore_standard_view();
        } else {
            self.view.mode = ViewMode::ResponseZoom {
                return_focus: self.view.focus,
            };
            self.view.focus = Focus::ResponseZoom;
        }
    }

    pub(super) fn restore_standard_view(&mut self) {
        let ViewMode::ResponseZoom { return_focus } = self.view.mode else {
            return;
        };
        self.view.mode = ViewMode::Standard;
        self.view.focus = return_focus;
        self.close_response_menu();
    }

    fn copy_current_response(&mut self) {
        if self.response_action_running {
            return;
        }
        let Some(body) = self
            .current_response()
            .map(|response| response.body_bytes.clone())
        else {
            self.view.notice = Some(Feedback::Warning(
                self.text().response_action_no_response().to_string(),
            ));
            return;
        };
        self.response_action_running = true;
        self.response_actions.copy(body);
    }

    fn download_current_response(&mut self) {
        if self.response_action_running {
            return;
        }
        let Some((body, headers)) = self
            .current_response()
            .map(|response| (response.body_bytes.clone(), response.headers.clone()))
        else {
            self.view.notice = Some(Feedback::Warning(
                self.text().response_action_no_response().to_string(),
            ));
            return;
        };
        let Some(request_id) = self.current_request().map(|request| request.id.clone()) else {
            self.view.notice = Some(Feedback::Warning(
                self.text().response_action_no_response().to_string(),
            ));
            return;
        };
        self.response_action_running = true;
        self.response_actions.download(
            body,
            headers,
            request_id,
            self.config.download_directory.clone(),
        );
    }
}
