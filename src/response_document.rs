use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use bytes::Bytes;
use memchr::memchr;
use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::{
    highlight::ResponseHighlightCache,
    response_format::{BodyFormat, FormatNote},
    settings::UiTheme,
};

const CHECKPOINT_LINE_INTERVAL: usize = 256;
const MAX_LINE_BYTES: usize = 512;

mod json;
use json::JsonIndex;
mod markup;
use markup::MarkupIndex;

#[derive(Clone)]
pub struct ResponseDocument {
    body: Bytes,
    displayed_bytes: usize,
    total_bytes: usize,
    index: Arc<DocumentIndex>,
    highlight: Option<Arc<ResponseHighlightCache>>,
    raw: Option<Arc<ResponseDocument>>,
    pub format_note: Option<FormatNote>,
}

impl std::fmt::Debug for ResponseDocument {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResponseDocument")
            .field("body_bytes", &self.body.len())
            .field("displayed_bytes", &self.displayed_bytes)
            .field("line_count", &self.line_count())
            .finish()
    }
}

impl ResponseDocument {
    pub fn new(body: Bytes, headers: &[(String, String)], max_display_bytes: usize) -> Self {
        let display_limit = max_display_bytes.max(1);
        let displayed_bytes = body.len().min(display_limit);
        let format = BodyFormat::detect(headers, &body);
        let syntax = format.syntax(headers);
        let displayed = &body[..displayed_bytes];
        let markup = matches!(syntax, Some("xml" | "html"));
        let mut raw = Self {
            index: Arc::new(DocumentIndex::new(displayed, markup)),
            highlight: if format == BodyFormat::Json || markup {
                None
            } else {
                ResponseHighlightCache::new(body.slice(..displayed_bytes), syntax)
            },
            total_bytes: body.len(),
            body,
            displayed_bytes,
            raw: None,
            format_note: None,
        };
        if format == BodyFormat::Json {
            return Self {
                body: raw.body.clone(),
                displayed_bytes,
                total_bytes: raw.total_bytes,
                index: Arc::new(DocumentIndex::Json(JsonIndex::new(
                    &raw.body[..displayed_bytes],
                ))),
                highlight: None,
                raw: Some(Arc::new(raw)),
                format_note: None,
            };
        }
        let (body, total_bytes) = match format.format(&raw.body, display_limit) {
            Ok(body) => body,
            Err(note) => {
                raw.format_note = Some(note);
                return raw;
            }
        };
        let displayed_bytes = body.len().min(display_limit);
        let displayed = &body[..displayed_bytes];
        Self {
            total_bytes,
            displayed_bytes,
            index: Arc::new(DocumentIndex::new(displayed, markup)),
            highlight: if markup {
                None
            } else {
                ResponseHighlightCache::new(body.clone(), syntax)
            },
            body,
            raw: Some(Arc::new(raw)),
            format_note: None,
        }
    }

    pub fn raw(&self) -> &Self {
        self.raw.as_deref().unwrap_or(self)
    }

    pub fn displayed_bytes(&self) -> usize {
        self.displayed_bytes
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub fn limited(&self) -> bool {
        self.displayed_bytes < self.total_bytes
    }

    pub fn line_count(&self) -> usize {
        self.index.line_count()
    }

    pub fn visible_lines(
        &self,
        offset: usize,
        count: usize,
        theme: &UiTheme,
    ) -> Option<Vec<Line<'static>>> {
        if self.highlight.is_none() {
            crate::highlight::clear_response_highlight_focus();
        }
        if count == 0 || offset >= self.line_count() {
            return Some(Vec::new());
        }
        if let DocumentIndex::Json(index) = self.index.as_ref() {
            return Some(index.visible_lines(
                &self.body[..self.displayed_bytes],
                offset,
                count,
                theme,
            ));
        }
        if let DocumentIndex::Markup(index) = self.index.as_ref() {
            return Some(index.visible_lines(
                &self.body[..self.displayed_bytes],
                offset,
                count,
                theme,
            ));
        }
        let ranges = self.line_ranges(offset, count);
        let highlight = ranges
            .first()
            .zip(ranges.last())
            .and_then(|(first, last)| self.highlight.as_ref()?.page(first.start..last.end));
        if self.highlight.is_some() && highlight.is_none() {
            return None;
        }
        Some(
            ranges
                .into_iter()
                .map(|range| {
                    if let Some(highlight) = &highlight {
                        highlight.line(&self.body, range, theme)
                    } else {
                        Line::from(Span::styled(
                            String::from_utf8_lossy(&self.body[range]).into_owned(),
                            Style::default().fg(theme.text),
                        ))
                    }
                })
                .collect(),
        )
    }

    fn line_ranges(&self, offset: usize, count: usize) -> Vec<Range<usize>> {
        if count == 0 || offset >= self.line_count() {
            return Vec::new();
        }
        let body = &self.body[..self.displayed_bytes];
        self.index.ranges(body, offset, count)
    }

