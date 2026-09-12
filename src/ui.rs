use crate::{
    app::{
        App, AppPrompt, Dialog, Focus, HeaderSource, KeyValueField, PreviewAction, PreviewTab,
        RequestStatus, ResponseMenuAction, VariablePageFocus, supports_method,
    },
    config::ApiRequest,
    highlight,
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    symbols::{border, scrollbar::VERTICAL},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Cell, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap,
    },
};

mod chrome;
mod dialog;
mod focus;
mod layout;
mod preview;
mod response;
mod widgets;

use chrome::*;
use dialog::*;
use preview::*;
use response::*;
use widgets::*;

use focus::FocusStyles;
use layout::{
    UiLayout, preview_summary_height, response_zoom, screen as screen_layout, screen_with_summary,
    variables_page,
};

const TABLE_HIGHLIGHT_WIDTH: u16 = 2;
const TABLE_COLUMN_SPACING: u16 = 1;
pub(crate) fn draw(frame: &mut Frame<'_>, app: &App) {
    let areas = screen_layout_for_app(frame.area(), app);
    let theme = &app.global_config.theme;

    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background).fg(theme.text)),
        frame.area(),
    );

    if let Some(variables) = &app.view.variables {
        draw_header(
            frame,
            areas.header,
            areas.header_content,
            Rect::default(),
            app,
        );
        draw_footer(frame, areas.footer, app);
        draw_variables_page(frame, app, variables, areas.response);
        return;
    }

    draw_header(
        frame,
        areas.header,
        areas.header_content,
        areas.send_button,
        app,
    );
    draw_footer(frame, areas.footer, app);
    if !app.response_zoomed() {
        draw_request_list(frame, areas, app);
        draw_preview(
            frame,
            areas.preview,
            areas.preview_summary,
            areas.preview_tabs,
            areas.preview_content,
            app,
        );
    }
    draw_response(
        frame,
        areas.response,
        areas.response_menu_button,
        areas.response_zoom_button,
        app,
    );
    if app.view.response.menu_selection.is_some() {
        draw_response_menu(
            frame,
            response_menu_area(areas.response, areas.response_menu_button),
            app,
        );
    }
    if let Some(Dialog::Configurations(dialog)) = &app.view.dialog {
        draw_configuration_dropdown(frame, app, dialog, areas.workspace_selector)
    }
    if app.view.prompt.is_some() {
        draw_app_prompt(frame, app);
    }
}

pub(crate) fn handle_mouse(app: &mut App, event: MouseEvent, area: Rect) {
    if matches!(event.kind, MouseEventKind::Up(MouseButton::Left)) {
        app.view.response.scroll.drag_anchor = None;
    }
    if app.view.prompt.is_some() {
        app.view.response.scroll.drag_anchor = None;
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
            app.view.response.scroll.drag_anchor = None;
            tracing::trace!(
                column = event.column,
                row = event.row,
                area = ?area,
                "处理鼠标左键点击"
            );
            handle_click(app, event.column, event.row, areas, is_double);
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            drag_response_scrollbar(app, event.row, areas);
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
        app.cancel_active_editors();
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
    } else if contains(areas.request_list, column, row) {
        click_request_list(app, column, row, areas.request_list);
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
    } else if contains(areas.response_zoom_button, column, row) {
        app.view.focus = Focus::ResponseZoom;
        app.toggle_response_zoom();
    } else if contains(areas.send_button, column, row)
        && app.can_execute_preview_action(PreviewAction::Send)
    {
        app.view.focus = Focus::SendButton;
        app.handle_preview_action(PreviewAction::Send);
    } else if contains(areas.response, column, row) {
        app.view.focus = Focus::Response;
        click_response_scrollbar(app, column, row, areas);
    }
}

fn screen_layout_for_app(area: Rect, app: &App) -> UiLayout {
    if app.view.variables.is_some() {
        return variables_page(area);
    }
    if app.response_zoomed() {
        return response_zoom(area);
    }
    let base = screen_layout(area);
    if !app.has_current_request() {
        return base;
    }
    let Some(request) = app.current_request() else {
        return base;
    };
    let method = app
        .current_effective_request()
        .map(|request| request.method)
        .unwrap_or_else(|| request.method.clone());
    let url = app.display_url(request);
    let summary_height = preview_summary_height(
        base.preview_details.width,
        &format!("[ {} ]", method),
        app.text().address(),
        &url,
    );
    screen_with_summary(area, summary_height)
}

fn click_request_list(app: &mut App, column: u16, row: u16, area: Rect) {
    if area.is_empty() || row < area.y || column >= area.right() {
        return;
    }
    let visible = usize::from(area.height);
    let offset = request_list_offset(
        app.workspace_state.selected_request.unwrap_or_default(),
        app.workspace_state.requests.len(),
        visible,
    );
    let index = offset.saturating_add(usize::from(row - area.y));
    if index >= app.workspace_state.requests.len() {
        return;
    }
    app.select_request(index);
    app.view.focus = Focus::Requests;
    tracing::debug!(index, "通过左侧接口列表选择接口");
}

fn handle_scroll(app: &mut App, column: u16, row: u16, areas: UiLayout, direction: isize) {
    focus_panel_at(app, column, row, areas);
    let response_menu = response_menu_area(areas.response, areas.response_menu_button);
    if app.view.response.menu_selection.is_some() && contains(response_menu, column, row) {
        app.move_response_menu_selection(direction);
    } else if contains(areas.request_list, column, row) {
        app.view.focus = Focus::Requests;
        tracing::trace!(column, row, direction, "滚动左侧接口列表");
        app.move_request(direction);
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
                app.move_dialog_selection(direction);
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

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn draw_app_prompt(frame: &mut Frame<'_>, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let width = frame.area().width.saturating_sub(4).min(56);
    let height = match app.view.prompt {
        Some(AppPrompt::ConfirmExit | AppPrompt::ConfirmDelete { .. }) => 5,
        None => return,
    }
    .min(frame.area().height);
    let area = Rect::new(
        frame.area().x + frame.area().width.saturating_sub(width) / 2,
        frame.area().y + frame.area().height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent))
            .style(Style::default().bg(theme.surface).fg(theme.text))
            .title(match app.view.prompt {
                Some(AppPrompt::ConfirmExit) => text.unsaved_requests(),
                Some(AppPrompt::ConfirmDelete { .. }) => text.delete_request(),
                None => "",
            }),
        area,
    );
    let inner = area.inner(Margin::new(2, 1));
    match &app.view.prompt {
        Some(AppPrompt::ConfirmExit) => {
            frame.render_widget(
                Paragraph::new(Text::from(vec![
                    Line::from(text.unsaved_exit_message()),
                    Line::from(Span::styled(
                        text.unsaved_exit_hint(),
                        Style::default().fg(theme.accent),
                    )),
                ])),
                inner,
            );
        }
        Some(AppPrompt::ConfirmDelete { .. }) => {
            frame.render_widget(
                Paragraph::new(Text::from(vec![
                    Line::from(text.delete_request_message()),
                    Line::from(Span::styled(
                        text.delete_request_hint(),
                        Style::default().fg(theme.accent),
                    )),
                ])),
                inner,
            );
        }
        None => {}
    }
}
