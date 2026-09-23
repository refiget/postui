use super::{CurlImportPage, Dialog, Feedback, PreviewTab, ResponseTab};
use crate::editor::{EditInput, EditMode, JsonScalarKind, terminal_width};
use std::{
    ops::Range,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Header,
    Requests,
    Preview,
    Response,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ViewMode {
    #[default]
    Standard,
    ResponseZoom,
}

/// 键盘和滚轮的滚动步长。
const SCROLL_STEP: usize = 3;

/// 按方向移动一个滚动步长；方向为 0 时不变。
fn stepped(offset: usize, direction: isize) -> usize {
    match direction {
        value if value < 0 => offset.saturating_sub(SCROLL_STEP),
        value if value > 0 => offset.saturating_add(SCROLL_STEP),
        _ => offset,
    }
}

#[derive(Debug, Default)]
pub(crate) struct ScrollState {
    offset: usize,
    viewport: usize,
}

impl ScrollState {
    pub(crate) fn offset(&self) -> usize {
        self.offset
    }

    pub(super) fn reset(&mut self) {
        self.offset = 0;
    }

    /// 记录预览内容区的可见行数。
    pub(crate) fn set_viewport(&mut self, height: usize) {
        self.viewport = height;
    }

    /// 移动视图，使 `line` 行落在可见范围内。
    pub(crate) fn reveal(&mut self, line: usize) {
        if line < self.offset {
            self.offset = line;
        } else if self.viewport > 0 && line >= self.offset.saturating_add(self.viewport) {
            self.offset = line.saturating_sub(self.viewport - 1);
        }
    }

    pub(crate) fn move_by(&mut self, direction: isize) -> bool {
        let previous = self.offset;
        self.offset = stepped(self.offset, direction);
        self.offset != previous
    }
}

#[derive(Debug, Default)]
pub(crate) struct ResponseScrollState {
    offset: usize,
    max_offset: usize,
    pub(crate) drag_anchor: Option<(u16, usize)>,
}

impl ResponseScrollState {
    pub(crate) fn offset(&self) -> usize {
        self.offset
    }

    pub(super) fn reset(&mut self) {
        self.offset = 0;
        self.drag_anchor = None;
    }

    pub(crate) fn set_offset(&mut self, offset: usize) {
        self.offset = offset.min(self.max_offset);
    }

    pub(crate) fn update_bounds(&mut self, max_offset: usize) {
        self.max_offset = max_offset;
        self.offset = self.offset.min(max_offset);
    }

    pub(crate) fn move_by(&mut self, direction: isize) -> bool {
        let previous = self.offset;
        self.offset = stepped(self.offset, direction).min(self.max_offset);
        self.offset != previous
    }
}

#[derive(Debug, Default)]
pub(crate) struct PreviewContentState {
    pub(crate) active_tab: PreviewTab,
    pub(crate) scroll: ScrollState,
    /// 内容页签里选中字段的下标。
    pub(crate) field_cursor: usize,
    pub(crate) editor: Option<ContentEditor>,
}

#[derive(Debug)]
pub(crate) enum AppPrompt {
    ConfirmDelete { request_id: String },
    ConfirmQuit,
}

#[derive(Debug)]
pub(crate) struct ContentEditor {
    pub(crate) target: ContentTarget,
    /// 渲染行号和值起始列。
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) input: EditInput,
}

/// 内容页签里正在编辑的对象。
#[derive(Debug)]
pub(crate) enum ContentTarget {
    /// 请求体 JSON 里的值：渲染文本、值的字节范围和类型。
    Body {
        document: String,
        span: Range<usize>,
        kind: JsonScalarKind,
    },
    Form(usize),
    File(usize),
}

/// 内容页签里可编辑字段的位置：内容行号和值起始列。
#[derive(Debug, Clone)]
pub(crate) struct ContentField {
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) source: ContentFieldSource,
}

/// 内容页签里可编辑字段的来源。
#[derive(Debug, Clone)]
pub(crate) enum ContentFieldSource {
    /// 请求体 JSON 里的值；编辑时按渲染位置重新定位。
    Body,
    Form(usize),
    File(usize),
}

impl ContentEditor {
    /// 请求体 JSON 编辑后的渲染文本；编辑其他对象时为 None。
    pub(crate) fn display_document(&self) -> Option<String> {
        let ContentTarget::Body { document, span, .. } = &self.target else {
            return None;
        };
        let mut document = document.clone();
        document.replace_range(span.clone(), self.input.value());
        Some(document)
    }

    /// 值加光标标记占用的列数。
    pub(crate) fn display_width(&self) -> usize {
        terminal_width(self.input.value()).max(1)
            + usize::from(self.input.mode() == EditMode::Insert)
    }

