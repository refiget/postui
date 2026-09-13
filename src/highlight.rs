use std::{
    collections::{HashMap, VecDeque},
    ops::Range,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Instant,
};

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::{HighlightLines, ScopeRegionIterator},
    highlighting::{FontStyle, Highlighter, Theme, ThemeSet},
    parsing::{ParseState, ScopeStack, SyntaxSet},
    util::LinesWithEndings,
};

use crate::settings::{DEFAULT_SYNTAX_THEME, UiTheme};

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEMES: OnceLock<ThemeSet> = OnceLock::new();

pub struct ResponseHighlight {
    range: Range<usize>,
    themes: Vec<String>,
    spans: Vec<ResponseSpan>,
}

struct ResponseSpan {
    range: Range<usize>,
    styles: Arc<[Style]>,
}

impl ResponseHighlight {
    pub fn line(&self, body: &[u8], range: Range<usize>, theme: &UiTheme) -> Line<'static> {
        let theme_index = self
            .themes
            .iter()
            .position(|name| name == &theme.syntax_theme)
            .or_else(|| {
                self.themes
                    .iter()
                    .position(|name| name == DEFAULT_SYNTAX_THEME)
            })
            .unwrap_or(0);
        let start = self
            .spans
            .partition_point(|span| span.range.end <= range.start);
        let mut spans = Vec::new();
        let mut position = range.start;
        for span in self.spans[start..]
            .iter()
            .take_while(|span| span.range.start < range.end)
        {
            let start = span.range.start.max(range.start);
            let end = span.range.end.min(range.end);
            if position < start {
                spans.push(Span::styled(
                    String::from_utf8_lossy(&body[position..start]).into_owned(),
                    plain_style(theme),
                ));
            }
            spans.push(Span::styled(
                String::from_utf8_lossy(&body[start..end]).into_owned(),
                span.styles[theme_index],
            ));
            position = end;
        }
        if position < range.end {
            spans.push(Span::styled(
                String::from_utf8_lossy(&body[position..range.end]).into_owned(),
                plain_style(theme),
            ));
        }
        Line::from(spans)
    }
}

const HIGHLIGHT_PAGE_BYTES: usize = 64 * 1024;
const CACHED_SCROLL_PAGES: usize = 3;
static NEXT_CACHE_ID: AtomicU64 = AtomicU64::new(1);
static HIGHLIGHT_QUEUED: AtomicU64 = AtomicU64::new(0);
static VISIBLE_CACHE_ID: AtomicU64 = AtomicU64::new(0);
static HIGHLIGHT_WORKER: OnceLock<Option<mpsc::SyncSender<HighlightJob>>> = OnceLock::new();
static HIGHLIGHT_CHANGED: AtomicBool = AtomicBool::new(false);

pub fn take_response_highlight_change() -> bool {
    HIGHLIGHT_CHANGED.swap(false, Ordering::Relaxed)
}

pub fn clear_response_highlight_focus() {
    VISIBLE_CACHE_ID.store(0, Ordering::Relaxed);
}

pub struct ResponseHighlightCache {
    id: u64,
    body: bytes::Bytes,
    extension: &'static str,
    first_page: Arc<ResponseHighlight>,
    state: Arc<Mutex<HighlightPage>>,
}

#[derive(Default)]
struct HighlightPage {
    wanted: Range<usize>,
    ready: VecDeque<Arc<ResponseHighlight>>,
    pending: bool,
    hits: u64,
    misses: u64,
    waiting_since: Option<Instant>,
}

struct HighlightJob {
    id: u64,
    body: bytes::Bytes,
    extension: &'static str,
    state: Weak<Mutex<HighlightPage>>,
    range: Range<usize>,
    viewport: Range<usize>,
    queued: Instant,
    prefetch: bool,
    initial: bool,
}

