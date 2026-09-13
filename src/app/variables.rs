use crate::editor::{EditAction, EditInput};
use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Focus};

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

        match key.code {
            KeyCode::Esc => Some(VariablesPageAction::Close),
            KeyCode::Tab => {
                self.focus = self.focus.next();
                None
            }
            KeyCode::BackTab => {
                self.focus = self.focus.previous();
                None
            }
            KeyCode::Up | KeyCode::Char('k') if self.focus == VariablePageFocus::Content => {
                self.move_selection(-1);
                None
            }
            KeyCode::Down | KeyCode::Char('j') if self.focus == VariablePageFocus::Content => {
                self.move_selection(1);
                None
            }
            KeyCode::Enter | KeyCode::Char(' ') => match self.focus {
                VariablePageFocus::Content => {
                    self.start_edit();
                    None
                }
                VariablePageFocus::Apply => Some(VariablesPageAction::Apply),
                VariablePageFocus::Close => Some(VariablesPageAction::Close),
            },
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
