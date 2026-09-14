use std::{env, ffi::OsString, ops::Range, path::Path, process::Command as ProcessCommand};

use crate::shortcuts::{self, Command, Context};
use anyhow::{Context as _, Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Line;
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn open_file(path: &Path) -> Result<()> {
    if let Some(editor) = configured_editor("VISUAL").or_else(|| configured_editor("EDITOR")) {
        return run_editor(editor, &[], path);
    }

    #[cfg(windows)]
    {
        return run_editor(OsString::from("notepad.exe"), &[], path);
    }

    #[cfg(target_os = "macos")]
    {
        return run_editor(OsString::from("open"), &["-W", "-t"], path);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run_editor(OsString::from("vi"), &[], path)
    }

    #[cfg(not(any(windows, target_os = "macos", unix)))]
    {
        bail!("No default editor is available")
    }
}

fn configured_editor(name: &str) -> Option<OsString> {
    env::var_os(name).filter(|value| !value.to_string_lossy().trim().is_empty())
}

fn run_editor(editor: OsString, arguments: &[&str], path: &Path) -> Result<()> {
    let status = ProcessCommand::new(&editor)
        .args(arguments)
        .arg(path)
        .status()
        .with_context(|| format!("Could not start editor: {}", editor.to_string_lossy()))?;
    if !status.success() {
        bail!("Editor exited with status {status}")
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct EditInput {
    value: String,
    cursor: usize,
    mode: EditMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditMode {
    Replace,
    Insert,
}

impl EditInput {
    pub(crate) fn new(value: String) -> Self {
        let cursor = value.len();
        Self {
            value,
            cursor,
            mode: EditMode::Replace,
        }
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }

    pub(crate) fn cursor_byte(&self) -> usize {
        self.cursor
    }

    pub(crate) fn cursor_width(&self) -> usize {
        terminal_width(&self.value[..self.cursor])
    }

    pub(crate) fn mode(&self) -> EditMode {
        self.mode
    }

    pub(crate) fn place_cursor(&mut self, column: usize) {
        self.cursor = text_position(&self.value, 0, column);
        self.mode = EditMode::Insert;
    }

    pub(crate) fn confirmed_value(self) -> String {
        self.value
    }

    fn insert(&mut self, character: char) {
        if character.is_control() {
            return;
        }
        if self.mode == EditMode::Replace {
            self.value.clear();
            self.cursor = 0;
            self.mode = EditMode::Insert;
        }
        self.value.insert(self.cursor, character);
        self.cursor += character.len_utf8();
    }

    fn backspace(&mut self) {
        if self.clear_selection() {
            return;
        }
        let Some(previous) = previous_grapheme(&self.value, self.cursor) else {
            return;
        };
        self.value.drain(previous..self.cursor);
        self.cursor = previous;
    }

    fn delete(&mut self) {
        if self.clear_selection() {
            return;
        }
        let Some(next) = next_grapheme(&self.value, self.cursor) else {
            return;
        };
        self.value.drain(self.cursor..next);
    }

    fn move_left(&mut self) {
        if self.mode == EditMode::Replace {
            self.cursor = 0;
            self.mode = EditMode::Insert;
            return;
        }
        if let Some(previous) = previous_grapheme(&self.value, self.cursor) {
            self.cursor = previous;
        }
    }

    fn move_right(&mut self) {
        if self.mode == EditMode::Replace {
            self.cursor = self.value.len();
            self.mode = EditMode::Insert;
            return;
        }
        if let Some(next) = next_grapheme(&self.value, self.cursor) {
            self.cursor = next;
        }
    }

    fn move_word_left(&mut self) {
        if self.mode == EditMode::Replace {
            self.cursor = 0;
            self.mode = EditMode::Insert;
            return;
        }
        self.cursor = previous_word(&self.value, self.cursor);
    }

    fn move_word_right(&mut self) {
        if self.mode == EditMode::Replace {
            self.cursor = self.value.len();
            self.mode = EditMode::Insert;
            return;
        }
        self.cursor = next_word(&self.value, self.cursor);
    }

    fn delete_word_left(&mut self) {
        if self.clear_selection() {
            return;
        }
        let previous = previous_word(&self.value, self.cursor);
        self.value.drain(previous..self.cursor);
        self.cursor = previous;
    }

    fn delete_word_right(&mut self) {
        if self.clear_selection() {
            return;
        }
        let next = next_word(&self.value, self.cursor);
        self.value.drain(self.cursor..next);
    }

    fn delete_line(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    fn clear_selection(&mut self) -> bool {
        if self.mode != EditMode::Replace {
            return false;
        }
        self.delete_line();
        self.mode = EditMode::Insert;
        true
    }

    fn delete_to_end(&mut self) {
        if self.clear_selection() {
            return;
        }
        self.value.truncate(self.cursor);
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> EditAction {
        let key = shortcuts::normalize(key);
        match shortcuts::resolve(Context::Editor, key, false) {
            Some(Command::SelectAll) => self.mode = EditMode::Replace,
            Some(Command::Clear) => self.delete_line(),
            Some(Command::WordLeft) => self.move_word_left(),
            Some(Command::WordRight) => self.move_word_right(),
            Some(Command::DeleteWordRight) => self.delete_word_right(),
            Some(Command::DeleteWordLeft) => self.delete_word_left(),
            Some(Command::DeleteToEnd) => self.delete_to_end(),
            Some(Command::Backspace) => self.backspace(),
            Some(Command::Delete) => self.delete(),
            Some(Command::Left) => self.move_left(),
            Some(Command::Right) => self.move_right(),
            Some(Command::Home) => {
                self.cursor = 0;
                self.mode = EditMode::Insert;
            }
            Some(Command::End) => {
                self.cursor = self.value.len();
                self.mode = EditMode::Insert;
            }
            Some(Command::Confirm) => return EditAction::Confirm,
            Some(Command::Back) => return EditAction::Cancel,
            None if key.kind != crossterm::event::KeyEventKind::Release
                && key.modifiers == KeyModifiers::NONE =>
            {
                if let KeyCode::Char(character) = key.code {
                    self.insert(character);
                }
            }
            _ => {}
        }
        EditAction::Continue
    }
}

fn previous_grapheme(value: &str, cursor: usize) -> Option<usize> {
    value[..cursor]
        .grapheme_indices(true)
        .next_back()
        .map(|(index, _)| index)
}

fn next_grapheme(value: &str, cursor: usize) -> Option<usize> {
    value[cursor..]
        .grapheme_indices(true)
        .nth(1)
        .map(|(index, _)| cursor + index)
}

fn is_word(value: &str) -> bool {
    value
        .chars()
        .any(|character| !character.is_whitespace() && !character.is_ascii_punctuation())
}

fn previous_word(value: &str, cursor: usize) -> usize {
    value
        .split_word_bound_indices()
        .filter(|(index, _)| *index < cursor)
        .rev()
        .find(|(_, word)| is_word(word))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_word(value: &str, cursor: usize) -> usize {
    value
        .split_word_bound_indices()
        .filter(|(index, _)| *index > cursor)
        .find(|(_, word)| is_word(word))
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditAction {
    Continue,
    Confirm,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsonScalarKind {
    String,
    Number,
    Boolean,
    Null,
}

#[derive(Debug, Clone)]
pub(crate) struct BodyValueEditor {
    pub(crate) document: String,
    pub(crate) span: Range<usize>,
    pub(crate) kind: JsonScalarKind,
    pub(crate) input: EditInput,
}

impl BodyValueEditor {
    pub(crate) fn display_document(&self) -> String {
        let mut value = self.document.clone();
        value.replace_range(self.span.clone(), self.input.value());
        value
    }

    pub(crate) fn position(&self) -> (usize, usize) {
        let before = &self.document[..self.span.start];
        let line = before.bytes().filter(|byte| *byte == b'\n').count();
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        (line, terminal_width(&before[line_start..]))
    }
}

pub(crate) fn terminal_width(value: &str) -> usize {
    Line::from(value).width()
}

pub(crate) fn text_position(value: &str, line: usize, column: usize) -> usize {
    let line_start = value
        .split_inclusive('\n')
        .take(line)
        .map(str::len)
        .sum::<usize>()
        .min(value.len());
    let line_end = value[line_start..]
        .find('\n')
        .map_or(value.len(), |index| line_start + index);
    let line = &value[line_start..line_end];
    let mut width = 0usize;
    for (index, character) in line.char_indices() {
        let mut encoded = [0; 4];
        let character_width = terminal_width(character.encode_utf8(&mut encoded));
        if width.saturating_add(character_width) > column {
            return line_start + index;
        }
        width = width.saturating_add(character_width);
    }
    line_end
}

pub(crate) fn json_scalar_at(
    document: &str,
    offset: usize,
) -> Option<(Range<usize>, JsonScalarKind, String)> {
    let mut found = None;
    visit_json_scalars(document, |span, kind| {
        if span.contains(&offset) {
            found = Some((span, kind));
            false
        } else {
            true
        }
    });
    let (span, kind) = found?;
    let token = &document[span.clone()];
    let input = match kind {
        JsonScalarKind::String => serde_json::from_str::<String>(token).ok()?,
        _ => token.to_string(),
    };
    Some((span, kind, input))
}

pub(crate) fn json_scalar_ranges(document: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    visit_json_scalars(document, |span, _| {
        ranges.push(span);
        true
    });
    ranges
}

fn visit_json_scalars(document: &str, mut visit: impl FnMut(Range<usize>, JsonScalarKind) -> bool) {
    let bytes = document.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let (span, kind) = match bytes[index] {
            b'"' => {
                let start = index;
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index = (index + 2).min(bytes.len()),
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
                let span = start..index;
                if document[index..].trim_start().starts_with(':') {
                    continue;
                }
                (span, JsonScalarKind::String)
            }
            b'-' | b'0'..=b'9' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && matches!(bytes[index], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    index += 1;
                }
                (start..index, JsonScalarKind::Number)
            }
            _ if document[index..].starts_with("true") => {
                index += 4;
                (index - 4..index, JsonScalarKind::Boolean)
            }
            _ if document[index..].starts_with("false") => {
                index += 5;
                (index - 5..index, JsonScalarKind::Boolean)
            }
            _ if document[index..].starts_with("null") => {
                index += 4;
                (index - 4..index, JsonScalarKind::Null)
            }
            _ => {
                index += 1;
                continue;
            }
        };
        if !visit(span, kind) {
            return;
        }
    }
}

pub(crate) fn convert_json_scalar(kind: JsonScalarKind, input: &str) -> Option<String> {
    match kind {
        JsonScalarKind::String => serde_json::to_string(input).ok(),
        JsonScalarKind::Number => serde_json::from_str::<serde_json::Number>(input)
            .ok()
            .map(|number| number.to_string()),
        JsonScalarKind::Boolean => input.parse::<bool>().ok().map(|value| value.to_string()),
        JsonScalarKind::Null => (input == "null").then(|| "null".to_string()),
    }
}

pub(crate) fn merge_json_edit(
    source_document: &str,
    rendered_document: &str,
    edited_document: &str,
) -> Option<String> {
    let source = serde_json::from_str(source_document).ok()?;
    let rendered = serde_json::from_str(rendered_document).ok()?;
    let edited = serde_json::from_str(edited_document).ok()?;
    serde_json::to_string_pretty(&merge_json_value(&source, &rendered, &edited)).ok()
}

fn merge_json_value(
    source: &serde_json::Value,
    rendered: &serde_json::Value,
    edited: &serde_json::Value,
) -> serde_json::Value {
    if rendered == edited {
        return source.clone();
    }

    match (source, rendered, edited) {
        (
            serde_json::Value::Object(source),
            serde_json::Value::Object(rendered),
            serde_json::Value::Object(edited),
        ) => serde_json::Value::Object(
            edited
                .iter()
                .map(|(name, edited_value)| {
                    let value = match (source.get(name), rendered.get(name)) {
                        (Some(source_value), Some(rendered_value)) => {
                            merge_json_value(source_value, rendered_value, edited_value)
                        }
                        _ => edited_value.clone(),
                    };
                    (name.clone(), value)
                })
                .collect(),
        ),
        (
            serde_json::Value::Array(source),
            serde_json::Value::Array(rendered),
            serde_json::Value::Array(edited),
        ) if source.len() == rendered.len() && rendered.len() == edited.len() => {
            serde_json::Value::Array(
                source
                    .iter()
                    .zip(rendered)
                    .zip(edited)
                    .map(|((source, rendered), edited)| merge_json_value(source, rendered, edited))
                    .collect(),
            )
        }
        _ => edited.clone(),
    }
}
