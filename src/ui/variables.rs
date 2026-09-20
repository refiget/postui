use super::{
    TABLE_COLUMN_SPACING, TABLE_HIGHLIGHT_WIDTH, contains,
    layout::{ListPageLayout, list_page_layout},
    widgets::{
        constraint_length, draw_scrollbar, edit_input_text_style, editor_view,
        handle_list_scroll_mouse, label_style, panel_block, section_style, styled_list_table,
        truncate_line,
    },
};
use crate::{app::App, highlight};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    text::Line,
    widgets::{Cell, Paragraph, Row, Table, TableState},
};

pub(super) fn draw_variables_page(
    frame: &mut Frame<'_>,
    app: &App,
    page: &crate::app::VariablesPage,
    area: Rect,
) {
    let layout = list_page_layout(area);
    if layout.area.is_empty() {
        return;
    }

    let theme = &app.global_config.theme;
    let text = app.text();
    let title = Line::from(format!(" {} ", text.variables()));
    frame.render_widget(panel_block(title, layout.area, theme), layout.area);

    draw_variables_table(frame, app, page, layout);
}

fn draw_variables_table(
    frame: &mut Frame<'_>,
    app: &App,
    page: &crate::app::VariablesPage,
    layout: ListPageLayout,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let visible = usize::from(layout.rows.content.height);
    let offset = page.scroll.offset(page.rows.len(), visible);
    let widths = variable_table_widths(layout.rows.content.width);
    let header = Row::new(vec![
        Cell::from(text.variables()).style(highlight::variable_style(Style::default(), theme)),
        Cell::from(text.current_value()).style(Style::default().fg(theme.accent)),
        Cell::from(text.default_value()).style(Style::default().fg(theme.secondary)),
    ])
    .style(section_style(theme));
    frame.render_widget(
        styled_list_table(
            Table::new(Vec::<Row<'static>>::new(), widths.as_slice()).header(header),
            theme,
        ),
        layout.table_header,
    );

    if page.rows.is_empty() {
        frame.render_widget(
            Paragraph::new(text.no_variables()).style(label_style(theme)),
            layout.rows.content,
        );
    } else {
        let rows = page
            .rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible)
            .map(|(index, row)| {
                let editing = page.editor.is_some() && page.selected == index;
                let raw_value = page
                    .editor
                    .as_ref()
                    .filter(|_| editing)
                    .map(|editor| editor_view(editor, usize::from(constraint_length(widths[1]))))
                    .unwrap_or_else(|| row.value.clone());
                let value = if row.secret && !editing && !raw_value.is_empty() {
                    "••••••".to_string()
                } else {
                    raw_value
                };
                let value_style = if value.is_empty() {
                    Style::default().fg(theme.muted)
                } else {
                    Style::default().fg(theme.accent)
                };
                let name_style = highlight::variable_style(Style::default(), theme);
                let default_style = Style::default().fg(theme.secondary);
                let value_width = usize::from(constraint_length(widths[1]));
                let default_width = usize::from(constraint_length(widths[2]));
                let mut value_cell = Cell::from(truncate_line(
                    highlight::template_line(&value, value_style, theme),
                    value_width,
                ));
                if editing {
                    value_cell = value_cell.style(edit_input_text_style(theme.accent));
                }
                let default = app.variable_default_value(&row.name);
                Row::new(vec![
                    Cell::from(truncate_line(
                        Line::styled(row.name.clone(), name_style),
                        usize::from(constraint_length(widths[0])),
                    )),
                    value_cell,
                    Cell::from(truncate_line(
                        Line::styled(default, default_style),
                        default_width,
                    )),
                ])
                .style(Style::default().fg(theme.text))
            })
            .collect::<Vec<_>>();
        let table = styled_list_table(
            Table::new(rows, widths.as_slice())
                .cell_highlight_style(super::focus::selection_style(theme, true)),
            theme,
        );
        let mut state = TableState::default();
        state.select(
            (offset..offset.saturating_add(visible))
                .contains(&page.selected)
                .then(|| page.selected - offset),
        );
        if page
            .editor
            .as_ref()
            .is_some_and(|editor| editor.mode() == crate::editor::EditMode::Replace)
        {
            state.select_column(Some(1));
        }
        frame.render_stateful_widget(table, layout.rows.content, &mut state);
    }
    draw_scrollbar(
        frame,
        layout.rows.scrollbar,
        page.rows.len(),
        visible,
        offset,
        theme,
    );
}

fn variable_table_widths(width: u16) -> [Constraint; 3] {
    let width =
        width.saturating_sub(TABLE_HIGHLIGHT_WIDTH + TABLE_COLUMN_SPACING.saturating_mul(2));
    let name = width.min(20);
    let default = width.saturating_sub(name).min(22);
    let value = width.saturating_sub(name).saturating_sub(default);
    [
        Constraint::Length(name),
        Constraint::Length(value),
        Constraint::Length(default),
    ]
}

pub(super) fn handle_variables_mouse(
    app: &mut App,
    event: MouseEvent,
    area: Rect,
    is_double: bool,
) {
    let Some((row_count, editing)) = app
        .view
        .variables
        .as_ref()
        .map(|page| (page.rows.len(), page.editor.is_some()))
    else {
        return;
    };
    let layout = list_page_layout(area);
    if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) && editing {
        app.confirm_active_input();
    }
    if app
        .view
        .variables
        .as_mut()
        .is_some_and(|page| handle_list_scroll_mouse(&mut page.scroll, event, layout, row_count))
    {
        return;
    }
    if !matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
        return;
    }
    if !contains(layout.rows.content, event.column, event.row) {
        app.cancel_variable_edit();
        return;
    }

    let visible = usize::from(layout.rows.content.height);
    let offset = app
        .view
        .variables
        .as_ref()
        .map_or(0, |page| page.scroll.offset(row_count, visible));
    let index = offset.saturating_add(usize::from(event.row - layout.rows.content.y));
    if index >= row_count {
        app.cancel_variable_edit();
        return;
    }

    let widths = variable_table_widths(layout.rows.content.width);
    let name_width = constraint_length(widths[0]);
    let value_start = layout
        .rows
        .content
        .x
        .saturating_add(TABLE_HIGHLIGHT_WIDTH)
        .saturating_add(name_width.saturating_add(TABLE_COLUMN_SPACING));
    let value_end = layout
        .rows
        .content
        .right()
        .saturating_sub(constraint_length(widths[2]).saturating_add(TABLE_COLUMN_SPACING));
    let edit = event.column >= value_start && event.column < value_end;
    let cursor = (edit && is_double).then(|| usize::from(event.column.saturating_sub(value_start)));
    app.click_variable_row(index, edit, cursor);
}
