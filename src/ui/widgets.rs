use super::*;

pub(super) fn constraint_length(constraint: Constraint) -> u16 {
    match constraint {
        Constraint::Length(length) => length,
        _ => 0,
    }
}

pub(super) fn request_list_offset(selected: usize, item_count: usize, visible: usize) -> usize {
    if visible == 0 || item_count <= visible {
        return 0;
    }
    selected
        .saturating_sub(visible.saturating_sub(1))
        .min(item_count.saturating_sub(visible))
}

pub(super) fn scroll_offset(offset: u16, content_length: usize, viewport_length: usize) -> u16 {
    let max_offset = content_length.saturating_sub(viewport_length);
    u16::try_from(usize::from(offset).min(max_offset)).unwrap_or(u16::MAX)
}

pub(super) fn wrapped_line_count(lines: &[Line<'_>], width: u16) -> usize {
    if width == 0 {
        return 0;
    }
    let width = usize::from(width);
    lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(width))
        .sum()
}

pub(super) fn draw_scrollbar(
    frame: &mut Frame<'_>,
    area: Rect,
    content_length: usize,
    viewport_length: usize,
    offset: usize,
    theme: &crate::settings::UiTheme,
) {
    if area.is_empty() || viewport_length == 0 || content_length <= viewport_length {
        return;
    }

    let position = scrollbar_position(offset, content_length, viewport_length);
    let mut state = ScrollbarState::new(content_length)
        .position(position)
        .viewport_content_length(viewport_length);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .symbols(VERTICAL)
        .thumb_style(
            Style::default()
                .fg(theme.accent)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD),
        )
        .track_style(Style::default().fg(theme.muted).bg(theme.surface))
        .begin_style(Style::default().fg(theme.primary).bg(theme.surface))
        .end_style(Style::default().fg(theme.primary).bg(theme.surface));
    let scrollbar = if area.height < 4 {
        scrollbar.begin_symbol(None).end_symbol(None)
    } else {
        scrollbar
    };
    frame.render_stateful_widget(scrollbar, area, &mut state);
}

pub(super) fn scrollbar_position(
    offset: usize,
    content_length: usize,
    viewport_length: usize,
) -> usize {
    let max_offset = content_length.saturating_sub(viewport_length);
    if max_offset == 0 {
        return 0;
    }

    let offset = offset.min(max_offset);
    let position_span = content_length.saturating_sub(1);
    offset.saturating_mul(position_span) / max_offset
}

pub(super) fn method_style(method: &str, theme: &crate::settings::UiTheme) -> Style {
    if !supports_method(method) {
        return Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD);
    }
    let color = match method {
        "GET" => theme.success,
        "POST" => theme.warning,
        _ => theme.primary,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

pub(super) fn request_status_style(
    status: RequestStatus,
    theme: &crate::settings::UiTheme,
) -> Style {
    let color = match status {
        RequestStatus::NotSent => theme.muted,
        RequestStatus::Sending => theme.warning,
        RequestStatus::Success => theme.success,
        RequestStatus::Failed | RequestStatus::Timeout => theme.error,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

pub(super) fn label_style(theme: &crate::settings::UiTheme) -> Style {
    Style::default().fg(theme.muted)
}

pub(super) fn section_style(theme: &crate::settings::UiTheme) -> Style {
    Style::default()
        .fg(theme.primary)
        .add_modifier(Modifier::BOLD)
}

pub(super) fn panel_block(
    title: impl Into<Line<'static>>,
    area: Rect,
    theme: &crate::settings::UiTheme,
) -> Block<'static> {
    let block = rounded_block(title, theme);
    if area.width >= 2 && area.height >= 2 {
        block
    } else {
        Block::default()
    }
}

pub(super) fn send_button_widget<'a>(
    label: &'a str,
    state: &'a ButtonState,
    theme: &crate::settings::UiTheme,
) -> Button<'a> {
    button_widget(
        label,
        state,
        primary_button_style(theme),
        ButtonVariant::Block,
    )
}

pub(super) fn action_button_widget<'a>(
    label: &'a str,
    state: &'a ButtonState,
    theme: &crate::settings::UiTheme,
) -> Button<'a> {
    button_widget(
        label,
        state,
        secondary_button_style(theme),
        ButtonVariant::Block,
    )
}

pub(super) fn preview_action_button_state(loading_or_disabled: bool, focused: bool) -> ButtonState {
    if loading_or_disabled {
        ButtonState::disabled()
    } else {
        let mut state = ButtonState::enabled();
        state.set_focused(focused);
        state
    }
}

pub(super) fn button_widget<'a>(
    label: &'a str,
    state: &'a ButtonState,
    style: ButtonStyle,
    variant: ButtonVariant,
) -> Button<'a> {
    Button::new(label, state).variant(variant).style(style)
}

pub(super) fn primary_button_style(theme: &crate::settings::UiTheme) -> ButtonStyle {
    let mut style = ButtonStyle::new(ButtonVariant::Block)
        .focused(theme.background, theme.accent)
        .unfocused(theme.accent, theme.surface);
    style.disabled_fg = theme.muted;
    style
}

pub(super) fn secondary_button_style(theme: &crate::settings::UiTheme) -> ButtonStyle {
    let mut style = ButtonStyle::new(ButtonVariant::Block)
        .focused(theme.text, theme.selection)
        .unfocused(theme.text, theme.surface);
    style.disabled_fg = theme.muted;
    style
}

pub(super) fn rounded_block(
    title: impl Into<Line<'static>>,
    theme: &crate::settings::UiTheme,
) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .title(title)
}

pub(super) fn truncate(value: &str, width: usize) -> String {
    if Line::from(value.to_string()).width() <= width {
        return value.to_string();
    }
    if width == 0 {
        return String::new();
    }

    let mut result = String::new();
    let mut used = 0_usize;
    let content_width = width.saturating_sub(Line::from("…").width());
    for character in value.chars() {
        let character_width = Line::from(character.to_string()).width();
        if used.saturating_add(character_width) > content_width {
            break;
        }
        result.push(character);
        used = used.saturating_add(character_width);
    }
    result.push('…');
    result
}
