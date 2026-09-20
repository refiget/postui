use super::{
    TABLE_HIGHLIGHT_WIDTH,
    focus::FocusStyles,
    layout::UiLayout,
    response_toolbar::draw_response_toolbar_button_left,
    widgets::{
        draw_flat_button_colored, draw_scrollbar, edit_input_style, editor_view, label_style,
        method_style, panel_block, request_status_style, request_status_symbol,
        scrollbar_offset_from_drag, scrollbar_offset_from_track, scrollbar_track_state, truncate,
    },
};
use crate::{
    app::{App, Focus, MainButton, PreviewAction, RequestStatus, ScrollDragTarget},
    config::ApiRequest,
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, HighlightSpacing, List, ListItem, ListState, Paragraph},
};

pub(super) fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() {
        return;
    }
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
    let status_area = Rect::new(area.x, area.y, area.width, u16::from(area.height > 1));
    let hint_area = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
    let mut status = Vec::new();
    if let Some(feedback) = feedback {
        status.extend([
            Span::styled(format!(" {symbol} "), Style::default().fg(color)),
            Span::styled(feedback.message(), Style::default().fg(color)),
        ]);
    } else {
        let container = match app.view.focus.container() {
            Focus::Requests => text.request_selector(),
            Focus::Preview => text.request_editor(),
            Focus::Response => text.response(),
            _ => "POSTUI",
        };
        let label = if app.view.help_scroll.is_some() {
            text.help_title()
        } else if app.view.variables.is_some() {
            text.variables()
        } else if app.view.extracts.is_some() {
            text.extracts()
        } else if app.view.curl_import.is_some() {
            text.curl_import_title()
        } else {
            container
        };
        status.push(Span::styled(
            format!(" {label} "),
            Style::default().fg(theme.accent),
        ));
        if let Some(request) = app.current_request() {
            status.push(Span::styled("› ", label_style(theme)));
            status.push(Span::styled(request.name.as_str(), label_style(theme)));
            if app.request_modified(&request.id) {
                status.push(Span::styled(" *", Style::default().fg(theme.warning)));
            }
        }
    }
    let theme_label = format!(" {} ", theme.name);
    let theme_width = if area.width >= 80 {
        u16::try_from(Line::from(theme_label.as_str()).width()).unwrap_or_default()
    } else {
        0
    };
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(status)),
        Rect::new(
            status_area.x,
            status_area.y,
            status_area.width.saturating_sub(theme_width),
            status_area.height,
        ),
    );
    if theme_width > 0 {
        frame.render_widget(
            Paragraph::new(theme_label).style(label_style(theme)),
            Rect::new(
                status_area.right().saturating_sub(theme_width),
                status_area.y,
                theme_width,
                status_area.height,
            ),
        );
    }

    let hints = hint
        .split("  ")
        .filter_map(|hint| hint.split_once(' '))
        .collect::<Vec<_>>();
    let help = hints.iter().find(|(key, _)| *key == "?").copied();
    let help_width = help.map_or(0, |(key, label)| {
        Line::from(format!(" {key} {label} ")).width() + 2
    });
    let mut spans = vec![Span::raw(" ")];
    let mut used = 1;
    let available = usize::from(area.width);
    for (key, label) in hints.iter().copied().filter(|(key, _)| *key != "?") {
        let width = Line::from(format!(" {key}  {label}  ")).width();
        if used + width + help_width > available {
            continue;
        }
        spans.extend(shortcut_spans(key, label, theme));
        used += width;
    }
    if let Some((key, label)) = help.filter(|_| used + help_width <= available) {
        spans.extend(shortcut_spans(key, label, theme));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), hint_area);
}

