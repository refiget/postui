use super::{
    chrome::{click_request_list_scrollbar, drag_request_list_scrollbar},
    contains,
    curl_import::{CurlImportLayout, compact_field_label_width, curl_import_layout},
    dialog::configuration_menu_area,
    help_layout,
    inline_editor::{
        drag_inline_editor_scrollbar, handle_inline_editor_click, place_inline_editor_cursor,
        scroll_inline_editor,
    },
    layout::UiLayout,
    preview::preview_tab_at,
    response::{
        begin_response_selection, click_response_scrollbar, drag_response_scrollbar,
        finish_response_selection, response_search_area, response_tab_at,
        update_response_selection,
    },
    response_toolbar::response_menu_area,
    screen_layout_for_app,
    widgets::coordinate,
};
use crate::app::{
    App, CurlImportFocus, Dialog, Focus, PreviewAction, PreviewTab, RequestStatus,
    ResponseMenuAction, ScrollDragTarget,
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Margin, Rect};

pub(crate) fn handle_mouse(app: &mut App, event: MouseEvent, area: Rect) {
    if app.error_page().is_some() {
        app.view.cancel_scroll_drag();
        return;
    }
    if matches!(
        event.kind,
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
    ) {
        app.view.cancel_scroll_drag();
    }
    let areas = screen_layout_for_app(area, app);
    if app.view.prompt.is_some() {
        return;
    }
    if app.view.help_scroll.is_some() {
        handle_help_mouse(app, event, area);
        return;
    }
    if app.view.requests.search.is_some()
        && handle_search_mouse(app, event, areas, SearchTarget::Requests)
    {
        return;
    }
    if app.view.response.search.is_some()
        && handle_search_mouse(app, event, areas, SearchTarget::Response)
    {
        return;
    }
    if app.view.curl_import.is_some() {
        let layout = curl_import_layout(areas.response);
        if matches!(app.view.dialog, Some(Dialog::Configurations(_))) {
            handle_configuration_mouse(app, event, area, areas, layout.workspace, Some(layout));
        } else {
            handle_curl_import_mouse(app, event, layout);
        }
        return;
    }
    if matches!(app.view.dialog, Some(Dialog::Configurations(_))) {
        handle_configuration_mouse(app, event, area, areas, areas.configuration_menu, None);
        return;
    }
    if handle_response_menu_mouse(app, event, areas) {
        return;
    }
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
            if app
                .view
                .response
                .selection
                .as_ref()
                .is_some_and(|selection| selection.dragging)
            {
                update_response_selection(app, event.column, event.row, areas);
                return;
            }
            let target = app.view.scroll_drag_target;
            match target {
                Some(ScrollDragTarget::Requests) => {
                    drag_request_list_scrollbar(app, event.row, areas);
                }
                Some(ScrollDragTarget::Preview) => {
                    drag_inline_editor_scrollbar(app, event.row, areas.preview_content);
                }
                Some(ScrollDragTarget::Response) => {
                    drag_response_scrollbar(app, event.row, areas);
                }
                None if contains(areas.request_scrollbar, event.column, event.row) => {
                    drag_request_list_scrollbar(app, event.row, areas);
                }
                None => {
                    if app.editing_preview_tab().is_some()
                        && drag_inline_editor_scrollbar(app, event.row, areas.preview_content)
                    {
                        return;
                    }
                    drag_response_scrollbar(app, event.row, areas);
                }
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
        MouseEventKind::Up(MouseButton::Left) => {
            finish_response_selection(app, event.column, event.row, areas);
        }
        _ => {}
    }
}

fn handle_help_mouse(app: &mut App, event: MouseEvent, area: Rect) {
    let content = app.text().help_content(app.key_context(), app.debug_mode);
    let (help_area, _) = help_layout(area, &content);
    match event.kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if contains(help_area, event.column, event.row) =>
        {
            if let Some(scroll) = app.view.help_scroll.as_mut() {
                if matches!(event.kind, MouseEventKind::ScrollUp) {
                    *scroll = scroll.saturating_sub(1);
                } else {
                    *scroll = scroll.saturating_add(1);
                }
            }
        }
        MouseEventKind::Down(MouseButton::Left)
            if !contains(help_area, event.column, event.row) =>
        {
            app.view.help_scroll = None;
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Copy)]
enum SearchTarget {
    Requests,
    Response,
}

