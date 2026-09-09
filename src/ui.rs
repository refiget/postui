use std::cmp::{max, min};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table,
        TableState, Wrap,
    },
};

use crate::{
    app::{App, Focus},
    highlight, template,
};

const REQUEST_STACK_WIDTH: u16 = 56;
const REQUEST_STACK_MIN_HEIGHT: u16 = 14;
const PREVIEW_STACK_WIDTH: u16 = 72;
const PREVIEW_STACK_MIN_HEIGHT: u16 = 11;
const INFO_BUTTON_STACK_WIDTH: u16 = 44;
const SEND_COLUMN_WIDTH: u16 = 11;
const SEND_BUTTON_HEIGHT: u16 = 3;

#[derive(Debug, Clone, Copy)]
struct UiLayout {
    header: Rect,
    requests: Rect,
    info: Rect,
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
    draw_request_info(frame, areas.info, areas.send_button, app);
    draw_variables(frame, areas.variables, areas.variable_list, app);
    draw_preview(frame, areas.preview, app);
    draw_response(frame, areas.response, app);
    draw_footer(frame, areas.footer, theme);

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
        MouseEventKind::ScrollUp if !app.editing && !app.dropdown_open => {
            tracing::debug!(column = event.column, row = event.row, "处理鼠标向上滚动");
            handle_scroll(app, event.column, event.row, areas, -1);
        }
        MouseEventKind::ScrollDown if !app.editing && !app.dropdown_open => {
            tracing::debug!(column = event.column, row = event.row, "处理鼠标向下滚动");
            handle_scroll(app, event.column, event.row, areas, 1);
        }
        _ => {}
    }
}

fn handle_click(app: &mut App, column: u16, row: u16, areas: UiLayout) {
    tracing::debug!(column, row, "处理鼠标左键点击");
    if app.dropdown_open {
        click_dropdown(app, column, row, areas.dropdown);
        return;
    }

    if contains(areas.requests, column, row) {
        click_request_list(app, row, areas.requests);
    } else if contains(areas.variable_list, column, row) {
        click_variable_list(app, column, row, areas.variable_list);
    } else if contains(areas.response, column, row) {
        click_response_extract(app, column, row, areas.response);
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

    let first_row = area.y.saturating_add(1);
    if row < first_row {
        return;
    }
    let index = usize::from(row - first_row);
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
    let visible_rows = usize::from(area.height).max(1);
    let selected_index = app.variable_index.min(names.len().saturating_sub(1));
    let offset = selected_index.min(names.len().saturating_sub(visible_rows));
    let row_index = usize::from(row - area.y);
    let index = offset + row_index;
    if index >= names.len() {
        return;
    }
    app.variable_index = index;
    match variable_action_at(area, row_index, column, row) {
        Some(VariableAction::Paste) => {
            tracing::debug!(index, "点击变量填入操作");
            app.paste_variable(index)
        }
        Some(VariableAction::Clear) => {
            tracing::debug!(index, "点击变量清空操作");
            app.clear_variable(index)
        }
        None => {
            tracing::debug!(index, "点击变量编辑区域");
            app.edit_current_variable()
        }
    }
}

fn click_response_extract(app: &mut App, column: u16, row: u16, area: Rect) {
    let inner = area.inner(Margin::new(1, 1));
    let first_row = inner.y.saturating_add(2);
    if row < first_row {
        return;
    }
    let row_index = usize::from(row - first_row);
    if !contains(response_extract_action_area(area, row_index), column, row) {
        return;
    }
    if row_index < app.current_request().extracts.len() {
        tracing::debug!(index = row_index, "点击响应字段提取操作");
        app.extract_response(row_index);
    }
}

fn click_dropdown(app: &mut App, column: u16, row: u16, area: Rect) {
    if !contains(area, column, row) {
        app.dropdown_open = false;
        app.focus = Focus::Requests;
        app.status = "已关闭接口列表".to_string();
        tracing::debug!("点击下拉列表外部，关闭接口列表");
        return;
    }

    let first_row = area.y.saturating_add(1);
    if row < first_row {
        return;
    }
    let index = usize::from(row - first_row);
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
    app.status = format!("已选择 {}", app.current_request().name);
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
    }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn screen_layout(area: Rect, app: &App) -> UiLayout {
    let header_height = if area.height >= 18 {
        4
    } else if area.height >= 4 {
        3
    } else {
        0
    };
    let footer_height = if area.height >= 8 { 1 } else { 0 };
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(0),
            Constraint::Length(footer_height),
        ])
        .split(area);

    let (requests, detail_area) = if sections[1].width < REQUEST_STACK_WIDTH
        && sections[1].height >= REQUEST_STACK_MIN_HEIGHT
    {
        let request_height = if sections[1].height >= 20 { 6 } else { 5 };
        let stacked = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(request_height.min(sections[1].height)),
                Constraint::Min(0),
            ])
            .split(sections[1]);
        (stacked[0], stacked[1])
    } else {
        let request_width = if area.width >= 100 {
            32
        } else if area.width >= 80 {
            28
        } else if area.width >= 64 {
            24
        } else {
            20
        }
        .min(sections[1].width);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(request_width), Constraint::Min(0)])
            .split(sections[1]);
        (columns[0], columns[1])
    };

    let variable_count = app.current_variable_names().len();
    let extract_count = u16::try_from(app.current_request().extracts.len()).unwrap_or(u16::MAX);
    let info_height = if detail_area.height >= 16 {
        7
    } else if detail_area.height >= 12 {
        6
    } else {
        detail_area.height.min(5)
    };
    let response_min_height = if extract_count == 0 {
        5
    } else {
        max(5, extract_count.saturating_add(4))
    };
    let desired_variable_height = u16::try_from(variable_count)
        .unwrap_or(u16::MAX)
        .saturating_add(3)
        .clamp(5, 10);
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
    let previews =
        if detail[2].width < PREVIEW_STACK_WIDTH && detail[2].height >= PREVIEW_STACK_MIN_HEIGHT {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(detail[2])
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(detail[2])
        };
    let (_, send_button) = request_info_parts(detail[0]);
    let variable_list = detail[1].inner(Margin::new(1, 1));

    UiLayout {
        header: sections[0],
        requests,
        info: detail[0],
        send_button,
        variables: detail[1],
        variable_list,
        preview: previews[0],
        response: previews[1],
        footer: sections[2],
        dropdown: centered_rect(70, 70, area),
    }
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

