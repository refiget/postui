use crate::settings::UiTheme;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

const CHECKPOINT_LINE_INTERVAL: usize = 256;
const CHECKPOINT_BYTE_INTERVAL: usize = 64 * 1024;
const MAX_LINE_BYTES: usize = 512;
const MAX_INDENT_BYTES: usize = 64;

pub(super) struct JsonIndex(SparseIndex<JsonCursor>);

impl JsonIndex {
    pub(super) fn new(body: &[u8]) -> Self {
        Self(build_json_index(body))
    }

    pub(super) fn line_count(&self) -> usize {
        self.0.line_count
    }

    pub(super) fn visible_lines(
        &self,
        body: &[u8],
        offset: usize,
        count: usize,
        theme: &UiTheme,
    ) -> Vec<Line<'static>> {
        json_lines(body, &self.0, offset, count)
            .into_iter()
            .map(|line| line.into_line(theme))
            .collect()
    }

    pub(super) fn find_in_batch(
        &self,
        body: &[u8],
        offset: usize,
        count: usize,
        query: &str,
        reverse: bool,
    ) -> Option<usize> {
        let lines = json_lines(body, &self.0, offset, count);
        let matches = |line: &LineBuilder| {
            line.parts
                .iter()
                .flatten()
                .map(|part| String::from_utf8_lossy(&part.bytes))
                .collect::<String>()
                .to_lowercase()
                .contains(query)
        };
        if reverse {
            lines.iter().rposition(matches)
        } else {
            lines.iter().position(matches)
        }
    }
}

#[derive(Debug)]
struct SparseIndex<C> {
    checkpoints: Vec<Checkpoint<C>>,
    line_count: usize,
}

#[derive(Debug)]
struct Checkpoint<C> {
    line: usize,
    cursor: C,
}

#[derive(Debug, Clone, Default)]
struct JsonCursor {
    offset: usize,
    depth: usize,
    mode: JsonMode,
}

