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
            inner.y.saturating_add(2),
            inner.width,
            inner.height.saturating_sub(2),
        )),
    }
}

pub(super) fn draw_response(
    frame: &mut Frame<'_>,
    area: Rect,
    menu_button: Rect,
    zoom_button: Rect,
    app: &App,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let focus = FocusStyles::new(app.focus, theme);
    frame.render_widget(
        panel_block(text.response(), area, theme).border_style(focus.response_border()),
        area,
    );
    if !app.has_current_request() {
        frame.render_widget(
            Paragraph::new(text.request_not_sent())
                .style(label_style(theme))
                .alignment(Alignment::Center),
            area.inner(Margin::new(2, 2)),
        );
        return;
    }
    draw_response_menu_button(frame, menu_button, app);
    draw_response_zoom_button(frame, zoom_button, app);
    let Some(request) = app.current_request() else {
        return;
    };
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

    if let Some(response) = response {
        let heading = Line::from(Span::styled(text.response_body(), section_style(theme)));
        if response.body_bytes.is_empty() {
            render_response_lines(
                frame,
                sections.body,
                app,
                vec![
                    heading,
                    Line::from(Span::styled(text.empty_response(), label_style(theme))),
                ],
                theme,
            );
        } else if let Some(document) = app.current_response_document() {
            let limited_line = document.limited().then(|| {
                Line::from(Span::styled(
                    text.response_body_limited(
                        document.displayed_bytes(),
                        response.body_bytes.len(),
                    ),
                    Style::default().fg(theme.warning),
                ))
            });
            render_response_document(
                frame,
                sections.body,
                app,
                heading,
                document,
                limited_line,
                theme,
            );
        }
    } else if let Some(error) = error {
        render_response_lines(
            frame,
            sections.body,
            app,
            vec![Line::from(Span::styled(
                request_status.error_message(text, error),
                Style::default().fg(theme.error),
            ))],
            theme,
        );
    }
}

fn render_response_document(
    frame: &mut Frame<'_>,
    areas: ScrollAreas,
    app: &App,
    heading: Line<'static>,
    document: &crate::response_document::ResponseDocument,
    limited_line: Option<Line<'static>>,
    theme: &crate::settings::UiTheme,
) {
    let body_length = document.line_count();
    let content_length = 1_usize
        .saturating_add(body_length)
        .saturating_add(usize::from(limited_line.is_some()));
    let viewport_length = usize::from(areas.content.height);
    let offset = scroll_offset(
        app.response_state.scroll.offset(),
        content_length,
        viewport_length,
    );
    let end = offset.saturating_add(viewport_length).min(content_length);
    let mut lines = Vec::with_capacity(viewport_length);

    if offset == 0 {
        lines.push(heading);
    }

    let body_start = offset.max(1).saturating_sub(1).min(body_length);
    let body_end = end.saturating_sub(1).min(body_length);
    if body_start < body_end {
        lines.extend(document.visible_lines(
            body_start,
            body_end.saturating_sub(body_start),
            theme,
        ));
    }

    let limited_index = 1usize.saturating_add(body_length);
    if offset <= limited_index && limited_index < end {
        if let Some(limited_line) = limited_line {
            lines.push(limited_line);
        }
    }

    frame.render_widget(Paragraph::new(lines), areas.content);
    draw_scrollbar(
        frame,
        areas.scrollbar,
        content_length,
        viewport_length,
        offset,
        theme,
    );
}

fn render_response_lines(
    frame: &mut Frame<'_>,
    areas: ScrollAreas,
    app: &App,
    lines: Vec<Line<'static>>,
    theme: &crate::settings::UiTheme,
) {
    let content_length = wrapped_line_count(&lines, areas.content.width);
    let viewport_length = usize::from(areas.content.height);
    let offset = scroll_offset(
        app.response_state.scroll.offset(),
        content_length,
        viewport_length,
    );
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(
        paragraph.scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0)),
        areas.content,
    );
    draw_scrollbar(
        frame,
        areas.scrollbar,
        content_length,
        viewport_length,
        offset,
        theme,
    );
}

pub(super) fn response_menu_area(panel: Rect, trigger: Rect) -> Rect {
    if panel.is_empty() || trigger.is_empty() {
        return Rect::default();
    }
    let width = 20.min(panel.width.saturating_sub(2));
    let height = u16::try_from(ResponseMenuAction::all().len())
        .unwrap_or(u16::MAX)
        .saturating_add(2)
        .min(panel.height);
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
    let focused = app.focus == Focus::ResponseActions;
    let style = if focused {
        Style::default()
            .fg(theme.background)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.background)
            .bg(theme.secondary)
            .add_modifier(Modifier::BOLD)
    };
    frame.render_widget(
        Paragraph::new(format!("{} ▾", text.response_menu()))
            .alignment(Alignment::Center)
            .style(style),
        area,
    );
}

pub(super) fn draw_response_zoom_button(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() {
        return;
    }
    let theme = &app.global_config.theme;
    let focused = app.focus == Focus::ResponseZoom;
    let style = Style::default().fg(theme.background).bg(theme.primary);
    let style = if focused {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    };
    let (symbol, label) = if app.response_zoomed() {
        ("↙", app.text().response_restore())
    } else {
        ("↗", app.text().response_zoom())
    };
    frame.render_widget(
        Paragraph::new(format!("{symbol} {label}"))
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