impl SearchTarget {
    fn area(self, areas: UiLayout) -> Rect {
        match self {
            Self::Requests => areas.request_search,
            Self::Response => response_search_area(areas.response),
        }
    }

    fn prefix_width(self) -> u16 {
        match self {
            Self::Requests => 3,
            Self::Response => 2,
        }
    }

    fn input_width(self, areas: UiLayout) -> u16 {
        self.area(areas).width.saturating_sub(self.prefix_width())
    }

    fn input_mut(self, app: &mut App) -> Option<&mut crate::editor::EditInput> {
        match self {
            Self::Requests => app.view.requests.search.as_mut(),
            Self::Response => app.view.response.search.as_mut(),
        }
    }
}

fn handle_search_mouse(
    app: &mut App,
    event: MouseEvent,
    areas: UiLayout,
    target: SearchTarget,
) -> bool {
    let search_area = target.area(areas);
    if matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
        && contains(search_area, event.column, event.row)
    {
        let column = usize::from(
            event
                .column
                .saturating_sub(search_area.x.saturating_add(target.prefix_width())),
        );
        let width = usize::from(target.input_width(areas));
        if let Some(input) = target.input_mut(app) {
            input.place_cursor(input.visible_column(width, column));
        }
        return true;
    }
    if matches!(
        event.kind,
        MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
    ) {
        app.confirm_active_input();
        return false;
    }
    true
}

/// 响应操作菜单的鼠标处理；返回 true 表示事件已由菜单处理。
fn handle_response_menu_mouse(app: &mut App, event: MouseEvent, areas: UiLayout) -> bool {
    if !app.view.response.menu.is_open() {
        return false;
    }
    if !matches!(
        event.kind,
        MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::Moved
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
    ) {
        return false;
    }
    let menu = response_menu_area(areas.response);
    let item_count = ResponseMenuAction::all().len();
    match app
        .view
        .response
        .menu
        .handle_mouse(event, Rect::default(), menu, item_count)
    {
        tui_assets_rust::DropdownEvent::Selected(index) => {
            app.choose_response_action(index);
            true
        }
        // 点击菜单外：菜单已关闭，点击继续按内容区域处理。
        tui_assets_rust::DropdownEvent::Closed => false,
        // 菜单内的其余事件不再传递给内容区域。
        _ => contains(menu, event.column, event.row),
    }
}

fn handle_configuration_mouse(
    app: &mut App,
    event: MouseEvent,
    screen: Rect,
    areas: UiLayout,
    anchor: Rect,
    curl_layout: Option<CurlImportLayout>,
) {
    let Some(row_count) = app.view.dialog.as_ref().and_then(|dialog| match dialog {
        Dialog::Configurations(dialog) => Some(dialog.rows.len()),
        _ => None,
    }) else {
        return;
    };
    let menu = configuration_menu_area(screen, anchor, row_count);
    let dropdown_event = match app.view.dialog.as_mut() {
        Some(Dialog::Configurations(dialog)) => {
            dialog
                .state
                .handle_mouse(event, Rect::default(), menu, row_count)
        }
        _ => return,
    };
    match dropdown_event {
        tui_assets_rust::DropdownEvent::Selected(_) => app.apply_dialog(),
        tui_assets_rust::DropdownEvent::Closed => {
            app.close_dialog();
            if let Some(layout) = curl_layout {
                handle_curl_import_click(app, event.column, event.row, layout);
            } else if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
                handle_click(app, event.column, event.row, areas, false);
            } else if matches!(
                event.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            ) {
                let direction = if matches!(event.kind, MouseEventKind::ScrollUp) {
                    -1
                } else {
                    1
                };
                handle_scroll(app, event.column, event.row, areas, direction);
            }
        }
        tui_assets_rust::DropdownEvent::Opened
        | tui_assets_rust::DropdownEvent::None
        | tui_assets_rust::DropdownEvent::SelectionChanged(_) => {}
    }
}

