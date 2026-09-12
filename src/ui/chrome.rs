use super::*;

pub(super) fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let feedback = app.current_feedback();
    let (symbol, color) = match feedback {
        Some(crate::app::Feedback::Info(_)) => ("◆", theme.primary),
        Some(crate::app::Feedback::Success(_)) => ("✓", theme.success),
        Some(crate::app::Feedback::Warning(_)) => ("!", theme.warning),
        Some(crate::app::Feedback::Error(_)) => ("×", theme.error),
        _ => ("›", theme.muted),
    };
    let owner = if app.view.variables.is_some() {
        text.variables()
    } else if app.view.notice.is_some() {
        text.operation_feedback()
    } else {
        app.current_request()
            .map_or(app.config.name.as_str(), |request| request.name.as_str())
    };
    let message = feedback.map_or(text.ready(), |feedback| feedback.message());
    let hint = if app.view.prompt.is_some() {
        text.confirmation_hint()
    } else if app.is_editing() {
        text.editing_hint()
    } else if app.view.variables.is_some() {
        text.variables_page_hint()
    } else if app.view.dialog.is_some() || app.view.response.menu_selection.is_some() {
        text.menu_hint()
    } else if app.response_zoomed() {
        text.response_hint()
    } else if app.debug_mode {
        text.debug_navigation_hint()
    } else {
        text.navigation_hint()
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(format!(" {symbol} "), Style::default().fg(color)),
                Span::styled(
                    format!("{} · ", truncate(owner, 20)),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(message, Style::default().fg(color)),
            ]),
            Line::from(Span::styled(
                format!(" {hint}"),
                Style::default().fg(theme.muted),
            )),
        ])
        .style(Style::default().bg(theme.background)),
        area,
    );
}

pub(super) fn draw_header(
    frame: &mut Frame<'_>,
    area: Rect,
    content_area: Rect,
    send_button: Rect,
    app: &App,
) {
    let theme = &app.global_config.theme;
    let focus = FocusStyles::new(app.view.focus, theme);
    let mut line = vec![
        Span::styled(
            " POSTUI ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", app.config.name),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ];
    if area.width >= 110 {
        line.push(Span::styled(
            format!("  │  {}", app.workspace_path().display()),
            Style::default().fg(theme.muted),
        ));
    }
    if app.debug_mode {
        line.push(Span::styled(
            format!("  ◆ {}", theme.name),
            Style::default().fg(theme.secondary),
        ));
    }
    frame.render_widget(
        panel_block("", area, theme).border_style(focus.header_border()),
        area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(line)).style(Style::default().fg(theme.text)),
        content_area,
    );

    if !send_button.is_empty() && app.has_current_request() {
        let Some(request) = app.current_request() else {
            return;
        };
        let request_status = app.request_status(&request.id);
        let loading = request_status == RequestStatus::Sending;
        let label = if loading {
            format!(
                "{} {}",
                request_status_symbol(request_status, app.view.animation_frame),
                app.text().send_button(true)
            )
        } else {
            format!("▶ {}", app.text().send_button(false))
        };
        draw_send_button(
            frame,
            send_button,
            &label,
            app.can_execute_preview_action(PreviewAction::Send),
            app.focused_preview_action() == Some(PreviewAction::Send),
            theme,
        );
    }
}

pub(super) fn draw_request_list(frame: &mut Frame<'_>, layout: UiLayout, app: &App) {
    let area = layout.requests;
    let workspace_selector_area = layout.workspace_selector;
    let variables_button_area = layout.variables_button;
    let list_area = layout.request_list;
    let scrollbar_area = layout.request_scrollbar;
    let theme = &app.global_config.theme;
    let text = app.text();
    let focus = FocusStyles::new(app.view.focus, theme);
    frame.render_widget(
        panel_block(text.request_selector(), area, theme).border_style(focus.sidebar_border()),
        area,
    );

    if !workspace_selector_area.is_empty() {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(workspace_selector_area);
        let fixed_width = Line::from(" ▾").width();
        let configuration = format!(
            "{} ▾",
            truncate(
                app.active_configuration(),
                usize::from(workspace_selector_area.width).saturating_sub(fixed_width)
            )
        );
        frame.render_widget(
            Paragraph::new(text.workspace())
                .style(Style::default().fg(theme.muted).bg(theme.surface)),
            rows[0],
        );
        draw_primary_button(
            frame,
            rows[1],
            &configuration,
            focus.workspace_focused(),
            theme,
        );
    }
    if !variables_button_area.is_empty() {
        let label = format!("{} ({})", text.variables(), app.variable_count());
        draw_primary_button(
            frame,
            variables_button_area,
            &label,
            focus.variables_focused(),
            theme,
        );
    }

    let request_list_area = list_area;
    let items = app
        .workspace_state
        .requests
        .iter()
        .map(|session| {
            request_item(
                &session.source,
                app.request_status(&session.source.id),
                session.dirty,
                theme,
                request_list_area.width,
                app.view.animation_frame,
            )
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .highlight_style(focus.request_selection())
        .highlight_symbol("› ");
    let offset = request_list_offset(
        app.workspace_state.selected_request.unwrap_or_default(),
        app.workspace_state.requests.len(),
        usize::from(request_list_area.height),
    );
    let mut state = ListState::default().with_offset(offset);
    if let Some(selected) = app.workspace_state.selected_request {
        state.select(Some(selected));
    }
    frame.render_stateful_widget(list, request_list_area, &mut state);
    draw_scrollbar(
        frame,
        scrollbar_area,
        app.workspace_state.requests.len(),
        usize::from(request_list_area.height),
        state.offset(),
        theme,
    );
}

pub(super) fn request_item(
    request: &ApiRequest,
    status: RequestStatus,
    dirty: bool,
    theme: &crate::settings::UiTheme,
    width: u16,
    animation_frame: usize,
) -> ListItem<'static> {
    let status_width = 2;
    let dirty_width = usize::from(dirty) * 2;
    let label_width = usize::from(width)
        .saturating_sub(usize::from(TABLE_HIGHLIGHT_WIDTH))
        .saturating_sub(status_width)
        .saturating_sub(dirty_width);
    let mut spans = vec![Span::styled(
        format!("{} ", request_status_symbol(status, animation_frame)),
        request_status_style(status, theme),
    )];
    if dirty {
        spans.push(Span::styled("* ", request_dirty_style(theme)));
    }
    spans.push(Span::styled(
        truncate(&request.name, label_width),
        Style::default().fg(theme.text),
    ));
    ListItem::new(Line::from(spans))
}
