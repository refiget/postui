use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::settings::UiTheme;

use super::LineCursor;
use super::{CHECKPOINT_LINE_INTERVAL, PlainIndex};

#[derive(Clone, Copy, Default)]
enum MarkupState {
    #[default]
    Text,
    Open,
    Declaration(u8),
    Tag {
        expect_name: bool,
        closing: bool,
        raw: RawKind,
    },
    Quote {
        quote: u8,
        closing: bool,
        raw: RawKind,
    },
    Comment(u8),
    Cdata(u8),
    Processing(u8),
    Raw(RawKind),
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum RawKind {
    #[default]
    None,
    Script,
    Style,
}

pub(super) struct MarkupIndex {
    lines: PlainIndex,
    states: Vec<MarkupState>,
}

impl MarkupIndex {
    pub(super) fn new(body: &[u8]) -> Self {
        let started = std::time::Instant::now();
        let mut state = MarkupState::Text;
        let mut cursor = LineCursor::default();
        let mut line_checkpoints = vec![cursor.clone()];
        let mut states = vec![state];
        let mut line_count = 0;
        while let Some(range) = cursor.next(body) {
            scan(&body[range], &mut state, None);
            line_count += 1;
            if line_count % CHECKPOINT_LINE_INTERVAL == 0 {
                line_checkpoints.push(cursor.clone());
                states.push(state);
            }
        }
        let lines = PlainIndex {
            checkpoints: line_checkpoints,
            line_count,
        };
        tracing::debug!(target: "postui::perf", bytes = body.len(), lines = lines.line_count,
            checkpoints = states.len(), build_us = started.elapsed().as_micros() as u64,
            "markup_index");
        Self { lines, states }
    }

    pub(super) fn line_count(&self) -> usize {
        self.lines.line_count
    }

    pub(super) fn ranges(
        &self,
        body: &[u8],
        offset: usize,
        count: usize,
    ) -> Vec<std::ops::Range<usize>> {
        self.lines.ranges(body, offset, count)
    }

