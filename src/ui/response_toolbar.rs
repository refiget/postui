use super::*;

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
        FlatButtonState::new(enabled, focused),
        app.global_config.theme.primary,
        &app.global_config.theme,
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
        FlatButtonState::new(true, app.view.focus == Focus::ResponseActions),
        app.global_config.theme.accent,
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
        FlatButtonState::new(true, app.view.focus == Focus::ResponseZoom),
        app.global_config.theme.secondary,
        &app.global_config.theme,
    );
}

fn draw_response_toolbar_button(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    symbol: &str,
    state: FlatButtonState,
    color: Color,
    theme: &crate::settings::UiTheme,
) {
    let width = usize::from(area.width);
    if width == 0 || area.is_empty() {
        return;
    }

    let content_width = width.saturating_sub(3);
    let text = if Line::from(label).width() <= content_width {
        label
    } else if Line::from(symbol).width() <= content_width {
        symbol
    } else {
        ""
    };
    draw_flat_button_colored(frame, area, text, state, color, theme, Alignment::Center);
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
