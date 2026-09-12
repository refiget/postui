use std::time::Instant;

use bytes::Bytes;
use memchr::memchr;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::settings::UiTheme;

const CHECKPOINT_LINE_INTERVAL: usize = 256;
const CHECKPOINT_BYTE_INTERVAL: usize = 64 * 1024;
const MAX_LINE_BYTES: usize = 512;
const MAX_INDENT_BYTES: usize = 64;

pub(crate) struct ResponseDocument {
    body: Bytes,
    displayed_bytes: usize,
    limited: bool,
    index: DocumentIndex,
}

impl std::fmt::Debug for ResponseDocument {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResponseDocument")
            .field("body_bytes", &self.body.len())
            .field("displayed_bytes", &self.displayed_bytes)
            .field("limited", &self.limited)
            .field("line_count", &self.line_count())
            .finish()
    }
}

impl ResponseDocument {
    pub(crate) fn new(body: Bytes, headers: &[(String, String)], max_display_bytes: usize) -> Self {
        let started = Instant::now();
        let displayed_bytes = body.len().min(max_display_bytes.max(1));
        let limited = displayed_bytes < body.len();
        let visible = &body[..displayed_bytes];
        let index = if is_json_response(headers, visible) {
            DocumentIndex::Json(build_json_index(visible))
        } else {
            DocumentIndex::Plain(build_plain_index(visible))
        };

        tracing::debug!(
            body_bytes = body.len(),
            displayed_bytes,
            limited,
            line_count = index.line_count(),
            format = index.format_name(),
            index_elapsed_ms = started.elapsed().as_millis(),
            "响应文档索引完成"
        );

        Self {
            body,
            displayed_bytes,
            limited,
            index,
        }
    }

    pub(crate) fn displayed_bytes(&self) -> usize {
        self.displayed_bytes
    }

    pub(crate) fn limited(&self) -> bool {
        self.limited
    }

    pub(crate) fn line_count(&self) -> usize {
        self.index.line_count()
    }

    pub(crate) fn visible_lines(
        &self,
        offset: usize,
        count: usize,
        theme: &UiTheme,
    ) -> Vec<Line<'static>> {
        if count == 0 || offset >= self.line_count() {
            return Vec::new();
        }
        let body = &self.body[..self.displayed_bytes];
        match &self.index {
            DocumentIndex::Json(index) => json_lines(body, index, offset, count, theme),
            DocumentIndex::Plain(index) => plain_lines(body, index, offset, count, theme),
        }
    }
}

#[derive(Debug)]
enum DocumentIndex {
    Json(SparseIndex<JsonCursor>),
    Plain(SparseIndex<PlainCursor>),
}

impl DocumentIndex {
    fn line_count(&self) -> usize {
        match self {
            Self::Json(index) => index.line_count,
            Self::Plain(index) => index.line_count,
        }
    }

    fn format_name(&self) -> &'static str {
        match self {
            Self::Json(_) => "json",
            Self::Plain(_) => "plain",
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
struct PlainCursor {
    offset: usize,
    logical_end: Option<usize>,
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

fn build_plain_index(body: &[u8]) -> SparseIndex<PlainCursor> {
    let mut cursor = PlainCursor::default();
    let mut checkpoints = vec![Checkpoint {
        line: 0,
        cursor: cursor.clone(),
    }];
    let mut line_count = 0usize;
    let mut checkpoint_offset = 0usize;

    while scan_plain_line(body, &mut cursor, false).is_some() {
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
    theme: &UiTheme,
) -> Vec<Line<'static>> {
    let checkpoint = checkpoint_for(&index.checkpoints, offset);
    let mut cursor = checkpoint.cursor.clone();
    for _ in checkpoint.line..offset {
        if scan_json_line(body, &mut cursor, false).is_none() {
            return Vec::new();
        }
    }

    (0..count)
        .map_while(|_| scan_json_line(body, &mut cursor, true))
        .map(|line| line.into_line(theme))
        .collect()
}

fn plain_lines(
    body: &[u8],
    index: &SparseIndex<PlainCursor>,
    offset: usize,
    count: usize,
    theme: &UiTheme,
) -> Vec<Line<'static>> {
    let checkpoint = checkpoint_for(&index.checkpoints, offset);
    let mut cursor = checkpoint.cursor.clone();
    for _ in checkpoint.line..offset {
        if scan_plain_line(body, &mut cursor, false).is_none() {
            return Vec::new();
        }
    }

    (0..count)
        .map_while(|_| scan_plain_line(body, &mut cursor, true))
        .map(|line| line.into_line(theme))
        .collect()
}

fn checkpoint_for<C>(checkpoints: &[Checkpoint<C>], line: usize) -> &Checkpoint<C> {
    let index = checkpoints
        .partition_point(|checkpoint| checkpoint.line <= line)
        .saturating_sub(1);
    &checkpoints[index]
}

fn scan_plain_line(body: &[u8], cursor: &mut PlainCursor, render: bool) -> Option<LineBuilder> {
    if cursor.offset >= body.len() {
        return None;
    }

    let start = cursor.offset;
    let logical_end = cursor.logical_end.unwrap_or_else(|| {
        let end = memchr(b'\n', &body[start..])
            .map(|offset| start + offset)
            .unwrap_or(body.len());
        cursor.logical_end = Some(end);
        end
    });
    let mut end = start.saturating_add(MAX_LINE_BYTES).min(logical_end);
    end = utf8_chunk_end(body, start, end);
    if end == start && logical_end > start {
        end = start.saturating_add(1).min(logical_end);
    }

    let mut visible_end = end;
    if end == logical_end && visible_end > start && body[visible_end - 1] == b'\r' {
        visible_end -= 1;
    }
    let mut line = LineBuilder::new(render);
    line.push_slice(&body[start..visible_end], TokenKind::Plain);

    cursor.offset = if end == logical_end {
        cursor.logical_end = None;
        if logical_end < body.len() {
            logical_end.saturating_add(1)
        } else {
            logical_end
        }
    } else {
        end
    };
    Some(line)
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

fn utf8_chunk_end(body: &[u8], start: usize, end: usize) -> usize {
    if end >= body.len() {
        return end;
    }
    let mut boundary = end;
    while boundary > start && is_utf8_continuation(body[boundary]) {
        boundary -= 1;
    }
    boundary
}

fn is_json_response(headers: &[(String, String)], body: &[u8]) -> bool {
    let content_type_is_json = headers.iter().any(|(name, value)| {
        if !name.eq_ignore_ascii_case("content-type") {
            return false;
        }
        let media_type = value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        media_type == "application/json" || media_type.ends_with("+json")
    });
    content_type_is_json
        || body
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_some_and(|byte| matches!(byte, b'{' | b'['))
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
