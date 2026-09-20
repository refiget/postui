use super::{
    App, CurlImportPage, Dialog, ExtractsPage, Feedback, PreviewAction, PreviewTab, ResponseTab,
    VariablesPage,
};
use crate::editor::{BodyValueEditor, EditInput};
use crossterm::event::MouseEvent;
use ratatui::layout::Rect;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Header,
    Requests,
    WorkspaceButton,
    Variables,
    Extracts,
    Preview,
    SendButton,
    ResponseActions,
    ResponseZoom,
    Response,
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MainButton {
    NewRequest,
    Workspace,
    Variables,
    Send,
    ResponseFormat,
    ResponseMenu,
    ResponseZoom,
}

impl MainButton {
    pub(crate) const COUNT: usize = Self::ResponseZoom as usize + 1;

    const fn index(self) -> usize {
        self as usize
    }

    pub(crate) fn enabled(self, app: &App) -> bool {
        match self {
            Self::NewRequest
            | Self::Workspace
            | Self::Variables
            | Self::ResponseMenu
            | Self::ResponseZoom => true,
            Self::Send => app.can_execute_preview_action(PreviewAction::Send),
            Self::ResponseFormat => app.current_response().is_some(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct MainButtonStates {
    interactions: [tui_assets_rust::ButtonInteraction; MainButton::COUNT],
}

impl MainButtonStates {
    pub(crate) fn handle_mouse(
        &mut self,
        button: MainButton,
        event: MouseEvent,
        area: Rect,
        enabled: bool,
    ) -> tui_assets_rust::ButtonEvent {
        let interaction = self.interaction_mut(button);
        interaction.set_enabled(enabled);
        interaction.handle_mouse(event, area)
    }

    pub(crate) fn visual_state(
        &self,
        button: MainButton,
        enabled: bool,
        focused: bool,
    ) -> tui_assets_rust::ButtonState {
        let interaction = self.interaction(button);
        if !enabled {
            tui_assets_rust::ButtonState::Disabled
        } else if interaction.pressed() {
            tui_assets_rust::ButtonState::Pressed
        } else if focused {
            tui_assets_rust::ButtonState::Focused
        } else if interaction.hovered() {
            tui_assets_rust::ButtonState::Hovered
        } else {
            tui_assets_rust::ButtonState::Idle
        }
    }

    fn interaction(&self, button: MainButton) -> &tui_assets_rust::ButtonInteraction {
        &self.interactions[button.index()]
    }

    fn interaction_mut(&mut self, button: MainButton) -> &mut tui_assets_rust::ButtonInteraction {
        &mut self.interactions[button.index()]
    }
}

impl Focus {
    pub(crate) const fn container(self) -> Self {
        match self {
            Self::Header => Self::Header,
            Self::Requests | Self::WorkspaceButton | Self::Variables | Self::Extracts => {
                Self::Requests
            }
            Self::Preview | Self::SendButton => Self::Preview,
            Self::ResponseActions | Self::ResponseZoom | Self::Response => Self::Response,
        }
    }
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
    pub(super) temporary_variables: TemporaryVariablesView,
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

#[derive(Debug)]
pub(crate) struct TemporaryVariableEditor {
    pub(crate) name: String,
    pub(crate) input: EditInput,
}

#[derive(Debug, Default)]
pub(super) struct TemporaryVariablesView {
    selected: usize,
    editor: Option<TemporaryVariableEditor>,
}

impl TemporaryVariablesView {
    pub(super) fn selected(&self, count: usize) -> Option<usize> {
        (count > 0).then(|| self.selected.min(count - 1))
    }

    pub(super) fn select(&mut self, index: usize, count: usize) -> bool {
        if index >= count {
            return false;
        }
        self.selected = index;
        true
    }

    pub(super) fn move_by(&mut self, direction: isize, count: usize) {
        let Some(selected) = self.selected(count) else {
            return;
        };
        self.selected = match direction {
            value if value < 0 => selected.saturating_sub(1),
            value if value > 0 => (selected + 1).min(count - 1),
            _ => selected,
        };
    }

    pub(super) fn editor(&self) -> Option<&TemporaryVariableEditor> {
        self.editor.as_ref()
    }

    pub(super) fn editor_mut(&mut self) -> Option<&mut TemporaryVariableEditor> {
        self.editor.as_mut()
    }

    pub(super) fn start_editing(&mut self, name: String, value: String) {
        self.editor = Some(TemporaryVariableEditor {
            name,
            input: EditInput::new(value),
        });
    }

    pub(super) fn finish_editing(&mut self) -> Option<TemporaryVariableEditor> {
        self.editor.take()
    }

    pub(super) fn cancel_editing(&mut self) {
        self.editor = None;
    }
}

#[derive(Debug, Default)]
pub(crate) struct ResponseContentState {
    pub(crate) scroll: ResponseScrollState,
    pub(crate) menu: tui_assets_rust::DropdownState,
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
    const STEP: usize = 3;

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
        self.offset = match direction {
            value if value < 0 => self.offset.saturating_sub(Self::STEP),
            value if value > 0 => self.offset.saturating_add(Self::STEP),
            _ => self.offset,
        }
        .min(max_offset);
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
    pub(crate) variables: Option<VariablesPage>,
    pub(crate) extracts: Option<ExtractsPage>,
    pub(crate) curl_import: Option<CurlImportPage>,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) prompt: Option<AppPrompt>,
    pub(crate) notice: Option<Feedback>,
    pub(crate) help_scroll: Option<u16>,
    pub(crate) animation_frame: usize,
    pub(crate) clicks: ClickSequence,
    pub(crate) scroll_drag_target: Option<ScrollDragTarget>,
    pub(crate) main_buttons: MainButtonStates,
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
            extracts: None,
            curl_import: None,
            dialog: None,
            prompt: None,
            notice: None,
            help_scroll: None,
            animation_frame: 0,
            clicks: ClickSequence::default(),
            scroll_drag_target: None,
            main_buttons: MainButtonStates::default(),
        }
    }
}

impl ViewState {
    pub(crate) fn cancel_scroll_drag(&mut self) {
        self.scroll_drag_target = None;
        self.response.scroll.drag_anchor = None;
        self.requests.scroll.drag_anchor = None;
        match self.dialog.as_mut() {
            Some(Dialog::Headers(dialog)) => dialog.scroll.drag_anchor = None,
            Some(Dialog::Params(dialog)) => dialog.scroll.drag_anchor = None,
            _ => {}
        }
        if let Some(page) = self.variables.as_mut() {
            page.scroll.drag_anchor = None;
        }
        if let Some(page) = self.extracts.as_mut() {
            page.scroll.drag_anchor = None;
        }
    }

    pub(crate) fn is_editing(&self) -> bool {
        self.preview.is_editing()
            || self.curl_import.is_some()
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
            ViewMode::Standard => &[Focus::Requests, Focus::Preview, Focus::Response],
            ViewMode::ResponseZoom { .. } => &[Focus::Response],
        };
        let container = self.focus.container();
        let Some(index) = order.iter().position(|focus| *focus == container) else {
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
        self.editor.is_some()
            || self.file_editor.is_some()
            || self.temporary_variables.editor().is_some()
    }

    pub(super) fn cancel_editor(&mut self) {
        self.editor = None;
        self.file_editor = None;
        self.temporary_variables.cancel_editing();
    }
}
