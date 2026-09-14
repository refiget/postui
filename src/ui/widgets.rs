use super::*;

pub(super) fn constraint_length(constraint: Constraint) -> u16 {
    match constraint {
        Constraint::Length(length) => length,
        _ => 0,
    }
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
    if http_method::parse(method).is_err() {
        return Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD);
    }
    let color = match method {
        "GET" => theme.success,
        "POST" => theme.warning,
        "PUT" | "PATCH" => theme.accent,
        "DELETE" => theme.error,
        "HEAD" | "OPTIONS" => theme.secondary,
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
    draw_flat_button_aligned(frame, area, label, enabled, focused, theme, alignment);
}

pub(super) fn draw_primary_button(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    focused: bool,
    theme: &crate::settings::UiTheme,
) {
    draw_flat_button_aligned(frame, area, label, true, focused, theme, Alignment::Center);
}

pub(super) fn draw_flat_button_aligned(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    enabled: bool,
    focused: bool,
    theme: &crate::settings::UiTheme,
    alignment: Alignment,
) {
    let state = FlatButtonState::new(enabled, focused);
    draw_flat_button_colored(frame, area, label, state, theme.accent, theme, alignment);
}

#[derive(Debug, Clone, Copy)]
pub(super) enum FlatButtonState {
    Disabled,
    Idle,
    Focused,
}

impl FlatButtonState {
    pub(super) fn new(enabled: bool, focused: bool) -> Self {
        match (enabled, focused) {
            (false, _) => Self::Disabled,
            (true, true) => Self::Focused,
            (true, false) => Self::Idle,
        }
    }
}

pub(super) fn draw_flat_button_colored(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    state: FlatButtonState,
    color: Color,
    theme: &crate::settings::UiTheme,
    alignment: Alignment,
) {
    let line = match state {
        FlatButtonState::Disabled => Line::from(vec![
            Span::styled("│", Style::default().fg(theme.muted)),
            Span::styled(
                format!(" {label} "),
                Style::default()
                    .fg(theme.muted)
                    .underline_color(theme.selection)
                    .add_modifier(Modifier::DIM | Modifier::UNDERLINED),
            ),
        ]),
        FlatButtonState::Idle | FlatButtonState::Focused => {
            let fill = if matches!(state, FlatButtonState::Focused) {
                theme.text
            } else {
                color
            };
            let edge = blend_rgb(color, theme.surface, 65);
            let style = Style::default()
                .fg(theme.background)
                .bg(fill)
                .underline_color(edge)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            Line::from(vec![
                Span::styled("▌", Style::default().fg(edge).bg(fill)),
                Span::styled(format!(" {label} "), style),
            ])
        }
    };
    frame.render_widget(Paragraph::new(line).alignment(alignment), area);
}

pub(super) fn blend_rgb(foreground: Color, background: Color, foreground_percent: u16) -> Color {
    let (Color::Rgb(fr, fg, fb), Color::Rgb(br, bg, bb)) = (foreground, background) else {
        return foreground;
    };
    let background_percent = 100 - foreground_percent;
    let blend = |front: u8, back: u8| {
        ((u16::from(front) * foreground_percent + u16::from(back) * background_percent) / 100) as u8
    };
    Color::Rgb(blend(fr, br), blend(fg, bg), blend(fb, bb))
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
