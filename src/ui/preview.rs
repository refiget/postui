use super::{
    focus::FocusStyles,
    inline_editor::draw_inline_editor,
    temporary_variables::draw_temporary_variables,
    widgets::{
        coordinate, edit_input_style, editor_view_with_cursor, label_style, method_style,
        panel_block, place_cursor, section_style, truncate_line,
    },
};
use crate::{
    app::{App, PreviewTab},
    highlight, http_method,
};
use ratatui::{
    Frame,
    layout::{Alignment, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub(super) fn draw_preview(
    frame: &mut Frame<'_>,
    area: Rect,
    summary: Rect,
    tabs: Rect,
    content: Rect,
    app: &App,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let focus = FocusStyles::new(app.view.focus, theme);
    if !app.has_current_request() {
        frame.render_widget(
            panel_block(text.request_editor(), area, theme).border_style(focus.preview_border()),
            area,
        );
        let empty = area.inner(Margin::new(2, 2));
        frame.render_widget(
            Paragraph::new(Span::styled(text.no_requests(), label_style(theme)))
                .alignment(Alignment::Center),
            empty,
        );
        return;
    }
    let Some(request) = app.current_request() else {
        return;
    };
    let title = Line::from(vec![
        Span::styled(
            format!("{}  ", text.request_editor()),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            request.name.clone(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
    ]);
    frame.render_widget(
        panel_block(title, area, theme).border_style(focus.preview_border()),
        area,
    );
    draw_preview_summary(frame, summary, app);
    draw_preview_tabs(frame, tabs, app);
    draw_preview_content(frame, content, app);
}

pub(super) fn draw_preview_summary(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() {
        return;
    }
    let lines = request_summary_lines(app)
        .into_iter()
        .map(|line| truncate_line(line, usize::from(area.width)))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

pub(super) fn preview_summary_height(_app: &App, width: u16) -> u16 {
    if width == 0 {
        return 0;
    }
    2
}

fn request_summary_lines(app: &App) -> Vec<Line<'static>> {
    let theme = &app.global_config.theme;
    let text = app.text();
    let Some(request) = app.current_request() else {
        return Vec::new();
    };
    let method = app
        .request_draft(&request.id)
        .map(|draft| draft.method.as_str())
        .unwrap_or(&request.method);
    let mut url_line = vec![
        Span::styled(format!("[ {} ]", method), method_style(method, theme)),
        Span::raw("  "),
    ];
    url_line.push(Span::styled(
        format!("{}  ", text.address()),
        label_style(theme),
    ));
    let url = app.display_url(request);
    if url.is_empty() {
        url_line.push(Span::styled(
            text.enter_url(),
            highlight::plain_style(theme),
        ));
    } else {
        url_line.extend(highlight::template_spans(
            &url,
            highlight::plain_style(theme),
            theme,
        ));
    }
    let description = if request.description.is_empty() {
        text.empty_description()
    } else {
        request.description.as_str()
    };
    let mut description_line = vec![Span::styled(
        format!("{}  ", text.description()),
        label_style(theme),
    )];
    description_line.extend(highlight::template_spans(
        description,
        highlight::plain_style(theme),
        theme,
    ));
    let mut lines = vec![Line::from(url_line), Line::from(description_line)];
    if http_method::parse(method).is_err() {
        lines[0] = Line::from(Span::styled(
            text.invalid_method(method),
            Style::default().fg(theme.warning),
        ));
    }
    lines
}

pub(super) fn draw_preview_tabs(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() {
        return;
    }
    let theme = &app.global_config.theme;
    let tabs = PreviewTab::all();
    let mut line = Vec::new();
    for (index, tab) in tabs.into_iter().enumerate() {
        if index > 0 {
            line.push(Span::raw(" "));
        }
        let active = app.view.preview.active_tab == tab;
        let color = match tab {
            PreviewTab::Body => theme.primary,
            PreviewTab::Params => theme.variable,
            PreviewTab::Headers => theme.secondary,
        };
        let fill = if active { color } else { theme.surface };
        let label = preview_tab_label(tab, app);
        let body = if let Some(body) = label.strip_prefix(' ') {
            body.to_string()
        } else {
            label
        };
        line.push(Span::styled(
            if active { "▌" } else { " " },
            Style::default().fg(theme.accent).bg(fill),
        ));
        let style = if active {
            Style::default()
                .fg(theme.background)
                .bg(fill)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted).bg(fill)
        };
        line.push(Span::styled(body, style));
    }
    frame.render_widget(Paragraph::new(Line::from(line)), area);
}

pub(super) fn draw_preview_content(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() {
        return;
    }
    match app.view.preview.active_tab {
        PreviewTab::Body => draw_body_editor(frame, area, app),
        tab @ (PreviewTab::Params | PreviewTab::Headers) => {
            if let Some(dialog) = app
                .view
                .dialog
                .as_ref()
                .filter(|dialog| dialog.preview_tab() == Some(tab))
            {
                draw_inline_editor(frame, area, app, dialog);
            } else if let Some(dialog) = app.preview_dialog(tab) {
                draw_inline_editor(frame, area, app, &dialog);
            }
        }
    }
}

pub(super) fn draw_body_editor(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let Some(request) = app.current_effective_request() else {
        return;
    };
    if app.temporary_variables_visible() {
        draw_temporary_variables(frame, area, app);
        return;
    }
    let has_body = !request.body_parts.is_empty();
    let offset = usize::from(app.view.preview.scroll.offset());
    let body_value = (has_body || app.body_editor().is_some()).then(|| {
        app.body_editor()
            .map_or_else(|| app.body_preview(), |editor| editor.display_document())
    });
    let mut lines = if let Some(value) = &body_value {
        highlight::json_text_lines_window(value, offset, usize::from(area.height), theme)
    } else {
        request_content_lines(app, &request)
            .into_iter()
            .skip(offset)
            .take(usize::from(area.height))
            .collect()
    };
    if let Some(value) = &body_value {
        underline_json_values(value, offset, &mut lines);
    }

    frame.render_widget(Paragraph::new(lines), area);
    if let Some(editor) = app.body_editor() {
        let (editor_line, editor_column) = editor.position();
        let scroll = usize::from(app.view.preview.scroll.offset());
        if editor_line >= scroll && editor_line < scroll + usize::from(area.height) {
            let input_area = Rect::new(
                area.x.saturating_add(coordinate(editor_column)),
                area.y.saturating_add((editor_line - scroll) as u16),
                coordinate(editor.display_width()),
                1,
            )
            .intersection(area);
            if !input_area.is_empty() {
                let (value, cursor_width) =
                    editor_view_with_cursor(&editor.input, usize::from(input_area.width));
                frame.render_widget(
                    Paragraph::new(value).style(edit_input_style(
                        &editor.input,
                        theme,
                        theme.text,
                        theme.background,
                    )),
                    input_area,
                );
                if let Some(cursor_width) = cursor_width {
                    place_cursor(frame, input_area, cursor_width, 0);
                }
            }
        }
    }
    if let Some(editor) = app.file_editor() {
        let scroll = usize::from(app.view.preview.scroll.offset());
        if editor.line >= scroll && editor.line < scroll + usize::from(area.height) {
            let editor_column = coordinate(editor.column);
            let input_area = Rect::new(
                area.x.saturating_add(editor_column),
                area.y.saturating_add((editor.line - scroll) as u16),
                area.width.saturating_sub(editor_column).max(1),
                1,
            )
            .intersection(area);
            if !input_area.is_empty() {
                let (value, cursor_width) =
                    editor_view_with_cursor(&editor.input, usize::from(input_area.width));
                frame.render_widget(
                    Paragraph::new(value).style(edit_input_style(
                        &editor.input,
                        theme,
                        theme.text,
                        theme.background,
                    )),
                    input_area,
                );
                if let Some(cursor_width) = cursor_width {
                    place_cursor(frame, input_area, cursor_width, 0);
                }
            }
        }
    }
}

fn underline_json_values(value: &str, offset: usize, lines: &mut [Line<'static>]) {
    let ranges = crate::editor::json_scalar_ranges(value);
    let mut line_start = value
        .split_inclusive('\n')
        .take(offset)
        .map(str::len)
        .sum::<usize>();
    for line in lines {
        let mut span_start = line_start;
        let mut underlined = Vec::new();
        for span in std::mem::take(&mut line.spans) {
            let text = span.content.as_ref();
            let span_end = span_start + text.len();
            let mut boundaries = vec![0, text.len()];
            for range in &ranges {
                if range.start < span_end && range.end > span_start {
                    boundaries.push(range.start.saturating_sub(span_start));
                    boundaries.push(range.end.min(span_end) - span_start);
                }
            }
            boundaries.sort_unstable();
            boundaries.dedup();
            for part in boundaries.windows(2) {
                let start = part[0];
                let end = part[1];
                if start == end {
                    continue;
                }
                let position = span_start + start;
                let style = if ranges.iter().any(|range| range.contains(&position)) {
                    span.style.add_modifier(Modifier::UNDERLINED)
                } else {
                    span.style
                };
                underlined.push(Span::styled(text[start..end].to_string(), style));
            }
            span_start = span_end;
        }
        line.spans = underlined;
        line_start = line_start.saturating_add(
            value[line_start..]
                .find('\n')
                .map_or(value.len() - line_start, |index| index + 1),
        );
    }
}

fn request_content_lines(app: &App, request: &crate::config::ApiRequest) -> Vec<Line<'static>> {
    let theme = &app.global_config.theme;
    let text = app.text();
    let mut lines = Vec::new();
    if !request.form.is_empty() {
        lines.push(Line::from(Span::styled(text.form(), section_style(theme))));
        for field in &request.form {
            lines.push(content_value_line(
                &field.name,
                &field.value,
                theme.text,
                theme,
            ));
        }
    }

    if !request.files.is_empty() {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(text.files(), section_style(theme))));
        for file in &request.files {
            lines.push(content_value_line(
                &file.field,
                &file.path,
                theme.text,
                theme,
            ));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            text.no_content(),
            label_style(theme),
        )));
    }
    lines
}

