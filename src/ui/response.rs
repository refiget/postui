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

type ResponseScrollbar = ScrollbarTrackState;

enum ResponseContent<'a> {
    Lines(Vec<Line<'static>>),
    Document {
        heading: Line<'static>,
        document: &'a crate::response_document::ResponseDocument,
        limited_line: Option<Line<'static>>,
    },
}

impl ResponseContent<'_> {
    fn line_count(&self, width: u16) -> usize {
        match self {
            Self::Lines(lines) => wrapped_line_count(lines, width),
            Self::Document {
                document,
                limited_line,
                ..
            } => 1 + document.line_count() + usize::from(limited_line.is_some()),
        }
    }
}

fn response_content(app: &App) -> Option<ResponseContent<'_>> {
    let theme = &app.global_config.theme;
    let text = app.text();
    let Some(response) = app.current_response() else {
        return app.current_error().map(|error| {
            ResponseContent::Lines(
                error
                    .lines()
                    .map(|line| {
                        Line::from(Span::styled(
                            line.to_string(),
                            Style::default().fg(theme.error),
                        ))
                    })
                    .collect(),
            )
        });
    };
    if app.view.response.active_tab == ResponseTab::Headers {
        let mut lines = vec![Line::from(Span::styled(
            text.response_headers(),
            section_style(theme),
        ))];
        lines.extend(response.headers.iter().map(|(name, value)| {
            Line::from(vec![
                Span::styled(format!("{name}: "), Style::default().fg(theme.secondary)),
                Span::styled(value.clone(), Style::default().fg(theme.text)),
            ])
        }));
        return Some(ResponseContent::Lines(lines));
    }

    let mut heading = Line::from(Span::styled(text.response_body(), section_style(theme)));
    if app.view.response.active_tab == ResponseTab::Formatted
        && !response.is_binary()
        && !response.body_bytes.is_empty()
    {
        if let Some(note) = app
            .current_response_document()
            .and_then(|doc| doc.format_note)
        {
            heading.spans.push(Span::styled(
                format!(" · {}", text.response_format_note(note)),
                Style::default().fg(theme.warning),
            ));
        }
    }
    let summary = if response.is_binary() {
        Some(Span::styled(
            text.binary_response_summary(response.body_bytes.len()),
            Style::default().fg(theme.warning),
        ))
    } else if response.body_bytes.is_empty() {
        Some(Span::styled(text.empty_response(), label_style(theme)))
    } else {
        None
    };
    if let Some(summary) = summary {
        return Some(ResponseContent::Lines(vec![heading, Line::from(summary)]));
    }
    let document = app.current_response_document()?;
    let limited_line = document.limited().then(|| {
        Line::from(Span::styled(
            text.response_body_limited(document.displayed_bytes(), document.total_bytes()),
            Style::default().fg(theme.warning),
        ))
    });
    Some(ResponseContent::Document {
        heading,
        document,
        limited_line,
    })
}

pub(super) fn sync_response_scroll(app: &mut App, areas: UiLayout) {
    let body = response_sections(areas.response).body;
    let length = response_content(app).map_or(0, |content| content.line_count(body.content.width));
    app.view
        .response
        .scroll
        .update_bounds(length.saturating_sub(usize::from(body.content.height)));
}

fn response_scrollbar(app: &App, areas: UiLayout) -> Option<ResponseScrollbar> {
    let body = response_sections(areas.response).body;
    let viewport = usize::from(body.content.height);
    let length = response_content(app)?.line_count(body.content.width);
    let max_offset = length.saturating_sub(viewport);
    let offset = app.view.response.scroll.offset().min(max_offset);
    scrollbar_track_state(body.scrollbar, length, viewport, offset)
}

pub(super) fn click_response_scrollbar(app: &mut App, column: u16, row: u16, areas: UiLayout) {
    let Some(bar) = response_scrollbar(app, areas) else {
        return;
    };
    if !contains(bar.area, column, row) {
        return;
    }
    let offset = scrollbar_offset_from_track(&bar, row);
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
    let offset = scrollbar_offset_from_drag(&bar, anchor_row, anchor_offset, row);
    app.view.response.scroll.set_offset(offset);
}

pub(super) fn response_sections(area: Rect) -> ResponseLayout {
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
    ResponseLayout {
        // The first inner row belongs exclusively to the action buttons.
        status: Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            u16::from(inner.height > 1),
        ),
        body: inner_scroll_areas(Rect::new(
            inner.x,
            inner.y.saturating_add(3),
            inner.width,
            inner.height.saturating_sub(3),
        )),
    }
}

pub(super) fn draw_response(
    frame: &mut Frame<'_>,
    area: Rect,
    format_button: Rect,
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
    draw_response_format_button(frame, format_button, app);
    draw_response_zoom_button(frame, zoom_button, app);
    let Some(request) = app.current_request() else {
        return;
    };
    let request_status = app.request_status(&request.id);
    let loading = request_status == RequestStatus::Sending;
    let response = app.current_response();
    let error = app.current_error();
    let sections = response_sections(area);

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
            let content_type = response.content_type().unwrap_or("—");
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
                    format!(
                        "  {} ms  {} bytes  {}",
                        response.elapsed_ms,
                        response.body_bytes.len(),
                        content_type
                    ),
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
        let metadata = Rect::new(
            sections.status.x,
            sections.status.y.saturating_add(1),
            sections.status.width,
            1,
        )
        .intersection(area.inner(Margin::new(1, 1)));
        let search = &app.view.response.search_query;
        let search_suffix =
            if search.is_empty() || app.view.response.active_tab == ResponseTab::Headers {
                String::new()
            } else {
                format!("   / {search}")
            };
        let tabs = response_tab_labels(app).map(|(_, label)| label).join("  ");
        let tabs = format!("{tabs}  ←/→{search_suffix}   ·   {}", response.final_url);
        if let Some(input) = app.view.response.search.as_ref() {
            frame.render_widget(
                Paragraph::new(format!(
                    "/ {}",
                    editor_view(input, usize::from(metadata.width.saturating_sub(2)))
                ))
                .style(edit_input_style(input, theme, theme.text, theme.surface)),
                metadata,
            );
        } else {
            frame.render_widget(
                Paragraph::new(tabs).style(Style::default().fg(theme.muted)),
                metadata,
            );
        }
    }

    match response_content(app) {
        Some(ResponseContent::Lines(lines)) => {
            crate::highlight::clear_response_highlight_focus();
            render_response_lines(frame, sections.body, app, lines, theme);
        }
        Some(ResponseContent::Document {
            heading,
            document,
            limited_line,
        }) => {
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
        None => crate::highlight::clear_response_highlight_focus(),
    }
}

