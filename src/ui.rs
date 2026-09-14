use crate::{
    app::{
        App, AppPrompt, Dialog, Focus, HeaderSource, KeyValueField, PreviewAction, PreviewTab,
        RequestStatus, ResponseMenuAction, ResponseTab, VariablePageFocus,
    },
    config::ApiRequest,
    highlight, http_method,
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    symbols::{border, scrollbar::VERTICAL},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap,
    },
};

mod chrome;
mod dialog;
mod focus;
mod inline_editor;
mod layout;
mod mouse;
mod preview;
mod response;
mod response_toolbar;
mod variables;
mod widgets;

use chrome::*;
use dialog::*;
use inline_editor::*;
pub(crate) use mouse::handle_mouse;
use preview::*;
use response::*;
use response_toolbar::*;
use variables::{draw_variables_page, handle_variables_mouse};
use widgets::*;

use focus::FocusStyles;
use layout::{
    ScrollAreas, UiLayout, inner_scroll_areas, response_zoom, screen as screen_layout,
    screen_with_summary, variables_page,
};

const TABLE_HIGHLIGHT_WIDTH: u16 = 2;
const TABLE_COLUMN_SPACING: u16 = 1;
const INLINE_DELETE_WIDTH: u16 = 3;
const DELETE_ICON: &str = "−";
pub(crate) fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let areas = screen_layout_for_app(frame.area(), app);
    if app.view.variables.is_none() {
        sync_response_scroll(app, areas);
    }
    let theme = &app.global_config.theme;

    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background).fg(theme.text)),
        frame.area(),
    );

    if app.error_page().is_some() {
        draw_error_page(frame, app);
        return;
    }

    draw_header(frame, areas.header, areas.header_content, app);
    draw_footer(frame, areas.footer, app);
    if let Some(variables) = &app.view.variables {
        draw_variables_page(frame, app, variables, areas.response);
    } else {
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
            draw_request_send_button(frame, areas.send_button, app);
        }
        draw_response(
            frame,
            areas.response,
            areas.response_format_button,
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
    }
    if app.view.prompt.is_some() {
        draw_app_prompt(frame, app);
    }
    if app.view.help_scroll.is_some() {
        draw_help(frame, app);
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
    let summary_height = preview_summary_height(app, base.preview_details.width);
    screen_with_summary(area, summary_height)
}

fn draw_help(frame: &mut Frame<'_>, app: &mut App) {
    let Some(scroll) = app.view.help_scroll else {
        return;
    };
    let theme = &app.global_config.theme;
    let content = app.text().help_content(app.key_context(), app.debug_mode);
    let width = frame.area().width.saturating_sub(4).min(72);
    let height = frame.area().height.saturating_sub(2).min(
        u16::try_from(content.lines().count())
            .unwrap_or(u16::MAX)
            .saturating_add(4),
    );
    let area = Rect::new(
        frame.area().x + frame.area().width.saturating_sub(width) / 2,
        frame.area().y + frame.area().height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .title(app.text().help_title());
    let inner = block.inner(area).inner(Margin::new(2, 1));
    frame.render_widget(block, area);
    let max_scroll = content
        .lines()
        .count()
        .saturating_sub(usize::from(inner.height));
    let scroll = scroll.min(u16::try_from(max_scroll).unwrap_or(u16::MAX));
    app.view.help_scroll = Some(scroll);
    frame.render_widget(Paragraph::new(content).scroll((scroll, 0)), inner);
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
    let (title, message, hint, height) = match app.view.prompt {
        Some(AppPrompt::ConfirmDelete { .. }) => (
            text.delete_request(),
            text.delete_request_message(),
            text.delete_request_hint(),
            5,
        ),
        Some(AppPrompt::ConfirmQuit) => (
            text.quit_title(),
            text.quit_modified_message(),
            text.confirmation_hint(),
            6,
        ),
        None => return,
    };
    let width = frame.area().width.saturating_sub(4).min(56);
    let height = height.min(frame.area().height);
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
            .title(title),
        area,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(message),
            Line::from(Span::styled(hint, Style::default().fg(theme.accent))),
        ]),
        area.inner(Margin::new(2, 1)),
    );
}

fn draw_error_page(frame: &mut Frame<'_>, app: &App) {
    let theme = &app.global_config.theme;
    let Some(error_page) = app.error_page() else {
        return;
    };
    if frame.area().width < 48 || frame.area().height < 12 {
        let area = centered_area(frame.area(), frame.area().width.saturating_sub(4).max(1), 7);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.error))
                .style(Style::default().bg(theme.surface).fg(theme.text))
                .title(" Error "),
            area,
        );
        frame.render_widget(
            Paragraph::new("Resize the terminal to view the configuration error.")
                .wrap(Wrap { trim: true }),
            area.inner(Margin::new(2, 1)),
        );
        return;
    }

    let width = frame.area().width.saturating_sub(4).min(84);
    let content_width = width.saturating_sub(6).max(1);
    let detail = error_page_detail(error_page);
    let detail_height = error_wrapped_line_count(&detail, content_width);
    let height = detail_height
        .saturating_add(6)
        .min(frame.area().height.saturating_sub(2))
        .max(8);
    let area = centered_area(frame.area(), width, height);

    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.error))
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .title(" Error ");
    let content = block.inner(area).inner(Margin::new(2, 1));
    frame.render_widget(block, area);

    let details_height = content.height.saturating_sub(2).max(1);
    let details_area = Rect::new(content.x, content.y, content.width, details_height);
    frame.render_widget(
        Paragraph::new(detail).wrap(Wrap { trim: false }),
        details_area,
    );

    let action_y = content.y.saturating_add(content.height.saturating_sub(1));
    let action_area = Rect::new(content.x, action_y, content.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "Esc",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Continue with defaults    "),
            Span::styled(
                "E",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Open file"),
        ]))
        .style(Style::default().fg(theme.muted)),
        action_area,
    );
}

fn error_page_detail(error_page: &crate::app::ErrorPage) -> String {
    let mut detail = error_page.message.clone();
    if let Some(editor_error) = &error_page.editor_error {
        detail.push_str("\n\nEditor: ");
        detail.push_str(editor_error);
    }
    detail
}

fn error_wrapped_line_count(text: &str, width: u16) -> u16 {
    let width = usize::from(width.max(1));
    text.lines()
        .map(|line| {
            let line_width = Line::from(line).width();
            u16::try_from(line_width.div_ceil(width))
                .unwrap_or(u16::MAX)
                .max(1)
        })
        .sum::<u16>()
        .max(1)
}

fn centered_area(area: Rect, width: u16, height: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(area.width.saturating_sub(width) / 2),
        area.y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width.min(area.width),
        height.min(area.height),
    )
}
