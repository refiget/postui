use std::cmp::min;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, HighlightSpacing, List, ListItem, ListState,
        Paragraph, Row, Table, TableState, Wrap,
    },
};

use crate::{
    app::{App, Focus, RequestStatus},
    config::{ApiRequest, DownloadTarget},
    highlight,
    i18n::UiText,
    template,
};

const REQUEST_STACK_WIDTH: u16 = 56;
const REQUEST_STACK_MIN_HEIGHT: u16 = 14;
const PREVIEW_STACK_WIDTH: u16 = 72;
const PREVIEW_STACK_MIN_HEIGHT: u16 = 11;
const INFO_BUTTON_STACK_WIDTH: u16 = 44;
const SEND_COLUMN_WIDTH: u16 = 11;
const SEND_BUTTON_HEIGHT: u16 = 3;
const VARIABLE_ROW_HEIGHT: u16 = 3;
const VARIABLE_ACTION_BUTTON_WIDTH: u16 = 9;
const TABLE_HIGHLIGHT_WIDTH: u16 = 2;

#[derive(Debug, Clone, Copy)]
struct UiLayout {
    header: Rect,
    requests: Rect,
    info: Rect,
    info_details: Rect,
    send_button: Rect,
    variables: Rect,
    variable_list: Rect,
    preview: Rect,
    response: Rect,
    footer: Rect,
    dropdown: Rect,
}

pub(crate) fn draw(frame: &mut Frame<'_>, app: &App) {
    let areas = screen_layout(frame.area(), app);
    let theme = &app.global_config.theme;

    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background).fg(theme.text)),
        frame.area(),
    );

    draw_header(frame, areas.header, app);
    draw_requests(frame, areas.requests, app);
    draw_request_info(
        frame,
        areas.info,
        areas.info_details,
        areas.send_button,
        app,
    );
    draw_variables(frame, areas.variables, areas.variable_list, app);
    draw_preview(frame, areas.preview, app);
    draw_response(frame, areas.response, app);
    draw_footer(frame, areas.footer, app);

    if app.dropdown_open {
        draw_dropdown(frame, app, areas.dropdown);
    }
}

