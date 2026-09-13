use super::*;

pub(crate) fn handle_mouse(app: &mut App, event: MouseEvent, area: Rect) {
    if app.error_page().is_some() {
        app.view.cancel_scroll_drag();
        return;
    }
    if matches!(
        event.kind,
        MouseEventKind::Up(MouseButton::Left) | MouseEventKind::Down(MouseButton::Left)
    ) {
        app.view.cancel_scroll_drag();
    }
    if app.view.prompt.is_some()
        || app.view.help_scroll.is_some()
        || app.view.requests.search.is_some()
        || app.view.response.search.is_some()
    {
        app.view.cancel_scroll_drag();
        return;
    }
    if app.view.variables.is_some() {
        app.view.response.scroll.drag_anchor = None;
        let is_double = matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            && app.view.clicks.register(event.column, event.row);
        handle_variables_mouse(app, event, variables_page(area).response, is_double);
        return;
    }
    if matches!(app.view.dialog, Some(Dialog::Configurations(_))) {
        app.view.response.scroll.drag_anchor = None;
        handle_configuration_mouse(app, event, area);
        return;
    }
    let areas = screen_layout_for_app(area, app);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let is_double = app.view.clicks.register(event.column, event.row);
            tracing::trace!(
                column = event.column,
                row = event.row,
                area = ?area,
                "处理鼠标左键点击"
            );
            handle_click(app, event.column, event.row, areas, is_double);
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if contains(areas.request_scrollbar, event.column, event.row) {
                app.view.response.scroll.drag_anchor = None;
                drag_request_list_scrollbar(app, event.row, areas);
            } else if app.editing_preview_tab().is_some()
                && drag_inline_editor_scrollbar(app, event.row, areas.preview_content)
            {
                app.view.response.scroll.drag_anchor = None;
            } else {
                drag_response_scrollbar(app, event.row, areas);
            }
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            app.view.clicks.reset();
            let direction = if matches!(event.kind, MouseEventKind::ScrollUp) {
                -1
            } else {
                1
            };
            tracing::trace!(
                kind = ?event.kind,
                column = event.column,
                row = event.row,
                direction,
                "处理鼠标滚动"
            );
            handle_scroll(app, event.column, event.row, areas, direction);
        }
        MouseEventKind::Moved if app.view.response.menu_selection.is_some() => {
            update_response_hover(app, event.column, event.row, areas);
        }
        _ => {}
    }
}

fn update_response_hover(app: &mut App, column: u16, row: u16, areas: UiLayout) {
    let content =
        response_menu_area(areas.response, areas.response_menu_button).inner(Margin::new(1, 1));
    if !contains(content, column, row) {
        return;
    }

    let index = usize::from(row.saturating_sub(content.y));
    if index < ResponseMenuAction::all().len() {
        app.view.response.menu_selection = Some(index);
    }
}

fn handle_configuration_mouse(app: &mut App, event: MouseEvent, area: Rect) {
    let areas = screen_layout_for_app(area, app);
    let Some(row_count) = app.view.dialog.as_ref().and_then(|dialog| match dialog {
        Dialog::Configurations(dialog) => Some(dialog.rows.len()),
        _ => None,
    }) else {
        return;
    };
    let menu = configuration_menu_area(area, areas.workspace_selector, row_count);
    let content = menu.inner(Margin::new(1, 1));
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if contains(content, event.column, event.row) {
                let index = usize::from(event.row.saturating_sub(content.y));
                if index < row_count {
                    app.click_configuration_row(index);
                    app.apply_dialog();
                }
            } else {
                app.close_dialog();
                if contains(areas.workspace_selector, event.column, event.row) {
                    app.view.focus = Focus::WorkspaceButton;
                } else {
                    handle_click(app, event.column, event.row, areas, false);
                }
            }
        }
        MouseEventKind::Moved if contains(content, event.column, event.row) => {
            let index = usize::from(event.row.saturating_sub(content.y));
            if index < row_count {
                app.click_configuration_row(index);
            }
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if contains(content, event.column, event.row) =>
        {
            let direction = if matches!(event.kind, MouseEventKind::ScrollUp) {
                -1
            } else {
                1
            };
            app.move_dialog_selection(direction);
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let direction = if matches!(event.kind, MouseEventKind::ScrollUp) {
                -1
            } else {
                1
            };
            app.close_dialog();
            handle_scroll(app, event.column, event.row, areas, direction);
        }
        _ => {}
    }
}

