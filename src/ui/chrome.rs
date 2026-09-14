use super::*;

pub(super) fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let feedback = app.current_feedback();
    let (symbol, color) = match feedback {
        Some(crate::app::Feedback::Success(_)) => ("✓", theme.success),
        Some(crate::app::Feedback::Warning(_)) => ("!", theme.warning),
        Some(crate::app::Feedback::Error(_)) => ("×", theme.error),
        None => ("", theme.muted),
    };
    let context = if app.view.help_scroll.is_some() {
        crate::shortcuts::Context::Help
    } else {
        app.key_context()
    };
    let hint = text.shortcut_hint(context, app.debug_mode);
    let mut lines = Vec::with_capacity(2);
    if let Some(feedback) = feedback {
        lines.push(Line::from(vec![
            Span::styled(format!(" {symbol} "), Style::default().fg(color)),
            Span::styled(feedback.message(), Style::default().fg(color)),
        ]));
    }
    lines.push(Line::from(Span::styled(
        format!(" {hint}"),
        Style::default().fg(theme.muted),
    )));
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.background)),
        area,
    );
}

pub(super) fn draw_header(
    frame: &mut Frame<'_>,
    area: Rect,
    content_area: Rect,
    action_area: Rect,
    app: &App,
) {
    let theme = &app.global_config.theme;
    let focus = FocusStyles::new(app.view.focus, theme);
    let title_area = if action_area.is_empty() {
        content_area
    } else {
        Rect::new(
            content_area.x,
            content_area.y,
            action_area.x.saturating_sub(content_area.x),
            content_area.height,
        )
    };
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
    frame.render_widget(
        panel_block("", area, theme).border_style(focus.header_border()),
        area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(line)).style(Style::default().fg(theme.text)),
        title_area,
    );
    draw_response_toolbar_button_left(
        frame,
        action_area,
        "+ 新建",
        "+",
        FlatButtonState::Idle,
        theme.primary,
        theme,
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
    let send_label = format!("▶ {}", app.text().send_button(false));
    let cancel_label = format!("■ {}", app.text().cancel_request());
    let label_width = Line::from(send_label.as_str())
        .width()
        .max(Line::from(cancel_label.as_str()).width());
    let mut label = if loading { cancel_label } else { send_label };
    let padding = label_width.saturating_sub(Line::from(label.as_str()).width());
    label.extend(std::iter::repeat_n(' ', padding));
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
        let value_width = usize::from(workspace_selector_area.width)
            .saturating_sub(Line::from(" ▾").width())
            .saturating_sub(Line::from("▌  ").width());
        let configuration = format!("{} ▾", truncate(app.active_configuration(), value_width));
        draw_flat_button_colored(
            frame,
            workspace_selector_area,
            &configuration,
            FlatButtonState::new(true, focus.workspace_focused()),
            theme.secondary,
            theme,
            Alignment::Center,
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
    let offset = app
        .view
        .requests
        .scroll
        .offset(visible.len(), usize::from(request_list_area.height));
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
        .highlight_symbol("› ")
        .highlight_spacing(HighlightSpacing::Always);
    let mut state = ListState::default();
    if let Some(selected) = selected.filter(|selected| {
        (offset..offset.saturating_add(usize::from(request_list_area.height))).contains(selected)
    }) {
        state.select(Some(selected - offset));
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
    let offset = app
        .view
        .requests
        .scroll
        .offset(visible_count, visible_height);
    let Some(bar) = scrollbar_track_state(
        areas.request_scrollbar,
        visible_count,
        visible_height,
        offset,
    ) else {
        return;
    };

    let target = scrollbar_offset_from_track(&bar, row);
    app.view
        .requests
        .scroll
        .set_offset(target, visible_count, visible_height);
    app.view.requests.scroll.drag_anchor = Some((row, target));
}

pub(super) fn drag_request_list_scrollbar(app: &mut App, row: u16, areas: UiLayout) {
    let visible_count = app.visible_request_indices().len();
    let visible_height = usize::from(areas.request_list.height);
    let offset = app
        .view
        .requests
        .scroll
        .offset(visible_count, visible_height);
    let Some((anchor_row, anchor_offset)) = app.view.requests.scroll.drag_anchor else {
        return;
    };
    let Some(bar) = scrollbar_track_state(
        areas.request_scrollbar,
        visible_count,
        visible_height,
        offset,
    ) else {
        return;
    };
    let target = scrollbar_offset_from_drag(&bar, anchor_row, anchor_offset, row);
    app.view
        .requests
        .scroll
        .set_offset(target, visible_count, visible_height);
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