    pub fn find_line(
        &self,
        query: &str,
        start: usize,
        reverse: bool,
        cancelled: &AtomicBool,
    ) -> Option<usize> {
        let query = query.to_lowercase();
        let search_range = |range: Range<usize>| {
            let mut batches = (range.start..range.end).step_by(CHECKPOINT_LINE_INTERVAL);
            loop {
                if cancelled.load(Ordering::Relaxed) {
                    return None;
                }
                let offset = if reverse {
                    batches.next_back()
                } else {
                    batches.next()
                }?;
                if let DocumentIndex::Json(index) = self.index.as_ref() {
                    if let Some(found) = index.find_in_batch(
                        &self.body[..self.displayed_bytes],
                        offset,
                        CHECKPOINT_LINE_INTERVAL.min(range.end - offset),
                        &query,
                        reverse,
                    ) {
                        return Some(offset + found);
                    }
                    continue;
                }
                let lines =
                    self.line_ranges(offset, CHECKPOINT_LINE_INTERVAL.min(range.end - offset));
                let matches = |range: &Range<usize>| {
                    String::from_utf8_lossy(&self.body[range.clone()])
                        .to_lowercase()
                        .contains(&query)
                };
                let found = if reverse {
                    lines.iter().rposition(matches)
                } else {
                    lines.iter().position(matches)
                };
                if let Some(index) = found {
                    return Some(offset + index);
                }
            }
        };
        if reverse {
            let end = self.line_count().min(start.saturating_add(1));
            search_range(0..end).or_else(|| search_range(end..self.line_count()))
        } else {
            let start = start.min(self.line_count());
            search_range(start..self.line_count()).or_else(|| search_range(0..start))
        }
    }

    pub fn same_document(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.index, &other.index)
    }
}

enum DocumentIndex {
    Plain(PlainIndex),
    Json(JsonIndex),
    Markup(MarkupIndex),
}

impl DocumentIndex {
    fn new(body: &[u8], markup: bool) -> Self {
        if markup {
            Self::Markup(MarkupIndex::new(body))
        } else {
            Self::Plain(PlainIndex::new(body))
        }
    }

    fn line_count(&self) -> usize {
        match self {
            Self::Plain(index) => index.line_count,
            Self::Json(index) => index.line_count(),
            Self::Markup(index) => index.line_count(),
        }
    }

    fn ranges(&self, body: &[u8], offset: usize, count: usize) -> Vec<Range<usize>> {
        match self {
            Self::Plain(index) => index.ranges(body, offset, count),
            Self::Markup(index) => index.ranges(body, offset, count),
            Self::Json(_) => Vec::new(),
        }
    }
}

struct PlainIndex {
    checkpoints: Vec<LineCursor>,
    line_count: usize,
}

impl PlainIndex {
    fn new(body: &[u8]) -> Self {
        let mut cursor = LineCursor::default();
        let mut checkpoints = vec![cursor.clone()];
        let mut line_count = 0;
        while cursor.next(body).is_some() {
            line_count += 1;
            if line_count % CHECKPOINT_LINE_INTERVAL == 0 {
                checkpoints.push(cursor.clone());
            }
        }
        Self {
            checkpoints,
            line_count,
        }
    }

    fn ranges(&self, body: &[u8], offset: usize, count: usize) -> Vec<Range<usize>> {
        let checkpoint = &self.checkpoints[offset / CHECKPOINT_LINE_INTERVAL];
        let mut cursor = checkpoint.clone();
        for _ in 0..offset % CHECKPOINT_LINE_INTERVAL {
            cursor.next(body);
        }
        (0..count).map_while(|_| cursor.next(body)).collect()
    }
}

#[derive(Clone, Default)]
struct LineCursor {
    offset: usize,
    logical_end: Option<usize>,
}

impl LineCursor {
    fn next(&mut self, body: &[u8]) -> Option<Range<usize>> {
        if self.offset >= body.len() {
            return None;
        }
        let start = self.offset;
        let logical_end = *self.logical_end.get_or_insert_with(|| {
            memchr(b'\n', &body[start..]).map_or(body.len(), |offset| start + offset)
        });
        let mut end = start.saturating_add(MAX_LINE_BYTES).min(logical_end);
        while end < body.len() && end > start && body[end] & 0b1100_0000 == 0b1000_0000 {
            end -= 1;
        }
        if end == start && logical_end > start {
            end = start + 1;
        }
        let visible_end = if end == logical_end && end > start && body[end - 1] == b'\r' {
            end - 1
        } else {
            end
        };
        self.offset = if end == logical_end {
            self.logical_end = None;
            if logical_end < body.len() {
                logical_end + 1
            } else {
                logical_end
            }
        } else {
            end
        };
        Some(start..visible_end)
    }
}
