use std::collections::BTreeMap;

use crate::{
    config::BodyPart,
    editor::{EditorAction, TextEditor},
};
use crossterm::event::{KeyCode, KeyEvent};

use super::PreviewTab;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogFocus {
    Content,
    Apply,
    Close,
}

impl DialogFocus {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeaderField {
    Name,
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeaderSource {
    Collection,
    Request,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamSource {
    Url,
    Query,
    Form,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyPartSource {
    Raw,
    UrlEncoded,
}

impl BodyPartSource {
    pub(super) fn to_part(self, value: String) -> BodyPart {
        match self {
            Self::Raw => BodyPart::Raw(value),
            Self::UrlEncoded => BodyPart::UrlEncoded(value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeaderRow {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) enabled: bool,
    pub(crate) source: HeaderSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VariableRow {
    pub(crate) name: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParamsDialogRow {
    pub(crate) source: ParamSource,
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) part_type: Option<BodyPartSource>,
    pub(crate) has_equals: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct VariablesDialog {
    pub(crate) rows: Vec<VariableRow>,
    pub(crate) selected: usize,
    pub(crate) focus: DialogFocus,
    pub(crate) editor: Option<TextEditor>,
}

#[derive(Debug, Clone)]
pub(crate) struct HeadersDialog {
    pub(crate) request_id: String,
    pub(crate) rows: Vec<HeaderRow>,
    pub(crate) selected: usize,
    pub(crate) field: HeaderField,
    pub(crate) editor: Option<TextEditor>,
}

#[derive(Debug, Clone)]
pub(crate) struct ParamsDialog {
    pub(crate) request_id: String,
    pub(crate) rows: Vec<ParamsDialogRow>,
    pub(crate) selected: usize,
    pub(crate) field: HeaderField,
    pub(crate) editor: Option<TextEditor>,
}

#[derive(Debug, Clone)]
pub(crate) enum Dialog {
    Variables(VariablesDialog),
    Headers(HeadersDialog),
    Params(ParamsDialog),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DialogAction {
    None,
    Changed,
    Apply,
    Cancel,
}

impl VariablesDialog {
    fn handle_key(&mut self, key: KeyEvent) -> DialogAction {
        if let Some(editor) = &mut self.editor {
            return match editor.handle_key(key) {
                EditorAction::Continue => DialogAction::None,
                EditorAction::Commit => {
                    self.commit_editor();
                    DialogAction::Changed
                }
                EditorAction::Cancel => {
                    self.editor = None;
                    DialogAction::None
                }
            };
        }

        match key.code {
            KeyCode::Esc => DialogAction::Cancel,
            KeyCode::Tab => {
                self.focus = self.focus.next();
                DialogAction::None
            }
            KeyCode::BackTab => {
                self.focus = self.focus.previous();
                DialogAction::None
            }
            KeyCode::Up | KeyCode::Char('k') if self.focus == DialogFocus::Content => {
                self.move_selection(-1);
                DialogAction::None
            }
            KeyCode::Down | KeyCode::Char('j') if self.focus == DialogFocus::Content => {
                self.move_selection(1);
                DialogAction::None
            }
            KeyCode::Enter | KeyCode::Char(' ') => match self.focus {
                DialogFocus::Content => {
                    self.start_edit();
                    DialogAction::None
                }
                DialogFocus::Apply => DialogAction::Apply,
                DialogFocus::Close => DialogAction::Cancel,
            },
            _ => DialogAction::None,
        }
    }

    fn move_selection(&mut self, direction: isize) {
        self.selected = move_index(self.selected, direction, self.rows.len());
    }

    fn start_edit(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            self.editor = Some(TextEditor::new(row.value.clone()));
        }
    }

    pub(super) fn commit_editor(&mut self) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        if let Some(row) = self.rows.get_mut(self.selected) {
            row.value = editor.value;
        }
    }

    fn click_row(&mut self, index: usize, edit: bool) {
        if index >= self.rows.len() {
            return;
        }
        self.selected = index;
        self.focus = DialogFocus::Content;
        if edit {
            self.start_edit();
        }
    }
}

impl HeadersDialog {
    fn handle_key(&mut self, key: KeyEvent) -> DialogAction {
        if let Some(editor) = &mut self.editor {
            return match editor.handle_key(key) {
                EditorAction::Continue => DialogAction::None,
                EditorAction::Commit => {
                    self.commit_editor();
                    DialogAction::Changed
                }
                EditorAction::Cancel => {
                    self.editor = None;
                    DialogAction::None
                }
            };
        }

        match key.code {
            KeyCode::Esc => DialogAction::Cancel,
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                DialogAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                DialogAction::None
            }
            KeyCode::Left => {
                self.field = HeaderField::Name;
                DialogAction::None
            }
            KeyCode::Right => {
                self.field = HeaderField::Value;
                DialogAction::None
            }
            KeyCode::Char('d') => {
                self.remove_selected();
                DialogAction::Changed
            }
            KeyCode::Enter => {
                self.start_edit();
                DialogAction::None
            }
            KeyCode::Char(' ') => {
                self.toggle_selected();
                DialogAction::Changed
            }
            KeyCode::Char('a') => {
                self.add_row();
                DialogAction::None
            }
            _ => DialogAction::None,
        }
    }

    fn move_selection(&mut self, direction: isize) {
        self.selected = move_index(self.selected, direction, self.rows.len());
    }

    fn start_edit(&mut self) {
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        let value = match self.field {
            HeaderField::Name => row.name.clone(),
            HeaderField::Value => row.value.clone(),
        };
        self.editor = Some(TextEditor::new(value));
    }

    fn commit_editor(&mut self) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        let Some(row) = self.rows.get_mut(self.selected) else {
            return;
        };
        let changed = match self.field {
            HeaderField::Name if row.name != editor.value => {
                row.name = editor.value;
                true
            }
            HeaderField::Value if row.value != editor.value => {
                row.value = editor.value;
                true
            }
            _ => false,
        };
        if changed {
            row.source = HeaderSource::Request;
        }
    }

    pub(super) fn add_row(&mut self) {
        self.rows.push(HeaderRow {
            name: String::new(),
            value: String::new(),
            enabled: true,
            source: HeaderSource::Request,
        });
        self.selected = self.rows.len().saturating_sub(1);
        self.field = HeaderField::Name;
        self.editor = Some(TextEditor::new(String::new()));
    }

    fn remove_selected(&mut self) {
        if self
            .rows
            .get(self.selected)
            .is_some_and(|row| row.source == HeaderSource::Request)
        {
            self.rows.remove(self.selected);
            self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        }
    }

    pub(super) fn toggle_selected(&mut self) {
        if let Some(row) = self.rows.get_mut(self.selected) {
            if row.source == HeaderSource::Request {
                row.enabled = !row.enabled;
            }
        }
    }

    fn click_row(&mut self, index: usize, field: HeaderField, edit: bool) {
        if index >= self.rows.len() {
            return;
        }
        self.selected = index;
        self.field = field;
        if edit {
            self.start_edit();
        }
    }
}

impl ParamsDialog {
    fn handle_key(&mut self, key: KeyEvent) -> DialogAction {
        if let Some(editor) = &mut self.editor {
            return match editor.handle_key(key) {
                EditorAction::Continue => DialogAction::None,
                EditorAction::Commit => {
                    self.commit_editor();
                    DialogAction::Changed
                }
                EditorAction::Cancel => {
                    self.editor = None;
                    DialogAction::None
                }
            };
        }

        match key.code {
            KeyCode::Esc => DialogAction::Cancel,
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                DialogAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                DialogAction::None
            }
            KeyCode::Left => {
                self.field = HeaderField::Name;
                DialogAction::None
            }
            KeyCode::Right => {
                self.field = HeaderField::Value;
                DialogAction::None
            }
            KeyCode::Char('d') => {
                self.remove_selected();
                DialogAction::Changed
            }
            KeyCode::Enter => {
                self.start_edit();
                DialogAction::None
            }
            KeyCode::Char('a') => {
                self.add_row();
                DialogAction::None
            }
            _ => DialogAction::None,
        }
    }

    fn move_selection(&mut self, direction: isize) {
        self.selected = move_index(self.selected, direction, self.rows.len());
    }

    fn start_edit(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        let value = match self.field {
            HeaderField::Name => row.key.clone(),
            HeaderField::Value => row.value.clone(),
        };
        self.editor = Some(TextEditor::new(value));
    }

    fn commit_editor(&mut self) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        let Some(row) = self.rows.get_mut(self.selected) else {
            return;
        };
        match self.field {
            HeaderField::Name if row.key != editor.value => row.key = editor.value,
            HeaderField::Value if row.value != editor.value => {
                row.value = editor.value;
                if row.source == ParamSource::Query {
                    row.has_equals = true;
                }
            }
            _ => {}
        }
    }

    pub(super) fn add_row(&mut self) {
        let source = if self.rows.iter().any(|row| row.source == ParamSource::Form)
            && !self.rows.iter().any(|row| {
                matches!(
                    row.source,
                    ParamSource::Url | ParamSource::Query | ParamSource::Body
                )
            }) {
            ParamSource::Form
        } else if self.rows.iter().any(|row| row.source == ParamSource::Body)
            && !self
                .rows
                .iter()
                .any(|row| matches!(row.source, ParamSource::Url | ParamSource::Query))
        {
            ParamSource::Body
        } else {
            ParamSource::Query
        };
        self.rows.push(ParamsDialogRow {
            source,
            key: String::new(),
            value: String::new(),
            part_type: matches!(source, ParamSource::Query | ParamSource::Body)
                .then_some(BodyPartSource::UrlEncoded),
            has_equals: true,
        });
        self.selected = self.rows.len().saturating_sub(1);
        self.field = HeaderField::Name;
        self.editor = Some(TextEditor::new(String::new()));
    }

    fn remove_selected(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.rows.remove(self.selected);
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }

    fn click_row(&mut self, index: usize, field: HeaderField, edit: bool) {
        if index >= self.rows.len() {
            return;
        }
        self.selected = index;
        self.field = field;
        if edit {
            self.start_edit();
        }
    }
}

impl Dialog {
    pub(crate) fn preview_tab(&self) -> Option<PreviewTab> {
        match self {
            Self::Variables(_) => None,
            Self::Headers(_) => Some(PreviewTab::Headers),
            Self::Params(_) => Some(PreviewTab::Params),
        }
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> DialogAction {
        match self {
            Self::Variables(dialog) => dialog.handle_key(key),
            Self::Headers(dialog) => dialog.handle_key(key),
            Self::Params(dialog) => dialog.handle_key(key),
        }
    }

    pub(super) fn move_selection(&mut self, direction: isize) {
        self.commit_editor();
        match self {
            Self::Variables(dialog) => dialog.move_selection(direction),
            Self::Headers(dialog) => dialog.move_selection(direction),
            Self::Params(dialog) => dialog.move_selection(direction),
        }
    }

    pub(super) fn click_variable_row(&mut self, index: usize, edit: bool) {
        if let Self::Variables(dialog) = self {
            dialog.click_row(index, edit);
        }
    }

    pub(super) fn click_param_row(&mut self, index: usize, field: HeaderField, edit: bool) {
        if let Self::Params(dialog) = self {
            dialog.click_row(index, field, edit);
        }
    }

    pub(super) fn commit_editor(&mut self) {
        match self {
            Self::Variables(dialog) => dialog.commit_editor(),
            Self::Headers(dialog) => dialog.commit_editor(),
            Self::Params(dialog) => dialog.commit_editor(),
        }
    }

    pub(super) fn is_editing(&self) -> bool {
        match self {
            Self::Variables(dialog) => dialog.editor.is_some(),
            Self::Headers(dialog) => dialog.editor.is_some(),
            Self::Params(dialog) => dialog.editor.is_some(),
        }
    }

    pub(super) fn click_header_row(&mut self, index: usize, field: HeaderField, edit: bool) {
        if let Self::Headers(dialog) = self {
            dialog.click_row(index, field, edit);
        }
    }
}

fn move_index(current: usize, direction: isize, length: usize) -> usize {
    if length == 0 {
        return 0;
    }
    match direction {
        value if value < 0 => current.saturating_sub(value.unsigned_abs()),
        value => current
            .saturating_add(value as usize)
            .min(length.saturating_sub(1)),
    }
}

pub(super) fn remove_header(rows: &mut Vec<HeaderRow>, name: &str) {
    rows.retain(|row| !row.name.eq_ignore_ascii_case(name));
}

pub(super) fn remove_header_map(headers: &mut BTreeMap<String, String>, name: &str) {
    if let Some(existing) = headers
        .keys()
        .find(|existing| existing.eq_ignore_ascii_case(name))
        .cloned()
    {
        headers.remove(&existing);
    }
}

pub(super) fn split_key_value(value: &str) -> (String, String, bool) {
    if let Some((key, value)) = value.split_once('=') {
        (key.to_string(), value.to_string(), true)
    } else {
        (value.to_string(), String::new(), false)
    }
}
