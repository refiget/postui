use super::{Dialog, Feedback, PreviewTab, ResponseTab, VariablesPage};
use crate::editor::{BodyValueEditor, EditInput};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Header,
    Requests,
    WorkspaceButton,
    Variables,
    Preview,
    SendButton,
    ResponseActions,
    ResponseZoom,
    Response,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ViewMode {
    #[default]
    Standard,
    ResponseZoom {
        return_focus: Focus,
    },
}

#[derive(Debug, Default)]
pub(crate) struct ScrollState {
    offset: u16,
}

impl ScrollState {
    const STEP: u16 = 3;

    pub(crate) fn offset(&self) -> u16 {
        self.offset
    }

    pub(super) fn reset(&mut self) {
        self.offset = 0;
    }

    pub(crate) fn move_by(&mut self, direction: isize) -> bool {
        let previous = self.offset;
        self.offset = match direction {
            -1 => self.offset.saturating_sub(Self::STEP),
            1 => self.offset.saturating_add(Self::STEP),
            _ => self.offset,
        };
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
    const STEP: usize = 3;

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
        self.offset = match direction {
            -1 => self.offset.saturating_sub(Self::STEP),
            1 => self.offset.saturating_add(Self::STEP),
            _ => self.offset,
        }
        .min(self.max_offset);
        self.offset != previous
    }
}

#[derive(Debug, Default)]
pub(crate) struct PreviewContentState {
    pub(crate) active_tab: PreviewTab,
    pub(crate) scroll: ScrollState,
    pub(crate) editor: Option<BodyValueEditor>,
    pub(crate) file_editor: Option<FileValueEditor>,
}

#[derive(Debug)]
pub(crate) enum AppPrompt {
    ConfirmDelete { request_id: String },
    ConfirmQuit,
}

#[derive(Debug)]
pub(crate) struct FileValueEditor {
    pub(crate) file_index: usize,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) input: EditInput,
}

#[derive(Debug, Default)]
pub(crate) struct ResponseContentState {
    pub(crate) scroll: ResponseScrollState,
    pub(crate) menu_selection: Option<usize>,
    pub(crate) active_tab: ResponseTab,
    pub(crate) search: Option<EditInput>,
    pub(crate) search_query: String,
    pub(crate) search_match_line: Option<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct RequestListState {
    pub(crate) search: Option<EditInput>,
    pub(crate) filter: String,
    pub(crate) filter_origin: Option<String>,
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
    pub(crate) variables: Option<VariablesPage>,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) prompt: Option<AppPrompt>,
    pub(crate) notice: Option<Feedback>,
    pub(crate) help_visible: bool,
    pub(crate) animation_frame: usize,
    pub(crate) clicks: ClickSequence,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            focus: Focus::Requests,
            mode: ViewMode::default(),
            requests: RequestListState::default(),
            preview: PreviewContentState::default(),
            response: ResponseContentState::default(),
            variables: None,
            dialog: None,
            prompt: None,
            notice: None,
            help_visible: false,
            animation_frame: 0,
            clicks: ClickSequence::default(),
        }
    }
}

impl ViewState {
    pub(crate) fn is_editing(&self) -> bool {
        self.preview.is_editing()
            || self.response.search.is_some()
            || self.requests.search.is_some()
            || self
                .variables
                .as_ref()
                .is_some_and(|page| page.editor.is_some())
            || self.dialog.as_ref().is_some_and(Dialog::is_editing)
    }

    pub(crate) fn cancel_active_editors(&mut self) {
        self.preview.cancel_editor();
        if let Some(page) = self.variables.as_mut() {
            page.cancel_editor();
        }
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.cancel_editor();
        }
    }

    pub(super) fn next_focus(&self, reverse: bool) -> Focus {
        let order: &[Focus] = match self.mode {
            ViewMode::Standard => &[
                Focus::Header,
                Focus::Requests,
                Focus::WorkspaceButton,
                Focus::Variables,
                Focus::Preview,
                Focus::SendButton,
                Focus::ResponseActions,
                Focus::ResponseZoom,
                Focus::Response,
            ],
            ViewMode::ResponseZoom { .. } => &[
                Focus::Header,
                Focus::ResponseActions,
                Focus::ResponseZoom,
                Focus::Response,
            ],
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
    pub(super) fn is_editing(&self) -> bool {
        self.editor.is_some() || self.file_editor.is_some()
    }

    pub(super) fn cancel_editor(&mut self) {
        self.editor = None;
        self.file_editor = None;
    }
}