impl ResponseHighlightCache {
    pub fn new(body: bytes::Bytes, extension: Option<&'static str>) -> Option<Arc<Self>> {
        let extension = extension?;
        HIGHLIGHT_WORKER
            .get_or_init(|| {
                let (sender, receiver) = mpsc::sync_channel::<HighlightJob>(8);
                std::thread::Builder::new()
                    .name("response-highlight".into())
                    .spawn(move || {
                        let mut scanner: Option<HighlightScanner> = None;
                        loop {
                            let job = match receiver.recv_timeout(std::time::Duration::from_secs(5))
                            {
                                Ok(job) => job,
                                Err(mpsc::RecvTimeoutError::Timeout) => {
                                    scanner = None;
                                    continue;
                                }
                                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            };
                            HIGHLIGHT_QUEUED.fetch_sub(1, Ordering::Relaxed);
                            if !job.is_current() {
                                job.cancel();
                                tracing::debug!(target: "postui::perf", cache_id = job.id, "highlight_stale_queue");
                                continue;
                            }
                            if scanner.as_ref().is_none_or(|scanner| {
                                scanner.body.as_ptr() != job.body.as_ptr()
                                    || scanner.body.len() != job.body.len()
                                    || scanner.extension != job.extension
                            }) {
                                scanner = HighlightScanner::new(job.body.clone(), job.extension);
                            }
                            let Some(scanner) = scanner.as_mut() else {
                                job.publish(ResponseHighlight::plain(job.range.clone()));
                                continue;
                            };
                            if !scanner.prepare(&job) {
                                continue;
                            }
                            let start = job.range.end;
                            if start < job.body.len() && job.is_current() {
                                let prefetch = HighlightJob {
                                    range: start..start.saturating_add(2 * HIGHLIGHT_PAGE_BYTES).min(job.body.len()),
                                    queued: Instant::now(),
                                    prefetch: true,
                                    ..job
                                };
                                scanner.prepare(&prefetch);
                            }
                        }
                    })
                    .ok()
                    .map(|_| sender)
            })
            .as_ref()?;
        let id = NEXT_CACHE_ID.fetch_add(1, Ordering::Relaxed);
        let state = Arc::new(Mutex::new(HighlightPage {
            wanted: 0..body.len().min(HIGHLIGHT_PAGE_BYTES),
            ..HighlightPage::default()
        }));
        // Response documents are prepared off the UI thread. Publish their first
        // page with styles already available, just like the native JSON view.
        let job = HighlightJob {
            id,
            body: body.clone(),
            extension,
            state: Arc::downgrade(&state),
            range: 0..body.len().min(HIGHLIGHT_PAGE_BYTES),
            viewport: 0..body.len().min(HIGHLIGHT_PAGE_BYTES),
            queued: Instant::now(),
            prefetch: false,
            initial: true,
        };
        let started = Instant::now();
        let result = HighlightScanner::new(body.clone(), extension)
            .ok_or("unsupported_syntax")
            .and_then(|mut scanner| scanner.page(&job));
        let failure = result.as_ref().err().copied();
        let first_page =
            Arc::new(result.unwrap_or_else(|_| ResponseHighlight::plain(job.range.clone())));
        tracing::debug!(target: "postui::perf", cache_id = id, syntax = extension,
            bytes = body.len(), prepare_us = started.elapsed().as_micros() as u64,
            spans = first_page.spans.len(), failure, "highlight_first_page");
        Some(Arc::new(Self {
            id,
            body,
            extension,
            first_page,
            state,
        }))
    }