fn content_value_line(
    name: &str,
    value: &str,
    value_color: ratatui::style::Color,
    theme: &crate::settings::UiTheme,
) -> Line<'static> {
    let mut line = Line::from(Span::styled(format!("{name}  "), label_style(theme)));
    line.spans.extend(highlight::template_spans(
        value,
        Style::default()
            .fg(value_color)
            .add_modifier(Modifier::UNDERLINED),
        theme,
    ));
    line
}

pub(super) fn preview_tab_label(tab: PreviewTab, app: &App) -> String {
    let text = app.text();
    match tab {
        PreviewTab::Body
            if app
                .current_resolved_request()
                .is_none_or(|request| request.raw_body.is_none()) =>
        {
            format!(" {} ", text.content())
        }
        PreviewTab::Body => format!(" {} ", tab.label(text)),
        PreviewTab::Params => format!(" {} {} ", tab.label(text), app.current_param_count()),
        PreviewTab::Headers => format!(" {} {} ", tab.label(text), app.current_header_count()),
    }
}

pub(super) fn preview_tab_at(area: Rect, column: u16, app: &App) -> Option<PreviewTab> {
    if area.is_empty() || column < area.x || column >= area.right() {
        return None;
    }
    let relative = usize::from(column.saturating_sub(area.x));
    let mut start = 0;
    for (index, tab) in PreviewTab::all().into_iter().enumerate() {
        if index > 0 {
            start += 1;
        }
        let end = start + crate::editor::terminal_width(&preview_tab_label(tab, app));
        if (start..end).contains(&relative) {
            return Some(tab);
        }
        start = end;
    }
    None
}
