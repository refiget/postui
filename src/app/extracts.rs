use crate::shortcuts::{self, Command, Context};
use crossterm::event::KeyEvent;

use super::{App, Feedback, Focus};

#[derive(Debug, Clone)]
pub(crate) struct ExtractRow {
    pub(crate) variable: String,
    pub(crate) path: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ExtractsPage {
    pub(crate) rows: Vec<ExtractRow>,
    pub(crate) selected: usize,
    pub(crate) scroll: super::ListScrollState,
    pub(crate) return_focus: Focus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExtractsPageAction {
    Reordered,
    Close,
}

impl ExtractsPage {
    pub(super) fn handle_key(&mut self, key: KeyEvent) -> Option<ExtractsPageAction> {
        match shortcuts::resolve(Context::Extracts, key, false) {
            Some(Command::Back) => Some(ExtractsPageAction::Close),
            Some(Command::Up) => {
                self.move_selection(-1);
                None
            }
            Some(Command::Down) => {
                self.move_selection(1);
                None
            }
            Some(Command::ReorderUp) => self.reorder(-1).then_some(ExtractsPageAction::Reordered),
            Some(Command::ReorderDown) => self.reorder(1).then_some(ExtractsPageAction::Reordered),
            _ => None,
        }
    }

    pub(super) fn move_selection(&mut self, direction: isize) {
        let count = self.rows.len();
        if count > 0 {
            self.selected =
                (self.selected as isize + direction).rem_euclid(count as isize) as usize;
        }
    }

    pub(super) fn select(&mut self, index: usize) {
        if index < self.rows.len() {
            self.selected = index;
        }
    }

    pub(super) fn reorder(&mut self, direction: isize) -> bool {
        let count = self.rows.len();
        if count < 2 {
            return false;
        }
        let target = match direction {
            value if value < 0 => self.selected.checked_sub(1),
            value if value > 0 => (self.selected + 1 < count).then_some(self.selected + 1),
            _ => None,
        };
        let Some(target) = target else {
            return false;
        };
        self.rows.swap(self.selected, target);
        self.selected = target;
        true
    }

    pub(super) fn order(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.variable.clone()).collect()
    }
}

impl App {
    pub(crate) fn open_extracts(&mut self) {
        let rows = self
            .current_effective_request()
            .map(|request| {
                request
                    .extracts
                    .into_iter()
                    .map(|extract| ExtractRow {
                        variable: extract.variable,
                        path: extract.path,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let return_focus = self.view.focus;
        tracing::debug!(extract_count = rows.len(), "打开提取顺序页面");
        self.view.extracts = Some(ExtractsPage {
            rows,
            selected: 0,
            scroll: Default::default(),
            return_focus,
        });
        self.view.focus = Focus::Extracts;
    }

    pub(super) fn handle_extracts_key(&mut self, key: KeyEvent) {
        let Some(page) = self.view.extracts.as_mut() else {
            return;
        };
        let action = page.handle_key(key);
        match action {
            Some(ExtractsPageAction::Reordered) => self.sync_extract_order(),
            Some(ExtractsPageAction::Close) => self.close_extracts(),
            None => {}
        }
    }

    pub(super) fn close_extracts(&mut self) {
        let Some(page) = self.view.extracts.take() else {
            return;
        };
        self.view.focus = page.return_focus;
        tracing::debug!("关闭提取顺序页面");
    }

    pub(crate) fn click_extract_row(&mut self, index: usize) {
        if let Some(page) = self.view.extracts.as_mut() {
            page.select(index);
        }
    }

    fn sync_extract_order(&mut self) {
        let Some(order) = self.view.extracts.as_ref().map(ExtractsPage::order) else {
            return;
        };
        let configured = self
            .config
            .configurations
            .get(self.active_configuration())
            .zip(self.workspace_state.current())
            .map(|(configuration, session)| session.configured_extract_names(configuration));
        let notice = Feedback::Success(self.text().extract_order_updated().to_string());
        let extract_count = order.len();
        let Some(session) = self.workspace_state.current_mut() else {
            return;
        };
        let unchanged = configured.as_deref() == Some(order.as_slice());
        session.set_extract_order((!unchanged).then_some(order));
        tracing::debug!(extract_count, "调整提取顺序");
        self.view.notice = Some(notice);
    }
}