pub(crate) fn handle_mouse(app: &mut App, event: MouseEvent, area: Rect) {
    let areas = screen_layout(area, app);
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
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if !app.editing && !app.dropdown_open =>
        {
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
    if app.dropdown_open {
        click_dropdown(app, column, row, areas.dropdown);
        return;
    }

    if contains(areas.requests, column, row) {
        click_request_list(app, row, areas.requests);
    } else if contains(areas.variable_list, column, row) {
        click_variable_list(app, column, row, areas.variable_list);
    } else if contains(areas.response, column, row) {
        click_response(app, column, row, areas.response);
    } else if contains(areas.send_button, column, row) {
        app.focus = Focus::Actions;
        app.send_current_request();
    }
}

fn click_request_list(app: &mut App, row: u16, area: Rect) {
    if row == area.y {
        app.dropdown_open = true;
        app.focus = Focus::Requests;
        tracing::debug!("点击接口栏标题，打开接口下拉列表");
        return;
    }

    let Some(index) = list_row_index(area, row) else {
        return;
    };
    if index >= app.config.requests.len() {
        return;
    }
    select_request_from_ui(app, index, "接口列表");
}

fn click_variable_list(app: &mut App, column: u16, row: u16, area: Rect) {
    app.focus = Focus::Variables;
    if row < area.y {
        return;
    }
    let names = app.current_variable_names();
    let visible_rows = usize::from(area.height / VARIABLE_ROW_HEIGHT).max(1);
    let selected_index = app.variable_index.min(names.len().saturating_sub(1));
    let offset = selected_index.min(names.len().saturating_sub(visible_rows));
    let row_index = usize::from(row - area.y) / usize::from(VARIABLE_ROW_HEIGHT);
    if row_index >= visible_rows {
        return;
    }
    let index = offset + row_index;
    if index >= names.len() {
        return;
    }
    app.variable_index = index;
    let has_extract = app.variable_has_extract(index);
    match variable_action_at(area, row_index, column, row, has_extract) {
        Some(VariableAction::Extract) => {
            tracing::debug!(index, "点击变量提取操作");
            app.extract_variable(index)
        }
        Some(VariableAction::Paste) => {
            tracing::debug!(index, "点击变量粘贴操作");
            app.paste_variable(index)
        }
        Some(VariableAction::Clear) => {
            tracing::debug!(index, "点击变量清理操作");
            app.clear_variable(index)
        }
        None => {
            tracing::debug!(index, "点击变量编辑区域");
            app.edit_current_variable()
        }
    }
}

fn click_response(app: &mut App, column: u16, row: u16, area: Rect) {
    let has_headers = app
        .current_response()
        .is_some_and(|response| !response.headers.is_empty());
    let sections = response_sections(area, has_headers);
    if contains(sections.headers, column, row) {
        app.toggle_response_headers();
    }
}

fn click_dropdown(app: &mut App, column: u16, row: u16, area: Rect) {
    if !contains(area, column, row) {
        app.dropdown_open = false;
        app.focus = Focus::Requests;
        app.status = app.text().request_list_closed().to_string();
        tracing::debug!("点击下拉列表外部，关闭接口列表");
        return;
    }

    let Some(index) = list_row_index(area, row) else {
        return;
    };
    if index >= app.config.requests.len() {
        return;
    }
    select_request_from_ui(app, index, "接口下拉列表");
    app.dropdown_open = false;
}

fn select_request_from_ui(app: &mut App, index: usize, source: &'static str) {
    app.select_request(index);
    app.focus = Focus::Requests;
    tracing::debug!(index, source, "通过鼠标选择接口");
    app.status = app.text().selected_request(&app.current_request().name);
}

fn handle_scroll(app: &mut App, column: u16, row: u16, areas: UiLayout, direction: isize) {
    if contains(areas.requests, column, row) {
        app.focus = Focus::Requests;
        tracing::debug!(column, row, direction, "滚动接口列表");
        app.move_request(direction);
    } else if contains(areas.variables, column, row) {
        app.focus = Focus::Variables;
        tracing::debug!(column, row, direction, "滚动变量列表");
        app.move_variable(direction);
    } else if contains(areas.response, column, row) {
        tracing::debug!(column, row, direction, "滚动响应内容");
        app.scroll_response(direction);
    }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn list_row_index(area: Rect, row: u16) -> Option<usize> {
    let first_row = area.y.saturating_add(1);
    (row >= first_row).then(|| usize::from(row - first_row))
}

fn screen_layout(area: Rect, app: &App) -> UiLayout {
    let header_height = header_height(area.height);
    let footer_height = u16::from(area.height >= 8);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(0),
            Constraint::Length(footer_height),
        ])
        .split(area);

    let (requests, detail_area) = split_request_area(sections[1], area.width);

    let variable_count = app.current_variable_names().len();
    let has_headers = app
        .current_response()
        .is_some_and(|response| !response.headers.is_empty());
    let info_height = request_info_height(detail_area.height);
    let response_min_height = response_min_height(has_headers);
    let desired_variable_height = u16::try_from(variable_count)
        .unwrap_or(u16::MAX)
        .saturating_mul(VARIABLE_ROW_HEIGHT)
        .saturating_add(2)
        .clamp(5, 14);
    let available_variable_height = detail_area
        .height
        .saturating_sub(info_height + response_min_height);
    let variable_height = min(desired_variable_height, available_variable_height);
    let detail = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(info_height),
            Constraint::Length(variable_height),
            Constraint::Min(0),
        ])
        .split(detail_area);
    let (preview, response) = split_preview_area(detail[2]);
    let (info_details, send_button) = request_info_parts(detail[0]);
    let variable_list = detail[1].inner(Margin::new(1, 1));

    UiLayout {
        header: sections[0],
        requests,
        info: detail[0],
        info_details,
        send_button,
        variables: detail[1],
        variable_list,
        preview,
        response,
        footer: sections[2],
        dropdown: centered_rect(70, 70, area),
    }
}

fn header_height(height: u16) -> u16 {
    match height {
        18.. => 4,
        4..=17 => 3,
        _ => 0,
    }
}

fn split_request_area(area: Rect, screen_width: u16) -> (Rect, Rect) {
    if area.width < REQUEST_STACK_WIDTH && area.height >= REQUEST_STACK_MIN_HEIGHT {
        let request_height = if area.height >= 20 { 6 } else { 5 };
        let stacked = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(request_height.min(area.height)),
                Constraint::Min(0),
            ])
            .split(area);
        return (stacked[0], stacked[1]);
    }

    let request_width = request_list_width(screen_width).min(area.width);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(request_width), Constraint::Min(0)])
        .split(area);
    (columns[0], columns[1])
}

fn request_list_width(screen_width: u16) -> u16 {
    match screen_width {
        100.. => 32,
        80..=99 => 28,
        64..=79 => 24,
        _ => 20,
    }
}

fn request_info_height(height: u16) -> u16 {
    match height {
        16.. => 7,
        12..=15 => 6,
        _ => height.min(5),
    }
}

fn response_min_height(has_headers: bool) -> u16 {
    5 + u16::from(has_headers)
}

fn split_preview_area(area: Rect) -> (Rect, Rect) {
    let direction = if area.width < PREVIEW_STACK_WIDTH && area.height >= PREVIEW_STACK_MIN_HEIGHT {
        Direction::Vertical
    } else {
        Direction::Horizontal
    };
    let previews = Layout::default()
        .direction(direction)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    (previews[0], previews[1])
}

