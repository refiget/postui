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

struct ResponseScrollbar {
    area: Rect,
    track_top: u16,
    track_length: usize,
    thumb_start: usize,
    thumb_length: usize,
    travel: usize,
    max_offset: usize,
    offset: usize,
}

fn response_scrollbar(app: &App, areas: UiLayout) -> Option<ResponseScrollbar> {
    let body = response_sections(areas.response, areas.response_menu_button).body;
    let viewport = usize::from(body.content.height);
    let length = if let Some(response) = app.current_response() {
        if response.body_bytes.is_empty() {
            wrapped_line_count(
                &[
                    Line::from(app.text().response_body()),
                    Line::from(app.text().empty_response()),
                ],
                body.content.width,
            )
        } else {
            let document = app.current_response_document()?;
            1 + document.line_count() + usize::from(document.limited())
        }
    } else {
        let lines = app
            .current_error()?
            .lines()
            .map(Line::from)
            .collect::<Vec<_>>();
        wrapped_line_count(&lines, body.content.width)
    };
    if body.scrollbar.is_empty() || viewport == 0 || length <= viewport {
        return None;
    }
    let arrows = u16::from(body.scrollbar.height >= 4);
    let track_length = usize::from(body.scrollbar.height - arrows * 2);
    let max_offset = length - viewport;
    let offset = app.view.response.scroll.offset().min(max_offset);
    let position = scrollbar_position(offset, length, viewport);
    // 与 Ratatui 的圆整方式保持一致，按住滑块时才不会发生位置跳变。
    let scale = track_length as f64 / (length - 1 + viewport) as f64;
    let thumb_start = ((position as f64 * scale).round() as usize).min(track_length - 1);
    let thumb_end = (((position + viewport) as f64 * scale).round() as usize).min(track_length);
    let travel = (((length - 1) as f64 * scale).round() as usize)
        .min(track_length - 1)
        .max(1);
    Some(ResponseScrollbar {
        area: body.scrollbar,
        track_top: body.scrollbar.y + arrows,
        track_length,
        thumb_start,
        thumb_length: thumb_end.saturating_sub(thumb_start).max(1),
        travel,
        max_offset,
        offset,
    })
}

pub(super) fn click_response_scrollbar(app: &mut App, column: u16, row: u16, areas: UiLayout) {
    let Some(bar) = response_scrollbar(app, areas) else {
        return;
    };
    if !contains(bar.area, column, row) {
        return;
    }
    if row < bar.track_top {
        app.view
            .response
            .scroll
            .set_offset(bar.offset.saturating_sub(1));
        return;
    }
    let track_row = usize::from(row - bar.track_top);
    if track_row >= bar.track_length {
        app.view
            .response
            .scroll
            .set_offset((bar.offset + 1).min(bar.max_offset));
        return;
    }
    let offset = if (bar.thumb_start..bar.thumb_start + bar.thumb_length).contains(&track_row) {
        bar.offset
    } else {
        track_row
            .saturating_sub(bar.thumb_length / 2)
            .min(bar.travel)
            * bar.max_offset
            / bar.travel
    };
    app.view.response.scroll.set_offset(offset);
    app.view.response.scroll.drag_anchor = Some((row, offset));
}

pub(super) fn drag_response_scrollbar(app: &mut App, row: u16, areas: UiLayout) {
    let Some((anchor_row, anchor_offset)) = app.view.response.scroll.drag_anchor else {
        return;
    };
    let Some(bar) = response_scrollbar(app, areas) else {
        app.view.response.scroll.drag_anchor = None;
        return;
    };
    let delta = i128::from(row) - i128::from(anchor_row);
    let offset = (anchor_offset as i128 + delta * bar.max_offset as i128 / bar.travel as i128)
        .clamp(0, bar.max_offset as i128) as usize;
    app.view.response.scroll.set_offset(offset);
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
    let focus = FocusStyles::new(app.view.focus, theme);
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
                    request_status_symbol(request_status, app.view.animation_frame)
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
                        request_status_symbol(request_status, app.view.animation_frame)
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
        (false, None, Some(_)) => Line::from(vec![
            Span::styled(
                format!(
                    "{} ",
                    request_status_symbol(request_status, app.view.animation_frame)
                ),
                request_status_style(request_status, theme),
            ),
            Span::styled(request_status.label(text), Style::default().fg(theme.error)),
        ]),
        (false, None, None) => Line::from(vec![
            Span::styled(
                format!(
                    "{} ",
                    request_status_symbol(request_status, app.view.animation_frame)
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
            error
                .lines()
                .map(|line| {
                    Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(theme.error),
                    ))
                })
                .collect(),
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
        app.view.response.scroll.offset(),
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
        app.view.response.scroll.offset(),
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
    draw_send_button(
        frame,
        area,
        &format!("{} ▾", app.text().response_menu()),
        true,
        app.view.focus == Focus::ResponseActions,
        &app.global_config.theme,
    );
}

pub(super) fn draw_response_zoom_button(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() {
        return;
    }
    let (symbol, label) = if app.response_zoomed() {
        ("↙", app.text().response_restore())
    } else {
        ("↗", app.text().response_zoom())
    };
    draw_send_button(
        frame,
        area,
        &format!("{symbol} {label}"),
        true,
        app.view.focus == Focus::ResponseZoom,
        &app.global_config.theme,
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
        app.view
            .response
            .menu_selection
            .unwrap_or_default()
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
