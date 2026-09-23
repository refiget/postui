use super::{
    contains,
    focus::FocusStyles,
    layout::{ScrollAreas, UiLayout, inner_scroll_areas},
    widgets::{
        ScrollbarTrackState, draw_scrollbar, edit_input_style, editor_view, label_style,
        panel_block, request_status_style, request_status_symbol, scroll_offset,
        scrollbar_offset_from_drag, scrollbar_offset_from_track, scrollbar_track_state,
        section_style, wrapped_line_count,
    },
};
use crate::app::{
    App, RequestStatus, ResponseSelection, ResponseTab, ResponseTextPoint, ScrollDragTarget,
};
use ratatui::{
    Frame,
    layout::{Alignment, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ResponseLayout {
    pub(super) status: Rect,
    pub(super) body: ScrollAreas,
}

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

fn response_scrollbar(app: &App, areas: UiLayout) -> Option<ScrollbarTrackState> {
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
    app.view.scroll_drag_target = Some(ScrollDragTarget::Response);
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

pub(super) fn begin_response_selection(
    app: &mut App,
    column: u16,
    row: u16,
    areas: UiLayout,
) -> bool {
    let Some(point) = response_text_point(app, column, row, areas) else {
        return false;
    };
    app.view.response.selection = Some(ResponseSelection {
        anchor: point,
        head: point,
        text: String::new(),
        dragging: true,
    });
    true
}

pub(super) fn update_response_selection(app: &mut App, column: u16, row: u16, areas: UiLayout) {
    let Some(point) = response_text_point(app, column, row, areas) else {
        return;
    };
    let Some((anchor, dragging)) = app
        .view
        .response
        .selection
        .as_ref()
        .map(|selection| (selection.anchor, selection.dragging))
    else {
        return;
    };
    if !dragging {
        return;
    }
    let text = selected_response_text(app, anchor, point);
    if let Some(selection) = app.view.response.selection.as_mut() {
        selection.head = point;
        selection.text = text;
    }
}

pub(super) fn finish_response_selection(app: &mut App, column: u16, row: u16, areas: UiLayout) {
    if !app
        .view
        .response
        .selection
        .as_ref()
        .is_some_and(|selection| selection.dragging)
    {
        return;
    }
    update_response_selection(app, column, row, areas);
    let text = app
        .view
        .response
        .selection
        .as_mut()
        .map(|selection| {
            selection.dragging = false;
            selection.text.clone()
        })
        .unwrap_or_default();
    app.copy_response_selection(text);
}

fn response_text_point(
    app: &App,
    column: u16,
    row: u16,
    areas: UiLayout,
) -> Option<ResponseTextPoint> {
    if app.view.response.active_tab == ResponseTab::Headers {
        return None;
    }
    let content = response_sections(areas.response).body.content;
    if !contains(content, column, row) {
        return None;
    }
    let document = app.current_response_document()?;
    let content_line = app
        .view
        .response
        .scroll
        .offset()
        .saturating_add(usize::from(row - content.y));
    let line = content_line.checked_sub(1)?;
    if line >= document.line_count() {
        return None;
    }
    let text = response_document_line(app, line)?;
    let display_column = usize::from(column - content.x);
    Some(ResponseTextPoint {
        line,
        grapheme: grapheme_at_column(&text, display_column),
    })
}

fn response_document_line(app: &App, line: usize) -> Option<String> {
    let document = app.current_response_document()?;
    let rendered = document.visible_lines(line, 1, &app.global_config.theme)?;
    rendered.first().map(|line| {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    })
}

fn grapheme_at_column(text: &str, column: usize) -> usize {
    let mut width = 0_usize;
    for (index, grapheme) in text.graphemes(true).enumerate() {
        let next = width.saturating_add(Line::from(grapheme).width());
        if column < next {
            return index;
        }
        width = next;
    }
    text.graphemes(true).count()
}

fn selected_response_text(app: &App, anchor: ResponseTextPoint, head: ResponseTextPoint) -> String {
    let (start, end) = if anchor <= head {
        (anchor, head)
    } else {
        (head, anchor)
    };
    let mut selected = String::new();
    for line in start.line..=end.line {
        let Some(text) = response_document_line(app, line) else {
            continue;
        };
        let graphemes = text.graphemes(true).collect::<Vec<_>>();
        let from = if line == start.line {
            start.grapheme.min(graphemes.len())
        } else {
            0
        };
        let to = if line == end.line {
            end.grapheme.min(graphemes.len())
        } else {
            graphemes.len()
        };
        if line > start.line {
            selected.push('\n');
        }
        selected.extend(graphemes[from..to].iter().copied());
    }
    selected
}

pub(super) fn response_sections(area: Rect) -> ResponseLayout {
    let inner = area.inner(Margin::new(1, 1));
    if inner.is_empty() {
        return ResponseLayout::default();
    }
    ResponseLayout {
        status: Rect::new(inner.x, inner.y, inner.width, u16::from(inner.height > 0)),
        body: inner_scroll_areas(Rect::new(
            inner.x,
            inner.y.saturating_add(2),
            inner.width,
            inner.height.saturating_sub(2),
        )),
    }
}

pub(super) fn response_search_area(area: Rect) -> Rect {
    let status = response_sections(area).status;
    Rect::new(status.x, status.y.saturating_add(1), status.width, 1)
        .intersection(area.inner(Margin::new(1, 1)))
}

pub(super) fn draw_response(frame: &mut Frame<'_>, area: Rect, app: &App) {
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
        let metadata = response_search_area(area);
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
            let search = &app.view.response.search_query;
            let search_suffix =
                if search.is_empty() || app.view.response.active_tab == ResponseTab::Headers {
                    String::new()
                } else {
                    format!("   / {search}")
                };
            let mut tabs = Vec::new();
            for (index, (tab, label)) in response_tab_labels(app).into_iter().enumerate() {
                if index > 0 {
                    tabs.push(Span::raw("  "));
                }
                let style = if app.view.response.active_tab == tab {
                    Style::default()
                        .fg(theme.accent)
                        .bg(theme.selection)
                        .add_modifier(Modifier::BOLD)
                } else {
                    label_style(theme)
                };
                tabs.push(Span::styled(label, style));
            }
            tabs.push(Span::styled(
                format!("  ←/→{search_suffix}   ·   {}", response.final_url),
                label_style(theme),
            ));
            frame.render_widget(Paragraph::new(Line::from(tabs)), metadata);
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
            if let Some(selection) = app.view.response.selection.as_ref() {
                for (index, line) in visible.iter_mut().enumerate() {
                    style_response_selection(line, body_start + index, selection, theme);
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

fn style_response_selection(
    line: &mut Line<'static>,
    line_index: usize,
    selection: &ResponseSelection,
    theme: &crate::settings::UiTheme,
) {
    let (start, end) = selection.ordered();
    if start == end || line_index < start.line || line_index > end.line {
        return;
    }
    let selected_start = if line_index == start.line {
        start.grapheme
    } else {
        0
    };
    let selected_end = if line_index == end.line {
        end.grapheme
    } else {
        usize::MAX
    };
    let selection_style = Style::default()
        .bg(theme.selection)
        .add_modifier(Modifier::BOLD);
    let mut grapheme_index = 0;
    let mut spans = Vec::new();
    for span in std::mem::take(&mut line.spans) {
        for grapheme in span.content.graphemes(true) {
            let style = if (selected_start..selected_end).contains(&grapheme_index) {
                span.style.patch(selection_style)
            } else {
                span.style
            };
            spans.push(Span::styled(grapheme.to_string(), style));
            grapheme_index = grapheme_index.saturating_add(1);
        }
    }
    line.spans = spans;
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

fn response_tab_labels(app: &App) -> [(ResponseTab, String); 3] {
    [
        (ResponseTab::Raw, "Raw"),
        (ResponseTab::Formatted, "Formatted"),
        (ResponseTab::Headers, app.text().response_headers_tab()),
    ]
    .map(|(tab, label)| (tab, format!(" {label} ")))
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
