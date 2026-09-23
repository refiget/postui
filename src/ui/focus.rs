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
        self.panel(self.focus == Focus::Requests)
    }

    pub(super) fn header_border(&self) -> Style {
        self.panel(self.focus == Focus::Header)
    }

    pub(super) fn preview_border(&self) -> Style {
        self.panel(self.focus == Focus::Preview)
    }

    pub(super) fn response_border(&self) -> Style {
        self.panel(self.focus == Focus::Response)
    }

    pub(super) fn request_selection(&self) -> Style {
        selection_style(self.theme, self.focus == Focus::Requests)
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
