use super::Feedback;
use super::{App, Focus, ResponseMenuAction, ResponseTab, ViewMode};
use crate::editor::{EditAction, EditInput};
use crate::response_action::FinishedResponseAction;
use crate::{http::ResponseData, response_document::ResponseDocument};
use crossterm::event::KeyEvent;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

pub(super) struct ResponseSearchTask {
    request: ResponseSearchRequest,
    cancelled: Arc<AtomicBool>,
    receiver: mpsc::Receiver<Option<usize>>,
    pending: Option<ResponseSearchRequest>,
}

struct ResponseSearchRequest {
    document: ResponseDocument,
    query: String,
    start: usize,
    reverse: bool,
}

impl ResponseSearchTask {
    fn start(request: ResponseSearchRequest) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        let document = request.document.clone();
        let query = request.query.clone();
        let token = cancelled.clone();
        let start = request.start;
        let reverse = request.reverse;
        std::thread::spawn(move || {
            let found = document.find_line(&query, start, reverse, &token);
            let _ = sender.send(found);
        });
        Self {
            request,
            cancelled,
            receiver,
            pending: None,
        }
    }
}

impl Drop for ResponseSearchTask {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl App {
    pub(crate) fn open_response_search(&mut self) {
        if self.view.response.active_tab == ResponseTab::Headers
            || self.current_response().is_none_or(ResponseData::is_binary)
            || self.current_response_document().is_none()
        {
            return;
        }
        self.view.response.search = Some(EditInput::new(self.view.response.search_query.clone()));
    }

    pub(crate) fn handle_response_search_key(&mut self, key: KeyEvent) {
        let action = match self.view.response.search.as_mut() {
            Some(search) => search.handle_key(key),
            None => return,
        };
        match action {
            EditAction::Cancel => self.view.response.search = None,
            EditAction::Confirm => {
                if let Some(search) = self.view.response.search.take() {
                    self.view.response.search_query = search.confirmed_value();
                    self.view.response.search_match_line = None;
                    self.find_response_match(false);
                }
            }
            EditAction::Continue => {}
        }
    }

    pub(crate) fn find_response_match(&mut self, reverse: bool) {
        if self.view.response.active_tab == ResponseTab::Headers
            || self.current_response().is_none_or(ResponseData::is_binary)
        {
            return;
        }
        let query = self.view.response.search_query.trim();
        if query.is_empty() {
            return;
        }
        let start = self.view.response.search_match_line.map_or_else(
            || if reverse { usize::MAX } else { 0 },
            |line| {
                if reverse {
                    line.checked_sub(1).unwrap_or(usize::MAX)
                } else {
                    line.saturating_add(1)
                }
            },
        );
        let Some(document) = self.current_response_document().cloned() else {
            return;
        };
        let request = ResponseSearchRequest {
            document,
            query: query.to_string(),
            start,
            reverse,
        };
        if let Some(task) = self.response_search_task.as_mut() {
            task.cancelled.store(true, Ordering::Relaxed);
            task.pending = Some(request);
        } else {
            self.response_search_task = Some(ResponseSearchTask::start(request));
        }
    }

    pub(crate) fn poll_response_search(&mut self) -> bool {
        let Some(task) = self.response_search_task.as_ref() else {
            return false;
        };
        let request = task.pending.as_ref().unwrap_or(&task.request);
        let valid = self.view.response.active_tab != ResponseTab::Headers
            && self
                .current_response_document()
                .is_some_and(|document| document.same_document(&request.document))
            && self.view.response.search_query.trim() == request.query;
        let task = self
            .response_search_task
            .as_mut()
            .expect("active response search");
        if !valid {
            task.cancelled.store(true, Ordering::Relaxed);
            task.pending = None;
        }
        let found = match task.receiver.try_recv() {
            Ok(found) => found,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.response_search_task = None;
                return false;
            }
        };
        let mut task = self
            .response_search_task
            .take()
            .expect("active response search");
        if valid {
            if let Some(request) = task.pending.take() {
                self.response_search_task = Some(ResponseSearchTask::start(request));
                return false;
            }
        }
        if !valid || task.cancelled.load(Ordering::Relaxed) {
            return false;
        }
        if let Some(line) = found {
            self.view.response.search_match_line = Some(line);
            self.view.response.scroll.set_offset(line.saturating_add(1));
        } else {
            self.view.notice = Some(Feedback::Warning(
                self.text().response_search_no_match().to_string(),
            ));
        }
        true
    }

