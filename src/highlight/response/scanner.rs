use super::super::{SYNTAXES, THEMES, syntect_style};
use super::{HIGHLIGHT_PAGE_BYTES, HighlightJob, ResponseHighlight, ResponseSpan};
use ratatui::style::Style;
use std::{collections::HashMap, sync::Arc, time::Instant};
use syntect::{
    easy::ScopeRegionIterator,
    highlighting::{Highlighter, ThemeSet},
    parsing::{ParseState, ScopeStack, SyntaxSet},
};

// Syntect's parser is not Send: construct it on the background thread using it,
// and share only byte ranges and resolved styles with the UI.
pub(super) struct HighlightScanner {
    pub(super) body: bytes::Bytes,
    pub(super) extension: &'static str,
    checkpoints: Vec<(usize, ParseState, ScopeStack)>,
    last_position: Option<(usize, ParseState, ScopeStack)>,
}

impl HighlightScanner {
    pub(super) fn new(body: bytes::Bytes, extension: &'static str) -> Option<Self> {
        let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
        let syntax = syntaxes.find_syntax_by_extension(extension)?;
        Some(Self {
            body,
            extension,
            checkpoints: vec![(0, ParseState::new(syntax), ScopeStack::new())],
            last_position: None,
        })
    }

    pub(super) fn prepare(&mut self, job: &HighlightJob) -> bool {
        let started = Instant::now();
        let queue_us = job.queued.elapsed().as_micros() as u64;
        let result = self.page(job);
        let current = job.is_current();
        tracing::debug!(target: "postui::perf", cache_id = job.id, syntax = self.extension,
            start = job.range.start, end = job.range.end, prefetch = job.prefetch,
            queue_us, parse_us = started.elapsed().as_micros() as u64,
            stale = !current, failure = result.as_ref().err().copied(),
            spans = result.as_ref().map_or(0, |page| page.spans.len()), "highlight_page");
        if !current {
            job.cancel();
            return false;
        }
        job.publish(result.unwrap_or_else(|_| ResponseHighlight::plain(job.range.clone())));
        true
    }

    pub(super) fn page(&mut self, job: &HighlightJob) -> Result<ResponseHighlight, &'static str> {
        let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
        let syntax = syntaxes
            .find_syntax_by_extension(self.extension)
            .ok_or("unsupported_syntax")?;
        let themes = THEMES.get_or_init(ThemeSet::load_defaults);
        let highlighters = themes
            .themes
            .values()
            .map(Highlighter::new)
            .collect::<Vec<_>>();
        let checkpoint = self
            .checkpoints
            .iter()
            .rev()
            .find(|(offset, _, _)| *offset <= job.range.start)
            .ok_or("missing_checkpoint")?;
        let (mut offset, mut parser, mut scopes) = self
            .last_position
            .as_ref()
            .filter(|(offset, _, _)| *offset <= job.range.start && *offset > checkpoint.0)
            .unwrap_or(checkpoint)
            .clone();
        let scan_start = offset;
        let mut progress = Instant::now();
        let mut skipped_lines = 0_u64;
        let span_limit = 32768 * job.range.len().div_ceil(HIGHLIGHT_PAGE_BYTES);
        let mut palette = HashMap::new();
        let mut spans = Vec::new();
        while offset < job.range.end {
            if !job.is_current() {
                return Err("superseded");
            }
            let end = memchr::memchr(b'\n', &self.body[offset..])
                .map_or(self.body.len(), |position| offset + position + 1);
            if end >= job.range.end {
                self.last_position = Some((offset, parser.clone(), scopes.clone()));
            }
            let line = std::str::from_utf8(&self.body[offset..end]).map_err(|_| "invalid_utf8")?;
            if line.len() > 8192 {
                // Only this logical line loses colour, not the rest of the document.
                parser = ParseState::new(syntax);
                scopes = ScopeStack::new();
                skipped_lines += 1;
            } else {
                let operations = parser
                    .parse_line(line, syntaxes)
                    .map_err(|_| "syntax_error")?;
                let mut position = offset;
                for (text, operation) in ScopeRegionIterator::new(&operations, line) {
                    scopes.apply(operation).map_err(|_| "scope_error")?;
                    if scopes.as_slice().len() > 64 {
                        return Err("scope_limit");
                    }
                    let end = position + text.len();
                    if !text.is_empty() && end > job.range.start && position < job.range.end {
                        if spans.len() >= span_limit {
                            return Err("span_limit");
                        }
                        let styles =
                            palette
                                .entry(scopes.as_slice().to_vec())
                                .or_insert_with(|| {
                                    Arc::<[Style]>::from(
                                        highlighters
                                            .iter()
                                            .map(|highlighter| {
                                                syntect_style(
                                                    highlighter.style_for_stack(scopes.as_slice()),
                                                )
                                            })
                                            .collect::<Vec<_>>(),
                                    )
                                });
                        spans.push(ResponseSpan {
                            range: position..end,
                            styles: Arc::clone(styles),
                        });
                    }
                    position = end;
                }
            }
            offset = end;
            if progress.elapsed().as_secs() >= 1 {
                tracing::debug!(target: "postui::perf", cache_id = job.id,
                    offset, target_end = job.range.end, scan_start, "highlight_progress");
                progress = Instant::now();
            }
            if offset
                >= self
                    .checkpoints
                    .last()
                    .ok_or("missing_checkpoint")?
                    .0
                    .saturating_add(1024 * 1024)
            {
                if self.checkpoints.len() >= 65 {
                    self.checkpoints.remove(1);
                }
                self.checkpoints
                    .push((offset, parser.clone(), scopes.clone()));
            }
        }
        tracing::debug!(target: "postui::perf", cache_id = job.id, scan_start,
            scan_bytes = offset - scan_start, skipped_lines,
            checkpoints = self.checkpoints.len(), "highlight_scan");
        Ok(ResponseHighlight {
            range: job.range.clone(),
            themes: themes.themes.keys().cloned().collect(),
            spans,
        })
    }
}