    /// 点击位置是否落在编辑器覆盖的值范围内。
    pub(crate) fn covers(&self, line: usize, column: usize) -> bool {
        if self.line != line || column < self.column {
            return false;
        }
        match self.target {
            // JSON 值右侧的文本属于渲染文档，光标只落在值本身。
            ContentTarget::Body { .. } => column < self.column.saturating_add(self.display_width()),
            // 表单字段和文件路径的值覆盖到内容区右边界。
            _ => true,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ResponseContentState {
    pub(crate) scroll: ResponseScrollState,
    pub(crate) menu: tui_assets_rust::DropdownState,
    pub(crate) active_tab: ResponseTab,
    pub(crate) selection: Option<ResponseSelection>,
    pub(crate) search: Option<EditInput>,
    pub(crate) search_query: String,
    pub(crate) search_match_line: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ResponseTextPoint {
    pub(crate) line: usize,
    pub(crate) grapheme: usize,
}

#[derive(Debug)]
pub(crate) struct ResponseSelection {
    pub(crate) anchor: ResponseTextPoint,
    pub(crate) head: ResponseTextPoint,
    pub(crate) text: String,
    pub(crate) dragging: bool,
}

impl ResponseSelection {
    pub(crate) fn ordered(&self) -> (ResponseTextPoint, ResponseTextPoint) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RequestListState {
    pub(crate) search: Option<EditInput>,
    pub(crate) filter: String,
    pub(crate) filter_origin: Option<String>,
    pub(crate) scroll: ListScrollState,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ListScrollState {
    offset: usize,
    pub(crate) drag_anchor: Option<(u16, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollDragTarget {
    Requests,
    Preview,
    Response,
}

impl ListScrollState {
    pub(crate) fn offset(&self, content_length: usize, viewport_length: usize) -> usize {
        self.offset
            .min(content_length.saturating_sub(viewport_length))
    }

    pub(crate) fn set_offset(
        &mut self,
        offset: usize,
        content_length: usize,
        viewport_length: usize,
    ) {
        self.offset = offset.min(content_length.saturating_sub(viewport_length));
    }

    pub(crate) fn move_by(
        &mut self,
        direction: isize,
        content_length: usize,
        viewport_length: usize,
    ) {
        let max_offset = content_length.saturating_sub(viewport_length);
        self.offset = stepped(self.offset, direction).min(max_offset);
    }
}

#[derive(Debug, Clone, Copy)]
struct MousePress {
    column: u16,
    row: u16,
    at: Instant,
}

#[derive(Debug, Default)]
pub(crate) struct ClickSequence {
    previous: Option<MousePress>,
}

impl ClickSequence {
    pub(crate) fn register(&mut self, column: u16, row: u16) -> bool {
        let now = Instant::now();
        let is_double = self.previous.is_some_and(|previous| {
            now.duration_since(previous.at) <= Duration::from_millis(400)
                && previous.column.abs_diff(column) <= 1
                && previous.row.abs_diff(row) <= 1
        });
        self.previous = if is_double {
            None
        } else {
            Some(MousePress {
                column,
                row,
                at: now,
            })
        };
        is_double
    }

    pub(crate) fn reset(&mut self) {
        self.previous = None;
    }
}

pub(crate) struct ViewState {
    pub(crate) focus: Focus,
    pub(crate) mode: ViewMode,
    pub(crate) requests: RequestListState,
    pub(crate) preview: PreviewContentState,
    pub(crate) response: ResponseContentState,
    pub(crate) curl_import: Option<CurlImportPage>,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) prompt: Option<AppPrompt>,
    pub(crate) notice: Option<Feedback>,
    pub(crate) help_scroll: Option<u16>,
    pub(crate) animation_frame: usize,
    pub(crate) clicks: ClickSequence,
    pub(crate) scroll_drag_target: Option<ScrollDragTarget>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            focus: Focus::Requests,
            mode: ViewMode::default(),
            requests: RequestListState::default(),
            preview: PreviewContentState::default(),
            response: ResponseContentState::default(),
            curl_import: None,
            dialog: None,
            prompt: None,
            notice: None,
            help_scroll: None,
            animation_frame: 0,
            clicks: ClickSequence::default(),
            scroll_drag_target: None,
        }
    }
}

impl ViewState {
    pub(crate) fn cancel_scroll_drag(&mut self) {
        self.scroll_drag_target = None;
        self.response.scroll.drag_anchor = None;
        self.requests.scroll.drag_anchor = None;
        if let Some(table) = self.dialog.as_mut().and_then(Dialog::table_mut) {
            table.scroll.drag_anchor = None;
        }
    }

    pub(crate) fn cancel_active_editors(&mut self) {
        self.preview.cancel_editor();
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.cancel_editor();
        }
    }

    pub(super) fn next_focus(&self, reverse: bool) -> Focus {
        let order: &[Focus] = match self.mode {
            ViewMode::Standard => &[Focus::Requests, Focus::Preview, Focus::Response],
            ViewMode::ResponseZoom => &[Focus::Response],
        };
        let Some(index) = order.iter().position(|focus| *focus == self.focus) else {
            return Focus::Response;
        };
        let next = if reverse {
            (index + order.len() - 1) % order.len()
        } else {
            (index + 1) % order.len()
        };
        order[next]
    }
}

impl PreviewContentState {
    pub(super) fn reset_content(&mut self) {
        self.scroll.reset();
        self.field_cursor = 0;
    }

    pub(super) fn is_editing(&self) -> bool {
        self.editor.is_some()
    }

    pub(super) fn cancel_editor(&mut self) {
        self.editor = None;
    }
}
