use super::{
    AppPrompt, Dialog, Feedback, Focus, PreviewContentState, ResponseContentState, VariablesPage,
    ViewMode,
};
use std::time::{Duration, Instant};

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
    pub(crate) preview: PreviewContentState,
    pub(crate) response: ResponseContentState,
    pub(crate) variables: Option<VariablesPage>,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) prompt: Option<AppPrompt>,
    pub(crate) notice: Option<Feedback>,
    pub(crate) animation_frame: usize,
    pub(crate) clicks: ClickSequence,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            focus: Focus::Requests,
            mode: ViewMode::default(),
            preview: PreviewContentState::default(),
            response: ResponseContentState::default(),
            variables: None,
            dialog: None,
            prompt: None,
            notice: None,
            animation_frame: 0,
            clicks: ClickSequence::default(),
        }
    }
}
