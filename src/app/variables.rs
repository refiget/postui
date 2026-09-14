use crate::editor::{EditAction, EditInput};
use crate::shortcuts::{self, Command, Context};
use crossterm::event::KeyEvent;

use super::{App, Focus};

impl App {
    pub(super) fn variable_definition(
        &self,
        variable: &str,
    ) -> Option<&crate::config::VariableDefinition> {
        self.config
            .configurations
            .get(self.active_configuration())
            .and_then(|configuration| configuration.variables.get(variable))
            .or_else(|| self.config.variables.get(variable))
    }

    pub(crate) fn variable_default_value(&self, variable: &str) -> String {
        if self.variable_is_secret(variable) {
            return "••••••".to_string();
        }
        self.variable_definition(variable)
            .and_then(|definition| definition.default.as_ref())
            .map(crate::config::value_to_string)
            .unwrap_or_else(|| "—".to_string())
    }

    pub(super) fn variable_is_secret(&self, variable: &str) -> bool {
        self.config
            .variables
            .get(variable)
            .is_some_and(|definition| definition.secret)
            || self
                .config
                .configurations
                .get(self.active_configuration())
                .and_then(|configuration| configuration.variables.get(variable))
                .is_some_and(|definition| definition.secret)
    }

    pub(crate) fn secret_variable_values(&self) -> Vec<String> {
        let mut values = self
            .workspace_state
            .variables
            .iter()
            .filter(|(name, value)| self.variable_is_secret(name) && !value.is_empty())
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>();
        if let Some(session) = self.workspace_state.current() {
            values.extend(
                session
                    .temporary_variables
                    .iter()
                    .filter(|(name, value)| self.variable_is_secret(name) && !value.is_empty())
                    .map(|(_, value)| value.clone()),
            );
        }
        values
    }

    pub(crate) fn open_variables(&mut self) {
        let rows = self
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
                secret: self.variable_is_secret(name),
            })
            .collect::<Vec<_>>();
        let return_focus = self.view.focus;
        self.view.variables = Some(VariablesPage {
            rows,
            selected: 0,
            scroll: Default::default(),
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
    pub(crate) secret: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct VariablesPage {
    pub(crate) rows: Vec<VariableRow>,
    pub(crate) selected: usize,
    pub(crate) scroll: super::ListScrollState,
    pub(crate) editor: Option<EditInput>,
    pub(crate) return_focus: Focus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VariablesPageAction {
    Changed,
    Close,
}

impl VariablesPage {
    pub(super) fn handle_key(&mut self, key: KeyEvent) -> Option<VariablesPageAction> {
        if let Some(editor) = &mut self.editor {
            return match editor.handle_key(key) {
                EditAction::Continue => None,
                EditAction::Confirm => self.commit_editor().then_some(VariablesPageAction::Changed),
                EditAction::Cancel => {
                    self.editor = None;
                    None
                }
            };
        }

        match shortcuts::resolve(Context::Variables, key, false) {
            Some(Command::Back) => Some(VariablesPageAction::Close),
            Some(Command::Up) => {
                self.move_selection(-1);
                None
            }
            Some(Command::Down) => {
                self.move_selection(1);
                None
            }
            Some(Command::Activate) => {
                self.start_edit();
                None
            }
            _ => None,
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

    fn commit_editor(&mut self) -> bool {
        let Some(editor) = self.editor.take() else {
            return false;
        };
        let Some(row) = self.rows.get_mut(self.selected) else {
            return false;
        };
        let value = editor.confirmed_value();
        let changed = row.value != value;
        row.value = value;
        changed
    }

    pub(super) fn click_row(&mut self, index: usize, edit: bool, cursor: Option<usize>) {
        if index >= self.rows.len() {
            return;
        }
        let same_cell = self.selected == index;
        self.selected = index;
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

    fn sync_variable(&mut self) {
        let Some(page) = self.view.variables.as_ref() else {
            return;
        };
        let Some(row) = page.rows.get(page.selected) else {
            return;
        };
        self.workspace_state
            .variables
            .insert(row.name.clone(), row.value.clone());
        tracing::debug!(variable = %row.name, "更新工作区变量");
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
            Some(VariablesPageAction::Changed) => self.sync_variable(),
            Some(VariablesPageAction::Close) => self.close_variables(),
            None => {}
        }
    }

    pub(crate) fn click_variable_row(&mut self, index: usize, edit: bool, cursor: Option<usize>) {
        if let Some(page) = self.view.variables.as_mut() {
            page.click_row(index, edit, cursor);
        }
    }

    pub(crate) fn cancel_variable_edit(&mut self) {
        if let Some(page) = self.view.variables.as_mut() {
            page.cancel_editor();
        }
    }
}