pub(super) fn draw_response_format_button(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() {
        return;
    }
    let enabled = app.current_response().is_some();
    let (symbol, label) = match app.view.response.active_tab {
        ResponseTab::Raw => ("↔", app.text().response_show_formatted()),
        ResponseTab::Formatted => ("↔", app.text().response_show_raw()),
        ResponseTab::Headers => ("↔", app.text().response_show_formatted()),
    };
    let focused = app.view.response.active_tab != ResponseTab::Headers;
    draw_response_toolbar_button(
        frame,
        area,
        &format!("{symbol} {label}"),
        symbol,
        enabled,
        focused,
        &app.global_config.theme,
    );
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
        if let Some(mut visible) =
            document.visible_lines(body_start, body_end.saturating_sub(body_start), theme)
        {
            if let Some(match_line) = app.view.response.search_match_line {
                if (body_start..body_end).contains(&match_line) {
                    if let Some(line) = visible.get_mut(match_line - body_start) {
                        for span in &mut line.spans {
                            span.style = span.style.add_modifier(Modifier::REVERSED);
                        }
                    }
                }
            }
            lines.extend(visible);
        } else {
            lines.push(Line::styled(
                app.text().response_preparing_highlight(),
                Style::default().fg(theme.muted),
            ));
        }
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
    let label = format!("{} ▾", app.text().response_menu());
    draw_response_toolbar_button(
        frame,
        area,
        &label,
        "▾",
        app.view.focus == Focus::ResponseActions,
        true,
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
    draw_response_toolbar_button(
        frame,
        area,
        &format!("{symbol} {label}"),
        symbol,
        app.view.focus == Focus::ResponseZoom,
        true,
        &app.global_config.theme,
    );
}

fn draw_response_toolbar_button(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    symbol: &str,
    enabled: bool,
    focused: bool,
    theme: &crate::settings::UiTheme,
) {
    let width = usize::from(area.width);
    if width == 0 || area.is_empty() {
        return;
    }

    let content_width = width.saturating_sub(2);
    let full_label = format!(" {label} ");
    let compact_label = format!(" {symbol} ");
    let text = if Line::from(full_label.as_str()).width() <= content_width {
        full_label
    } else if Line::from(compact_label.as_str()).width() <= content_width {
        compact_label
    } else {
        symbol.chars().take(content_width).collect()
    };
    let text_width = Line::from(text.as_str()).width();
    let left_padding = width.saturating_sub(text_width) / 2;
    let right_padding = width.saturating_sub(text_width + left_padding);

    let (cap, body) = if !enabled {
        (
            Style::default().fg(theme.muted).add_modifier(Modifier::DIM),
            Style::default().fg(theme.muted).add_modifier(Modifier::DIM),
        )
    } else if focused {
        (
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
            Style::default()
                .fg(theme.background)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (
            Style::default().fg(theme.accent),
            Style::default()
                .fg(theme.text)
                .bg(theme.selection)
                .add_modifier(Modifier::BOLD),
        )
    };

    let line = Line::from(vec![
        Span::raw(" ".repeat(left_padding)),
        Span::styled("[", cap),
        Span::styled(text, body),
        Span::styled("]", cap),
        Span::raw(" ".repeat(right_padding.saturating_sub(2))),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Left), area);
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
        ResponseMenuAction::CopyBody | ResponseMenuAction::CopyHeaders => "⧉",
    }
}

fn response_action_label(action: ResponseMenuAction, text: crate::i18n::UiText) -> &'static str {
    match action {
        ResponseMenuAction::Download => text.response_download(),
        ResponseMenuAction::CopyBody => text.response_copy_body(),
        ResponseMenuAction::CopyHeaders => text.response_copy_headers(),
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

fn response_tab_labels(app: &App) -> [(ResponseTab, String); 3] {
    [
        (ResponseTab::Raw, "Raw"),
        (ResponseTab::Formatted, "Formatted"),
        (ResponseTab::Headers, app.text().response_headers_tab()),
    ]
    .map(|(tab, label)| {
        (
            tab,
            if app.view.response.active_tab == tab {
                format!("[{label}]")
            } else {
                label.to_string()
            },
        )
    })
}

pub(super) fn response_tab_at(
    app: &App,
    column: u16,
    row: u16,
    areas: UiLayout,
) -> Option<ResponseTab> {
    app.current_response()?;
    let status = response_sections(areas.response).status;
    if status.is_empty()
        || row != status.y.saturating_add(1)
        || row >= areas.response.bottom().saturating_sub(1)
        || column < status.x
        || column >= status.right()
    {
        return None;
    }
    let mut x = usize::from(status.x);
    for (tab, label) in response_tab_labels(app) {
        let width = Line::from(label).width();
        if (x..x + width).contains(&usize::from(column)) {
            return Some(tab);
        }
        x += width + 2;
    }
    None
}
