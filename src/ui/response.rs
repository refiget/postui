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

pub(super) fn response_sections(area: Rect, menu_button: Rect) -> ResponseLayout {
    let inner = area.inner(Margin::new(1, 1));
    if inner.is_empty() {
        return ResponseLayout {
            status: Rect::default(),
            body: ScrollAreas {
                content: Rect::default(),
                scrollbar: Rect::default(),
            },
        };
    }
    let status_width = if menu_button.is_empty() {
        inner.width
    } else {
        inner
            .width
            .saturating_sub(menu_button.width.saturating_add(1))
    };
    ResponseLayout {
        status: Rect::new(inner.x, inner.y, status_width, 1),
        body: inner_scroll_areas(Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        )),
    }
}

pub(super) fn draw_response(frame: &mut Frame<'_>, area: Rect, menu_button: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    frame.render_widget(panel_block(text.response(), area, theme), area);
    draw_response_menu_button(frame, menu_button, app);
    let request = app.current_request();
    let request_status = app.request_status(&request.id);
    let loading = request_status == RequestStatus::Sending;
    let response = app.current_response();
    let error = app.current_error();
    let sections = response_sections(area, menu_button);

    let status = match (loading, response, error) {
        (true, _, _) => Line::from(vec![
            Span::styled(
                format!(
                    "{} ",
                    request_status_symbol(request_status, app.animation_frame)
                ),
                request_status_style(request_status, theme),
            ),
            Span::styled(
                text.waiting_response(),
                Style::default().fg(theme.secondary),
            ),
        ]),
        (false, Some(response), _) => {
            let status_style = request_status_style(request_status, theme);
            let status = if response.reason.is_empty() {
                format!("HTTP {}", response.status)
            } else {
                format!("HTTP {} {}", response.status, response.reason)
            };
            Line::from(vec![
                Span::styled(
                    format!(
                        "{} ",
                        request_status_symbol(request_status, app.animation_frame)
                    ),
                    status_style,
                ),
                Span::styled(status, status_style),
                Span::styled(
                    format!("  {} ms", response.elapsed_ms),
                    Style::default().fg(theme.muted),
                ),
            ])
        }
        (false, None, Some(error)) => {
            let message = request_status.error_message(text, error);
            Line::from(vec![
                Span::styled(
                    format!(
                        "{} ",
                        request_status_symbol(request_status, app.animation_frame)
                    ),
                    request_status_style(request_status, theme),
                ),
                Span::styled(message, Style::default().fg(theme.error)),
            ])
        }
        (false, None, None) => Line::from(vec![
            Span::styled(
                format!(
                    "{} ",
                    request_status_symbol(request_status, app.animation_frame)
                ),
                request_status_style(request_status, theme),
            ),
            Span::styled(text.request_not_sent(), label_style(theme)),
        ]),
    };
    frame.render_widget(Paragraph::new(status), sections.status);

    let mut body_lines = Vec::new();
    if let Some(response) = response {
        body_lines.push(Line::from(Span::styled(
            text.response_body(),
            section_style(theme),
        )));
        if response.body.is_empty() {
            body_lines.push(Line::from(Span::styled(
                text.empty_response(),
                label_style(theme),
            )));
        } else {
            match highlight::json_text_lines_if_valid(&response.body, theme) {
                Some(lines) => body_lines.extend(lines),
                None => body_lines.extend(highlight::plain_lines(&response.body, theme)),
            }
        }
    } else if let Some(error) = error {
        let message = request_status.error_message(text, error);
        body_lines.push(Line::from(Span::styled(
            message,
            Style::default().fg(theme.error),
        )));
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

pub(super) fn response_menu_area(panel: Rect, trigger: Rect) -> Rect {
    if panel.is_empty() || trigger.is_empty() {
        return Rect::default();
    }
    let width = 20.min(panel.width.saturating_sub(2));
    let height = 4.min(panel.height);
    if width < 3 || height < 3 {
        return Rect::default();
    }
    let y = trigger
        .bottom()
        .min(panel.bottom().saturating_sub(height))
        .max(panel.y);
    Rect::new(
        panel.right().saturating_sub(width).saturating_sub(1),
        y,
        width,
        height,
    )
}

pub(super) fn draw_response_menu_button(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() {
        return;
    }
    let theme = &app.global_config.theme;
    let text = app.text();
    let style = Style::default()
        .fg(theme.text)
        .bg(theme.selection)
        .add_modifier(Modifier::BOLD);
    frame.render_widget(
        Paragraph::new(format!("{} ▾", text.response_menu()))
            .alignment(Alignment::Center)
            .style(style),
        area,
    );
}

pub(super) fn draw_response_menu(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() || area.width < 3 || area.height < 3 {
        return;
    }
    let theme = &app.global_config.theme;
    let text = app.text();
    let items = ResponseMenuAction::all()
        .into_iter()
        .map(|action| {
            let style = Style::default().fg(theme.text).bg(theme.surface);
            ListItem::new(Line::from(vec![
                Span::styled(format!("{}  ", response_action_symbol(action)), style),
                Span::styled(response_action_label(action, text), style),
            ]))
            .style(style)
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::PLAIN)
            .border_style(Style::default().fg(theme.accent))
            .style(Style::default().bg(theme.surface)),
        area,
    );
    let inner = area.inner(Margin::new(1, 1));
    if inner.is_empty() {
        return;
    }
    let mut state = ListState::default().with_selected(Some(
        app.response_state
            .menu_selected
            .min(ResponseMenuAction::all().len().saturating_sub(1)),
    ));
    let list = List::new(items).highlight_style(
        Style::default()
            .fg(theme.background)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_stateful_widget(list, inner, &mut state);
}

fn response_action_symbol(action: ResponseMenuAction) -> &'static str {
    match action {
        ResponseMenuAction::Download => "↓",
        ResponseMenuAction::Copy => "⧉",
    }
}

fn response_action_label(action: ResponseMenuAction, text: crate::i18n::UiText) -> &'static str {
    match action {
        ResponseMenuAction::Download => text.response_download(),
        ResponseMenuAction::Copy => text.response_copy(),
    }
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
