use ratatui::style::{Modifier, Style};

use crate::{app::Focus, settings::UiTheme};

pub(super) struct FocusStyles<'a> {
    focus: Focus,
    theme: &'a UiTheme,
}

impl<'a> FocusStyles<'a> {
    pub(super) fn new(focus: Focus, theme: &'a UiTheme) -> Self {
        Self { focus, theme }
    }

    pub(super) fn sidebar_border(&self) -> Style {
        self.panel(matches!(
            self.focus,
            Focus::Requests | Focus::WorkspaceButton | Focus::Variables
        ))
    }

    pub(super) fn header_border(&self) -> Style {
        self.panel(self.focus == Focus::Header)
    }

    pub(super) fn preview_border(&self) -> Style {
        self.panel(matches!(self.focus, Focus::Preview | Focus::SendButton))
    }

    pub(super) fn response_border(&self) -> Style {
        self.panel(matches!(
            self.focus,
            Focus::Response | Focus::ResponseActions | Focus::ResponseZoom
        ))
    }

    pub(super) fn request_selection(&self) -> Style {
        selection_style(self.theme, self.focus == Focus::Requests)
    }

    pub(super) fn variables_focused(&self) -> bool {
        self.focus == Focus::Variables
    }

    pub(super) fn workspace_focused(&self) -> bool {
        self.focus == Focus::WorkspaceButton
    }

    fn panel(&self, active: bool) -> Style {
        if active {
            Style::default()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tui_assets_rust::blend_rgb(
                self.theme.muted,
                self.theme.surface,
                60,
            ))
        }
    }
}

pub(super) fn selection_style(theme: &UiTheme, focused: bool) -> Style {
    if focused {
        Style::default()
            .bg(theme.selection)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(tui_assets_rust::blend_rgb(
            theme.selection,
            theme.surface,
            45,
        ))
    }
}
