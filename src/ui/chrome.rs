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
    } else if app.view.is_editing() {
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

pub(super) fn draw_header(frame: &mut Frame<'_>, area: Rect, content_area: Rect, app: &App) {
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
    if app.current_request_is_insecure() {
        line.push(Span::styled(
            "  ⚠ TLS ",
            Style::default()
                .fg(theme.background)
                .bg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ));
    }
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
}

pub(super) fn draw_request_send_button(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() || !app.has_current_request() {
        return;
    }
    let Some(request) = app.current_request() else {
        return;
    };
    let request_status = app.request_status(&request.id);
    let loading = request_status == RequestStatus::Sending;
    let label = if loading {
        format!("■ {}", app.text().cancel_request())
    } else {
        format!("▶ {}", app.text().send_button(false))
    };
    draw_send_button_aligned(
        frame,
        area,
        &label,
        app.can_execute_preview_action(PreviewAction::Send),
        app.focused_preview_action() == Some(PreviewAction::Send),
        &app.global_config.theme,
        Alignment::Right,
    );
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
    let title = app.request_search_query().map_or_else(
        || text.request_selector().to_string(),
        |query| format!("{} / {}", text.request_selector(), query),
    );
    frame.render_widget(
        panel_block(title, area, theme).border_style(focus.sidebar_border()),
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
    let visible = app.visible_request_indices();
    let selected = app
        .workspace_state
        .selected_request
        .and_then(|selected| visible.iter().position(|index| *index == selected));
    let offset = request_list_offset(
        selected.unwrap_or_default(),
        visible.len(),
        usize::from(request_list_area.height),
    );
    let items = visible
        .iter()
        .skip(offset)
        .take(usize::from(request_list_area.height))
        .filter_map(|index| app.workspace_state.requests.get(*index))
        .map(|session| {
            request_item(
                &session.source,
                session.status(),
                app.request_modified(&session.source.id),
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
    let mut state = ListState::default();
    if let Some(selected) = selected {
        state.select(Some(selected.saturating_sub(offset)));
    }
    frame.render_stateful_widget(list, request_list_area, &mut state);
    draw_scrollbar(
        frame,
        scrollbar_area,
        visible.len(),
        usize::from(request_list_area.height),
        offset,
        theme,
    );
}

pub(super) fn click_request_list_scrollbar(app: &mut App, row: u16, areas: UiLayout) {
    let visible = app.visible_request_indices();
    let visible_count = visible.len();
    let visible_height = usize::from(areas.request_list.height);
    let selected_position = app
        .workspace_state
        .selected_request
        .and_then(|selected| visible.iter().position(|index| *index == selected))
        .unwrap_or_default();
    let offset = request_list_offset(selected_position, visible_count, visible_height);
    let Some(bar) = scrollbar_track_state(
        areas.request_scrollbar,
        visible_count,
        visible_height,
        offset,
    ) else {
        return;
    };

    let selected_visible = selected_position
        .saturating_sub(offset)
        .min(visible_height.saturating_sub(1));
    let selected = scrollbar_offset_from_track(&bar, row)
        .saturating_add(selected_visible)
        .min(visible_count.saturating_sub(1));

    if let Some(index) = visible.get(selected).copied() {
        app.select_request(index);
    }
}

pub(super) fn drag_request_list_scrollbar(app: &mut App, row: u16, areas: UiLayout) {
    click_request_list_scrollbar(app, row, areas);
}

pub(super) fn request_item(
    request: &ApiRequest,
    status: RequestStatus,
    modified: bool,
    theme: &crate::settings::UiTheme,
    width: u16,
    animation_frame: usize,
) -> ListItem<'static> {
    let status_width = if modified { 4 } else { 2 };
    let label_width = usize::from(width)
        .saturating_sub(usize::from(TABLE_HIGHLIGHT_WIDTH))
        .saturating_sub(status_width);
    let mut spans = vec![Span::styled(
        format!("{} ", request_status_symbol(status, animation_frame)),
        request_status_style(status, theme),
    )];
    spans.push(Span::styled(
        truncate(&request.name, label_width),
        Style::default().fg(theme.text),
    ));
    if modified {
        spans.push(Span::styled(" *", Style::default().fg(theme.warning)));
    }
    ListItem::new(Line::from(spans))
}
