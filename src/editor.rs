use std::ops::Range;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Line;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextEditor {
    pub(crate) value: String,
    pub(crate) cursor: usize,
}

impl TextEditor {
    pub(crate) fn new(value: String) -> Self {
        let cursor = value.len();
        Self { value, cursor }
    }

    fn insert(&mut self, character: char) {
        if character.is_control() {
            return;
        }
        self.value.insert(self.cursor, character);
        self.cursor += character.len_utf8();
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = self.value[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.value.drain(previous..self.cursor);
        self.cursor = previous;
    }

    fn delete(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        let next = self.value[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| self.cursor + index)
            .unwrap_or(self.value.len());
        self.value.drain(self.cursor..next);
    }

    fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.value[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
        }
    }

    fn move_right(&mut self) {
        if self.cursor < self.value.len() {
            self.cursor += self.value[self.cursor..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(0);
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> EditorAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('a') => self.cursor = 0,
                KeyCode::Char('e') => self.cursor = self.value.len(),
                KeyCode::Char('u') => {
                    self.value.clear();
                    self.cursor = 0;
                }
                _ => {}
            }
            return EditorAction::Continue;
        }

        match key.code {
            KeyCode::Char(character) => self.insert(character),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.value.len(),
            KeyCode::Enter | KeyCode::Tab => return EditorAction::Commit,
            KeyCode::Esc => return EditorAction::Cancel,
            _ => {}
        }
        EditorAction::Continue
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorAction {
    Continue,
    Commit,
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
    pub(crate) input: TextEditor,
}

impl BodyValueEditor {
    pub(crate) fn display_document(&self) -> String {
        let mut value = self.document.clone();
        value.replace_range(self.span.clone(), &self.input.value);
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
        if !span.contains(&offset) {
            continue;
        }
        let token = &document[span.clone()];
        let input = match kind {
            JsonScalarKind::String => serde_json::from_str::<String>(token).ok()?,
            _ => token.to_string(),
        };
        return Some((span, kind, input));
    }
    None
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
