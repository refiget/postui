use super::{
    TABLE_COLUMN_SPACING, TABLE_HIGHLIGHT_WIDTH, contains,
    layout::{ListPageLayout, list_page_layout},
    widgets::{
        constraint_length, draw_scrollbar, handle_list_scroll_mouse, label_style, panel_block,
        section_style, styled_list_table, truncate_line,
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

pub(super) fn draw_extracts_page(
    frame: &mut Frame<'_>,
    app: &App,
    page: &crate::app::ExtractsPage,
    area: Rect,
) {
    let layout = list_page_layout(area);
    if layout.area.is_empty() {
        return;
    }

    let theme = &app.global_config.theme;
    let text = app.text();
    let title = Line::from(format!(" {} ", text.extracts()));
    frame.render_widget(panel_block(title, layout.area, theme), layout.area);

    draw_extracts_table(frame, app, page, layout);
}

fn draw_extracts_table(
    frame: &mut Frame<'_>,
    app: &App,
    page: &crate::app::ExtractsPage,
    layout: ListPageLayout,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let visible = usize::from(layout.rows.content.height);
    let offset = page.scroll.offset(page.rows.len(), visible);
    let widths = extract_table_widths(layout.rows.content.width);
    let header = Row::new(vec![
        Cell::from(text.extract_order()).style(section_style(theme)),
        Cell::from(text.extract_variable())
            .style(highlight::variable_style(Style::default(), theme)),
        Cell::from(text.extract_path()).style(Style::default().fg(theme.accent)),
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
            Paragraph::new(text.no_extracts()).style(label_style(theme)),
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
                let order_style = Style::default().fg(theme.secondary);
                Row::new(vec![
                    Cell::from(truncate_line(
                        Line::styled((index + 1).to_string(), order_style),
                        usize::from(constraint_length(widths[0])),
                    )),
                    Cell::from(truncate_line(
                        Line::styled(
                            row.variable.clone(),
                            highlight::variable_style(Style::default(), theme),
                        ),
                        usize::from(constraint_length(widths[1])),
                    )),
                    Cell::from(truncate_line(
                        Line::styled(row.path.clone(), Style::default().fg(theme.accent)),
                        usize::from(constraint_length(widths[2])),
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

fn extract_table_widths(width: u16) -> [Constraint; 3] {
    let width =
        width.saturating_sub(TABLE_HIGHLIGHT_WIDTH + TABLE_COLUMN_SPACING.saturating_mul(2));
    let order = width.min(5);
    let variable = width.saturating_sub(order).min(20);
    let path = width.saturating_sub(order).saturating_sub(variable);
    [
        Constraint::Length(order),
        Constraint::Length(variable),
        Constraint::Length(path),
    ]
}

pub(super) fn handle_extracts_mouse(app: &mut App, event: MouseEvent, area: Rect) {
    let Some(row_count) = app.view.extracts.as_ref().map(|page| page.rows.len()) else {
        return;
    };
    let layout = list_page_layout(area);
    if app
        .view
        .extracts
        .as_mut()
        .is_some_and(|page| handle_list_scroll_mouse(&mut page.scroll, event, layout, row_count))
    {
        return;
    }
    if !matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
        || !contains(layout.rows.content, event.column, event.row)
    {
        return;
    }
    let visible = usize::from(layout.rows.content.height);
    let offset = app
        .view
        .extracts
        .as_ref()
        .map_or(0, |page| page.scroll.offset(row_count, visible));
    let index = offset.saturating_add(usize::from(event.row - layout.rows.content.y));
    app.click_extract_row(index);
}
