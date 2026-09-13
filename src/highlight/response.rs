use super::plain_style;
use crate::settings::{DEFAULT_SYNTAX_THEME, UiTheme};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use std::{
    collections::VecDeque,
    ops::Range,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Instant,
};

mod scanner;
use scanner::HighlightScanner;

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
