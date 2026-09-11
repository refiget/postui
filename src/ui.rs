use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    symbols::scrollbar::VERTICAL,
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap,
    },
};
use ratatui_interact::components::{Button, ButtonState, ButtonStyle, ButtonVariant};

use crate::{
    app::{
        App, Dialog, DialogFocus, Focus, HeaderField, HeaderSource, PreviewAction, PreviewTab,
        RequestStatus, supports_method,
    },
    config::ApiRequest,
    highlight, template,
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
use layout::{UiLayout, preview_sections, screen as screen_layout};

const TABLE_HIGHLIGHT_WIDTH: u16 = 2;
const PREVIEW_ACTION_COUNT: usize = 2;

#[cfg(test)]
use layout::{PREVIEW_ACTION_WIDTH, SEND_BUTTON_HEIGHT};

pub(crate) fn draw(frame: &mut Frame<'_>, app: &App) {
    let areas = screen_layout(frame.area());
    let theme = &app.global_config.theme;

    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background).fg(theme.text)),
        frame.area(),
    );

    draw_header(frame, areas.header, app);
    draw_request_list(
        frame,
        areas.requests,
        areas.collection_label,
        areas.variables_button,
        areas.request_list,
        areas.request_scrollbar,
        app,
    );
    draw_preview(
        frame,
        areas.preview,
        areas.preview_details,
        areas.edit_button,
        areas.send_button,
        app,
    );
    draw_response(frame, areas.response, app);
    draw_footer(frame, areas.footer, app);
    if let Some(dialog @ Dialog::Variables(_)) = &app.dialog {
        draw_dialog(frame, app, dialog);
    }
}

pub(crate) fn handle_mouse(app: &mut App, event: MouseEvent, area: Rect) {
    if matches!(app.dialog, Some(Dialog::Variables(_))) {
        handle_dialog_mouse(app, event, area);
        return;
    }
    let areas = screen_layout(area);
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
        _ => {}
    }
}

fn handle_click(app: &mut App, column: u16, row: u16, areas: UiLayout) {
    app.blur_body_editor();

    if contains(areas.variables_button, column, row) {
        app.focus = Focus::Variables;
        app.open_variables();
    } else if contains(areas.request_list, column, row) {
        click_request_list(app, column, row, areas.request_list);
    } else if contains(areas.preview_content, column, row) {
        if app.preview_state.active_tab == PreviewTab::Body {
            let line = usize::from(row.saturating_sub(areas.preview_content.y))
                .saturating_add(usize::from(app.preview_state.scroll.offset()));
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
    } else if contains(areas.preview_tabs, column, row) {
        app.focus = Focus::Preview;
        if let Some(hit) = preview_tab_at(areas.preview_tabs, column, app) {
            match hit {
                PreviewTabHit::Activate(tab) => app.activate_preview_tab(tab),
                PreviewTabHit::Add(tab) => app.add_preview_row(tab),
            }
        }
    } else if let Some(action) = preview_action_hit(
        [
            (
                areas.edit_button,
                PreviewAction::Edit(app.preview_state.active_tab),
            ),
            (areas.send_button, PreviewAction::Send),
        ],
        column,
        row,
    ) {
        app.handle_preview_action(action);
    }
}

fn preview_action_hit(
    actions: [(Rect, PreviewAction); PREVIEW_ACTION_COUNT],
    column: u16,
    row: u16,
) -> Option<PreviewAction> {
    actions
        .into_iter()
        .find_map(|(area, action)| contains(area, column, row).then_some(action))
}

fn click_request_list(app: &mut App, column: u16, row: u16, area: Rect) {
    if area.is_empty() || row < area.y || column >= area.right() {
        return;
    }
    let visible = usize::from(area.height);
    let offset = request_list_offset(
        app.requests_state.selected_request,
        app.config.requests.len(),
        visible,
    );
    let index = offset.saturating_add(usize::from(row - area.y));
    if index >= app.config.requests.len() {
        return;
    }
    app.select_request(index);
    app.focus = Focus::Requests;
    tracing::debug!(index, "通过左侧接口列表选择接口");
    app.status = app.text().selected_request(&app.current_request().name);
}

fn handle_scroll(app: &mut App, column: u16, row: u16, areas: UiLayout, direction: isize) {
    if contains(areas.request_list, column, row) {
        app.focus = Focus::Requests;
        tracing::debug!(column, row, direction, "滚动左侧接口列表");
        app.move_request(direction);
    } else if contains(areas.response, column, row) {
        tracing::debug!(column, row, direction, "滚动响应内容");
        app.scroll_response(direction);
    } else if contains(areas.preview_content, column, row) && app.editing_preview_tab().is_none() {
        app.focus = Focus::Preview;
        app.preview_state.scroll.move_by(direction);
    }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

#[cfg(test)]
mod tests;
