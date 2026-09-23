use crate::shortcuts::{self, Command, Context};
use crate::{
    config::{DataPart, RequestParam},
    editor::{EditAction, EditInput},
};
use crossterm::event::KeyEvent;

use super::{ListScrollState, PreviewTab};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum KeyValueField {
    #[default]
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

pub(crate) trait InlineRow {
    fn column(&self, field: KeyValueField) -> &str;
}

impl InlineRow for HeaderRow {
    fn column(&self, field: KeyValueField) -> &str {
        match field {
            KeyValueField::Name => &self.name,
            KeyValueField::Value => &self.value,
        }
    }
}

impl InlineRow for ParamsDialogRow {
    fn column(&self, field: KeyValueField) -> &str {
        match field {
            KeyValueField::Name => &self.key,
            KeyValueField::Value => &self.value,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ConfigurationsDialog {
    pub(crate) rows: Vec<String>,
    pub(crate) state: tui_assets_rust::DropdownState,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct InlineTable {
    pub(crate) selected: usize,
    pub(crate) scroll: ListScrollState,
    pub(crate) field: KeyValueField,
    pub(crate) editor: Option<EditInput>,
}

#[derive(Debug, Clone)]
pub(crate) struct HeadersDialog {
    pub(crate) request_id: String,
    pub(crate) rows: Vec<HeaderRow>,
    pub(crate) table: InlineTable,
    pub(crate) preset_selection: Option<usize>,
}

pub(crate) const HEADER_PRESETS: &[(&str, &str)] = &[
    ("Authorization", "Bearer "),
    ("Accept", "application/json"),
    ("Content-Type", "application/json"),
    ("Cookie", ""),
    ("User-Agent", ""),
    ("X-Request-Id", ""),
];

#[derive(Debug, Clone)]
pub(crate) struct ParamsDialog {
    pub(crate) request_id: String,
    pub(crate) rows: Vec<ParamsDialogRow>,
    pub(crate) table: InlineTable,
}

fn move_row_selection<R>(table: &mut InlineTable, rows: &[R], direction: isize) {
    table.selected = move_index(table.selected, direction, rows.len());
}

fn start_cell_edit<R: InlineRow>(table: &mut InlineTable, rows: &[R]) {
    let Some(row) = rows.get(table.selected) else {
        return;
    };
    table.editor = Some(EditInput::new(row.column(table.field).to_string()));
}

fn click_cell<R: InlineRow>(
    table: &mut InlineTable,
    rows: &[R],
    index: usize,
    field: KeyValueField,
    edit: bool,
    cursor: Option<usize>,
) {
    if index >= rows.len() {
        return;
    }
    let same_cell = table.selected == index && table.field == field;
    table.selected = index;
    table.field = field;
    if !edit {
        return;
    }
    if cursor.is_none() || !same_cell || table.editor.is_none() {
        table.editor = Some(EditInput::new(rows[index].column(field).to_string()));
    }
    if let (Some(editor), Some(column)) = (&mut table.editor, cursor) {
        editor.place_cursor(column);
    }
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
        if direction < 0 {
            self.state.previous(self.rows.len());
        } else if direction > 0 {
            self.state.next(self.rows.len());
        }
    }
}

impl HeadersDialog {
    fn handle_key(&mut self, key: KeyEvent) -> DialogAction {
        if let Some(selected) = self.preset_selection {
            return match shortcuts::resolve(Context::Menu, key, false) {
                Some(Command::Back) => {
                    self.preset_selection = None;
                    DialogAction::None
                }
                Some(Command::Up) => {
                    self.preset_selection =
                        Some(move_index(selected, -1, HEADER_PRESETS.len() + 1));
                    DialogAction::None
                }
                Some(Command::Down) => {
                    self.preset_selection = Some(move_index(selected, 1, HEADER_PRESETS.len() + 1));
                    DialogAction::None
                }
                Some(Command::Activate) => {
                    self.add_preset(selected);
                    DialogAction::Changed
                }
                _ => DialogAction::None,
            };
        }
        if let Some(editor) = &mut self.table.editor {
            return match editor.handle_key(key) {
                EditAction::Continue => DialogAction::None,
                EditAction::Confirm => {
                    self.commit_editor();
                    DialogAction::Changed
                }
                EditAction::Cancel => {
                    self.table.editor = None;
                    DialogAction::None
                }
            };
        }

        match shortcuts::resolve(Context::Headers, key, false) {
            Some(Command::Back) => DialogAction::Cancel,
            Some(Command::Up) => {
                move_row_selection(&mut self.table, &self.rows, -1);
                DialogAction::None
            }
            Some(Command::Down) => {
                move_row_selection(&mut self.table, &self.rows, 1);
                DialogAction::None
            }
            Some(Command::Left) => {
                self.table.field = KeyValueField::Name;
                DialogAction::None
            }
            Some(Command::Right) => {
                self.table.field = KeyValueField::Value;
                DialogAction::None
            }
            Some(Command::Delete) => DialogAction::RemoveRow {
                tab: PreviewTab::Headers,
                index: self.table.selected,
            },
            Some(Command::Activate) => {
                start_cell_edit(&mut self.table, &self.rows);
                DialogAction::None
            }
            Some(Command::Toggle) => {
                self.toggle_selected();
                DialogAction::Changed
            }
            Some(Command::Add) => {
                self.preset_selection = Some(0);
                DialogAction::None
            }
            _ => DialogAction::None,
        }
    }

    fn commit_editor(&mut self) {
        let Some(editor) = self.table.editor.take() else {
            return;
        };
        let Some(row) = self.rows.get_mut(self.table.selected) else {
            return;
        };
        let value = editor.confirmed_value();
        let changed = match self.table.field {
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
        self.table.selected = self.rows.len().saturating_sub(1);
        self.table.field = KeyValueField::Name;
        self.table.editor = Some(EditInput::new(String::new()));
    }

    fn add_preset(&mut self, selected: usize) {
        self.preset_selection = None;
        let Some((name, value)) = HEADER_PRESETS.get(selected).copied() else {
            self.add_row();
            return;
        };
        if let Some(index) = self
            .rows
            .iter()
            .position(|row| row.name.eq_ignore_ascii_case(name))
        {
            self.table.selected = index;
            self.table.field = KeyValueField::Value;
            self.table.editor = Some(EditInput::new(self.rows[index].value.clone()));
            return;
        }
        self.rows.push(HeaderRow {
            name: name.to_string(),
            value: value.to_string(),
            enabled: true,
            source: HeaderSource::Request,
        });
        self.table.selected = self.rows.len() - 1;
        self.table.field = KeyValueField::Value;
        self.table.editor = Some(EditInput::new(value.to_string()));
    }

    pub(super) fn open_preset_menu(&mut self) {
        self.table.editor = None;
        self.preset_selection = Some(0);
    }

    pub(super) fn remove_row(&mut self, index: usize) -> Option<HeaderRow> {
        if index >= self.rows.len() {
            return None;
        }
        self.table.editor = None;
        let removed = self.rows.remove(index);
        self.table.selected = index.min(self.rows.len().saturating_sub(1));
        Some(removed)
    }

    pub(super) fn toggle_selected(&mut self) {
        if let Some(row) = self.rows.get_mut(self.table.selected) {
            if row.source == HeaderSource::Request {
                row.enabled = !row.enabled;
            }
        }
    }
}

impl ParamsDialog {
    fn handle_key(&mut self, key: KeyEvent) -> DialogAction {
        if let Some(editor) = &mut self.table.editor {
            return match editor.handle_key(key) {
                EditAction::Continue => DialogAction::None,
                EditAction::Confirm => {
                    self.commit_editor();
                    DialogAction::Changed
                }
                EditAction::Cancel => {
                    self.table.editor = None;
                    DialogAction::None
                }
            };
        }

        match shortcuts::resolve(Context::Params, key, false) {
            Some(Command::Back) => DialogAction::Cancel,
            Some(Command::Up) => {
                move_row_selection(&mut self.table, &self.rows, -1);
                DialogAction::None
            }
            Some(Command::Down) => {
                move_row_selection(&mut self.table, &self.rows, 1);
                DialogAction::None
            }
            Some(Command::Left) => {
                self.table.field = KeyValueField::Name;
                DialogAction::None
            }
            Some(Command::Right) => {
                self.table.field = KeyValueField::Value;
                DialogAction::None
            }
            Some(Command::Delete) => DialogAction::RemoveRow {
                tab: PreviewTab::Params,
                index: self.table.selected,
            },
            Some(Command::Activate) => {
                start_cell_edit(&mut self.table, &self.rows);
                DialogAction::None
            }
            Some(Command::Add) => {
                self.add_row();
                DialogAction::None
            }
            _ => DialogAction::None,
        }
    }

    fn commit_editor(&mut self) {
        let Some(editor) = self.table.editor.take() else {
            return;
        };
        let Some(row) = self.rows.get_mut(self.table.selected) else {
            return;
        };
        let value = editor.confirmed_value();
        match self.table.field {
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
        self.table.selected = self.rows.len().saturating_sub(1);
        self.table.field = KeyValueField::Name;
        self.table.editor = Some(EditInput::new(String::new()));
    }

    pub(super) fn remove_row(&mut self, index: usize) {
        if index >= self.rows.len() {
            return;
        }
        self.table.editor = None;
        self.rows.remove(index);
        self.table.selected = index.min(self.rows.len().saturating_sub(1));
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

    pub(crate) fn table_row_count(&self) -> Option<usize> {
        match self {
            Self::Configurations(_) => None,
            Self::Headers(dialog) => Some(dialog.rows.len()),
            Self::Params(dialog) => Some(dialog.rows.len()),
        }
    }

    pub(crate) fn table(&self) -> Option<&InlineTable> {
        match self {
            Self::Configurations(_) => None,
            Self::Headers(dialog) => Some(&dialog.table),
            Self::Params(dialog) => Some(&dialog.table),
        }
    }

    pub(crate) fn table_mut(&mut self) -> Option<&mut InlineTable> {
        match self {
            Self::Configurations(_) => None,
            Self::Headers(dialog) => Some(&mut dialog.table),
            Self::Params(dialog) => Some(&mut dialog.table),
        }
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> DialogAction {
        match self {
            Self::Configurations(dialog) => dialog.handle_key(key),
            Self::Headers(dialog) => dialog.handle_key(key),
            Self::Params(dialog) => dialog.handle_key(key),
        }
    }

    pub(super) fn click_row(
        &mut self,
        tab: PreviewTab,
        index: usize,
        field: KeyValueField,
        edit: bool,
        cursor: Option<usize>,
    ) {
        match (tab, self) {
            (PreviewTab::Headers, Self::Headers(dialog)) => {
                click_cell(&mut dialog.table, &dialog.rows, index, field, edit, cursor);
            }
            (PreviewTab::Params, Self::Params(dialog)) => {
                click_cell(&mut dialog.table, &dialog.rows, index, field, edit, cursor);
            }
            _ => {}
        }
    }

    pub(super) fn cancel_editor(&mut self) {
        if let Self::Headers(dialog) = self {
            dialog.preset_selection = None;
        }
        if let Some(table) = self.table_mut() {
            table.editor = None;
        }
    }

    pub(super) fn is_editing(&self) -> bool {
        self.table().is_some_and(|table| table.editor.is_some())
    }

    pub(super) fn paste(&mut self, value: &str) -> bool {
        self.table_mut()
            .and_then(|table| table.editor.as_mut())
            .is_some_and(|editor| editor.paste(value))
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
