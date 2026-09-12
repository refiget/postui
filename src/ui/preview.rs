use super::*;

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
    if !app.has_current_request() {
        frame.render_widget(panel_block(text.request_editor(), area, theme), area);
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
    let request_status = app.request_status(&request.id);
    let mut title = Line::from(vec![
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
    title.extend(request_status_spans(
        request_status,
        text,
        theme,
        app.animation_frame,
    ));
    let focus = FocusStyles::new(app.focus, theme);
    frame.render_widget(
        focused_panel_block(title, area, theme, focus.preview_focused())
            .border_style(focus.preview_border()),
        area,
    );
    draw_preview_summary(frame, summary, app, request_status);
    draw_preview_tabs(frame, tabs, app);
    draw_preview_content(frame, content, app);
}

pub(super) fn draw_preview_summary(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    request_status: RequestStatus,
) {
    if area.is_empty() {
        return;
    }
    let theme = &app.global_config.theme;
    let text = app.text();
    let Some(request) = app.current_request() else {
        return;
    };
    let mut url_line = vec![
        Span::styled(
            format!("[ {} ]", request.method),
            method_style(&request.method, theme),
        ),
        Span::raw("  "),
    ];
    url_line.push(Span::styled(
        format!("{}  ", text.address()),
        label_style(theme),
    ));
    if let Some(editor) = &app.preview_state.url_editor {
        url_line.push(Span::styled(
            if editor.value().is_empty() {
                " ".to_string()
            } else {
                editor.value().to_string()
            },
            Style::default()
                .fg(theme.text)
                .bg(theme.selection)
                .add_modifier(Modifier::UNDERLINED),
        ));
    } else {
        let url = app.resolved_url(request);
        url_line.push(Span::styled(
            if url.is_empty() {
                text.enter_url().to_string()
            } else {
                url
            },
            highlight::plain_style(theme),
        ));
    }
    let mut lines = wrap_spans(url_line, area.width);
    if area.height > lines.len() as u16 {
        let description = if request.description.is_empty() {
            text.empty_description()
        } else {
            request.description.as_str()
        };
        let mut description_line =
            request_status_spans(request_status, text, theme, app.animation_frame);
        description_line.push(Span::styled("  ·  ", label_style(theme)));
        description_line.push(Span::styled(
            format!("{}  ", text.description()),
            label_style(theme),
        ));
        description_line.extend(highlight::template_spans(
            description,
            highlight::plain_style(theme),
            theme,
        ));
        lines.push(Line::from(description_line));
    }
    if !supports_method(&request.method) && area.height > 0 {
        lines[0] = Line::from(Span::styled(
            text.unsupported_method(&request.method),
            Style::default().fg(theme.warning),
        ));
    }
    frame.render_widget(Paragraph::new(lines), area);
    if let Some(editor) = &app.preview_state.url_editor {
        let prefix = Line::from(format!("[ {} ]  {}  ", request.method, text.address())).width();
        let position = prefix + editor.cursor_width();
        let width = usize::from(area.width.max(1));
        let row = (position / width).min(usize::from(area.height.saturating_sub(1)));
        let column = position % width;
        frame.set_cursor_position((
            area.x
                .saturating_add(u16::try_from(column).unwrap_or(u16::MAX)),
            area.y
                .saturating_add(u16::try_from(row).unwrap_or(u16::MAX)),
        ));
    }
}

fn wrap_spans(spans: Vec<Span<'static>>, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width.max(1));
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut current_width: usize = 0;

    for span in spans {
        for character in span.content.chars() {
            let character_text = character.to_string();
            let character_width = Line::from(character_text.clone()).width();
            if !current.is_empty() && current_width.saturating_add(character_width) > width {
                lines.push(Line::from(std::mem::take(&mut current)));
                current_width = 0;
            }
            current.push(Span::styled(character_text, span.style));
            current_width = current_width.saturating_add(character_width);
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(Line::from(current));
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
            line.push(Span::styled(" │", Style::default().fg(theme.muted)));
        }
        let active = app.preview_state.active_tab == tab;
        let style = if active {
            Style::default()
                .fg(theme.text)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default()
                .fg(theme.text)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        };
        line.push(Span::styled(preview_tab_label(tab, app), style));
    }
    frame.render_widget(Paragraph::new(Line::from(line)), area);
}

pub(super) fn draw_preview_content(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() {
        return;
    }
    match app.preview_state.active_tab {
        PreviewTab::Body => draw_body_editor(frame, area, app),
        tab @ (PreviewTab::Params | PreviewTab::Headers) => {
            if let Some(dialog) = app
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
    let Some(resolved) = app.current_resolved_request() else {
        return;
    };
    let has_body = resolved.raw_body.is_some();
    let lines = if has_body || app.body_editor().is_some() {
        let value = app
            .body_editor()
            .map_or_else(|| app.body_preview(), |editor| editor.display_document());
        highlight::json_text_lines(&value, theme)
    } else {
        request_content_lines(app, &resolved)
    };

    frame.render_widget(
        Paragraph::new(lines).scroll((app.preview_state.scroll.offset(), 0)),
        area,
    );
    if let Some(editor) = app.body_editor() {
        let (editor_line, editor_column) = editor.position();
        let scroll = usize::from(app.preview_state.scroll.offset());
        if editor_line >= scroll && editor_line < scroll + usize::from(area.height) {
            let input_area = Rect::new(
                area.x.saturating_add(editor_column as u16),
                area.y.saturating_add((editor_line - scroll) as u16),
                u16::try_from(crate::editor::terminal_width(editor.input.value()).max(1))
                    .unwrap_or(u16::MAX),
                1,
            );
            frame.render_widget(
                Paragraph::new(editor.input.value()).style(
                    Style::default()
                        .fg(theme.text)
                        .bg(theme.selection)
                        .add_modifier(Modifier::UNDERLINED),
                ),
                input_area,
            );
            frame.set_cursor_position((
                input_area
                    .x
                    .saturating_add(u16::try_from(editor.input.cursor_width()).unwrap_or(u16::MAX)),
                input_area.y,
            ));
        }
    }
    if let Some(editor) = app.file_editor() {
        let scroll = usize::from(app.preview_state.scroll.offset());
        if editor.line >= scroll && editor.line < scroll + usize::from(area.height) {
            let input_area = Rect::new(
                area.x.saturating_add(editor.column as u16),
                area.y.saturating_add((editor.line - scroll) as u16),
                area.width.saturating_sub(editor.column as u16).max(1),
                1,
            );
            frame.render_widget(
                Paragraph::new(editor.input.value()).style(
                    Style::default()
                        .fg(theme.text)
                        .bg(theme.selection)
                        .add_modifier(Modifier::UNDERLINED),
                ),
                input_area,
            );
            frame.set_cursor_position((
                input_area
                    .x
                    .saturating_add(u16::try_from(editor.input.cursor_width()).unwrap_or(u16::MAX)),
                input_area.y,
            ));
        }
    }
    if let Some(editor) = app.variable_editor() {
        let scroll = usize::from(app.preview_state.scroll.offset());
        if editor.line >= scroll && editor.line < scroll + usize::from(area.height) {
            let input_area = Rect::new(
                area.x.saturating_add(editor.column as u16),
                area.y.saturating_add((editor.line - scroll) as u16),
                area.width.saturating_sub(editor.column as u16).max(1),
                1,
            );
            let input = if editor.input.value().is_empty() {
                " "
            } else {
                editor.input.value()
            };
            frame.render_widget(
                Paragraph::new(input).style(
                    Style::default()
                        .fg(theme.text)
                        .bg(theme.selection)
                        .add_modifier(Modifier::UNDERLINED),
                ),
                input_area,
            );
            frame.set_cursor_position((
                input_area
                    .x
                    .saturating_add(u16::try_from(editor.input.cursor_width()).unwrap_or(u16::MAX)),
                input_area.y,
            ));
        }
    }
}

fn request_content_lines(
    app: &App,
    request: &crate::template::ResolvedRequest,
) -> Vec<Line<'static>> {
    let theme = &app.global_config.theme;
    let text = app.text();
    let mut lines = Vec::new();
    let variables = app.current_url_variables();
    let has_content = !request.form.is_empty() || !request.files.is_empty();

    for variable in variables {
        lines.push(request_variable_line(app, &variable, theme));
    }
    if !lines.is_empty() && has_content {
        lines.push(Line::default());
    }

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

pub(super) fn request_variable_line(
    app: &App,
    variable: &str,
    theme: &crate::settings::UiTheme,
) -> Line<'static> {
    let editing_value = app
        .variable_editor()
        .filter(|editor| editor.variable == variable)
        .map(|editor| editor.input.value().to_string());
    let value = editing_value.unwrap_or_else(|| app.request_variable_value(variable));
    let display = if value.is_empty() {
        "__".to_string()
    } else {
        value
    };
    Line::from(vec![
        Span::styled(format!("{variable}: "), highlight::variable_style(theme)),
        Span::styled(
            display,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::UNDERLINED),
        ),
    ])
}

pub(super) fn request_variable_at(area: Rect, column: u16, row: u16, app: &App) -> Option<String> {
    if area.is_empty() || !contains(area, column, row) {
        return None;
    }
    if app
        .current_resolved_request()
        .is_some_and(|request| request.raw_body.is_some())
    {
        return None;
    }
    let line = usize::from(row.saturating_sub(area.y))
        .saturating_add(usize::from(app.preview_state.scroll.offset()));
    let column = usize::from(column.saturating_sub(area.x));
    app.current_url_variables()
        .into_iter()
        .enumerate()
        .find_map(|(index, variable)| {
            if index != line {
                return None;
            }
            let value = app.request_variable_value(&variable);
            let value_width = Line::from(if value.is_empty() {
                "__".to_string()
            } else {
                value
            })
            .width()
            .max(1);
            let value_column = crate::editor::terminal_width(&variable).saturating_add(2);
            (value_column..value_column.saturating_add(value_width))
                .contains(&column)
                .then_some(variable)
        })
}

fn content_value_line(
    name: &str,
    value: &str,
    value_color: ratatui::style::Color,
    theme: &crate::settings::UiTheme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{name}  "), label_style(theme)),
        Span::styled(value.to_string(), Style::default().fg(value_color)),
    ])
}

