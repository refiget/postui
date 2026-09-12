use ratatui::style::{Color, Style};

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
        self.panel(matches!(self.focus, Focus::Requests | Focus::Variables))
    }

    pub(super) fn sidebar_focused(&self) -> bool {
        matches!(self.focus, Focus::Requests | Focus::Variables)
    }

    pub(super) fn preview_border(&self) -> Style {
        self.panel(matches!(self.focus, Focus::Preview | Focus::Actions))
    }

    pub(super) fn preview_focused(&self) -> bool {
        matches!(self.focus, Focus::Preview | Focus::Actions)
    }

    pub(super) fn request_selection(&self) -> Color {
        if self.focus == Focus::Requests {
            self.theme.selection
        } else {
            self.theme.surface
        }
    }

    pub(super) fn variables_focused(&self) -> bool {
        self.focus == Focus::Variables
    }

    fn panel(&self, active: bool) -> Style {
        Style::default().fg(if active {
            self.theme.accent
        } else {
            self.theme.muted
        })
    }
}