fn draw_footer(frame: &mut Frame<'_>, area: Rect, theme: &crate::settings::UiTheme) {
    let line = if area.width >= 88 {
        Line::from(vec![
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(" 区域  "),
            Span::styled("↑↓/jk", Style::default().fg(theme.accent)),
            Span::raw(" 移动  "),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" 选择/编辑  "),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(" 发送  "),
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(" 清空  "),
            Span::styled("响应区", Style::default().fg(theme.accent)),
            Span::raw(" 提取  "),
            Span::styled("q", Style::default().fg(theme.accent)),
            Span::raw(" 退出  "),
            Span::styled("鼠标", Style::default().fg(theme.accent)),
            Span::raw(" 点击"),
        ])
    } else if area.width >= 54 {
        Line::from(vec![
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(" 区域  "),
            Span::styled("↑↓", Style::default().fg(theme.accent)),
            Span::raw(" 移动  "),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" 编辑  "),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(" 发送  "),
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(" 清空  "),
            Span::styled("q", Style::default().fg(theme.accent)),
            Span::raw(" 退出"),
        ])
    } else {
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" 操作  "),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(" 发送  "),
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(" 清空  "),
            Span::styled("q", Style::default().fg(theme.accent)),
            Span::raw(" 退出"),
        ])
    };
    let footer = Paragraph::new(line).style(Style::default().fg(theme.muted));
    frame.render_widget(footer, area);
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
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
            format!("  请求配置: {}", app.config_path.display()),
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
    let items = app
        .config
        .requests
        .iter()
        .map(|request| {
            let mut line = vec![
                Span::styled(
                    request.method.as_str(),
                    method_style(&request.method, theme),
                ),
                Span::raw("  "),
            ];
            line.extend(highlight::template_spans(
                &request.name,
                Style::default().fg(theme.text),
                theme,
            ));
            ListItem::new(Line::from(line))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(panel_block("接口", area, theme))
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

fn draw_request_info(frame: &mut Frame<'_>, area: Rect, send_button: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let request = app.current_request();
    let url = template::display_url(request);
    let loading = app.is_request_loading(&request.id);
    let status = if loading { "发送中" } else { "待发送" };
    let mut address_line = vec![Span::styled("地址  ", label_style(theme))];
    address_line.extend(highlight::template_spans(
        &url,
        highlight::plain_style(theme),
        theme,
    ));
    let description = if request.description.is_empty() {
        "（未填写）"
    } else {
        request.description.as_str()
    };
    let mut description_line = vec![Span::styled("说明  ", label_style(theme))];
    description_line.extend(highlight::template_spans(
        description,
        highlight::plain_style(theme),
        theme,
    ));
    let lines = vec![
        Line::from(vec![
            Span::styled("方法  ", label_style(theme)),
            Span::styled(
                request.method.as_str(),
                method_style(&request.method, theme),
            ),
            Span::styled("  状态  ", label_style(theme)),
            Span::raw(status),
        ]),
        Line::from(address_line),
        Line::from(vec![
            Span::styled("标识  ", label_style(theme)),
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
    let (details, _) = request_info_parts(area);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), details);

    let button_color = if loading {
        theme.muted
    } else {
        theme.secondary
    };
    let button_style = if loading {
        Style::default().fg(theme.muted)
    } else if app.focus == Focus::Actions {
        Style::default()
            .fg(theme.background)
            .bg(theme.secondary)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.secondary)
            .add_modifier(Modifier::BOLD)
    };
    let button = Paragraph::new(if loading { "发送中…" } else { "发送" })
        .alignment(Alignment::Center)
        .style(button_style)
        .block(action_block(send_button, button_color, theme));
    frame.render_widget(button, send_button);
}

fn draw_variables(frame: &mut Frame<'_>, area: Rect, list_area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let variables = app.current_variables();
    frame.render_widget(panel_block("变量", area, theme), area);
    if variables.is_empty() {
        let inner = area.inner(Margin::new(1, 1));
        frame.render_widget(
            Paragraph::new(Span::styled("这个接口没有可替换的变量", label_style(theme))),
            inner,
        );
        return;
    }

    let columns = variable_columns(list_area);
    let selected_index = app.variable_index.min(variables.len().saturating_sub(1));
    let rows = variables
        .iter()
        .enumerate()
        .map(|(index, (name, value))| {
            let editing = app.editing && index == selected_index;
            let display_value = if editing {
                format!("{}▌", app.edit_buffer)
            } else if value.is_empty() {
                "（未设置）".to_string()
            } else {
                value.clone()
            };
            let value_style = if editing {
                Style::default().fg(theme.secondary)
            } else if value.is_empty() {
                Style::default().fg(theme.muted)
            } else {
                Style::default().fg(theme.text)
            };
            Row::new(vec![
                Cell::from(highlight::template_line(
                    &variable_label(name, columns.name),
                    highlight::variable_style(theme),
                    theme,
                )),
                Cell::from(highlight::template_line(&display_value, value_style, theme)),
                Cell::from(variable_action_label(columns.actions))
                    .style(Style::default().fg(theme.muted)),
            ])
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
    .row_highlight_style(Style::default().bg(theme.selection).fg(theme.text))
    .highlight_symbol("▸ ");
    let mut state = TableState::default();
    state.select(Some(selected_index));
    frame.render_stateful_widget(table, list_area, &mut state);
}

enum VariableAction {
    Paste,
    Clear,
}

#[derive(Debug, Clone, Copy)]
struct VariableColumns {
    name: u16,
    value: u16,
    actions: u16,
}

fn variable_columns(area: Rect) -> VariableColumns {
    let (name, actions) = match area.width {
        0..=15 => (area.width, 0),
        16..=21 => (8, 8),
        22..=27 => (10, 8),
        28..=41 => (12, 10),
        _ => (16, 12),
    };
    let actions = actions.min(area.width);
    let name = name.min(area.width.saturating_sub(actions));
    let value = area.width.saturating_sub(name.saturating_add(actions));
    VariableColumns {
        name,
        value,
        actions,
    }
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

fn variable_action_label(width: u16) -> &'static str {
    match width {
        10.. => "填入  清空",
        8..=9 => "填入 清",
        4..=7 => "填 清",
        _ => "",
    }
}

fn variable_action_at(
    area: Rect,
    row: usize,
    column: u16,
    screen_row: u16,
) -> Option<VariableAction> {
    let row_area = Rect::new(area.x, area.y.saturating_add(row as u16), area.width, 1);
    let columns = variable_columns(area);
    if columns.actions == 0 {
        return None;
    }
    let table_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(columns.name),
            Constraint::Length(columns.value),
            Constraint::Length(columns.actions),
        ])
        .split(row_area);
    let paste_width = columns.actions / 2;
    let actions = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(paste_width),
            Constraint::Length(columns.actions.saturating_sub(paste_width)),
        ])
        .split(table_columns[2]);
    if contains(actions[0], column, screen_row) {
        Some(VariableAction::Paste)
    } else if contains(actions[1], column, screen_row) {
        Some(VariableAction::Clear)
    } else {
        None
    }
}