fn request_info_parts(area: Rect) -> (Rect, Rect) {
    let inner = area.inner(Margin::new(1, 1));
    if inner.width < INFO_BUTTON_STACK_WIDTH {
        let button_height = inner.height.min(SEND_BUTTON_HEIGHT);
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(button_height)])
            .split(inner);
        return (parts[0], parts[1]);
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(SEND_COLUMN_WIDTH)])
        .split(inner);
    let button = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(SEND_BUTTON_HEIGHT),
            Constraint::Min(1),
        ])
        .split(columns[1])[1];
    (columns[0], button)
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let line = if area.width >= 100 {
        Line::from(vec![
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_focus())),
            Span::styled("↑↓/jk", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_move())),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_select_edit())),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_send())),
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_clear())),
            Span::styled("q", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_quit())),
            Span::styled(text.footer_mouse(), Style::default().fg(theme.accent)),
            Span::raw(format!(" {}", text.footer_click())),
        ])
    } else if area.width >= 54 {
        Line::from(vec![
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_focus())),
            Span::styled("↑↓", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_move())),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_edit())),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_send())),
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_clear())),
            Span::styled("q", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}", text.footer_quit())),
        ])
    } else {
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_action())),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_send())),
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}  ", text.footer_clear())),
            Span::styled("q", Style::default().fg(theme.accent)),
            Span::raw(format!(" {}", text.footer_quit())),
        ])
    };
    let footer = Paragraph::new(line).style(Style::default().fg(theme.muted));
    frame.render_widget(footer, area);
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
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

fn draw_requests(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let items = app
        .config
        .requests
        .iter()
        .map(|request| request_item(request, app, theme, false))
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(panel_block(text.requests(), area, theme))
        .highlight_style(
            Style::default()
                .bg(theme.selection)
                .fg(theme.text)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");
    let mut state = ListState::default();
    state.select(Some(app.selected_request));
    frame.render_stateful_widget(list, area, &mut state);
}

fn request_item(
    request: &ApiRequest,
    app: &App,
    theme: &crate::settings::UiTheme,
    include_url: bool,
) -> ListItem<'static> {
    let status = app.request_status(&request.id);
    let mut line = vec![
        Span::styled(
            format!("[{}]", status.tag()),
            request_status_style(status, theme),
        ),
        Span::raw(" "),
        Span::styled(request.method.clone(), method_style(&request.method, theme)),
        Span::raw("  "),
    ];
    line.extend(highlight::template_spans(
        &request.name,
        Style::default().fg(theme.text),
        theme,
    ));
    if include_url {
        let url = template::display_url(request);
        line.push(Span::styled("  ", label_style(theme)));
        line.extend(highlight::template_spans(&url, label_style(theme), theme));
    }
    ListItem::new(Line::from(line))
}

fn draw_request_info(
    frame: &mut Frame<'_>,
    area: Rect,
    details: Rect,
    send_button: Rect,
    app: &App,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let request = app.current_request();
    let url = template::display_url(request);
    let request_status = app.request_status(&request.id);
    let loading = request_status == RequestStatus::Sending;
    let mut address_line = vec![Span::styled(
        format!("{}  ", text.address()),
        label_style(theme),
    )];
    address_line.extend(highlight::template_spans(
        &url,
        highlight::plain_style(theme),
        theme,
    ));
    let description = if request.description.is_empty() {
        text.empty_description()
    } else {
        request.description.as_str()
    };
    let mut description_line = vec![Span::styled(
        format!("{}  ", text.description()),
        label_style(theme),
    )];
    description_line.extend(highlight::template_spans(
        description,
        highlight::plain_style(theme),
        theme,
    ));
    let lines = vec![
        Line::from(vec![
            Span::styled(format!("{}  ", text.method()), label_style(theme)),
            Span::styled(
                request.method.as_str(),
                method_style(&request.method, theme),
            ),
            Span::styled(format!("  {}  ", text.status()), label_style(theme)),
            Span::styled(
                request_status.label(text),
                request_status_style(request_status, theme),
            ),
        ]),
        Line::from(address_line),
        Line::from(vec![
            Span::styled(format!("{}  ", text.identifier()), label_style(theme)),
            Span::raw(request.id.as_str()),
        ]),
        Line::from(description_line),
    ];
    frame.render_widget(
        panel_block(
            highlight::template_line(&request.name, highlight::plain_style(theme), theme),
            area,
            theme,
        ),
        area,
    );
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), details);

    draw_action_button(
        frame,
        send_button,
        text.send_button(loading),
        theme.secondary,
        app.focus == Focus::Actions,
        !loading,
        theme,
    );
}

