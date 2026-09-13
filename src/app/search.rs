use super::{App, Focus};
use crate::editor::{EditAction, EditInput};
use crossterm::event::KeyEvent;

impl App {
    pub(crate) fn request_search_query(&self) -> Option<&str> {
        self.view
            .requests
            .search
            .as_ref()
            .map(EditInput::value)
            .or((!self.view.requests.filter.is_empty())
                .then_some(self.view.requests.filter.as_str()))
    }

    pub(crate) fn visible_request_indices(&self) -> Vec<usize> {
        let query = self
            .request_search_query()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        self.workspace_state
            .requests
            .iter()
            .enumerate()
            .filter_map(|(index, session)| {
                let request = &session.source;
                (query.is_empty()
                    || request.name.to_lowercase().contains(&query)
                    || request.id.to_lowercase().contains(&query)
                    || request.url.to_lowercase().contains(&query)
                    || request.method.to_lowercase().contains(&query))
                .then_some(index)
            })
            .collect()
    }

    pub(super) fn open_request_search(&mut self) {
        if self.view.requests.filter_origin.is_none() {
            self.view.requests.filter_origin =
                self.current_request().map(|request| request.id.clone());
        }
        self.view.requests.search = Some(EditInput::new(self.view.requests.filter.clone()));
        self.view.focus = Focus::Requests;
    }

    fn request_filter_origin_index(&self) -> Option<usize> {
        let id = self.view.requests.filter_origin.as_deref()?;
        self.workspace_state
            .requests
            .iter()
            .position(|session| session.source.id == id)
    }

    pub(super) fn clear_request_search(&mut self) {
        let previous = self.request_filter_origin_index();
        self.view.requests.search = None;
        self.view.requests.filter.clear();
        self.view.requests.filter_origin = None;
        if let Some(index) = previous {
            self.select_request(index);
        }
    }

    pub(super) fn handle_request_search_key(&mut self, key: KeyEvent) {
        let action = match self.view.requests.search.as_mut() {
            Some(search) => search.handle_key(key),
            None => return,
        };
        match action {
            EditAction::Cancel => self.clear_request_search(),
            EditAction::Confirm => {
                if let Some(search) = self.view.requests.search.take() {
                    self.view.requests.filter = search.confirmed_value();
                    if self.view.requests.filter.trim().is_empty() {
                        self.clear_request_search();
                    }
                }
            }
            EditAction::Continue => {
                let index = if self
                    .request_search_query()
                    .is_none_or(|query| query.trim().is_empty())
                {
                    self.request_filter_origin_index()
                } else {
                    self.visible_request_indices().first().copied()
                };
                if let Some(index) = index {
                    self.select_request(index);
                }
            }
        }
    }
}