    pub(crate) fn poll_response_actions(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.response_actions.try_recv() {
            changed = true;
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
                FinishedResponseAction::Downloaded(Ok(_)) => {
                    self.view.notice = Some(Feedback::Success(
                        self.text().response_downloaded().to_string(),
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
            .map(|document| {
                if self.view.response.active_tab == ResponseTab::Raw {
                    document.raw()
                } else {
                    document
                }
            })
    }

    pub(crate) fn current_error(&self) -> Option<&str> {
        self.workspace_state
            .current()
            .and_then(|session| session.runtime.error())
    }

    pub(crate) fn scroll_response(&mut self, direction: isize) {
        if self.view.response.scroll.move_by(direction) {
            tracing::trace!(
                offset = self.view.response.scroll.offset(),
                direction,
                "滚动响应内容"
            );
        }
    }

    pub(crate) fn open_response_menu(&mut self) {
        self.view.cancel_active_editors();
        if self.editing_preview_tab().is_some() {
            self.view.dialog = None;
        }
        self.view
            .response
            .menu
            .select(0, ResponseMenuAction::all().len());
        self.view.response.menu.open();
        self.view.focus = Focus::ResponseActions;
    }

    pub(crate) fn close_response_menu(&mut self) {
        self.view.response.menu.close();
    }

    pub(crate) fn move_response_menu_selection(&mut self, direction: isize) {
        let action_count = ResponseMenuAction::all().len();
        if direction < 0 {
            self.view.response.menu.previous(action_count);
        } else if direction > 0 {
            self.view.response.menu.next(action_count);
        }
    }

    pub(crate) fn choose_response_action(&mut self, index: usize) {
        self.view
            .response
            .menu
            .select(index, ResponseMenuAction::all().len());
        self.activate_selected_response_action();
    }

    pub(crate) fn activate_selected_response_action(&mut self) {
        let action = ResponseMenuAction::from_index(self.view.response.menu.active());
        self.close_response_menu();
        if let Some(action) = action {
            self.activate_response_action(action);
        }
    }

    fn activate_response_action(&mut self, action: ResponseMenuAction) {
        match action {
            ResponseMenuAction::Download => self.download_current_response(),
            ResponseMenuAction::CopyBody => self.copy_current_response(),
            ResponseMenuAction::CopyHeaders => self.copy_current_response_headers(),
        }
    }

    pub(crate) fn move_response_tab(&mut self, reverse: bool) {
        let tab = if reverse {
            self.view.response.active_tab.previous()
        } else {
            self.view.response.active_tab.next()
        };
        self.select_response_tab(tab);
    }

    pub(crate) fn toggle_response_format_tab(&mut self) {
        if self.current_response().is_none() {
            return;
        }
        self.select_response_tab(self.view.response.active_tab.toggle_format());
    }

    pub(crate) fn select_response_tab(&mut self, tab: ResponseTab) {
        if self.view.response.active_tab == tab {
            return;
        }
        if let Some(task) = self.response_search_task.as_mut() {
            task.cancelled.store(true, Ordering::Relaxed);
            task.pending = None;
        }
        self.view.response.active_tab = tab;
        self.view.response.scroll.reset();
        self.view.response.search_match_line = None;
        self.view.response.search = None;
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
        if self.response_actions.is_running() {
            return;
        }
        let Some(response) = self.current_response() else {
            self.view.notice = Some(Feedback::Warning(
                self.text().response_action_no_response().to_string(),
            ));
            return;
        };
        if response.is_binary() {
            self.view.notice = Some(Feedback::Warning(
                self.text().binary_response_download().to_string(),
            ));
            return;
        }
        let body = response.body_bytes.clone();
        self.response_actions.copy(body);
    }

    fn copy_current_response_headers(&mut self) {
        if self.response_actions.is_running() {
            return;
        }
        let Some(value) = self.current_response().map(ResponseData::headers_text) else {
            self.view.notice = Some(Feedback::Warning(
                self.text().response_action_no_response().to_string(),
            ));
            return;
        };
        self.response_actions.copy(value.into());
    }

    fn download_current_response(&mut self) {
        if self.response_actions.is_running() {
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
        self.response_actions.download(
            body,
            headers,
            request_id,
            self.config.download_directory.clone(),
        );
    }
}
