use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct ResponseLayout {
    pub(super) status: Rect,
    pub(super) body: ScrollAreas,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ScrollAreas {
    pub(super) content: Rect,
    pub(super) scrollbar: Rect,
}

pub(super) fn response_sections(area: Rect) -> ResponseLayout {
    let inner = area.inner(Margin::new(1, 1));
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);
    ResponseLayout {
        status: sections[0],
        body: inner_scroll_areas(sections[1]),
    }
}

pub(super) fn draw_response(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    frame.render_widget(panel_block(text.response(), area, theme), area);
    let request = app.current_request();
    let request_status = app.request_status(&request.id);
    let loading = request_status == RequestStatus::Sending;
    let response = app.current_response();
    let error = app.current_error();
    let sections = response_sections(area);

    let status = match (loading, response, error) {
        (true, _, _) => Line::from(Span::styled(
            text.waiting_response(),
            Style::default().fg(theme.secondary),
        )),
        (false, Some(response), _) => {
            let status_style = request_status_style(request_status, theme);
            let status = if response.reason.is_empty() {
                format!("HTTP {}", response.status)
            } else {
                format!("HTTP {} {}", response.status, response.reason)
            };
            Line::from(vec![
                Span::styled(status, status_style),
                Span::styled(
                    format!("  {} ms", response.elapsed_ms),
                    Style::default().fg(theme.muted),
                ),
            ])
        }
        (false, None, Some(error)) => {
            let message = request_status.error_message(text, error);
            Line::from(Span::styled(message, Style::default().fg(theme.error)))
        }
        (false, None, None) => {
            Line::from(Span::styled(text.request_not_sent(), label_style(theme)))
        }
    };
    frame.render_widget(Paragraph::new(status), sections.status);

    let mut body_lines = Vec::new();
    if let Some(response) = response {
        body_lines.push(Line::from(Span::styled(
            text.response_body(),
            section_style(theme),
        )));
        match response.download_path.as_ref() {
            Some(path) => body_lines.push(Line::from(Span::styled(
                text.download_saved(&path.display().to_string()),
                Style::default().fg(theme.success),
            ))),
            None if response.body.is_empty() => body_lines.push(Line::from(Span::styled(
                text.empty_response(),
                label_style(theme),
            ))),
            None => match highlight::json_text_lines_if_valid(&response.body, theme) {
                Some(lines) => body_lines.extend(lines),
                None => body_lines.extend(highlight::plain_lines(&response.body, theme)),
            },
        }
    } else if let Some(error) = error {
        let message = request_status.error_message(text, error);
        body_lines.push(Line::from(Span::styled(
            message,
            Style::default().fg(theme.error),
        )));
    } else {
        body_lines.push(Line::from(text.send_hint()));
    }
    let content_length = wrapped_line_count(&body_lines, sections.body.content.width);
    let paragraph = Paragraph::new(body_lines).wrap(Wrap { trim: false });
    let viewport_length = usize::from(sections.body.content.height);
    let offset = scroll_offset(
        app.response_state.scroll.offset(),
        content_length,
        viewport_length,
    );
    frame.render_widget(paragraph.scroll((offset, 0)), sections.body.content);
    draw_scrollbar(
        frame,
        sections.body.scrollbar,
        content_length,
        viewport_length,
        usize::from(offset),
        theme,
    );
}

pub(super) fn panel_scroll_areas(area: Rect) -> ScrollAreas {
    inner_scroll_areas(area.inner(Margin::new(1, 1)))
}

pub(super) fn inner_scroll_areas(area: Rect) -> ScrollAreas {
    if area.is_empty() {
        return ScrollAreas {
            content: Rect::default(),
            scrollbar: Rect::default(),
        };
    }
    let scrollbar_width = u16::from(area.width > 1);
    let content_width = area.width.saturating_sub(scrollbar_width);
    ScrollAreas {
        content: Rect::new(area.x, area.y, content_width, area.height),
        scrollbar: Rect::new(
            area.x.saturating_add(content_width),
            area.y,
            scrollbar_width,
            area.height,
        ),
    }
}