fn handle_curl_import_mouse(app: &mut App, event: MouseEvent, layout: CurlImportLayout) {
    if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
        handle_curl_import_click(app, event.column, event.row, layout);
    }
}

fn handle_curl_import_click(app: &mut App, column: u16, row: u16, layout: CurlImportLayout) {
    if let Some((focus, position)) = curl_field_at(app, layout, column, row) {
        if let Some((line, column)) = position {
            app.place_curl_import_cursor(focus, line, column);
        } else {
            app.focus_curl_import(focus);
        }
    } else if contains(layout.workspace, column, row) {
        app.focus_curl_import(CurlImportFocus::Workspace);
    }
}

fn curl_field_at(
    app: &App,
    layout: CurlImportLayout,
    column: u16,
    row: u16,
) -> Option<(CurlImportFocus, Option<(usize, usize)>)> {
    for focus in [
        CurlImportFocus::Name,
        CurlImportFocus::Description,
        CurlImportFocus::RegisteredVariables,
    ] {
        let Some(area) = curl_field_area(layout, focus) else {
            continue;
        };
        if contains_curl_field(area, layout.wide(), column, row) {
            return Some((focus, curl_field_position(app, layout, focus, column, row)));
        }
    }
    if contains(layout.command, column, row) {
        return Some((
            CurlImportFocus::Command,
            curl_field_position(app, layout, CurlImportFocus::Command, column, row),
        ));
    }
    None
}

fn curl_field_position(
    app: &App,
    layout: CurlImportLayout,
    focus: CurlImportFocus,
    column: u16,
    row: u16,
) -> Option<(usize, usize)> {
    if focus == CurlImportFocus::Command {
        let input = layout.command.inner(Margin::new(2, 2));
        return contains(input, column, row).then(|| {
            (
                usize::from(row.saturating_sub(input.y)),
                usize::from(column.saturating_sub(input.x)),
            )
        });
    }
    let area = curl_field_area(layout, focus)?;
    let page = app.view.curl_import.as_ref()?;
    // 聚焦字段按可见窗口换算光标位置。
    let place = |position: usize, width: usize| {
        if page.focused(focus) {
            crate::editor::visible_column(
                page.field_value(focus),
                page.cursor(focus),
                width,
                position,
            )
        } else {
            position
        }
    };
    if layout.wide() {
        (row >= area.y && row < area.bottom()).then(|| {
            let line = usize::from(row - area.y);
            let width = usize::from(area.width).saturating_sub(2);
            let position = usize::from(column.saturating_sub(area.x.saturating_add(2)));
            (
                line,
                if line == 0 {
                    place(position, width)
                } else {
                    position
                },
            )
        })
    } else {
        let label_width = compact_field_label_width(app.text());
        let label_cells = coordinate(label_width.saturating_add(2));
        let position = usize::from(column.saturating_sub(area.x.saturating_add(label_cells)));
        let width = usize::from(area.width).saturating_sub(label_width.saturating_add(2));
        Some((0, place(position, width)))
    }
}

fn curl_field_area(layout: CurlImportLayout, focus: CurlImportFocus) -> Option<Rect> {
    match focus {
        CurlImportFocus::Name => Some(layout.name),
        CurlImportFocus::Description => Some(layout.description),
        CurlImportFocus::RegisteredVariables => Some(layout.registered_variables),
        CurlImportFocus::Workspace | CurlImportFocus::Command => None,
    }
}

fn contains_curl_field(area: Rect, stacked: bool, column: u16, row: u16) -> bool {
    contains(area, column, row)
        || stacked && row == area.y.saturating_sub(1) && column >= area.x && column < area.right()
}

