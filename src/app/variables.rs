use crate::editor::{EditAction, EditInput};
use crate::shortcuts::{self, Command, Context};
use crossterm::event::KeyEvent;

use super::{App, Feedback, Focus};

impl App {
    pub(crate) fn variable_default_value(&self, variable: &str) -> String {
        let definition = self
            .config
            .configurations
            .get(self.active_configuration())
            .and_then(|configuration| configuration.variables.get(variable))
            .or_else(|| self.config.variables.get(variable));
        if self.variable_is_secret(variable) {
            return "••••••".to_string();
        }
        definition
            .and_then(|definition| definition.default.as_ref())
            .map(crate::config::value_to_string)
            .unwrap_or_else(|| "—".to_string())
    }

    pub(super) fn variable_is_secret(&self, variable: &str) -> bool {
        let scenario_secret = self
            .config
            .configurations
            .get(self.active_configuration())
            .and_then(|configuration| configuration.variables.get(variable))
            .is_some_and(|definition| definition.secret);
        scenario_secret
            || self
                .config
                .variables
                .get(variable)
                .is_some_and(|definition| definition.secret)
    }

    pub(crate) fn secret_variable_values(&self) -> Vec<String> {
        self.workspace_state
            .variables
            .iter()
            .filter(|(name, value)| self.variable_is_secret(name) && !value.is_empty())
            .map(|(_, value)| value.clone())
            .collect()
    }

    pub(crate) fn open_variables(&mut self) {
        self.open_variables_at(None, None);
    }