fn shortcut_spans<'a>(
    key: &'a str,
    label: &'a str,
    theme: &crate::settings::UiTheme,
) -> [Span<'a>; 3] {
    [
        Span::styled(
            format!(" {key} "),
            Style::default().fg(theme.accent).bg(theme.selection),
        ),
        Span::styled(format!(" {label}"), label_style(theme)),
        Span::raw("  "),
    ]
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
        app.text().new_request(),
        "+",
        app.view.main_buttons.visual_state(
            MainButton::NewRequest,
            MainButton::NewRequest.enabled(app),
            app.view.focus == Focus::Header,
        ),
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
    let send_label = format!("▶ {}  s", app.text().send_button(false));
    let cancel_label = format!("■ {}  s", app.text().cancel_request());
    let label_width = Line::from(send_label.as_str())
        .width()
        .max(Line::from(cancel_label.as_str()).width());
    let mut label = if loading { cancel_label } else { send_label };
    let padding = label_width.saturating_sub(Line::from(label.as_str()).width());
    label.extend(std::iter::repeat_n(' ', padding));
    draw_flat_button_colored(
        frame,
        area,
        &label,
        app.view.main_buttons.visual_state(
            MainButton::Send,
            MainButton::Send.enabled(app),
            app.focused_preview_action() == Some(PreviewAction::Send),
        ),
        app.global_config.theme.accent,
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
    let visible = app.visible_request_indices();
    let count = if app.request_search_query().is_some() {
        format!("{}/{}", visible.len(), app.workspace_state.requests.len())
    } else {
        visible.len().to_string()
    };
    let mut title = Line::from(vec![
        Span::styled(
            format!(" {} ", text.request_selector()),
            Style::default().fg(theme.text),
        ),
        Span::styled(format!("{count} "), label_style(theme)),
    ]);
    if layout.request_search.is_empty() {
        if let Some(input) = app.view.requests.search.as_ref() {
            title.spans.push(Span::styled(
                format!(
                    "/ {}",
                    editor_view(input, usize::from(area.width.saturating_sub(12)))
                ),
                Style::default().fg(theme.accent),
            ));
        } else if let Some(query) = app.request_search_query() {
            title.spans.push(Span::styled(
                format!("/ {query}"),
                Style::default().fg(theme.accent),
            ));
        }
    }
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
            app.view.main_buttons.visual_state(
                MainButton::Workspace,
                MainButton::Workspace.enabled(app),
                focus.workspace_focused(),
            ),
            theme.secondary,
            theme,
            Alignment::Center,
        );
    }
    if !variables_button_area.is_empty() {
        let label = format!("{} ({})", text.variables(), app.variable_count());
        draw_flat_button_colored(
            frame,
            variables_button_area,
            &label,
            app.view.main_buttons.visual_state(
                MainButton::Variables,
                MainButton::Variables.enabled(app),
                focus.variables_focused(),
            ),
            theme.accent,
            theme,
            Alignment::Center,
        );
    }

    draw_request_search(frame, layout.request_search, app);
    let request_list_area = list_area;
    if visible.is_empty() {
        let label = if app.workspace_state.requests.is_empty() {
            text.no_requests()
        } else {
            text.workspace_no_match()
        };
        frame.render_widget(
            Paragraph::new(label)
                .style(label_style(theme))
                .alignment(Alignment::Center),
            request_list_area,
        );
        return;
    }
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
                &session.draft.method,
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
        .highlight_symbol("▎ ")
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

fn draw_request_search(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.is_empty() {
        return;
    }
    let theme = &app.global_config.theme;
    let width = usize::from(area.width.saturating_sub(3));
    let (value, style) = if let Some(input) = app.view.requests.search.as_ref() {
        let value = if input.value().is_empty() && width > 0 {
            "▏".to_string()
        } else {
            editor_view(input, width)
        };
        (
            value,
            edit_input_style(input, theme, theme.text, theme.selection),
        )
    } else if let Some(query) = app.request_search_query() {
        (
            truncate(query, width),
            Style::default().fg(theme.accent).bg(theme.selection),
        )
    } else {
        (
            truncate(app.text().request_filter(), width),
            label_style(theme),
        )
    };
    frame.render_widget(Paragraph::new(format!(" / {value}")).style(style), area);
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
    app.view.scroll_drag_target = Some(ScrollDragTarget::Requests);
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
    method: &str,
    status: RequestStatus,
    modified: bool,
    theme: &crate::settings::UiTheme,
    width: u16,
    animation_frame: usize,
) -> ListItem<'static> {
    let show_method = width >= 26;
    let method_width = if show_method { 7 } else { 0 };
    let status_width = if modified { 4 } else { 2 };
    let label_width = usize::from(width)
        .saturating_sub(usize::from(TABLE_HIGHLIGHT_WIDTH))
        .saturating_sub(status_width + method_width);
    let mut spans = vec![Span::styled(
        format!("{} ", request_status_symbol(status, animation_frame)),
        request_status_style(status, theme),
    )];
    if show_method {
        spans.push(Span::styled(
            format!("{:<6} ", truncate(method, 6)),
            method_style(method, theme),
        ));
    }
    spans.push(Span::styled(
        truncate(&request.name, label_width),
        Style::default().fg(theme.text),
    ));
    if modified {
        spans.push(Span::styled(" *", Style::default().fg(theme.warning)));
    }
    ListItem::new(Line::from(spans))
}
