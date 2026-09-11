use super::*;

pub(super) fn draw_preview(
    frame: &mut Frame<'_>,
    area: Rect,
    details: Rect,
    edit_button: Rect,
    send_button: Rect,
    app: &App,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let request = app.current_request();
    let url = template::display_url(request);
    let request_status = app.request_status(&request.id);
    let loading = request_status == RequestStatus::Sending;
    let focused_action = app.focused_preview_action();
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
        Span::styled(
            format!("[{}]", request_status.tag()),
            request_status_style(request_status, theme),
        ),
    ]);
    let focus = FocusStyles::new(app.focus, theme);
    frame.render_widget(
        panel_block(title, area, theme).border_style(focus.preview_border()),
        area,
    );
    let sections = preview_sections(details);
    draw_preview_summary(frame, sections.summary, app, &url, request_status);
    draw_preview_tabs(frame, sections.tabs, app);
    draw_preview_content(frame, sections.content, app);

    let edit_action = PreviewAction::Edit(app.preview_state.active_tab);
    let edit_label = if app.editing_preview_tab() == Some(app.preview_state.active_tab) {
        text.apply()
    } else if app.preview_state.active_tab == PreviewTab::Body && app.body_editor().is_some() {
        text.close()
    } else {
        match app.preview_state.active_tab {
            PreviewTab::Body => text.edit_body(),
            PreviewTab::Params => text.edit_params(),
            PreviewTab::Headers => text.edit_headers(),
        }
    };
    let actions = [
        (edit_button, edit_action, edit_label),
        (send_button, PreviewAction::Send, text.send_button(loading)),
    ];
    for (area, action, label) in actions {
        if area.is_empty() {
            continue;
        }
        let disabled = !app.can_execute_preview_action(action);
        let button_state = preview_action_button_state(disabled, focused_action == Some(action));
        if matches!(action, PreviewAction::Send) {
            frame.render_widget(send_button_widget(label, &button_state, theme), area);
        } else {
            frame.render_widget(action_button_widget(label, &button_state, theme), area);
        }
    }
}

pub(super) fn draw_preview_summary(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    url: &str,
    request_status: RequestStatus,
) {
    if area.is_empty() {
        return;
    }
    let theme = &app.global_config.theme;
    let text = app.text();
    let request = app.current_request();
    let mut method_line = vec![
        Span::styled(
            request.method.as_str(),
            method_style(&request.method, theme),
        ),
        Span::styled(
            format!("  {}  ", request_status.label(text)),
            request_status_style(request_status, theme),
        ),
    ];
    method_line.push(Span::styled(
        format!("{}  ", text.address()),
        label_style(theme),
    ));
    method_line.extend(highlight::template_spans(
        url,
        highlight::plain_style(theme),
        theme,
    ));
    let mut lines = vec![Line::from(method_line)];
    if area.height >= 2 {
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
        lines.push(Line::from(description_line));
    }
    if !supports_method(&request.method) && area.height >= 2 {
        lines[1] = Line::from(Span::styled(
            text.unsupported_method(&request.method),
            Style::default().fg(theme.warning),
        ));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

pub(super) fn draw_preview_tabs(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() {
        return;
    }
    let theme = &app.global_config.theme;
    let text = app.text();
    let tabs = PreviewTab::all();
    let mut line = Vec::new();
    for (index, tab) in tabs.into_iter().enumerate() {
        if index > 0 {
            line.push(Span::raw("  "));
        }
        let style = if app.preview_state.active_tab == tab {
            Style::default()
                .fg(theme.background)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        line.push(Span::styled(preview_tab_label(tab, app), style));
    }
    let hint = match app.preview_state.active_tab {
        PreviewTab::Body => text.json_value_hint(),
        PreviewTab::Params | PreviewTab::Headers => text.editable_value_hint(),
    };
    line.push(Span::styled(
        format!("  {hint}"),
        Style::default().fg(theme.muted),
    ));
    frame.render_widget(Paragraph::new(Line::from(line)), area);
}

pub(super) fn draw_preview_content(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() {
        return;
    }
    if let Some(dialog) = app
        .dialog
        .as_ref()
        .filter(|dialog| dialog.preview_tab() == Some(app.preview_state.active_tab))
    {
        draw_inline_editor(frame, area, app, dialog);
        return;
    }
    match app.preview_state.active_tab {
        PreviewTab::Body => draw_body_editor(frame, area, app),
        PreviewTab::Params => draw_params_preview(frame, area, app),
        PreviewTab::Headers => draw_headers_preview(frame, area, app),
    }
}

pub(super) fn draw_body_editor(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let value = app
        .body_editor()
        .map_or_else(|| app.body_preview(), |editor| editor.display_document());
    let lines = highlight::json_text_lines(&value, theme);

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.preview_state.scroll.offset(), 0)),
        area,
    );
    if let Some(editor) = app.body_editor() {
        let (editor_line, editor_column) = editor.position();
        let scroll = usize::from(app.preview_state.scroll.offset());
        if editor_line >= scroll && editor_line < scroll + usize::from(area.height) {
            let input_area = Rect::new(
                area.x.saturating_add(editor_column as u16),
                area.y.saturating_add((editor_line - scroll) as u16),
                u16::try_from(editor.input.value.chars().count().max(1)).unwrap_or(u16::MAX),
                1,
            );
            frame.render_widget(
                Paragraph::new(editor.input.value.clone()).style(
                    Style::default()
                        .fg(theme.text)
                        .bg(theme.selection)
                        .add_modifier(Modifier::UNDERLINED),
                ),
                input_area,
            );
            frame.set_cursor_position((
                input_area.x.saturating_add(
                    editor.input.value[..editor.input.cursor].chars().count() as u16,
                ),
                input_area.y,
            ));
        }
    }
}