#[derive(Debug, Clone, Copy, Default)]
enum JsonMode {
    #[default]
    Normal,
    String {
        kind: TokenKind,
        opening: bool,
        escaped: bool,
    },
    Atom(TokenKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Plain,
    Punctuation,
    Key,
    String,
    Number,
    Boolean,
    Null,
}

struct LineBuilder {
    parts: Option<Vec<LinePart>>,
    len: usize,
}

struct LinePart {
    kind: TokenKind,
    bytes: Vec<u8>,
}

impl LineBuilder {
    fn new(render: bool) -> Self {
        Self {
            parts: render.then(Vec::new),
            len: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn remaining(&self) -> usize {
        MAX_LINE_BYTES.saturating_sub(self.len)
    }

    fn push_byte(&mut self, byte: u8, kind: TokenKind) {
        self.push_slice(&[byte], kind);
    }

    fn push_slice(&mut self, value: &[u8], kind: TokenKind) {
        self.len = self.len.saturating_add(value.len());
        let Some(parts) = self.parts.as_mut() else {
            return;
        };
        if let Some(previous) = parts.last_mut() {
            if previous.kind == kind {
                previous.bytes.extend_from_slice(value);
                return;
            }
        }
        parts.push(LinePart {
            kind,
            bytes: value.to_vec(),
        });
    }

    fn push_spaces(&mut self, count: usize) {
        self.len = self.len.saturating_add(count);
        let Some(parts) = self.parts.as_mut() else {
            return;
        };
        if let Some(previous) = parts.last_mut() {
            if previous.kind == TokenKind::Plain {
                previous.bytes.resize(previous.bytes.len() + count, b' ');
                return;
            }
        }
        parts.push(LinePart {
            kind: TokenKind::Plain,
            bytes: vec![b' '; count],
        });
    }

    fn into_line(self, theme: &UiTheme) -> Line<'static> {
        let spans = self
            .parts
            .unwrap_or_default()
            .into_iter()
            .map(|part| {
                Span::styled(
                    String::from_utf8_lossy(&part.bytes).into_owned(),
                    token_style(part.kind, theme),
                )
            })
            .collect::<Vec<_>>();
        Line::from(spans)
    }
}

fn build_json_index(body: &[u8]) -> SparseIndex<JsonCursor> {
    let mut cursor = JsonCursor::default();
    let mut checkpoints = vec![Checkpoint {
        line: 0,
        cursor: cursor.clone(),
    }];
    let mut line_count = 0usize;
    let mut checkpoint_offset = 0usize;

    while scan_json_line(body, &mut cursor, false).is_some() {
        line_count = line_count.saturating_add(1);
        if line_count % CHECKPOINT_LINE_INTERVAL == 0
            || cursor.offset.saturating_sub(checkpoint_offset) >= CHECKPOINT_BYTE_INTERVAL
        {
            checkpoint_offset = cursor.offset;
            checkpoints.push(Checkpoint {
                line: line_count,
                cursor: cursor.clone(),
            });
        }
    }

    SparseIndex {
        checkpoints,
        line_count,
    }
}

fn json_lines(
    body: &[u8],
    index: &SparseIndex<JsonCursor>,
    offset: usize,
    count: usize,
) -> Vec<LineBuilder> {
    let checkpoint = checkpoint_for(&index.checkpoints, offset);
    let mut cursor = checkpoint.cursor.clone();
    for _ in checkpoint.line..offset {
        if scan_json_line(body, &mut cursor, false).is_none() {
            return Vec::new();
        }
    }

    (0..count)
        .map_while(|_| scan_json_line(body, &mut cursor, true))
        .collect()
}

fn checkpoint_for<C>(checkpoints: &[Checkpoint<C>], line: usize) -> &Checkpoint<C> {
    let index = checkpoints
        .partition_point(|checkpoint| checkpoint.line <= line)
        .saturating_sub(1);
    &checkpoints[index]
}

fn scan_json_line(body: &[u8], cursor: &mut JsonCursor, render: bool) -> Option<LineBuilder> {
    let mut line = LineBuilder::new(render);

    loop {
        if cursor.offset >= body.len() {
            cursor.mode = JsonMode::Normal;
            return (!line.is_empty()).then_some(line);
        }

        match cursor.mode {
            JsonMode::String {
                kind,
                mut opening,
                mut escaped,
            } => {
                ensure_indent(&mut line, cursor.depth);
                let start = cursor.offset;
                let available = line.remaining();
                while cursor.offset < body.len() {
                    let byte = body[cursor.offset];
                    cursor.offset += 1;

                    if opening {
                        opening = false;
                    } else if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == b'"' {
                        cursor.mode = JsonMode::Normal;
                        break;
                    }

                    if cursor.offset.saturating_sub(start) >= available
                        && (cursor.offset >= body.len()
                            || !is_utf8_continuation(body[cursor.offset]))
                    {
                        line.push_slice(&body[start..cursor.offset], kind);
                        cursor.mode = JsonMode::String {
                            kind,
                            opening,
                            escaped,
                        };
                        return Some(line);
                    }
                }
                line.push_slice(&body[start..cursor.offset], kind);

                if !matches!(cursor.mode, JsonMode::Normal) {
                    cursor.mode = JsonMode::String {
                        kind,
                        opening,
                        escaped,
                    };
                }
            }
            JsonMode::Atom(kind) => {
                ensure_indent(&mut line, cursor.depth);
                let start = cursor.offset;
                let available = line.remaining();
                while cursor.offset < body.len() {
                    let byte = body[cursor.offset];
                    if is_atom_delimiter(byte) {
                        cursor.mode = JsonMode::Normal;
                        break;
                    }
                    cursor.offset += 1;
                    if cursor.offset.saturating_sub(start) >= available
                        && (cursor.offset >= body.len()
                            || !is_utf8_continuation(body[cursor.offset]))
                    {
                        line.push_slice(&body[start..cursor.offset], kind);
                        return Some(line);
                    }
                }
                line.push_slice(&body[start..cursor.offset], kind);
            }
            JsonMode::Normal => {
                skip_json_whitespace(body, &mut cursor.offset);
                if cursor.offset >= body.len() {
                    return (!line.is_empty()).then_some(line);
                }
                let byte = body[cursor.offset];
                match byte {
                    b'{' | b'[' => {
                        ensure_indent(&mut line, cursor.depth);
                        line.push_byte(byte, TokenKind::Punctuation);
                        cursor.offset += 1;

                        let closing = if byte == b'{' { b'}' } else { b']' };
                        let mut next = cursor.offset;
                        skip_json_whitespace(body, &mut next);
                        if body.get(next) == Some(&closing) {
                            line.push_byte(closing, TokenKind::Punctuation);
                            cursor.offset = next + 1;
                            continue;
                        }

                        cursor.depth = cursor.depth.saturating_add(1);
                        return Some(line);
                    }
                    b'}' | b']' => {
                        if !line.is_empty() {
                            return Some(line);
                        }
                        cursor.depth = cursor.depth.saturating_sub(1);
                        ensure_indent(&mut line, cursor.depth);
                        line.push_byte(byte, TokenKind::Punctuation);
                        cursor.offset += 1;

                        let mut next = cursor.offset;
                        skip_json_whitespace(body, &mut next);
                        if body.get(next) == Some(&b',') {
                            line.push_byte(b',', TokenKind::Punctuation);
                            cursor.offset = next + 1;
                        }
                        return Some(line);
                    }
                    b',' => {
                        ensure_indent(&mut line, cursor.depth);
                        line.push_byte(byte, TokenKind::Punctuation);
                        cursor.offset += 1;
                        return Some(line);
                    }
                    b':' => {
                        ensure_indent(&mut line, cursor.depth);
                        line.push_slice(b": ", TokenKind::Punctuation);
                        cursor.offset += 1;
                    }
                    b'"' => {
                        let kind = json_string_kind(body, cursor.offset);
                        cursor.mode = JsonMode::String {
                            kind,
                            opening: true,
                            escaped: false,
                        };
                    }
                    _ => {
                        cursor.mode = JsonMode::Atom(atom_kind(byte));
                    }
                }
            }
        }

        if line.remaining() == 0
            && (cursor.offset >= body.len() || !is_utf8_continuation(body[cursor.offset]))
        {
            return Some(line);
        }
    }
}

fn ensure_indent(line: &mut LineBuilder, depth: usize) {
    if !line.is_empty() {
        return;
    }
    let indent = depth.saturating_mul(2).min(MAX_INDENT_BYTES);
    if indent > 0 {
        line.push_spaces(indent);
    }
}

fn skip_json_whitespace(body: &[u8], offset: &mut usize) {
    while body
        .get(*offset)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        *offset += 1;
    }
}

fn json_string_kind(body: &[u8], opening_quote: usize) -> TokenKind {
    let mut offset = opening_quote.saturating_add(1);
    let mut escaped = false;
    while let Some(&byte) = body.get(offset) {
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            offset += 1;
            skip_json_whitespace(body, &mut offset);
            return if body.get(offset) == Some(&b':') {
                TokenKind::Key
            } else {
                TokenKind::String
            };
        }
        offset += 1;
    }
    TokenKind::String
}

fn is_atom_delimiter(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b',' | b']' | b'}' | b':')
}

fn atom_kind(first: u8) -> TokenKind {
    match first {
        b't' | b'f' => TokenKind::Boolean,
        b'n' => TokenKind::Null,
        b'-' | b'0'..=b'9' => TokenKind::Number,
        _ => TokenKind::Plain,
    }
}

fn is_utf8_continuation(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

fn token_style(kind: TokenKind, theme: &UiTheme) -> Style {
    match kind {
        TokenKind::Plain => Style::default().fg(theme.text),
        TokenKind::Punctuation => Style::default().fg(theme.muted),
        TokenKind::Key => Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD),
        TokenKind::String => Style::default().fg(theme.success),
        TokenKind::Number => Style::default().fg(theme.warning),
        TokenKind::Boolean => Style::default()
            .fg(theme.secondary)
            .add_modifier(Modifier::BOLD),
        TokenKind::Null => Style::default().fg(theme.muted),
    }
}