fn draw_variables(frame: &mut Frame<'_>, area: Rect, list_area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let variables = app.current_variables();
    let has_extract_actions = !app.current_request().extracts.is_empty();
    frame.render_widget(panel_block(text.variables(), area, theme), area);
    if variables.is_empty() {
        let inner = area.inner(Margin::new(1, 1));
        frame.render_widget(
            Paragraph::new(Span::styled(text.no_variables(), label_style(theme))),
            inner,
        );
        return;
    }

    let columns = variable_columns(list_area, has_extract_actions);
    let selected_index = app.variable_index.min(variables.len().saturating_sub(1));
    let rows = variables
        .iter()
        .enumerate()
        .map(|(index, (name, value))| {
            let editing = app.editing && index == selected_index;
            let (display_value, value_style) = match (editing, value.is_empty()) {
                (true, _) => (
                    format!("{}▌", app.edit_buffer),
                    Style::default().fg(theme.secondary),
                ),
                (false, true) => (text.unset().to_string(), Style::default().fg(theme.muted)),
                (false, false) => (value.clone(), Style::default().fg(theme.text)),
            };
            Row::new(vec![
                Cell::from(highlight::template_line(
                    &variable_label(name, columns.name),
                    highlight::variable_style(theme),
                    theme,
                )),
                Cell::from(highlight::template_line(&display_value, value_style, theme)),
                Cell::from(""),
            ])
            .height(VARIABLE_ROW_HEIGHT)
        })
        .collect::<Vec<_>>();
    let table = Table::new(
        rows,
        [
            Constraint::Length(columns.name),
            Constraint::Length(columns.value),
            Constraint::Length(columns.actions),
        ],
    )
    .column_spacing(0)
    .highlight_spacing(HighlightSpacing::Always)
    .row_highlight_style(Style::default().bg(theme.selection).fg(theme.text))
    .highlight_symbol("▸ ");
    let mut state = TableState::default();
    state.select(Some(selected_index));
    frame.render_stateful_widget(table, list_area, &mut state);

    let visible_rows = usize::from(list_area.height / VARIABLE_ROW_HEIGHT);
    for (display_row, index) in (state.offset()..variables.len())
        .take(visible_rows)
        .enumerate()
    {
        let selected = index == selected_index;
        let variable_has_extract = app.variable_has_extract(index);
        let [extract_button, paste_button, clear_button] =
            variable_action_areas(list_area, display_row, variable_has_extract);
        if variable_has_extract {
            draw_action_button(
                frame,
                extract_button,
                text.extract_button(),
                theme.accent,
                selected && app.focus == Focus::Variables,
                app.can_extract_variable(index),
                theme,
            );
        }
        draw_action_button(
            frame,
            paste_button,
            text.paste_button(),
            theme.secondary,
            selected && app.focus == Focus::Variables,
            true,
            theme,
        );
        draw_action_button(
            frame,
            clear_button,
            text.clear_button(),
            theme.error,
            selected && app.focus == Focus::Variables,
            true,
            theme,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VariableAction {
    Extract,
    Paste,
    Clear,
}

#[derive(Debug, Clone, Copy)]
struct VariableColumns {
    name: u16,
    value: u16,
    actions: u16,
}

fn variable_columns(area: Rect, has_extract_actions: bool) -> VariableColumns {
    let content_width = area.width.saturating_sub(TABLE_HIGHLIGHT_WIDTH);
    let action_width = variable_action_width(has_extract_actions);
    if content_width < action_width.saturating_add(6) {
        return VariableColumns {
            name: content_width,
            value: 0,
            actions: 0,
        };
    }

    let name = if content_width >= 30 {
        16
    } else if content_width >= 26 {
        12
    } else {
        6
    };
    let name = name.min(content_width.saturating_sub(action_width));
    let value = content_width.saturating_sub(name.saturating_add(action_width));
    VariableColumns {
        name,
        value,
        actions: action_width,
    }
}

fn variable_action_width(has_extract: bool) -> u16 {
    VARIABLE_ACTION_BUTTON_WIDTH * variable_button_count(has_extract)
}

fn variable_button_count(has_extract: bool) -> u16 {
    if has_extract { 3 } else { 2 }
}

fn variable_label(name: &str, width: u16) -> String {
    if width < 5 {
        return truncate(name, usize::from(width));
    }
    format!(
        "{{{{{}}}}}",
        truncate(name, usize::from(width).saturating_sub(4))
    )
}

fn variable_action_at(
    area: Rect,
    row: usize,
    column: u16,
    screen_row: u16,
    has_extract: bool,
) -> Option<VariableAction> {
    let [extract_button, paste_button, clear_button] =
        variable_action_areas(area, row, has_extract);
    if contains(extract_button, column, screen_row) {
        Some(VariableAction::Extract)
    } else if contains(paste_button, column, screen_row) {
        Some(VariableAction::Paste)
    } else if contains(clear_button, column, screen_row) {
        Some(VariableAction::Clear)
    } else {
        None
    }
}

fn variable_action_areas(area: Rect, row: usize, has_extract: bool) -> [Rect; 3] {
    let columns = variable_columns(area, has_extract);
    let action_width = variable_action_width(has_extract);
    if columns.actions < action_width {
        return [Rect::default(), Rect::default(), Rect::default()];
    }

    let row_area = Rect::new(
        area.x,
        area.y.saturating_add(
            u16::try_from(row)
                .unwrap_or(u16::MAX)
                .saturating_mul(VARIABLE_ROW_HEIGHT),
        ),
        area.width,
        VARIABLE_ROW_HEIGHT,
    );
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(TABLE_HIGHLIGHT_WIDTH),
            Constraint::Length(columns.name),
            Constraint::Length(columns.value),
            Constraint::Length(columns.actions),
        ])
        .spacing(0)
        .split(row_area);
    let button_count = variable_button_count(has_extract);
    let buttons_width = VARIABLE_ACTION_BUTTON_WIDTH * button_count;
    let buttons_area = Rect::new(
        columns[3]
            .x
            .saturating_add(columns[3].width.saturating_sub(buttons_width)),
        columns[3].y,
        buttons_width.min(columns[3].width),
        columns[3].height,
    );
    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            std::iter::repeat_n(
                Constraint::Length(VARIABLE_ACTION_BUTTON_WIDTH),
                usize::from(button_count),
            )
            .collect::<Vec<_>>(),
        )
        .spacing(0)
        .split(buttons_area);
    if has_extract {
        [buttons[0], buttons[1], buttons[2]]
    } else {
        [Rect::default(), buttons[0], buttons[1]]
    }
}