    pub fn page(&self, range: Range<usize>) -> Option<Arc<ResponseHighlight>> {
        VISIBLE_CACHE_ID.store(self.id, Ordering::Relaxed);
        let first = (self.first_page.range.end >= range.end).then(|| Arc::clone(&self.first_page));
        let mut state = match self.state.try_lock() {
            Ok(state) => state,
            Err(std::sync::TryLockError::WouldBlock) => {
                HIGHLIGHT_CHANGED.store(true, Ordering::Relaxed);
                return first;
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                tracing::debug!(target: "postui::perf", cache_id = self.id, "highlight_cache_poisoned");
                return Some(Arc::new(ResponseHighlight::plain(range)));
            }
        };
        let start = range.start / HIGHLIGHT_PAGE_BYTES * HIGHLIGHT_PAGE_BYTES;
        let wanted = start
            ..range
                .end
                .max(start.saturating_add(2 * HIGHLIGHT_PAGE_BYTES))
                .min(self.body.len());
        if state.wanted != wanted {
            state.wanted = wanted.clone();
            state.pending = false;
            state.waiting_since = None;
        }
        let cached = first.or_else(|| {
            let index = state
                .ready
                .iter()
                .position(|page| page.range.start <= range.start && page.range.end >= range.end)?;
            let page = state.ready.remove(index)?;
            state.ready.push_back(Arc::clone(&page));
            Some(page)
        });
        if cached.is_some() {
            state.hits += 1;
            if let Some(started) = state.waiting_since.take() {
                tracing::debug!(target: "postui::perf", cache_id = self.id,
                    start = range.start, end = range.end,
                    wait_us = started.elapsed().as_micros() as u64, "highlight_visible");
            }
        } else {
            state.misses += 1;
            state.waiting_since.get_or_insert_with(Instant::now);
        }
        let covered = state
            .ready
            .iter()
            .any(|page| page.range.start <= wanted.start && page.range.end >= wanted.end)
            || self.first_page.range.end >= wanted.end;
        if !covered && !state.pending {
            let job = HighlightJob {
                id: self.id,
                body: self.body.clone(),
                extension: self.extension,
                state: Arc::downgrade(&self.state),
                range: wanted.clone(),
                viewport: wanted,
                queued: Instant::now(),
                prefetch: false,
                initial: false,
            };
            HIGHLIGHT_QUEUED.fetch_add(1, Ordering::Relaxed);
            match HIGHLIGHT_WORKER.get()?.as_ref()?.try_send(job) {
                Ok(()) => {
                    state.pending = true;
                    tracing::debug!(target: "postui::perf", cache_id = self.id,
                    start = state.wanted.start, end = state.wanted.end,
                    cache_hit = cached.is_some(), hits = state.hits, misses = state.misses,
                    pages = state.ready.len(), "highlight_request");
                }
                Err(mpsc::TrySendError::Full(_)) => {
                    HIGHLIGHT_QUEUED.fetch_sub(1, Ordering::Relaxed);
                    HIGHLIGHT_CHANGED.store(true, Ordering::Relaxed);
                    tracing::debug!(target: "postui::perf", cache_id = self.id, "highlight_queue_full");
                }
                Err(mpsc::TrySendError::Disconnected(job)) => {
                    HIGHLIGHT_QUEUED.fetch_sub(1, Ordering::Relaxed);
                    let page = Arc::new(ResponseHighlight::plain(job.range));
                    if state.ready.len() >= CACHED_SCROLL_PAGES {
                        state.ready.pop_front();
                    }
                    state.ready.push_back(Arc::clone(&page));
                    tracing::debug!(target: "postui::perf", cache_id = self.id, "highlight_worker_stopped");
                    return Some(page);
                }
            }
        }
        cached
    }
}

impl HighlightJob {
    fn is_current(&self) -> bool {
        (self.initial || VISIBLE_CACHE_ID.load(Ordering::Relaxed) == self.id)
            && (!self.prefetch || HIGHLIGHT_QUEUED.load(Ordering::Relaxed) == 0)
            && self.state.upgrade().is_some_and(|state| {
                state
                    .lock()
                    .is_ok_and(|state| state.wanted == self.viewport)
            })
    }

    fn cancel(&self) {
        if self.prefetch {
            return;
        }
        if let Some(state) = self.state.upgrade() {
            if let Ok(mut state) = state.lock() {
                if state.wanted == self.viewport {
                    state.pending = false;
                    HIGHLIGHT_CHANGED.store(true, Ordering::Relaxed);
                }
            }
        }
    }

    fn publish(&self, page: ResponseHighlight) {
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let Ok(mut state) = state.lock() else { return };
        if state.wanted != self.viewport {
            return;
        }
        state.ready.retain(|cached| cached.range != page.range);
        state.ready.push_back(Arc::new(page));
        while state.ready.len() > CACHED_SCROLL_PAGES {
            state.ready.pop_front();
        }
        if !self.prefetch {
            state.pending = false;
        }
        HIGHLIGHT_CHANGED.store(true, Ordering::Relaxed);
    }
}

impl Drop for ResponseHighlightCache {
    fn drop(&mut self) {
        if let Ok(state) = self.state.try_lock() {
            tracing::debug!(target: "postui::perf", cache_id = self.id,
                hits = state.hits, misses = state.misses, "highlight_cache_released");
        }
    }
}

impl ResponseHighlight {
    fn plain(range: Range<usize>) -> Self {
        Self {
            range,
            themes: Vec::new(),
            spans: Vec::new(),
        }
    }
}

// Syntect's parser is not Send: construct it on the background thread using it,
// and share only byte ranges and resolved styles with the UI.
struct HighlightScanner {
    body: bytes::Bytes,
    extension: &'static str,
    checkpoints: Vec<(usize, ParseState, ScopeStack)>,
    last_position: Option<(usize, ParseState, ScopeStack)>,
}

impl HighlightScanner {
    fn new(body: bytes::Bytes, extension: &'static str) -> Option<Self> {
        let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
        let syntax = syntaxes.find_syntax_by_extension(extension)?;
        Some(Self {
            body,
            extension,
            checkpoints: vec![(0, ParseState::new(syntax), ScopeStack::new())],
            last_position: None,
        })
    }