pub(super) fn inline_dialog_layout(area: Rect, row_count: usize) -> DialogLayout {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(area.height.min(1)), Constraint::Min(0)])
        .split(area);
    let content = sections[1];
    let add_height = content.height.min(3);
    let add_y = content.y.saturating_add(
        u16::try_from(row_count)
            .unwrap_or(u16::MAX)
            .min(content.height.saturating_sub(add_height)),
    );
    let add_button = if content.is_empty() {
        Rect::default()
    } else {
        Rect::new(content.x, add_y, content.width, add_height)
    };
    let table_area = Rect::new(
        content.x,
        content.y,
        content.width,
        add_y.saturating_sub(content.y),
    );
    DialogLayout {
        area,
        table_header: sections[0],
        rows: inner_scroll_areas(table_area),
        add_button,
        apply_button: Rect::default(),
        close_button: Rect::default(),
    }
}

pub(super) fn draw_inline_editor(frame: &mut Frame<'_>, area: Rect, app: &App, dialog: &Dialog) {
    let row_count = match dialog {
        Dialog::Headers(dialog) => dialog.rows.len(),
        Dialog::Params(dialog) => dialog.rows.len(),
        Dialog::Variables(_) => 0,
    };
    let layout = inline_dialog_layout(area, row_count);
    match dialog {
        Dialog::Headers(dialog) => draw_headers_dialog(frame, app, dialog, layout),
        Dialog::Params(dialog) => draw_params_dialog(frame, app, dialog, layout),
        Dialog::Variables(_) => {}
    }
    if !layout.add_button.is_empty() && !matches!(dialog, Dialog::Variables(_)) {
        let theme = &app.global_config.theme;
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().bg(theme.selection))
                .border_style(Style::default().fg(theme.text).bg(theme.selection)),
            layout.add_button,
        );
        frame.render_widget(
            Paragraph::new("+").alignment(Alignment::Center).style(
                Style::default()
                    .fg(theme.text)
                    .bg(theme.selection)
                    .add_modifier(Modifier::BOLD),
            ),
            layout.add_button.inner(Margin::new(1, 1)),
        );
    }
}

