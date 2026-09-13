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

pub(super) fn scroll_offset(offset: usize, content_length: usize, viewport_length: usize) -> usize {
    let max_offset = content_length.saturating_sub(viewport_length);
    offset.min(max_offset)
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

#[derive(Debug, Clone, Copy)]
pub(super) struct ScrollbarTrackState {
    pub(super) area: Rect,
    pub(super) track_top: u16,
    pub(super) track_length: usize,
    pub(super) thumb_start: usize,
    pub(super) thumb_length: usize,
    pub(super) travel: usize,
    pub(super) max_offset: usize,
    pub(super) offset: usize,
}

pub(super) fn scrollbar_track_state(
    area: Rect,
    content_length: usize,
    viewport_length: usize,
    offset: usize,
) -> Option<ScrollbarTrackState> {
    if area.is_empty() || viewport_length == 0 || content_length <= viewport_length {
        return None;
    }

    let arrows = u16::from(area.height >= 4);
    let track_length = usize::from(area.height.saturating_sub(arrows * 2));
    if track_length == 0 {
        return None;
    }

    let max_offset = content_length.saturating_sub(viewport_length);
    let offset = offset.min(max_offset);
    let position = scrollbar_position(offset, content_length, viewport_length);
    let scale = track_length as f64 / (content_length + viewport_length - 1) as f64;
    let thumb_start = ((position as f64 * scale).round() as usize).min(track_length - 1);
    let thumb_end =
        (((position + viewport_length) as f64 * scale).round() as usize).min(track_length);
    let travel = (((content_length - 1) as f64 * scale).round() as usize)
        .min(track_length.saturating_sub(1))
        .max(1);
    Some(ScrollbarTrackState {
        area,
        track_top: area.y + arrows,
        track_length,
        thumb_start,
        thumb_length: thumb_end.saturating_sub(thumb_start).max(1),
        travel,
        max_offset,
        offset,
    })
}

pub(super) fn scrollbar_offset_from_track(track: &ScrollbarTrackState, row: u16) -> usize {
    if row < track.track_top {
        return track.offset.saturating_sub(1);
    }
    let track_row = usize::from(row.saturating_sub(track.track_top));
    if track_row >= track.track_length {
        return (track.offset + 1).min(track.max_offset);
    }
    if (track.thumb_start..track.thumb_start.saturating_add(track.thumb_length))
        .contains(&track_row)
    {
        return track.offset;
    }
    track_row
        .saturating_sub(track.thumb_length / 2)
        .min(track.travel)
        .saturating_mul(track.max_offset)
        .saturating_div(track.travel.max(1))
        .min(track.max_offset)
}

pub(super) fn scrollbar_offset_from_drag(
    track: &ScrollbarTrackState,
    anchor_row: u16,
    anchor_offset: usize,
    row: u16,
) -> usize {
    if track.max_offset == 0 {
        return 0;
    }
    let delta = i128::from(row) - i128::from(anchor_row);
    let max_offset = track.max_offset as i128;
    let travel = track.travel.max(1) as i128;
    (anchor_offset as i128 + delta * max_offset / travel).clamp(0, max_offset) as usize
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
        RequestStatus::Sending | RequestStatus::Timeout => theme.warning,
        RequestStatus::Success => theme.success,
        RequestStatus::Failed => theme.error,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

pub(super) fn request_status_symbol(status: RequestStatus, animation_frame: usize) -> &'static str {
    const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    match status {
        RequestStatus::NotSent => "●",
        RequestStatus::Sending => SPINNER[animation_frame % SPINNER.len()],
        RequestStatus::Success => "●",
        RequestStatus::Failed | RequestStatus::Timeout => "●",
    }
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
    let block = bordered_block(title, theme, border::ROUNDED);
    if area.width >= 2 && area.height >= 2 {
        block
    } else {
        Block::default()
    }
}

pub(super) fn draw_send_button(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    enabled: bool,
    focused: bool,
    theme: &crate::settings::UiTheme,
) {
    draw_send_button_aligned(
        frame,
        area,
        label,
        enabled,
        focused,
        theme,
        Alignment::Center,
    );
}

pub(super) fn draw_send_button_aligned(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    enabled: bool,
    focused: bool,
    theme: &crate::settings::UiTheme,
    alignment: Alignment,
) {
    let line = if enabled {
        let edge = if focused {
            theme.secondary
        } else {
            theme.accent
        };
        Line::from(vec![
            Span::styled("▐", Style::default().fg(edge)),
            Span::styled(
                format!(" {label} "),
                Style::default()
                    .fg(theme.background)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("▌", Style::default().fg(edge)),
        ])
    } else {
        Line::from(vec![
            Span::styled("│", Style::default().fg(theme.muted)),
            Span::styled(format!(" {label} "), Style::default().fg(theme.muted)),
            Span::styled("│", Style::default().fg(theme.muted)),
        ])
    };
    frame.render_widget(Paragraph::new(line).alignment(alignment), area);
}

pub(super) fn draw_primary_button(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    focused: bool,
    theme: &crate::settings::UiTheme,
) {
    draw_single_line_button(
        frame,
        area,
        label,
        true,
        focused,
        ButtonPalette::new(
            theme.text,
            theme.primary,
            theme.background,
            theme.accent,
            theme.muted,
        ),
    );
}

fn draw_single_line_button(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    enabled: bool,
    focused: bool,
    palette: ButtonPalette,
) {
    render_button_line(frame, area, label, palette.style(enabled, focused));
}

fn render_button_line(frame: &mut Frame<'_>, area: Rect, label: &str, style: Style) {
    let line = Line::from(Span::styled(format!(" {label} "), style));
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

#[derive(Debug, Clone, Copy)]
struct ButtonPalette {
    focused: Style,
    unfocused: Style,
    disabled: Style,
}

impl ButtonPalette {
    fn new(
        focused_fg: Color,
        focused_bg: Color,
        unfocused_fg: Color,
        unfocused_bg: Color,
        disabled_fg: Color,
    ) -> Self {
        Self {
            focused: Style::default()
                .fg(focused_fg)
                .bg(focused_bg)
                .add_modifier(Modifier::BOLD),
            unfocused: Style::default().fg(unfocused_fg).bg(unfocused_bg),
            disabled: Style::default().fg(disabled_fg),
        }
    }

    fn style(self, enabled: bool, focused: bool) -> Style {
        if !enabled {
            self.disabled
        } else if focused {
            self.focused
        } else {
            self.unfocused
        }
    }
}

fn bordered_block(
    title: impl Into<Line<'static>>,
    theme: &crate::settings::UiTheme,
    symbols: border::Set,
) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_set(symbols)
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

pub(super) fn editor_view(editor: &crate::editor::EditInput, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if editor.mode() == crate::editor::EditMode::Replace {
        return truncate(editor.value(), width);
    }
    let cursor = editor.cursor_byte();
    let before = &editor.value()[..cursor];
    let after = &editor.value()[cursor..];
    let marker = "▏";
    let marker_width = Line::from(marker).width();
    let available = width.saturating_sub(marker_width);
    let after_width = Line::from(after).width().min(available / 2);
    let before_width = available.saturating_sub(after_width);

    let mut before_chars = Vec::new();
    let mut used = 0_usize;
    for character in before.chars().rev() {
        let character_width = Line::from(character.to_string()).width();
        if used.saturating_add(character_width) > before_width {
            break;
        }
        before_chars.push(character);
        used = used.saturating_add(character_width);
    }
    before_chars.reverse();

    let mut result = before_chars.into_iter().collect::<String>();
    result.push_str(marker);
    used = 0;
    for character in after.chars() {
        let character_width = Line::from(character.to_string()).width();
        if used.saturating_add(character_width) > after_width {
            break;
        }
        result.push(character);
        used = used.saturating_add(character_width);
    }
    result
}

pub(super) fn edit_input_style(
    editor: &crate::editor::EditInput,
    theme: &crate::settings::UiTheme,
    foreground: Color,
    background: Color,
) -> Style {
    edit_input_text_style(foreground).bg(if editor.mode() == crate::editor::EditMode::Replace {
        theme.selection
    } else {
        background
    })
}

pub(super) fn edit_input_text_style(foreground: Color) -> Style {
    Style::default()
        .fg(foreground)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}