    pub(super) fn open_variables_at(
        &mut self,
        missing_variables: Option<&[String]>,
        selected_name: Option<String>,
    ) {
        let missing = missing_variables
            .map(|variables| {
                variables
                    .iter()
                    .map(String::as_str)
                    .collect::<std::collections::BTreeSet<_>>()
            })
            .unwrap_or_default();
        let mut rows = self
            .config
            .editable_variables
            .iter()
            .map(|name| VariableRow {
                name: name.clone(),
                value: self
                    .workspace_state
                    .variables
                    .get(name)
                    .cloned()
                    .unwrap_or_default(),
                missing: missing.contains(name.as_str()),
                secret: self.variable_is_secret(name),
            })
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| !row.missing);
        let selected = selected_name
            .as_deref()
            .and_then(|name| rows.iter().position(|row| row.name == name))
            .or_else(|| rows.iter().position(|row| row.missing))
            .unwrap_or_default();
        let return_focus = self.view.focus;
        self.view.variables = Some(VariablesPage {
            rows,
            selected,
            scroll: Default::default(),
            focus: VariablePageFocus::Content,
            editor: None,
            return_focus,
        });
        self.view.focus = Focus::Variables;
        tracing::debug!(variable_count = self.variable_count(), "打开工作区变量页面");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VariableRow {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) missing: bool,
    pub(crate) secret: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VariablePageFocus {
    Content,
    Apply,
    Close,
}

impl VariablePageFocus {
    fn next(self) -> Self {
        match self {
            Self::Content => Self::Apply,
            Self::Apply => Self::Close,
            Self::Close => Self::Content,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Content => Self::Close,
            Self::Apply => Self::Content,
            Self::Close => Self::Apply,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VariablesPage {
    pub(crate) rows: Vec<VariableRow>,
    pub(crate) selected: usize,
    pub(crate) scroll: super::ListScrollState,
    pub(crate) focus: VariablePageFocus,
    pub(crate) editor: Option<EditInput>,
    pub(crate) return_focus: Focus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VariablesPageAction {
    Apply,
    Close,
}

impl VariablesPage {
    pub(super) fn handle_key(&mut self, key: KeyEvent) -> Option<VariablesPageAction> {
        if let Some(editor) = &mut self.editor {
            return match editor.handle_key(key) {
                EditAction::Continue => None,
                EditAction::Confirm => {
                    self.commit_editor();
                    None
                }
                EditAction::Cancel => {
                    self.editor = None;
                    None
                }
            };
        }

        match shortcuts::resolve(Context::Variables, key, false) {
            Some(Command::Back) => Some(VariablesPageAction::Close),
            Some(Command::FocusNext) => {
                self.focus = self.focus.next();
                None
            }
            Some(Command::FocusPrevious) => {
                self.focus = self.focus.previous();
                None
            }
            Some(Command::Up) if self.focus == VariablePageFocus::Content => {
                self.move_selection(-1);
                None
            }
            Some(Command::Down) if self.focus == VariablePageFocus::Content => {
                self.move_selection(1);
                None
            }
            Some(Command::Activate) => self.activate(),
            _ => None,
        }
    }

    pub(super) fn activate(&mut self) -> Option<VariablesPageAction> {
        match self.focus {
            VariablePageFocus::Content => {
                self.start_edit();
                None
            }
            VariablePageFocus::Apply => Some(VariablesPageAction::Apply),
            VariablePageFocus::Close => Some(VariablesPageAction::Close),
        }
    }

    pub(super) fn move_selection(&mut self, direction: isize) {
        self.editor = None;
        let count = self.rows.len();
        if count > 0 {
            self.selected =
                (self.selected as isize + direction).rem_euclid(count as isize) as usize;
        }
    }

    fn start_edit(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            self.editor = Some(EditInput::new(row.value.clone()));
        }
    }

    pub(super) fn commit_editor(&mut self) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        if let Some(row) = self.rows.get_mut(self.selected) {
            row.value = editor.confirmed_value();
        }
    }

    pub(super) fn click_row(&mut self, index: usize, edit: bool, cursor: Option<usize>) {
        if index >= self.rows.len() {
            return;
        }
        let same_cell = self.selected == index;
        self.selected = index;
        self.focus = VariablePageFocus::Content;
        if !edit {
            self.editor = None;
            return;
        }
        if let Some(column) = cursor {
            if !same_cell || self.editor.is_none() {
                self.start_edit();
            }
            if let Some(editor) = &mut self.editor {
                editor.place_cursor(column);
            }
        } else {
            self.editor = None;
            self.start_edit();
        }
    }

    pub(super) fn cancel_editor(&mut self) {
        self.editor = None;
    }
}

impl App {
    pub(super) fn close_variables(&mut self) {
        let Some(page) = self.view.variables.take() else {
            return;
        };
        self.view.focus = page.return_focus;
        tracing::debug!("关闭工作区变量页面");
    }

    fn apply_variables(&mut self) {
        let Some(mut page) = self.view.variables.take() else {
            return;
        };
        page.cancel_editor();
        for row in page.rows {
            self.workspace_state.variables.insert(row.name, row.value);
        }
        self.view.notice = Some(Feedback::Success(
            self.text().variables_applied().to_string(),
        ));
        tracing::debug!(
            variable_count = self.workspace_state.variables.len(),
            "应用工作区变量修改"
        );
        self.view.focus = page.return_focus;
    }

    pub(super) fn handle_variables_key(&mut self, key: KeyEvent) {
        let Some(page) = self.view.variables.as_mut() else {
            return;
        };
        let action = page.handle_key(key);
        self.handle_variables_action(action);
    }

    fn handle_variables_action(&mut self, action: Option<VariablesPageAction>) {
        match action {
            Some(VariablesPageAction::Apply) => self.apply_variables(),
            Some(VariablesPageAction::Close) => self.close_variables(),
            None => {}
        }
    }

    pub(crate) fn click_variable_row(&mut self, index: usize, edit: bool, cursor: Option<usize>) {
        if let Some(page) = self.view.variables.as_mut() {
            page.click_row(index, edit, cursor);
        }
    }

    pub(crate) fn focus_variables_page(&mut self, focus: VariablePageFocus) {
        if let Some(page) = self.view.variables.as_mut() {
            page.focus = focus;
        }
    }

    pub(crate) fn click_variables_page_button(&mut self, focus: VariablePageFocus) {
        if let Some(page) = self.view.variables.as_mut() {
            page.cancel_editor();
        }
        self.focus_variables_page(focus);
        let action = self
            .view
            .variables
            .as_mut()
            .and_then(VariablesPage::activate);
        self.handle_variables_action(action);
    }

    pub(crate) fn cancel_variable_edit(&mut self) {
        if let Some(page) = self.view.variables.as_mut() {
            page.cancel_editor();
        }
    }
}