pub(super) fn handle_inline_editor_click(app: &mut App, column: u16, row: u16, area: Rect) {
    let row_count = match app.dialog.as_ref() {
        Some(Dialog::Headers(dialog)) => dialog.rows.len(),
        Some(Dialog::Params(dialog)) => dialog.rows.len(),
        _ => return,
    };
    let layout = inline_dialog_layout(area, row_count);
    if contains(layout.add_button, column, row) {
        app.add_preview_row(app.preview_state.active_tab);
        return;
    }
    if !contains(layout.rows.content, column, row) {
        return;
    }
    let (row_count, selected) = match app.dialog.as_ref() {
        Some(Dialog::Headers(dialog)) => (dialog.rows.len(), dialog.selected),
        Some(Dialog::Params(dialog)) => (dialog.rows.len(), dialog.selected),
        _ => return,
    };
    let visible = usize::from(layout.rows.content.height);
    let offset = request_list_offset(selected, row_count, visible);
    let index = offset.saturating_add(usize::from(row - layout.rows.content.y));
    if index >= row_count {
        return;
    }
    match app.dialog.as_ref() {
        Some(Dialog::Headers(_)) => {
            let request_row = app
                .dialog
                .as_ref()
                .and_then(|dialog| match dialog {
                    Dialog::Headers(dialog) => dialog.rows.get(index),
                    _ => None,
                })
                .is_some_and(|row| row.source == HeaderSource::Request);
            let widths = inline_header_table_widths(layout.rows.content.width);
            let value_start = layout
                .rows
                .content
                .x
                .saturating_add(TABLE_HIGHLIGHT_WIDTH)
                .saturating_add(constraint_length(widths[0]))
                .saturating_add(TABLE_COLUMN_SPACING);
            if column < value_start {
                if request_row {
                    app.click_header_row(index, KeyValueField::Name, true);
                } else {
                    app.toggle_header_row(index);
                }
            } else {
                app.click_header_row(index, KeyValueField::Value, true);
            }
        }
        Some(Dialog::Params(_)) => {
            let widths = inline_param_table_widths(layout.rows.content.width);
            let value_start = layout
                .rows
                .content
                .x
                .saturating_add(TABLE_HIGHLIGHT_WIDTH)
                .saturating_add(constraint_length(widths[0]))
                .saturating_add(TABLE_COLUMN_SPACING);
            let field = if column < value_start {
                KeyValueField::Name
            } else {
                KeyValueField::Value
            };
            app.click_param_row(index, field, true);
        }
        _ => {}
    }
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
            start += 2;
        }
        let end = start + crate::editor::terminal_width(&preview_tab_label(tab, app));
        if (start..end).contains(&relative) {
            return Some(tab);
        }
        start = end;
    }
    None
}
