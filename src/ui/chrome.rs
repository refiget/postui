use super::*;

pub(super) fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let line = if area.width >= 80 {
        Line::from(vec![
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_focus())),
            Span::styled("↑↓/jk", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_move())),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_select())),
            Span::styled("v", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.variables())),
            Span::styled("←→", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.request_editor())),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_send())),
            Span::styled("q", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_quit())),
            Span::styled(text.footer_mouse(), Style::default().fg(theme.accent)),
            Span::raw(format!(" {}", text.footer_click())),
        ])
    } else if area.width >= 48 {
        Line::from(vec![
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_focus())),
            Span::styled("↑↓", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_move())),
            Span::styled("v", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.variables())),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_send())),
            Span::styled("q", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}", text.footer_quit())),
        ])
    } else {
        Line::from(vec![
            Span::styled("v", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.variables())),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_send())),
            Span::styled("q", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}", text.footer_quit())),
        ])
    };
    frame.render_widget(
        Paragraph::new(line).style(Style::default().fg(theme.muted)),
        area,
    );
}

pub(super) fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let mut line = vec![
        Span::styled(
            " PostUI ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("· {}", app.config.name),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ];
    if area.width >= 72 {
        line.push(Span::styled(
            format!("  {}: {}", text.request_config(), app.config_path.display()),
            Style::default().fg(theme.muted),
        ));
    }
    let mut status_line = vec![Span::styled("› ", Style::default().fg(theme.secondary))];
    status_line.extend(highlight::template_spans(
        app.status.as_str(),
        Style::default().fg(theme.text),
        theme,
    ));
    let header = Paragraph::new(Text::from(vec![Line::from(line), Line::from(status_line)]))
        .block(panel_block("", area, theme).border_style(Style::default().fg(theme.accent)))
        .style(Style::default().fg(theme.text));
    frame.render_widget(header, area);
}

pub(super) fn draw_request_list(
    frame: &mut Frame<'_>,
    area: Rect,
    collection_label_area: Rect,
    variables_button_area: Rect,
    list_area: Rect,
    scrollbar_area: Rect,
    app: &App,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let focus = FocusStyles::new(app.focus, theme);
    frame.render_widget(
        panel_block(text.request_selector(), area, theme).border_style(focus.sidebar_border()),
        area,
    );

    if !collection_label_area.is_empty() {
        let collection = format!(
            "{}  {}",
            text.collection(),
            truncate(
                &app.config.name,
                usize::from(collection_label_area.width.saturating_sub(2))
            )
        );
        frame.render_widget(
            Paragraph::new(collection).style(Style::default().fg(theme.text)),
            collection_label_area,
        );
    }
    if !variables_button_area.is_empty() {
        let mut state = ButtonState::enabled();
        state.set_focused(focus.variables_focused());
        let label = format!("{} ({})", text.variables(), app.variable_count());
        frame.render_widget(
            compact_button_widget(&label, &state, theme),
            variables_button_area,
        );
    }

    let items = app
        .config
        .requests
        .iter()
        .map(|request| request_item(request, app, theme, list_area.width))
        .collect::<Vec<_>>();
    let list = List::new(items)
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .highlight_style(
            Style::default()
                .bg(focus.request_selection())
                .fg(theme.text),
        )
        .highlight_symbol("▸ ");
    let mut state = ListState::default();
    if !app.config.requests.is_empty() {
        state.select(Some(app.requests_state.selected_request));
    }
    frame.render_stateful_widget(list, list_area, &mut state);
    draw_scrollbar(
        frame,
        scrollbar_area,
        app.config.requests.len(),
        usize::from(list_area.height),
        state.offset(),
        theme,
    );
}

pub(super) fn request_item(
    request: &ApiRequest,
    app: &App,
    theme: &crate::settings::UiTheme,
    width: u16,
) -> ListItem<'static> {
    let status = app.request_status(&request.id);
    let status_text = format!("[{}] ", status.tag());
    let method_text = format!("{} ", request.method);
    let prefix_width = u16::try_from(
        Line::from(status_text.as_str()).width() + Line::from(method_text.as_str()).width(),
    )
    .unwrap_or(u16::MAX);
    let label_width = width
        .saturating_sub(TABLE_HIGHLIGHT_WIDTH)
        .saturating_sub(prefix_width);
    let label = format!("{}  {}", request.name, template::display_url(request));
    let mut spans = vec![
        Span::styled(status_text, request_status_style(status, theme)),
        Span::styled(method_text, method_style(&request.method, theme)),
    ];
    if label_width > 0 {
        spans.push(Span::styled(
            truncate(&label, usize::from(label_width)),
            Style::default().fg(theme.text),
        ));
    }
    ListItem::new(Line::from(spans))
}