fn handle_click(app: &mut App, column: u16, row: u16, areas: UiLayout, is_double: bool) {
    app.view.response.selection = None;
    if contains(areas.preview_content, column, row) {
        let editing = if app.view.preview.active_tab == PreviewTab::Body {
            let line = usize::from(row - areas.preview_content.y)
                .saturating_add(app.view.preview.scroll.offset());
            let column = usize::from(column - areas.preview_content.x);
            app.place_content_editor_cursor(line, column)
        } else {
            place_inline_editor_cursor(app, column, row, areas.preview_content)
        };
        if editing {
            app.view.focus = Focus::Preview;
            return;
        }
    }
    if !is_double && app.is_editing() {
        app.confirm_active_input();
    }
    focus_panel_at(app, column, row, areas);

    if contains(areas.request_search, column, row) {
        app.open_request_search();
    } else if contains(areas.request_scrollbar, column, row) {
        app.view.focus = Focus::Requests;
        click_request_list_scrollbar(app, row, areas);
    } else if contains(areas.request_list, column, row) {
        click_request_list(app, column, row, areas.request_list, is_double);
    } else if (contains(areas.preview_summary, column, row)
        || contains(areas.preview_content, column, row))
        && app.has_current_request()
    {
        app.view.focus = Focus::Preview;
        if contains(areas.preview_summary, column, row) {
            let method_width = app
                .current_request()
                .and_then(|request| app.request_draft(&request.id))
                .map(|draft| {
                    u16::try_from(draft.method.len())
                        .unwrap_or(u16::MAX)
                        .saturating_add(3)
                })
                .unwrap_or_default();
            if column < areas.preview_summary.x.saturating_add(method_width) {
                app.cycle_method();
            }
        } else {
            if app.view.preview.active_tab == PreviewTab::Body {
                let line = usize::from(row.saturating_sub(areas.preview_content.y))
                    .saturating_add(app.view.preview.scroll.offset());
                let column = usize::from(column.saturating_sub(areas.preview_content.x));
                app.select_content_field_at(line);
                app.start_content_edit_at(line, column, is_double);
                return;
            }
            let tab = app.view.preview.active_tab;
            if open_inline_editor(app, tab) {
                handle_inline_editor_click(app, column, row, areas.preview_content, is_double);
            }
        }
    } else if contains(areas.preview_tabs, column, row) && app.has_current_request() {
        app.view.focus = Focus::Preview;
        if let Some(tab) = preview_tab_at(areas.preview_tabs, column, app) {
            app.activate_preview_tab(tab);
        }
    } else if contains(areas.response, column, row) {
        app.view.focus = Focus::Response;
        if !begin_response_selection(app, column, row, areas) {
            if let Some(tab) = response_tab_at(app, column, row, areas) {
                app.select_response_tab(tab);
            } else {
                click_response_scrollbar(app, column, row, areas);
            }
        }
    }
}

fn click_request_list(app: &mut App, column: u16, row: u16, area: Rect, is_double: bool) {
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
    let same_request = app.workspace_state.selected_request == Some(index);
    app.select_request(index);
    app.view.focus = Focus::Requests;
    tracing::debug!(index, "通过左侧接口列表选择接口");
    if is_double
        && same_request
        && app.can_execute_preview_action(PreviewAction::Send)
        && app
            .current_request()
            .is_some_and(|request| app.request_status(&request.id) != RequestStatus::Sending)
    {
        app.handle_preview_action(PreviewAction::Send);
    }
}

fn handle_scroll(app: &mut App, column: u16, row: u16, areas: UiLayout, direction: isize) {
    if contains(areas.request_scrollbar, column, row) || contains(areas.request_list, column, row) {
        tracing::trace!(column, row, direction, "滚动左侧接口列表");
        let count = app.visible_request_indices().len();
        app.view
            .requests
            .scroll
            .move_by(direction, count, usize::from(areas.request_list.height));
    } else if contains(areas.response, column, row) {
        tracing::trace!(column, row, direction, "滚动响应内容");
        app.scroll_response(direction);
    } else if contains(areas.preview_content, column, row) {
        if app.view.preview.active_tab == PreviewTab::Body {
            app.view.preview.scroll.move_by(direction);
        } else {
            // 打开表格会把焦点移到预览。
            let focus = app.view.focus;
            if open_inline_editor(app, app.view.preview.active_tab) {
                app.view.focus = focus;
                scroll_inline_editor(app, direction, areas.preview_content);
            }
        }
    }
}

/// 参数或请求头页签的表格；未打开时先打开。
fn open_inline_editor(app: &mut App, tab: PreviewTab) -> bool {
    if app.editing_preview_tab().is_none() {
        app.handle_preview_action(PreviewAction::Edit(tab));
    }
    app.editing_preview_tab() == Some(tab)
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