fn handle_click(app: &mut App, column: u16, row: u16, areas: UiLayout, is_double: bool) {
    if !is_double {
        app.view.cancel_active_editors();
    }
    focus_panel_at(app, column, row, areas);

    if app.view.response.menu_selection.is_some() {
        let menu = response_menu_area(areas.response, areas.response_menu_button);
        let content = menu.inner(Margin::new(1, 1));
        if contains(content, column, row) {
            app.view.focus = Focus::ResponseActions;
            app.choose_response_action(usize::from(row.saturating_sub(content.y)));
            return;
        }
        if contains(areas.response_menu_button, column, row) {
            app.close_response_menu();
            return;
        }
        app.close_response_menu();
    }

    if contains(areas.workspace_selector, column, row) {
        app.view.focus = Focus::WorkspaceButton;
        app.open_configurations();
    } else if contains(areas.variables_button, column, row) {
        app.view.focus = Focus::Variables;
        app.open_variables();
    } else if contains(areas.request_scrollbar, column, row) {
        app.view.focus = Focus::Requests;
        click_request_list_scrollbar(app, row, areas);
    } else if contains(areas.request_list, column, row) {
        click_request_list(app, column, row, areas.request_list);
    } else if contains(areas.send_button, column, row)
        && app.can_execute_preview_action(PreviewAction::Send)
    {
        app.view.focus = Focus::SendButton;
        app.handle_preview_action(PreviewAction::Send);
    } else if (contains(areas.preview_summary, column, row)
        || contains(areas.preview_content, column, row))
        && app.has_current_request()
    {
        app.view.focus = Focus::Preview;
        if contains(areas.preview_summary, column, row) {
            let method_width = app
                .current_effective_request()
                .map(|request| u16::try_from(request.method.len()).unwrap_or(u16::MAX) + 3)
                .unwrap_or_default();
            if column < areas.preview_summary.x.saturating_add(method_width) {
                app.cycle_method();
            }
        } else {
            if app.view.preview.active_tab == PreviewTab::Body {
                let line = usize::from(row.saturating_sub(areas.preview_content.y))
                    .saturating_add(usize::from(app.view.preview.scroll.offset()));
                let column = usize::from(column.saturating_sub(areas.preview_content.x));
                app.start_body_edit_at(line, column, is_double);
                return;
            }
            if app.editing_preview_tab().is_none() {
                app.handle_preview_action(PreviewAction::Edit(app.view.preview.active_tab));
            }
            if app.editing_preview_tab().is_some() {
                handle_inline_editor_click(app, column, row, areas.preview_content, is_double);
            }
        }
    } else if contains(areas.preview_tabs, column, row) && app.has_current_request() {
        app.view.focus = Focus::Preview;
        if let Some(tab) = preview_tab_at(areas.preview_tabs, column, app) {
            app.activate_preview_tab(tab);
        }
    } else if contains(areas.response_menu_button, column, row) {
        app.view.focus = Focus::ResponseActions;
        app.open_response_menu();
    } else if contains(areas.response_format_button, column, row) {
        if app.current_response().is_some() {
            app.toggle_response_format_tab();
        }
        app.view.focus = Focus::Response;
    } else if contains(areas.response_zoom_button, column, row) {
        app.view.focus = Focus::ResponseZoom;
        app.toggle_response_zoom();
    } else if contains(areas.response, column, row) {
        app.view.focus = Focus::Response;
        if let Some(tab) = response_tab_at(app, column, row, areas) {
            app.select_response_tab(tab);
        } else {
            click_response_scrollbar(app, column, row, areas);
        }
    }
}

fn click_request_list(app: &mut App, column: u16, row: u16, area: Rect) {
    if area.is_empty() || row < area.y || column >= area.right() {
        return;
    }
    let visible = usize::from(area.height);
    let visible_indices = app.visible_request_indices();
    let offset = app
        .view
        .requests
        .scroll
        .offset(visible_indices.len(), visible);
    let visible_index = offset.saturating_add(usize::from(row - area.y));
    let Some(index) = visible_indices.get(visible_index).copied() else {
        return;
    };
    app.select_request(index);
    app.view.focus = Focus::Requests;
    tracing::debug!(index, "通过左侧接口列表选择接口");
}

fn handle_scroll(app: &mut App, column: u16, row: u16, areas: UiLayout, direction: isize) {
    focus_panel_at(app, column, row, areas);
    let response_menu = response_menu_area(areas.response, areas.response_menu_button);
    if app.view.response.menu_selection.is_some() && contains(response_menu, column, row) {
        app.move_response_menu_selection(direction);
    } else if contains(areas.request_scrollbar, column, row)
        || contains(areas.request_list, column, row)
    {
        app.view.focus = Focus::Requests;
        tracing::trace!(column, row, direction, "滚动左侧接口列表");
        let count = app.visible_request_indices().len();
        app.view
            .requests
            .scroll
            .move_by(direction, count, usize::from(areas.request_list.height));
    } else if contains(areas.response, column, row) {
        app.view.focus = Focus::Response;
        tracing::trace!(column, row, direction, "滚动响应内容");
        app.scroll_response(direction);
    } else if contains(areas.preview_content, column, row) {
        app.view.focus = Focus::Preview;
        if app.view.preview.active_tab == PreviewTab::Body {
            app.view.preview.scroll.move_by(direction);
        } else {
            let tab = app.view.preview.active_tab;
            if app.editing_preview_tab() != Some(tab) {
                app.handle_preview_action(PreviewAction::Edit(tab));
            }
            if app.editing_preview_tab() == Some(tab) {
                scroll_inline_editor(app, direction, areas.preview_content);
            }
        }
    }
}

fn focus_panel_at(app: &mut App, column: u16, row: u16, areas: UiLayout) {
    app.view.focus = if contains(areas.header, column, row) {
        Focus::Header
    } else if contains(areas.requests, column, row) {
        Focus::Requests
    } else if contains(areas.preview, column, row) {
        Focus::Preview
    } else if contains(areas.response, column, row) {
        Focus::Response
    } else {
        return;
    };
}