pub(super) fn inline_dialog_layout(area: Rect) -> DialogLayout {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(area.height.min(1)), Constraint::Min(0)])
        .split(area);
    DialogLayout {
        area,
        table_header: sections[0],
        rows: inner_scroll_areas(sections[1]),
        add_button: Rect::default(),
        apply_button: Rect::default(),
        close_button: Rect::default(),
    }
}

pub(super) fn draw_inline_editor(frame: &mut Frame<'_>, area: Rect, app: &App, dialog: &Dialog) {
    let layout = inline_dialog_layout(area);
    match dialog {
        Dialog::Headers(dialog) => draw_headers_dialog(frame, app, dialog, layout),
        Dialog::Params(dialog) => draw_params_dialog(frame, app, dialog, layout),
        Dialog::Variables(_) => {}
    }
}

pub(super) fn handle_inline_editor_click(app: &mut App, column: u16, row: u16, area: Rect) {
    let layout = inline_dialog_layout(area);
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
            let widths = header_table_widths(layout.rows.content.width);
            let name_start = layout
                .rows
                .content
                .x
                .saturating_add(constraint_length(widths[0]))
                .saturating_add(1);
            let value_start = name_start
                .saturating_add(constraint_length(widths[1]))
                .saturating_add(1);
            let value_end = value_start.saturating_add(constraint_length(widths[2]));
            if column < name_start {
                app.toggle_header_row(index);
            } else if column >= value_start && column < value_end {
                app.click_header_row(index, HeaderField::Value, true);
            } else {
                app.click_header_row(index, HeaderField::Name, false);
            }
        }
        Some(Dialog::Params(_)) => {
            let widths = param_table_widths(layout.rows.content.width);
            let value_start = layout
                .rows
                .content
                .x
                .saturating_add(constraint_length(widths[0]))
                .saturating_add(constraint_length(widths[1]))
                .saturating_add(constraint_length(widths[2]))
                .saturating_add(3);
            let field = if column < value_start {
                HeaderField::Name
            } else {
                HeaderField::Value
            };
            app.click_param_row(index, field, column >= value_start);
        }
        _ => {}
    }
}

pub(super) fn draw_params_preview(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let request = app.current_resolved_request();
    let mut lines = Vec::new();
    for part in &request.query_parts {
        lines.push(parameter_line(text.query(), part, theme));
    }
    for (name, value) in &request.form {
        lines.push(parameter_line(
            text.form(),
            &format!("{name}={value}"),
            theme,
        ));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            text.no_params(),
            label_style(theme),
        )));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

pub(super) fn draw_headers_preview(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let resolved = app.current_resolved_request();
    let mut lines = Vec::new();
    for (name, value) in resolved.headers.iter().take(4) {
        let value = truncate(
            value,
            usize::from(
                area.width.saturating_sub(
                    u16::try_from(name.chars().count())
                        .unwrap_or(u16::MAX)
                        .saturating_add(4),
                ),
            ),
        );
        lines.push(Line::from(vec![
            Span::styled(format!("{name}: "), section_style(theme)),
            Span::styled(value, Style::default().fg(theme.text)),
        ]));
    }
    if resolved.headers.len() > 4 {
        lines.push(Line::from(Span::styled(
            text.remaining_items(resolved.headers.len() - 4),
            label_style(theme),
        )));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            text.no_headers(),
            label_style(theme),
        )));
    }
    lines.push(Line::from(Span::styled(
        format!("h / Enter  {}", text.edit_headers()),
        Style::default().fg(theme.secondary),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

pub(super) fn parameter_line(
    kind: &str,
    value: &str,
    theme: &crate::settings::UiTheme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{kind}  "), section_style(theme)),
        Span::styled(value.to_string(), Style::default().fg(theme.text)),
    ])
}

pub(super) fn preview_tab_label(tab: PreviewTab, app: &App) -> String {
    let text = app.text();
    match tab {
        PreviewTab::Body => format!(" {} ", tab.label(text)),
        PreviewTab::Params => format!(" + {} · {} ", tab.label(text), app.current_param_count()),
        PreviewTab::Headers => format!(" + {} · {} ", tab.label(text), app.current_header_count()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PreviewTabHit {
    Activate(PreviewTab),
    Add(PreviewTab),
}

pub(super) fn preview_tab_at(area: Rect, column: u16, app: &App) -> Option<PreviewTabHit> {
    if area.is_empty() || column < area.x || column >= area.right() {
        return None;
    }
    let relative = usize::from(column.saturating_sub(area.x));
    let mut start = 0;
    for (index, tab) in PreviewTab::all().into_iter().enumerate() {
        if index > 0 {
            start += 2;
        }
        let end = start + preview_tab_label(tab, app).chars().count();
        if (start..end).contains(&relative) {
            let add = tab != PreviewTab::Body && relative < start.saturating_add(3);
            return Some(if add {
                PreviewTabHit::Add(tab)
            } else {
                PreviewTabHit::Activate(tab)
            });
        }
        start = end;
    }
    None
}
