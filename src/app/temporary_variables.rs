use std::collections::BTreeMap;

use crossterm::event::KeyEvent;

use super::{App, RequestStatus, TemporaryVariableEditor, VariableRow};
use crate::editor::EditAction;

impl App {
    pub(crate) fn temporary_variables_visible(&self) -> bool {
        self.current_effective_request().is_some_and(|request| {
            request.body_parts.is_empty()
                && request.form.is_empty()
                && request.files.is_empty()
                && self
                    .workspace_state
                    .current()
                    .is_some_and(|session| !session.temporary_variables.is_empty())
        })
    }

    pub(crate) fn temporary_variable_rows(&self) -> Vec<VariableRow> {
        self.workspace_state
            .current()
            .map(|session| {
                session
                    .temporary_variables
                    .iter()
                    .map(|(name, value)| VariableRow {
                        name: name.clone(),
                        value: value.clone(),
                        secret: self.variable_is_secret(name),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn temporary_variable_editor(&self) -> Option<&TemporaryVariableEditor> {
        self.view.preview.temporary_variables.editor()
    }

    pub(crate) fn selected_temporary_variable(&self) -> Option<usize> {
        let count = self
            .workspace_state
            .current()
            .map_or(0, |session| session.temporary_variables.len());
        self.view.preview.temporary_variables.selected(count)
    }

    pub(crate) fn current_request_variables(&self) -> BTreeMap<String, String> {
        let mut variables = self.workspace_state.variables.clone();
        if let Some(session) = self.workspace_state.current() {
            variables.extend(session.temporary_variables.clone());
        }
        variables
    }

    pub(crate) fn move_temporary_variable(&mut self, direction: isize) -> bool {
        if !self.temporary_variables_visible() {
            return false;
        }
        let count = self
            .workspace_state
            .current()
            .map_or(0, |session| session.temporary_variables.len());
        self.view
            .preview
            .temporary_variables
            .move_by(direction, count);
        true
    }

    pub(crate) fn start_temporary_variable_edit(&mut self) -> bool {
        if !self.temporary_variables_visible() {
            return false;
        }
        let Some(request) = self.current_request() else {
            return false;
        };
        if self.request_status(&request.id) == RequestStatus::Sending {
            return false;
        }
        let Some(index) = self.selected_temporary_variable() else {
            return false;
        };
        let Some((name, value)) = self
            .workspace_state
            .current()
            .and_then(|session| session.temporary_variables.iter().nth(index))
            .map(|(name, value)| (name.clone(), value.clone()))
        else {
            return false;
        };
        self.view
            .preview
            .temporary_variables
            .start_editing(name, value);
        true
    }

    pub(crate) fn select_temporary_variable(&mut self, index: usize, edit: bool) -> bool {
        if !self.temporary_variables_visible() {
            return false;
        }
        let count = self
            .workspace_state
            .current()
            .map_or(0, |session| session.temporary_variables.len());
        if !self.view.preview.temporary_variables.select(index, count) {
            return false;
        }
        if edit {
            self.start_temporary_variable_edit();
        }
        true
    }

    pub(crate) fn handle_temporary_variable_editor_key(&mut self, key: KeyEvent) {
        let action = self
            .view
            .preview
            .temporary_variables
            .editor_mut()
            .map(|editor| editor.input.handle_key(key));
        match action {
            Some(EditAction::Confirm) => {
                let Some(editor) = self.view.preview.temporary_variables.finish_editing() else {
                    return;
                };
                if let Some(session) = self.workspace_state.current_mut() {
                    session
                        .temporary_variables
                        .insert(editor.name, editor.input.confirmed_value());
                }
            }
            Some(EditAction::Cancel) => self.view.preview.temporary_variables.cancel_editing(),
            Some(EditAction::Continue) | None => {}
        }
    }
}