    fn prepare(&mut self, job: &HighlightJob) -> bool {
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

    fn page(&mut self, job: &HighlightJob) -> Result<ResponseHighlight, &'static str> {
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

pub fn plain_style(theme: &UiTheme) -> Style {
    Style::default().fg(theme.text)
}

pub fn variable_style(base: Style, theme: &UiTheme) -> Style {
    base.patch(
        Style::default()
            .fg(theme.variable)
            .add_modifier(Modifier::BOLD),
    )
}

pub fn template_line(value: &str, base_style: Style, theme: &UiTheme) -> Line<'static> {
    Line::from(template_spans(value, base_style, theme))
}

pub fn template_spans(value: &str, base_style: Style, theme: &UiTheme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = value;

    while let Some((start, end, _)) = crate::template::find_placeholder(rest) {
        if start > 0 {
            spans.push(Span::styled(rest[..start].to_string(), base_style));
        }
        spans.push(Span::styled(
            rest[start..end].to_string(),
            variable_style(base_style, theme),
        ));
        rest = &rest[end..];
    }

    if !rest.is_empty() {
        spans.push(Span::styled(rest.to_string(), base_style));
    }
    spans
}

pub fn json_text_lines_window(
    value: &str,
    offset: usize,
    count: usize,
    theme: &UiTheme,
) -> Vec<Line<'static>> {
    if count == 0 {
        return Vec::new();
    }
    if value.len() > 64 * 1024 {
        return LinesWithEndings::from(value)
            .skip(offset)
            .take(count)
            .map(|line| {
                let mut end = line.len().min(8192);
                while !line.is_char_boundary(end) {
                    end -= 1;
                }
                template_line(trim_line_ending(&line[..end]), plain_style(theme), theme)
            })
            .collect();
    }
    let syntax_set = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let Some(syntax) = syntax_set.find_syntax_by_extension("json") else {
        return plain_lines(value, theme)
            .into_iter()
            .skip(offset)
            .take(count)
            .collect();
    };
    let mut highlighter = HighlightLines::new(syntax, syntax_theme(&theme.syntax_theme));

    LinesWithEndings::from(value)
        .enumerate()
        .filter_map(|(index, line)| {
            let line = trim_line_ending(line);
            let highlighted = highlighter.highlight_line(line, syntax_set);
            if index < offset {
                return None;
            }
            match highlighted {
                Ok(regions) => {
                    let mut spans = Vec::new();
                    for (style, text) in regions {
                        spans.extend(template_spans(text, syntect_style(style), theme));
                    }
                    Some(Line::from(spans))
                }
                Err(error) => {
                    tracing::debug!(error = %error, "JSON 语法高亮失败，使用普通文本");
                    Some(template_line(line, plain_style(theme), theme))
                }
            }
        })
        .take(count)
        .collect()
}

pub fn plain_lines(value: &str, theme: &UiTheme) -> Vec<Line<'static>> {
    LinesWithEndings::from(value)
        .map(|line| template_line(trim_line_ending(line), plain_style(theme), theme))
        .collect()
}

fn syntax_theme(name: &str) -> &'static Theme {
    let themes = THEMES.get_or_init(ThemeSet::load_defaults);
    if let Some(theme) = themes.themes.get(name) {
        return theme;
    }
    tracing::debug!(
        syntax_theme = %name,
        fallback = DEFAULT_SYNTAX_THEME,
        "找不到配置的语法主题，使用默认语法主题"
    );
    themes
        .themes
        .get(DEFAULT_SYNTAX_THEME)
        .or_else(|| themes.themes.values().next())
        .expect("syntect 默认主题不应为空")
}

fn syntect_style(style: syntect::highlighting::Style) -> Style {
    let mut tui_style = Style::default().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ));
    if style.font_style.intersects(FontStyle::BOLD) {
        tui_style = tui_style.add_modifier(Modifier::BOLD);
    }
    if style.font_style.intersects(FontStyle::ITALIC) {
        tui_style = tui_style.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.intersects(FontStyle::UNDERLINE) {
        tui_style = tui_style.add_modifier(Modifier::UNDERLINED);
    }
    tui_style
}

fn trim_line_ending(value: &str) -> &str {
    value
        .strip_suffix("\r\n")
        .or_else(|| value.strip_suffix('\n'))
        .or_else(|| value.strip_suffix('\r'))
        .unwrap_or(value)
}