    pub(super) fn visible_lines(
        &self,
        body: &[u8],
        offset: usize,
        count: usize,
        theme: &UiTheme,
    ) -> Vec<Line<'static>> {
        let checkpoint_line = offset / CHECKPOINT_LINE_INTERVAL * CHECKPOINT_LINE_INTERVAL;
        let mut state = self.states[offset / CHECKPOINT_LINE_INTERVAL];
        for range in self
            .lines
            .ranges(body, checkpoint_line, offset - checkpoint_line)
        {
            scan(&body[range], &mut state, None);
        }
        self.lines
            .ranges(body, offset, count)
            .into_iter()
            .map(|range| {
                let mut tokens = Vec::new();
                scan(&body[range.clone()], &mut state, Some(&mut tokens));
                markup_line(body, range.start, range.end, tokens, theme)
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
enum TokenKind {
    Punctuation,
    Name,
    Attribute,
    String,
    Comment,
    Entity,
    Raw,
}

fn scan(
    bytes: &[u8],
    state: &mut MarkupState,
    mut tokens: Option<&mut Vec<(usize, usize, TokenKind)>>,
) {
    let mut offset = 0;
    while offset < bytes.len() {
        let start = offset;
        match *state {
            MarkupState::Text => {
                if bytes[offset] == b'&' {
                    offset = consume_until(bytes, offset + 1, b';');
                    push(&mut tokens, start, offset, TokenKind::Entity);
                } else if bytes[offset] == b'<' {
                    *state = MarkupState::Open;
                    offset += 1;
                    push(&mut tokens, start, offset, TokenKind::Punctuation);
                } else {
                    offset += 1;
                }
            }
            MarkupState::Open => match bytes[offset] {
                b'/' => {
                    offset += 1;
                    *state = MarkupState::Tag {
                        expect_name: true,
                        closing: true,
                        raw: RawKind::None,
                    };
                    push(&mut tokens, start, offset, TokenKind::Punctuation);
                }
                b'?' => {
                    offset += 1;
                    *state = MarkupState::Processing(0);
                    push(&mut tokens, start, offset, TokenKind::Punctuation);
                }
                b'!' => {
                    offset += 1;
                    *state = MarkupState::Declaration(0);
                    push(&mut tokens, start, offset, TokenKind::Punctuation);
                }
                _ => {
                    *state = MarkupState::Tag {
                        expect_name: true,
                        closing: false,
                        raw: RawKind::None,
                    };
                }
            },
            MarkupState::Declaration(progress) => {
                const COMMENT: &[u8] = b"--";
                const CDATA: &[u8] = b"[CDATA[";
                let expected = if progress < 2 { COMMENT } else { CDATA };
                let index = if progress < 2 {
                    usize::from(progress)
                } else {
                    usize::from(progress - 2)
                };
                if expected.get(index) == Some(&bytes[offset]) {
                    offset += 1;
                    let progress = progress + 1;
                    *state = if progress == 2 {
                        MarkupState::Comment(0)
                    } else if progress == 9 {
                        MarkupState::Cdata(0)
                    } else {
                        MarkupState::Declaration(progress)
                    };
                    push(
                        &mut tokens,
                        start,
                        offset,
                        if progress <= 2 {
                            TokenKind::Comment
                        } else {
                            TokenKind::Punctuation
                        },
                    );
                } else if progress == 0 && bytes[offset] == b'[' {
                    offset += 1;
                    *state = MarkupState::Declaration(3);
                    push(&mut tokens, start, offset, TokenKind::Punctuation);
                } else {
                    *state = MarkupState::Tag {
                        expect_name: true,
                        closing: false,
                        raw: RawKind::None,
                    };
                }
            }
            MarkupState::Tag {
                expect_name,
                closing,
                raw,
            } => {
                let byte = bytes[offset];
                if byte == b'>' {
                    offset += 1;
                    push(&mut tokens, start, offset, TokenKind::Punctuation);
                    *state = if !closing && raw != RawKind::None {
                        MarkupState::Raw(raw)
                    } else {
                        MarkupState::Text
                    };
                } else if matches!(byte, b'\'' | b'"') {
                    *state = MarkupState::Quote {
                        quote: byte,
                        closing,
                        raw,
                    };
                    offset += 1;
                    push(&mut tokens, start, offset, TokenKind::String);
                } else if is_name(byte) {
                    offset += 1;
                    while offset < bytes.len() && is_name(bytes[offset]) {
                        offset += 1;
                    }
                    let kind = if expect_name {
                        TokenKind::Name
                    } else {
                        TokenKind::Attribute
                    };
                    push(&mut tokens, start, offset, kind);
                    let detected = if expect_name
                        && bytes[start..offset].eq_ignore_ascii_case(b"script")
                    {
                        RawKind::Script
                    } else if expect_name && bytes[start..offset].eq_ignore_ascii_case(b"style") {
                        RawKind::Style
                    } else {
                        raw
                    };
                    *state = MarkupState::Tag {
                        expect_name: false,
                        closing,
                        raw: detected,
                    };
                } else {
                    offset += 1;
                    if !byte.is_ascii_whitespace() {
                        push(&mut tokens, start, offset, TokenKind::Punctuation);
                    }
                }
            }
            MarkupState::Quote {
                quote,
                closing,
                raw,
            } => {
                offset += 1;
                while offset < bytes.len() && bytes[offset] != quote {
                    offset += 1;
                }
                if offset < bytes.len() {
                    offset += 1;
                    *state = MarkupState::Tag {
                        expect_name: false,
                        closing,
                        raw,
                    };
                }
                push(&mut tokens, start, offset, TokenKind::String);
            }
            MarkupState::Comment(matched) => {
                let (end, next) = consume_sequence(bytes, offset, b"-->", matched);
                offset = end;
                *state = if next == 3 {
                    MarkupState::Text
                } else {
                    MarkupState::Comment(next)
                };
                push(&mut tokens, start, offset, TokenKind::Comment);
            }
            MarkupState::Cdata(matched) => {
                let (end, next) = consume_sequence(bytes, offset, b"]]>", matched);
                offset = end;
                *state = if next == 3 {
                    MarkupState::Text
                } else {
                    MarkupState::Cdata(next)
                };
                push(&mut tokens, start, offset, TokenKind::Raw);
            }
            MarkupState::Processing(matched) => {
                let (end, next) = consume_sequence(bytes, offset, b"?>", matched);
                offset = end;
                *state = if next == 2 {
                    MarkupState::Text
                } else {
                    MarkupState::Processing(next)
                };
                push(&mut tokens, start, offset, TokenKind::Attribute);
            }
            MarkupState::Raw(raw) => {
                let name = if raw == RawKind::Script {
                    b"script".as_slice()
                } else {
                    b"style".as_slice()
                };
                if bytes[offset] == b'<'
                    && bytes.get(offset + 1) == Some(&b'/')
                    && bytes
                        .get(offset + 2..offset + 2 + name.len())
                        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
                {
                    *state = MarkupState::Tag {
                        expect_name: true,
                        closing: true,
                        raw: RawKind::None,
                    };
                } else {
                    offset += 1;
                    push(&mut tokens, start, offset, TokenKind::Raw);
                }
            }
        }
    }
}

fn consume_until(bytes: &[u8], mut offset: usize, terminal: u8) -> usize {
    while offset < bytes.len() {
        let done = bytes[offset] == terminal;
        offset += 1;
        if done {
            break;
        }
    }
    offset
}

fn consume_sequence(
    bytes: &[u8],
    mut offset: usize,
    sequence: &[u8],
    mut matched: u8,
) -> (usize, u8) {
    while offset < bytes.len() && usize::from(matched) < sequence.len() {
        matched = if bytes[offset] == sequence[usize::from(matched)] {
            matched + 1
        } else {
            u8::from(bytes[offset] == sequence[0])
        };
        offset += 1;
    }
    (offset, matched)
}

fn push(
    tokens: &mut Option<&mut Vec<(usize, usize, TokenKind)>>,
    start: usize,
    end: usize,
    kind: TokenKind,
) {
    if start < end {
        if let Some(tokens) = tokens {
            if let Some(last) = tokens.last_mut() {
                if last.1 == start
                    && std::mem::discriminant(&last.2) == std::mem::discriminant(&kind)
                {
                    last.1 = end;
                    return;
                }
            }
            tokens.push((start, end, kind));
        }
    }
}

fn markup_line(
    body: &[u8],
    line_start: usize,
    line_end: usize,
    tokens: Vec<(usize, usize, TokenKind)>,
    theme: &UiTheme,
) -> Line<'static> {
    let mut spans = Vec::with_capacity(tokens.len() * 2 + 1);
    let mut position = 0;
    for (start, end, kind) in tokens {
        if position < start {
            spans.push(Span::styled(
                String::from_utf8_lossy(&body[line_start + position..line_start + start])
                    .into_owned(),
                Style::default().fg(theme.text),
            ));
        }
        let style = match kind {
            TokenKind::Punctuation => Style::default().fg(theme.muted),
            TokenKind::Name => Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
            TokenKind::Attribute => Style::default().fg(theme.warning),
            TokenKind::String => Style::default().fg(theme.success),
            TokenKind::Comment => Style::default().fg(theme.muted),
            TokenKind::Entity => Style::default().fg(theme.secondary),
            TokenKind::Raw => Style::default().fg(theme.text),
        };
        spans.push(Span::styled(
            String::from_utf8_lossy(&body[line_start + start..line_start + end]).into_owned(),
            style,
        ));
        position = end;
    }
    if position < line_end - line_start {
        spans.push(Span::styled(
            String::from_utf8_lossy(&body[line_start + position..line_end]).into_owned(),
            Style::default().fg(theme.text),
        ));
    }
    Line::from(spans)
}

fn is_name(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':' | b'.')
}
