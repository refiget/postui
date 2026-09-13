use crate::shortcuts::{self, Command, Context};
use crate::{
    config::{DataPart, RequestParam},
    editor::{EditAction, EditInput},
};
use crossterm::event::KeyEvent;

use super::PreviewTab;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyValueField {
    Name,
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeaderSource {
    Collection,
    Request,
    Suppressed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamSource {
    Url,
    Query,
    Form,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataPartSource {
    Raw,
    UrlEncoded,
}

impl DataPartSource {
    pub(super) fn to_part(self, value: RequestParam) -> DataPart {
        match self {
            Self::Raw => DataPart::Raw(value.to_text()),
            Self::UrlEncoded => DataPart::UrlEncoded(value),
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
pub(crate) struct ParamsDialogRow {
    pub(crate) source: ParamSource,
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) part_type: Option<DataPartSource>,
    pub(crate) has_equals: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ConfigurationsDialog {
    pub(crate) rows: Vec<String>,
    pub(crate) selected: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct HeadersDialog {
    pub(crate) request_id: String,
    pub(crate) rows: Vec<HeaderRow>,
    pub(crate) selected: usize,
    pub(crate) scroll: super::ListScrollState,
    pub(crate) field: KeyValueField,
    pub(crate) editor: Option<EditInput>,
}

#[derive(Debug, Clone)]
pub(crate) struct ParamsDialog {
    pub(crate) request_id: String,
    pub(crate) rows: Vec<ParamsDialogRow>,
    pub(crate) selected: usize,
    pub(crate) scroll: super::ListScrollState,
    pub(crate) field: KeyValueField,
    pub(crate) editor: Option<EditInput>,
}

#[derive(Debug, Clone)]
pub(crate) enum Dialog {
    Configurations(ConfigurationsDialog),
    Headers(HeadersDialog),
    Params(ParamsDialog),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DialogAction {
    None,
    Changed,
    RemoveRow { tab: PreviewTab, index: usize },
    Apply,
    Cancel,
}

impl ConfigurationsDialog {
    fn handle_key(&mut self, key: KeyEvent) -> DialogAction {
        match shortcuts::resolve(Context::Menu, key, false) {
            Some(Command::Back) => DialogAction::Cancel,
            Some(Command::Up) => {
                self.move_selection(-1);
                DialogAction::None
            }
            Some(Command::Down) => {
                self.move_selection(1);
                DialogAction::None
            }
            Some(Command::Activate) => DialogAction::Apply,
            _ => DialogAction::None,
        }
    }

    fn move_selection(&mut self, direction: isize) {
        self.selected = move_index(self.selected, direction, self.rows.len());
    }

    fn click_row(&mut self, index: usize) {
        if index < self.rows.len() {
            self.selected = index;
        }
    }
}

impl HeadersDialog {
    fn handle_key(&mut self, key: KeyEvent) -> DialogAction {
        if let Some(editor) = &mut self.editor {
            return match editor.handle_key(key) {
                EditAction::Continue => DialogAction::None,
                EditAction::Confirm => {
                    self.commit_editor();
                    DialogAction::Changed
                }
                EditAction::Cancel => {
                    self.editor = None;
                    DialogAction::None
                }
            };
        }

        match shortcuts::resolve(Context::Headers, key, false) {
            Some(Command::Back) => DialogAction::Cancel,
            Some(Command::Up) => {
                self.move_selection(-1);
                DialogAction::None
            }
            Some(Command::Down) => {
                self.move_selection(1);
                DialogAction::None
            }
            Some(Command::Left) => {
                self.field = KeyValueField::Name;
                DialogAction::None
            }
            Some(Command::Right) => {
                self.field = KeyValueField::Value;
                DialogAction::None
            }
            Some(Command::Delete) => DialogAction::RemoveRow {
                tab: PreviewTab::Headers,
                index: self.selected,
            },
            Some(Command::Activate) => {
                self.start_edit();
                DialogAction::None
            }
            Some(Command::Toggle) => {
                self.toggle_selected();
                DialogAction::Changed
            }
            Some(Command::Add) => {
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
            KeyValueField::Name => row.name.clone(),
            KeyValueField::Value => row.value.clone(),
        };
        self.editor = Some(EditInput::new(value));
    }

    fn commit_editor(&mut self) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        let Some(row) = self.rows.get_mut(self.selected) else {
            return;
        };
        let value = editor.confirmed_value();
        let changed = match self.field {
            KeyValueField::Name if row.name != value => {
                row.name = value;
                true
            }
            KeyValueField::Value if row.value != value => {
                row.value = value;
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
        self.field = KeyValueField::Name;
        self.editor = Some(EditInput::new(String::new()));
    }

    pub(super) fn remove_row(&mut self, index: usize) -> Option<HeaderRow> {
        if index >= self.rows.len() {
            return None;
        }
        self.editor = None;
        let removed = self.rows.remove(index);
        self.selected = index.min(self.rows.len().saturating_sub(1));
        Some(removed)
    }

    pub(super) fn toggle_selected(&mut self) {
        if let Some(row) = self.rows.get_mut(self.selected) {
            if row.source == HeaderSource::Request {
                row.enabled = !row.enabled;
            }
        }
    }

    fn click_row(&mut self, index: usize, field: KeyValueField, edit: bool, cursor: Option<usize>) {
        if index >= self.rows.len() {
            return;
        }
        let same_cell = self.selected == index && self.field == field;
        self.selected = index;
        self.field = field;
        if edit {
            if cursor.is_none() || !same_cell || self.editor.is_none() {
                self.editor = None;
                self.start_edit();
            }
            if let (Some(editor), Some(column)) = (&mut self.editor, cursor) {
                editor.place_cursor(column);
            }
        }
    }
}

impl ParamsDialog {
    fn handle_key(&mut self, key: KeyEvent) -> DialogAction {
        if let Some(editor) = &mut self.editor {
            return match editor.handle_key(key) {
                EditAction::Continue => DialogAction::None,
                EditAction::Confirm => {
                    self.commit_editor();
                    DialogAction::Changed
                }
                EditAction::Cancel => {
                    self.editor = None;
                    DialogAction::None
                }
            };
        }

        match shortcuts::resolve(Context::Params, key, false) {
            Some(Command::Back) => DialogAction::Cancel,
            Some(Command::Up) => {
                self.move_selection(-1);
                DialogAction::None
            }
            Some(Command::Down) => {
                self.move_selection(1);
                DialogAction::None
            }
            Some(Command::Left) => {
                self.field = KeyValueField::Name;
                DialogAction::None
            }
            Some(Command::Right) => {
                self.field = KeyValueField::Value;
                DialogAction::None
            }
            Some(Command::Delete) => DialogAction::RemoveRow {
                tab: PreviewTab::Params,
                index: self.selected,
            },
            Some(Command::Activate) => {
                self.start_edit();
                DialogAction::None
            }
            Some(Command::Add) => {
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
            KeyValueField::Name => row.key.clone(),
            KeyValueField::Value => row.value.clone(),
        };
        self.editor = Some(EditInput::new(value));
    }

    fn commit_editor(&mut self) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        let Some(row) = self.rows.get_mut(self.selected) else {
            return;
        };
        let value = editor.confirmed_value();
        match self.field {
            KeyValueField::Name if row.key != value => row.key = value,
            KeyValueField::Value if row.value != value => {
                row.value = value;
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
                .then_some(DataPartSource::UrlEncoded),
            has_equals: true,
        });
        self.selected = self.rows.len().saturating_sub(1);
        self.field = KeyValueField::Name;
        self.editor = Some(EditInput::new(String::new()));
    }

    pub(super) fn remove_row(&mut self, index: usize) {
        if index >= self.rows.len() {
            return;
        }
        self.editor = None;
        self.rows.remove(index);
        self.selected = index.min(self.rows.len().saturating_sub(1));
    }

    fn click_row(&mut self, index: usize, field: KeyValueField, edit: bool, cursor: Option<usize>) {
        if index >= self.rows.len() {
            return;
        }
        let same_cell = self.selected == index && self.field == field;
        self.selected = index;
        self.field = field;
        if edit {
            if cursor.is_none() || !same_cell || self.editor.is_none() {
                self.editor = None;
                self.start_edit();
            }
            if let (Some(editor), Some(column)) = (&mut self.editor, cursor) {
                editor.place_cursor(column);
            }
        }
    }
}

impl Dialog {
    pub(crate) fn preview_tab(&self) -> Option<PreviewTab> {
        match self {
            Self::Configurations(_) => None,
            Self::Headers(_) => Some(PreviewTab::Headers),
            Self::Params(_) => Some(PreviewTab::Params),
        }
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> DialogAction {
        match self {
            Self::Configurations(dialog) => dialog.handle_key(key),
            Self::Headers(dialog) => dialog.handle_key(key),
            Self::Params(dialog) => dialog.handle_key(key),
        }
    }

    pub(super) fn move_selection(&mut self, direction: isize) {
        self.cancel_editor();
        match self {
            Self::Configurations(dialog) => dialog.move_selection(direction),
            Self::Headers(dialog) => dialog.move_selection(direction),
            Self::Params(dialog) => dialog.move_selection(direction),
        }
    }

    pub(super) fn click_configuration_row(&mut self, index: usize) {
        if let Self::Configurations(dialog) = self {
            dialog.click_row(index);
        }
    }

    pub(super) fn click_param_row(
        &mut self,
        index: usize,
        field: KeyValueField,
        edit: bool,
        cursor: Option<usize>,
    ) {
        if let Self::Params(dialog) = self {
            dialog.click_row(index, field, edit, cursor);
        }
    }

    pub(super) fn cancel_editor(&mut self) {
        match self {
            Self::Configurations(_) => {}
            Self::Headers(dialog) => dialog.editor = None,
            Self::Params(dialog) => dialog.editor = None,
        }
    }

    pub(super) fn is_editing(&self) -> bool {
        match self {
            Self::Configurations(_) => false,
            Self::Headers(dialog) => dialog.editor.is_some(),
            Self::Params(dialog) => dialog.editor.is_some(),
        }
    }

    pub(super) fn click_header_row(
        &mut self,
        index: usize,
        field: KeyValueField,
        edit: bool,
        cursor: Option<usize>,
    ) {
        if let Self::Headers(dialog) = self {
            dialog.click_row(index, field, edit, cursor);
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