fn response_extract_action_area(area: Rect, row: usize) -> Rect {
    let inner = area.inner(Margin::new(1, 1));
    let row_area = Rect::new(
        inner.x,
        inner.y.saturating_add(2 + row as u16),
        inner.width,
        1,
    );
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(response_extract_action_width(inner)),
        ])
        .split(row_area);
    columns[1]
}

fn response_extract_action_width(area: Rect) -> u16 {
    match area.width {
        0..=7 => 0,
        _ => 8,
    }
}

fn draw_preview(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
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
        lines.push(Line::from(Span::styled("请求头", section_style(theme))));
        for (key, value) in &request.headers {
            lines.push(highlight::template_line(
                &format!("{key}: {value}"),
                highlight::plain_style(theme),
                theme,
            ));
        }
    }
    if let Some(body) = &request.raw_body {
        lines.push(Line::from(Span::styled("请求体", section_style(theme))));
        if let Some(body_lines) = highlight::json_text_lines_if_valid(body, theme) {
            lines.extend(body_lines);
        } else {
            lines.extend(highlight::plain_lines(body, theme));
        }
    }
    if !request.form.is_empty() {
        lines.push(Line::from(Span::styled("表单", section_style(theme))));
        for (key, value) in &request.form {
            lines.push(highlight::template_line(
                &format!("{key}: {value}"),
                highlight::plain_style(theme),
                theme,
            ));
        }
    }
    if !request.files.is_empty() {
        lines.push(Line::from(Span::styled("文件", section_style(theme))));
        lines.push(highlight::template_line(
            &format!("目录: {}", app.config.file_directory.display()),
            highlight::plain_style(theme),
            theme,
        ));
        for file in &request.files {
            let filename = file.filename.as_deref().unwrap_or("自动取文件名");
            lines.push(highlight::template_line(
                &format!("{}: {} ({filename})", file.field, file.path),
                highlight::plain_style(theme),
                theme,
            ));
        }
    }
    let unresolved = template::unresolved_request_names(&request);
    if !unresolved.is_empty() {
        lines.insert(
            0,
            Line::from(Span::styled(
                format!("未替换: {}", unresolved.join(", ")),
                Style::default().fg(theme.secondary),
            )),
        );
    }
    let block = panel_block("预览", area, theme);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_response(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    frame.render_widget(panel_block("响应", area, theme), area);
    let inner = area.inner(Margin::new(1, 1));
    let request = app.current_request();
    let loading = app.is_request_loading(&request.id);
    let response = app.current_response();
    let can_extract = response.is_some() && !loading;
    let extract_count = request.extracts.len();
    let extract_height = if extract_count == 0 {
        0
    } else {
        u16::try_from(extract_count.saturating_add(1))
            .unwrap_or(u16::MAX)
            .min(inner.height)
    };
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(extract_height),
            Constraint::Min(0),
        ])
        .split(inner);

    let status = if loading {
        Line::from(Span::styled(
            "正在等待响应…",
            Style::default().fg(theme.secondary),
        ))
    } else if let Some(response) = response {
        let status_style = if response.status < 400 {
            Style::default().fg(theme.success)
        } else {
            Style::default().fg(theme.error)
        };
        let status = if response.reason.is_empty() {
            format!("HTTP {}", response.status)
        } else {
            format!("HTTP {} {}", response.status, response.reason)
        };
        Line::from(vec![
            Span::styled(status, status_style.add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("  {} ms", response.elapsed_ms),
                Style::default().fg(theme.muted),
            ),
        ])
    } else {
        Line::from(Span::styled("还没有发送这个请求", label_style(theme)))
    };
    frame.render_widget(Paragraph::new(status), sections[0]);

    if extract_count > 0 && extract_height > 0 {
        let extract_parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(sections[1]);
        frame.render_widget(
            Paragraph::new(Span::styled("提取", section_style(theme))),
            extract_parts[0],
        );
        let action_width = response_extract_action_width(extract_parts[1]);
        let extract_rows = request
            .extracts
            .iter()
            .map(|extract| {
                let target = format!("{{{{{}}}}}", extract.variable);
                let label = if extract.name.trim().is_empty() {
                    target
                } else {
                    format!("{}  {target}", extract.name)
                };
                let action = if can_extract {
                    if action_width >= 8 { "提取" } else { "提" }
                } else {
                    if action_width >= 8 {
                        "待响应"
                    } else {
                        "待"
                    }
                };
                Row::new(vec![
                    Cell::from(highlight::template_line(
                        &format!("{}  {}", truncate(&label, 20), extract.path),
                        label_style(theme),
                        theme,
                    )),
                    Cell::from(action).style(if can_extract {
                        Style::default().fg(theme.secondary)
                    } else {
                        label_style(theme)
                    }),
                ])
            })
            .collect::<Vec<_>>();
        let table = Table::new(
            extract_rows,
            [Constraint::Min(0), Constraint::Length(action_width)],
        );
        frame.render_widget(table, extract_parts[1]);
    }

    let mut body_lines = Vec::new();
    if let Some(response) = response {
        if !response.headers.is_empty() {
            body_lines.push(Line::from(Span::styled("响应头", section_style(theme))));
            for (key, value) in &response.headers {
                body_lines.push(highlight::template_line(
                    &format!("{key}: {value}"),
                    highlight::plain_style(theme),
                    theme,
                ));
            }
        }
        body_lines.push(Line::from(Span::styled("响应体", section_style(theme))));
        if response.body.is_empty() {
            body_lines.push(Line::from(Span::styled("（空）", label_style(theme))));
        } else if let Some(lines) = highlight::json_text_lines_if_valid(&response.body, theme) {
            body_lines.extend(lines);
        } else {
            body_lines.extend(highlight::plain_lines(&response.body, theme));
        }
    } else {
        body_lines.push(Line::from("按 r 发送请求"));
    }
    frame.render_widget(
        Paragraph::new(body_lines).wrap(Wrap { trim: false }),
        sections[2],
    );
}

fn draw_dropdown(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = &app.global_config.theme;
    frame.render_widget(Clear, area);
    let items = app
        .config
        .requests
        .iter()
        .map(|request| {
            let url = template::display_url(request);
            let mut line = vec![
                Span::styled(
                    request.method.as_str(),
                    method_style(&request.method, theme),
                ),
                Span::raw("  "),
            ];
            line.extend(highlight::template_spans(
                &request.name,
                Style::default().fg(theme.text),
                theme,
            ));
            line.push(Span::styled("  ", label_style(theme)));
            line.extend(highlight::template_spans(&url, label_style(theme), theme));
            ListItem::new(Line::from(line))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(panel_block("选择接口", area, theme).border_style(Style::default().fg(theme.accent)))
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
    use crate::{app::App, config::load};

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
    fn truncates_using_terminal_cell_width() {
        assert_eq!(truncate("接口", 3), "接…");
        assert_eq!(truncate("接口", 2), "…");
    }
}
