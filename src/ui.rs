use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    symbols::{border, scrollbar::VERTICAL},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Cell, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap,
    },
};
use ratatui_interact::components::{Button, ButtonState, ButtonStyle, ButtonVariant};

use crate::{
    app::{
        App, AppPrompt, Dialog, DialogFocus, Focus, HeaderField, HeaderSource, PreviewAction,
        PreviewTab, RequestStatus, ResponseMenuAction, supports_method,
    },
    config::ApiRequest,
    highlight,
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
use layout::{UiLayout, preview_summary_height, screen as screen_layout, screen_with_summary};

const TABLE_HIGHLIGHT_WIDTH: u16 = 2;
const TABLE_COLUMN_SPACING: u16 = 1;
pub(crate) fn draw(frame: &mut Frame<'_>, app: &App) {
    let areas = screen_layout_for_app(frame.area(), app);
    let theme = &app.global_config.theme;

    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background).fg(theme.text)),
        frame.area(),
    );

    draw_header(
        frame,
        areas.header,
        areas.header_content,
        areas.send_button,
        app,
    );
    draw_request_list(
        frame,
        areas.requests,
        areas.workspace_label,
        areas.variables_button,
        areas.request_list,
        areas.request_scrollbar,
        app,
    );
    draw_preview(
        frame,
        areas.preview,
        areas.preview_summary,
        areas.preview_tabs,
        areas.preview_content,
        app,
    );
    draw_response(frame, areas.response, areas.response_menu_button, app);
    if app.response_state.menu_open {
        draw_response_menu(
            frame,
            response_menu_area(areas.response, areas.response_menu_button),
            app,
        );
    }
    draw_footer(frame, areas.footer, app);
    if let Some(Dialog::Variables(dialog)) = &app.dialog {
        draw_dialog(frame, app, dialog);
    }
    if app.prompt.is_some() {
        draw_app_prompt(frame, app);
    }
}

pub(crate) fn handle_mouse(app: &mut App, event: MouseEvent, area: Rect) {
    if matches!(app.dialog, Some(Dialog::Variables(_))) {
        handle_dialog_mouse(app, event, area);
        return;
    }
    let areas = screen_layout_for_app(area, app);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            tracing::debug!(
                column = event.column,
                row = event.row,
                area = ?area,
                "处理鼠标左键点击"
            );
            handle_click(app, event.column, event.row, areas);
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let direction = if matches!(event.kind, MouseEventKind::ScrollUp) {
                -1
            } else {
                1
            };
            tracing::debug!(
                kind = ?event.kind,
                column = event.column,
                row = event.row,
                direction,
                "处理鼠标滚动"
            );
            handle_scroll(app, event.column, event.row, areas, direction);
        }
        MouseEventKind::Moved if app.response_state.menu_open => {
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
        app.response_state.menu_selected = index;
    }
}

fn handle_click(app: &mut App, column: u16, row: u16, areas: UiLayout) {
    app.commit_active_editors();

    if app.response_state.menu_open {
        let menu = response_menu_area(areas.response, areas.response_menu_button);
        let content = menu.inner(Margin::new(1, 1));
        if contains(content, column, row) {
            app.choose_response_action(usize::from(row.saturating_sub(content.y)));
            return;
        }
        if contains(areas.response_menu_button, column, row) {
            app.close_response_menu();
            return;
        }
        app.close_response_menu();
    }

    if contains(areas.variables_button, column, row) {
        app.focus = Focus::Variables;
        app.open_variables();
    } else if contains(areas.request_list, column, row) {
        click_request_list(app, column, row, areas.request_list);
    } else if contains(areas.preview_summary, column, row) && app.has_current_request() {
        app.focus = Focus::Preview;
        let method_width = app
            .current_request()
            .map(|request| u16::try_from(request.method.len()).unwrap_or(u16::MAX) + 3)
            .unwrap_or_default();
        if column < areas.preview_summary.x.saturating_add(method_width) {
            app.cycle_method();
        } else {
            app.start_url_edit();
        }
    } else if contains(areas.preview_content, column, row) && app.has_current_request() {
        if app.preview_state.active_tab == PreviewTab::Body {
            let line = usize::from(row.saturating_sub(areas.preview_content.y))
                .saturating_add(usize::from(app.preview_state.scroll.offset()));
            if let Some(variable) = request_variable_at(areas.preview_content, column, row, app) {
                app.start_request_variable_edit(variable, line);
                app.focus = Focus::Preview;
                return;
            }
            let column = usize::from(column.saturating_sub(areas.preview_content.x));
            app.start_body_edit(line, column);
            app.focus = Focus::Preview;
            return;
        }
        if app.editing_preview_tab().is_none() {
            app.focus = Focus::Preview;
            app.handle_preview_action(PreviewAction::Edit(app.preview_state.active_tab));
        }
        if app.editing_preview_tab().is_some() {
            handle_inline_editor_click(app, column, row, areas.preview_content);
        }
    } else if contains(areas.preview_tabs, column, row) && app.has_current_request() {
        app.focus = Focus::Preview;
        if let Some(tab) = preview_tab_at(areas.preview_tabs, column, app) {
            app.activate_preview_tab(tab);
        }
    } else if contains(areas.response_menu_button, column, row) {
        app.open_response_menu();
    } else if contains(areas.send_button, column, row)
        && app.can_execute_preview_action(PreviewAction::Send)
    {
        app.handle_preview_action(PreviewAction::Send);
    }
}

fn screen_layout_for_app(area: Rect, app: &App) -> UiLayout {
    let base = screen_layout(area);
    if !app.has_current_request() {
        return base;
    }
    let Some(request) = app.current_request() else {
        return base;
    };
    let url = app.resolved_url(request);
    let summary_height = preview_summary_height(
        base.preview_details.width,
        &format!("[ {} ]", request.method),
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
    app.focus = Focus::Requests;
    tracing::debug!(index, "通过左侧接口列表选择接口");
}

fn handle_scroll(app: &mut App, column: u16, row: u16, areas: UiLayout, direction: isize) {
    let response_menu = response_menu_area(areas.response, areas.response_menu_button);
    if app.response_state.menu_open && contains(response_menu, column, row) {
        app.move_response_menu_selection(direction);
    } else if contains(areas.request_list, column, row) {
        app.focus = Focus::Requests;
        tracing::debug!(column, row, direction, "滚动左侧接口列表");
        app.move_request(direction);
    } else if contains(areas.response, column, row) {
        tracing::debug!(column, row, direction, "滚动响应内容");
        app.scroll_response(direction);
    } else if contains(areas.preview_content, column, row) {
        app.focus = Focus::Preview;
        if app.preview_state.active_tab == PreviewTab::Body {
            app.preview_state.scroll.move_by(direction);
        } else {
            let tab = app.preview_state.active_tab;
            if app.editing_preview_tab() != Some(tab) {
                app.handle_preview_action(PreviewAction::Edit(tab));
            }
            if app.editing_preview_tab() == Some(tab) {
                app.move_dialog_selection(direction);
            }
        }
    }
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
    let height = match app.prompt {
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
            .title(match app.prompt {
                Some(AppPrompt::ConfirmExit) => text.unsaved_requests(),
                Some(AppPrompt::ConfirmDelete { .. }) => text.delete_request(),
                None => "",
            }),
        area,
    );
    let inner = area.inner(Margin::new(2, 1));
    match &app.prompt {
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