#[derive(Debug, Clone, Copy)]
struct ResponseLayout {
    status: Rect,
    body: Rect,
    headers: Rect,
}

fn response_sections(area: Rect, has_headers: bool) -> ResponseLayout {
    let inner = area.inner(Margin::new(1, 1));
    let headers_height = u16::from(has_headers);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(headers_height),
        ])
        .split(inner);
    ResponseLayout {
        status: sections[0],
        body: sections[1],
        headers: sections[2],
    }
}

fn draw_preview(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let request = app.current_resolved_request();
    let mut request_line = vec![
        Span::styled(
            request.method.as_str(),
            method_style(&request.method, theme),
        ),
        Span::raw(" "),
    ];
    request_line.extend(highlight::template_spans(
        &request.url,
        highlight::plain_style(theme),
        theme,
    ));
    let mut lines = vec![Line::from(request_line)];

    if !request.headers.is_empty() {
        lines.push(Line::from(Span::styled(
            text.request_headers(),
            section_style(theme),
        )));
        for (key, value) in &request.headers {
            lines.push(field_line(key, value, theme));
        }
    }
    if let Some(body) = &request.raw_body {
        lines.push(Line::from(Span::styled(
            text.request_body(),
            section_style(theme),
        )));
        if let Some(body_lines) = highlight::json_text_lines_if_valid(body, theme) {
            lines.extend(body_lines);
        } else {
            lines.extend(highlight::plain_lines(body, theme));
        }
    }
    if !request.form.is_empty() {
        lines.push(Line::from(Span::styled(text.form(), section_style(theme))));
        for (key, value) in &request.form {
            lines.push(field_line(key, value, theme));
        }
    }
    if !request.files.is_empty() {
        lines.push(Line::from(Span::styled(text.files(), section_style(theme))));
        lines.push(highlight::template_line(
            &format!(
                "{}: {}",
                text.directory(),
                app.config.file_directory.display()
            ),
            highlight::plain_style(theme),
            theme,
        ));
        for file in &request.files {
            let filename = file.filename.as_deref().unwrap_or(text.auto_filename());
            lines.push(highlight::template_line(
                &format!("{}: {} ({filename})", file.field, file.path),
                highlight::plain_style(theme),
                theme,
            ));
        }
    }
    if let Some(download) = &request.download {
        lines.push(Line::from(Span::styled(
            text.download(),
            section_style(theme),
        )));
        lines.push(highlight::template_line(
            &format!(
                "{}: {}",
                text.directory(),
                app.config.download_directory.display()
            ),
            highlight::plain_style(theme),
            theme,
        ));
        lines.push(highlight::template_line(
            &download_target_label(download, text),
            highlight::plain_style(theme),
            theme,
        ));
    }
    let unresolved = template::unresolved_request_names(&request);
    if !unresolved.is_empty() {
        lines.insert(
            0,
            Line::from(Span::styled(
                text.unresolved(&unresolved.join(", ")),
                Style::default().fg(theme.secondary),
            )),
        );
    }
    let block = panel_block(text.preview(), area, theme);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_response(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    frame.render_widget(panel_block(text.response(), area, theme), area);
    let request = app.current_request();
    let request_status = app.request_status(&request.id);
    let loading = request_status == RequestStatus::Sending;
    let response = app.current_response();
    let error = app.current_error();
    let has_headers = response.is_some_and(|response| !response.headers.is_empty());
    let sections = response_sections(area, has_headers);

    let status = match (loading, response, error) {
        (true, _, _) => Line::from(Span::styled(
            text.waiting_response(),
            Style::default().fg(theme.secondary),
        )),
        (false, Some(response), _) => {
            let status_style = request_status_style(request_status, theme);
            let status = if response.reason.is_empty() {
                format!("HTTP {}", response.status)
            } else {
                format!("HTTP {} {}", response.status, response.reason)
            };
            Line::from(vec![
                Span::styled(status, status_style),
                Span::styled(
                    format!("  {} ms", response.elapsed_ms),
                    Style::default().fg(theme.muted),
                ),
            ])
        }
        (false, None, Some(error)) => {
            let message = request_error_message(text, request_status, error);
            Line::from(Span::styled(message, Style::default().fg(theme.error)))
        }
        (false, None, None) => {
            Line::from(Span::styled(text.request_not_sent(), label_style(theme)))
        }
    };
    frame.render_widget(Paragraph::new(status), sections.status);

    if let Some(response) = response.filter(|response| !response.headers.is_empty()) {
        let marker = if app.response_headers_expanded {
            "▾"
        } else {
            "▸"
        };
        let label = format!(
            "{marker} {} ({})",
            text.response_headers(),
            response.headers.len()
        );
        let style = if app.response_headers_expanded {
            Style::default().fg(theme.secondary)
        } else {
            label_style(theme)
        };
        frame.render_widget(Paragraph::new(Span::styled(label, style)), sections.headers);
    }

    let mut body_lines = Vec::new();
    if let Some(response) = response {
        body_lines.push(Line::from(Span::styled(
            text.response_body(),
            section_style(theme),
        )));
        match response.download_path.as_ref() {
            Some(path) => body_lines.push(Line::from(Span::styled(
                text.download_saved(&path.display().to_string()),
                Style::default().fg(theme.success),
            ))),
            None if response.body.is_empty() => body_lines.push(Line::from(Span::styled(
                text.empty_response(),
                label_style(theme),
            ))),
            None => match highlight::json_text_lines_if_valid(&response.body, theme) {
                Some(lines) => body_lines.extend(lines),
                None => body_lines.extend(highlight::plain_lines(&response.body, theme)),
            },
        }
        if app.response_headers_expanded && !response.headers.is_empty() {
            body_lines.push(Line::from(Span::styled(
                text.response_headers(),
                section_style(theme),
            )));
            for (key, value) in &response.headers {
                body_lines.push(field_line(key, value, theme));
            }
        }
    } else if let Some(error) = error {
        let message = request_error_message(text, request_status, error);
        body_lines.push(Line::from(Span::styled(
            message,
            Style::default().fg(theme.error),
        )));
    } else {
        body_lines.push(Line::from(text.send_hint()));
    }
    frame.render_widget(
        Paragraph::new(body_lines)
            .wrap(Wrap { trim: false })
            .scroll((app.response_scroll.offset(), 0)),
        sections.body,
    );
}

fn download_target_label(target: &DownloadTarget, text: UiText) -> String {
    match target {
        DownloadTarget::Path(path) => path.clone(),
        DownloadTarget::RemoteName {
            use_content_disposition: true,
        } => text.remote_filename_from_header().to_string(),
        DownloadTarget::RemoteName {
            use_content_disposition: false,
        } => text.remote_filename().to_string(),
        DownloadTarget::Auto => text.auto_filename().to_string(),
    }
}

fn draw_dropdown(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = &app.global_config.theme;
    let text = app.text();
    frame.render_widget(Clear, area);
    let items = app
        .config
        .requests
        .iter()
        .map(|request| request_item(request, app, theme, true))
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(
            panel_block(text.choose_request(), area, theme)
                .border_style(Style::default().fg(theme.accent)),
        )
        .highlight_style(Style::default().bg(theme.selection).fg(theme.text))
        .highlight_symbol("▸ ");
    let mut state = ListState::default();
    state.select(Some(app.selected_request));
    frame.render_stateful_widget(list, area, &mut state);
}

fn method_style(method: &str, theme: &crate::settings::UiTheme) -> Style {
    let color = match method {
        "GET" => theme.success,
        "POST" => theme.warning,
        "PUT" | "PATCH" => theme.accent,
        "DELETE" => theme.error,
        _ => theme.primary,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn request_status_style(status: RequestStatus, theme: &crate::settings::UiTheme) -> Style {
    let color = match status {
        RequestStatus::NotSent => theme.muted,
        RequestStatus::Sending => theme.warning,
        RequestStatus::Success => theme.success,
        RequestStatus::Failed | RequestStatus::Timeout => theme.error,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn field_line(key: &str, value: &str, theme: &crate::settings::UiTheme) -> Line<'static> {
    highlight::template_line(
        &format!("{key}: {value}"),
        highlight::plain_style(theme),
        theme,
    )
}

fn request_error_message(text: UiText, status: RequestStatus, error: &str) -> String {
    if status == RequestStatus::Timeout {
        text.request_timeout(error)
    } else {
        text.request_failed(error)
    }
}

fn label_style(theme: &crate::settings::UiTheme) -> Style {
    Style::default().fg(theme.muted)
}

fn section_style(theme: &crate::settings::UiTheme) -> Style {
    Style::default()
        .fg(theme.primary)
        .add_modifier(Modifier::BOLD)
}

fn panel_block(
    title: impl Into<Line<'static>>,
    area: Rect,
    theme: &crate::settings::UiTheme,
) -> Block<'static> {
    let block = rounded_block(title, theme);
    if area.width >= 2 && area.height >= 2 {
        block
    } else {
        Block::default()
    }
}

fn draw_action_button(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    color: Color,
    active: bool,
    enabled: bool,
    theme: &crate::settings::UiTheme,
) {
    if area.is_empty() {
        return;
    }
    let button = Paragraph::new(label)
        .alignment(Alignment::Center)
        .style(action_button_style(color, active, enabled, theme))
        .block(action_block(
            area,
            if enabled { color } else { theme.muted },
            theme,
        ));
    frame.render_widget(button, area);
}

fn action_button_style(
    color: Color,
    active: bool,
    enabled: bool,
    theme: &crate::settings::UiTheme,
) -> Style {
    if !enabled {
        Style::default().fg(theme.muted)
    } else if active {
        Style::default()
            .fg(theme.background)
            .bg(color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }
}

fn action_block(area: Rect, color: Color, theme: &crate::settings::UiTheme) -> Block<'static> {
    if area.width >= 6 && area.height >= 3 {
        rounded_block("", theme).border_style(Style::default().fg(color))
    } else {
        Block::default()
    }
}

fn rounded_block(
    title: impl Into<Line<'static>>,
    theme: &crate::settings::UiTheme,
) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .title(title)
}

fn truncate(value: &str, width: usize) -> String {
    if Line::from(value.to_string()).width() <= width {
        return value.to_string();
    }
    if width == 0 {
        return String::new();
    }

    let mut result = String::new();
    let mut used = 0_usize;
    let content_width = width.saturating_sub(Line::from("…").width());
    for character in value.chars() {
        let character_width = Line::from(character.to_string()).width();
        if used.saturating_add(character_width) > content_width {
            break;
        }
        result.push(character);
        used = used.saturating_add(character_width);
    }
    result.push('…');
    result
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{
        app::{App, RequestRuntimeState},
        config::load,
        http::ResponseData,
    };

    #[test]
    fn renders_without_overflow_at_compact_sizes() {
        let config = load(Path::new(".postui/requests.yaml")).expect("示例请求配置应当可以加载");
        let mut app = App::new(
            config,
            PathBuf::from(".postui/requests.yaml"),
            crate::settings::GlobalConfig::default(),
        );
        app.select_request(1);

        for (width, height) in [
            (120, 40),
            (80, 24),
            (64, 20),
            (48, 24),
            (48, 16),
            (36, 20),
            (36, 12),
            (24, 8),
        ] {
            let area = Rect::new(0, 0, width, height);
            let layout = screen_layout(area, &app);
            for rect in [
                layout.header,
                layout.requests,
                layout.info,
                layout.info_details,
                layout.send_button,
                layout.variables,
                layout.variable_list,
                layout.preview,
                layout.response,
                layout.footer,
                layout.dropdown,
            ] {
                assert!(
                    u32::from(rect.x) + u32::from(rect.width) <= u32::from(width)
                        && u32::from(rect.y) + u32::from(rect.height) <= u32::from(height),
                    "布局越界: {rect:?}，终端: {width}x{height}"
                );
            }

            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("应创建测试终端");
            terminal
                .draw(|frame| draw(frame, &app))
                .expect("普通布局应当可以渲染");

            app.dropdown_open = true;
            terminal
                .draw(|frame| draw(frame, &app))
                .expect("下拉列表应当可以渲染");
            app.dropdown_open = false;
        }
    }

    #[test]
    fn response_body_is_primary_and_headers_are_collapsed() {
        let config = load(Path::new(".postui/requests.yaml")).expect("示例请求配置应当可以加载");
        let mut app = App::new(
            config,
            PathBuf::from(".postui/requests.yaml"),
            crate::settings::GlobalConfig::default(),
        );
        app.select_request(1);
        let response_state = RequestRuntimeState::from_response(ResponseData {
            status: 200,
            reason: "OK".to_string(),
            headers: vec![
                ("content-type".to_string(), "application/json".to_string()),
                ("x-test".to_string(), "postui".to_string()),
            ],
            body: r#"{"message":"hello"}"#.to_string(),
            download_path: None,
            elapsed_ms: 1,
        });
        app.request_states
            .insert("post-json".to_string(), response_state);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("应创建测试终端");
        terminal
            .draw(|frame| draw(frame, &app))
            .expect("响应布局应当可以渲染");
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Paste"));
        assert!(rendered.contains("Clear"));
        let body_position = rendered.find("Response body").expect("应显示响应体标题");
        let headers_position = rendered
            .find("▸ Response headers (2)")
            .expect("响应头默认应折叠");
        assert!(body_position < headers_position);
        assert!(!rendered.contains("content-type: application/json"));
        assert!(!rendered.contains("json.message"));

        app.toggle_response_headers();
        assert!(app.response_headers_expanded);
    }

    #[test]
    fn renders_extract_action_on_a_configured_variable() {
        let config = load(Path::new(".postui/requests.yaml")).expect("示例请求配置应当可以加载");
        let mut app = App::new(
            config,
            PathBuf::from(".postui/requests.yaml"),
            crate::settings::GlobalConfig::default(),
        );
        app.select_request(1);
        let index = app
            .current_variable_names()
            .iter()
            .position(|name| name == "posted_message")
            .expect("POST JSON 接口应声明提取变量");
        app.variable_index = index;
        app.request_states.insert(
            app.current_request().id.clone(),
            RequestRuntimeState::from_response(ResponseData {
                status: 200,
                reason: "OK".to_string(),
                headers: Vec::new(),
                body: r#"{"json":{"message":"from-response"}}"#.to_string(),
                download_path: None,
                elapsed_ms: 1,
            }),
        );

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("应创建测试终端");
        terminal
            .draw(|frame| draw(frame, &app))
            .expect("提取变量行应当可以渲染");
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Extract"));
        assert!(rendered.contains("Paste"));
        assert!(rendered.contains("Clear"));
    }

    #[test]
    fn request_list_shows_status_for_each_request_independently() {
        let config = load(Path::new(".postui/requests.yaml")).expect("示例请求配置应当可以加载");
        let first_id = config.requests[0].id.clone();
        let second_id = config.requests[1].id.clone();
        let mut app = App::new(
            config,
            PathBuf::from(".postui/requests.yaml"),
            crate::settings::GlobalConfig::default(),
        );
        app.request_states
            .get_mut(&first_id)
            .expect("第一个接口应当有运行状态")
            .status = RequestStatus::Success;
        app.request_states
            .get_mut(&second_id)
            .expect("第二个接口应当有运行状态")
            .status = RequestStatus::Failed;

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("应创建测试终端");
        terminal
            .draw(|frame| draw(frame, &app))
            .expect("状态列表应当可以渲染");
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("[OK]"));
        assert!(rendered.contains("[ERR]"));
        assert!(rendered.contains("[--]"));
    }

    #[test]
    fn large_response_stays_inside_a_scrollable_viewport() {
        let config = load(Path::new(".postui/requests.yaml")).expect("示例请求配置应当可以加载");
        let mut app = App::new(
            config,
            PathBuf::from(".postui/requests.yaml"),
            crate::settings::GlobalConfig::default(),
        );
        let body = (0..2_000)
            .map(|index| format!("  {{\"index\":{index}}}"))
            .collect::<Vec<_>>()
            .join(",\n");
        app.request_states.insert(
            app.current_request().id.clone(),
            RequestRuntimeState::from_response(ResponseData {
                status: 200,
                reason: "OK".to_string(),
                headers: Vec::new(),
                body: format!("[\n{body}\n]"),
                download_path: None,
                elapsed_ms: 1,
            }),
        );

        let area = Rect::new(0, 0, 80, 24);
        let layout = screen_layout(area, &app);
        assert!(u32::from(layout.response.x) + u32::from(layout.response.width) <= 80);
        assert!(u32::from(layout.response.y) + u32::from(layout.response.height) <= 24);

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("应创建测试终端");
        terminal
            .draw(|frame| draw(frame, &app))
            .expect("大响应应当可以渲染");
        app.scroll_response(1);
        assert!(app.response_scroll.offset() > 0);
    }

    #[test]
    fn variable_buttons_have_stable_hit_areas() {
        let area = Rect::new(4, 6, 40, VARIABLE_ROW_HEIGHT);
        let [extract, paste, clear] = variable_action_areas(area, 0, true);

        assert_eq!(extract.height, VARIABLE_ROW_HEIGHT);
        assert_eq!(paste.height, VARIABLE_ROW_HEIGHT);
        assert_eq!(clear.height, VARIABLE_ROW_HEIGHT);
        assert!(matches!(
            variable_action_at(area, 0, extract.x, extract.y, true),
            Some(VariableAction::Extract)
        ));
        assert!(matches!(
            variable_action_at(area, 0, paste.x, paste.y, true),
            Some(VariableAction::Paste)
        ));
        assert!(matches!(
            variable_action_at(area, 0, clear.x, clear.y, true),
            Some(VariableAction::Clear)
        ));

        let [no_extract, paste, clear] = variable_action_areas(area, 0, false);
        assert_eq!(no_extract, Rect::default());
        assert!(matches!(
            variable_action_at(area, 0, paste.x, paste.y, false),
            Some(VariableAction::Paste)
        ));
        assert!(matches!(
            variable_action_at(area, 0, clear.x, clear.y, false),
            Some(VariableAction::Clear)
        ));
    }

    #[test]
    fn truncates_using_terminal_cell_width() {
        assert_eq!(truncate("接口", 3), "接…");
        assert_eq!(truncate("接口", 2), "…");
    }
}
