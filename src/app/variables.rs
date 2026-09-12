use crate::editor::{EditAction, EditInput};
use crossterm::event::{KeyCode, KeyEvent};

use super::Focus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VariableRow {
    pub(crate) name: String,
    pub(crate) value: String,
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
