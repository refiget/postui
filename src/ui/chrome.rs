use super::*;

pub(super) fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let line = if area.width >= 80 {
        Line::from(vec![
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.footer_focus())),
            Span::styled("↑↓/jk", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.footer_move())),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.footer_select())),
            Span::styled("v", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.variables())),
            Span::styled("←→", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.request_editor())),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.footer_send())),
            Span::styled("Ctrl+S", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.footer_save())),
            Span::styled("q", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.footer_quit())),
            Span::styled(text.footer_mouse(), Style::default().fg(theme.accent)),
            Span::raw(format!(" {}", text.footer_click())),
        ])
    } else if area.width >= 48 {
        Line::from(vec![
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.footer_focus())),
            Span::styled("↑↓", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.footer_move())),
            Span::styled("v", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.variables())),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.footer_send())),
            Span::styled("q", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}", text.footer_quit())),
        ])
    } else {
        Line::from(vec![
            Span::styled("v", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.variables())),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  │  ", text.footer_send())),
            Span::styled("q", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}", text.footer_quit())),
        ])
    };
    frame.render_widget(
        Paragraph::new(line).style(Style::default().fg(theme.muted)),
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
    if area.width >= 72 {
        line.push(Span::styled(
            format!("  │  {}", app.workspace_path().display()),
            Style::default().fg(theme.muted),
        ));
    }
    let mut status_line = vec![Span::styled("  › ", Style::default().fg(theme.secondary))];
    status_line.extend(highlight::template_spans(
        app.status.as_str(),
        Style::default().fg(theme.text),
        theme,
    ));
    frame.render_widget(
        panel_block("", area, theme).border_style(Style::default().fg(theme.accent)),
        area,
    );
    frame.render_widget(
        Paragraph::new(Text::from(vec![Line::from(line), Line::from(status_line)]))
            .style(Style::default().fg(theme.text)),
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
                request_status_symbol(request_status, app.animation_frame),
                app.text().send_button(true)
            )
        } else {
            format!("[ {} ]", app.text().send_button(false))
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

pub(super) fn draw_request_list(
    frame: &mut Frame<'_>,
    area: Rect,
    workspace_label_area: Rect,
    variables_button_area: Rect,
    list_area: Rect,
    scrollbar_area: Rect,
    app: &App,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let focus = FocusStyles::new(app.focus, theme);
    frame.render_widget(
        focused_panel_block(
            text.request_selector(),
            area,
            theme,
            focus.sidebar_focused(),
        )
        .border_style(focus.sidebar_border()),
        area,
    );

    if !workspace_label_area.is_empty() {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(workspace_label_area);
        let fixed_width = Line::from("◆ ").width();
        let workspace = format!(
            "◆ {}",
            truncate(
                &app.config.name,
                usize::from(workspace_label_area.width).saturating_sub(fixed_width)
            )
        );
        let style = Style::default()
            .fg(theme.accent)
            .bg(theme.selection)
            .add_modifier(Modifier::BOLD);
        frame.render_widget(
            Paragraph::new(text.workspace())
                .style(Style::default().fg(theme.muted).bg(theme.surface)),
            rows[0],
        );
        frame.render_widget(Paragraph::new(workspace).style(style), rows[1]);
    }
    if !variables_button_area.is_empty() {
        let label = format!("◇ {} ({})", text.variables(), app.variable_count());
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
            let mut item = request_item(&session.source, theme, request_list_area.width);
            if session.dirty {
                item = ListItem::new(Line::from(vec![
                    Span::styled("● ", Style::default().fg(theme.warning)),
                    Span::styled(
                        truncate(
                            &session.source.name,
                            usize::from(request_list_area.width).saturating_sub(4),
                        ),
                        Style::default().fg(theme.text),
                    ),
                ]));
            }
            item
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .highlight_style(
            Style::default()
                .bg(focus.request_selection())
                .fg(theme.text),
        )
        .highlight_symbol("› ");
    let mut state = ListState::default();
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
    theme: &crate::settings::UiTheme,
    width: u16,
) -> ListItem<'static> {
    let label_width = width
        .saturating_sub(TABLE_HIGHLIGHT_WIDTH)
        .saturating_sub(1);
    ListItem::new(Line::from(Span::styled(
        truncate(&request.name, usize::from(label_width)),
        Style::default().fg(theme.text),
    )))
}
