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
        self.panel(matches!(
            self.focus,
            Focus::Collection | Focus::Requests | Focus::Variables
        ))
    }

    pub(super) fn sidebar_focused(&self) -> bool {
        matches!(
            self.focus,
            Focus::Collection | Focus::Requests | Focus::Variables
        )
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

    pub(super) fn collection_focused(&self) -> bool {
        self.focus == Focus::Collection
    }

    fn panel(&self, active: bool) -> Style {
        Style::default().fg(if active {
            self.theme.accent
        } else {
            self.theme.muted
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_panel_stays_active_for_each_child_focus() {
        let theme = UiTheme::default();

        assert_eq!(
            FocusStyles::new(Focus::Variables, &theme).sidebar_border(),
            Style::default().fg(theme.accent)
        );
        assert_eq!(
            FocusStyles::new(Focus::Actions, &theme).preview_border(),
            Style::default().fg(theme.accent)
        );
    }

    #[test]
    fn selection_fill_only_tracks_the_request_list() {
        let theme = UiTheme::default();

        assert_eq!(
            FocusStyles::new(Focus::Requests, &theme).request_selection(),
            theme.selection
        );
        assert_eq!(
            FocusStyles::new(Focus::Variables, &theme).request_selection(),
            theme.surface
        );
    }
}
