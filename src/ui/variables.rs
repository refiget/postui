use super::*;

#[derive(Debug, Clone, Copy)]
struct VariablesLayout {
    area: Rect,
    table_header: Rect,
    rows: ScrollAreas,
    apply_button: Rect,
    close_button: Rect,
}

pub(super) fn draw_variables_page(
    frame: &mut Frame<'_>,
    app: &App,
    page: &crate::app::VariablesPage,
    area: Rect,
) {
    let layout = variables_page_layout(area);
    if layout.area.is_empty() {
        return;
    }

    let theme = &app.global_config.theme;
    let text = app.text();
    let title = Line::from(format!(" {} ", text.variables()));
    frame.render_widget(panel_block(title, layout.area, theme), layout.area);

    draw_variables_table(frame, app, page, layout);
}

fn variables_page_layout(area: Rect) -> VariablesLayout {
    let inner = area.inner(Margin::new(u16::from(area.width >= 48) + 1, 1));
    let footer_height = inner.height.min(3);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(footer_height),
        ])
        .split(inner);
    let (apply_button, close_button) = dialog_buttons(sections[2]);
    VariablesLayout {
        area,
        table_header: sections[0],
        rows: inner_scroll_areas(sections[1]),
        apply_button,
        close_button,
    }
}

fn dialog_buttons(area: Rect) -> (Rect, Rect) {
    if area.is_empty() {
        return (Rect::default(), Rect::default());
    }

    if area.height >= 3 && area.width >= 22 {
        let buttons = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(10),
                Constraint::Length(10),
            ])
            .split(area);
        return (buttons[1], buttons[2]);
    }
    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    (buttons[0], buttons[1])
}

fn draw_variables_table(
    frame: &mut Frame<'_>,
    app: &App,
    page: &crate::app::VariablesPage,
    layout: VariablesLayout,
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
        Table::new(Vec::<Row<'static>>::new(), widths.as_slice())
            .header(header)
            .column_spacing(TABLE_COLUMN_SPACING)
            .highlight_symbol("▸ ")
            .highlight_spacing(HighlightSpacing::Always)
            .style(Style::default().bg(theme.surface).fg(theme.text)),
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
                let mut value_cell =
                    Cell::from(highlight::template_line(&value, value_style, theme));
                if editing {
                    value_cell = value_cell.style(edit_input_text_style(theme.accent));
                }
                let default = app.variable_default_value(&row.name);
                Row::new(vec![
                    Cell::from(row.name.clone()).style(name_style),
                    value_cell,
                    Cell::from(default).style(default_style),
                ])
                .style(Style::default().fg(theme.text))
            })
            .collect::<Vec<_>>();
        let table = Table::new(rows, widths.as_slice())
            .column_spacing(TABLE_COLUMN_SPACING)
            .cell_highlight_style(super::focus::selection_style(
                theme,
                page.focus == VariablePageFocus::Content,
            ))
            .highlight_symbol("▸ ")
            .highlight_spacing(HighlightSpacing::Always)
            .style(Style::default().bg(theme.surface).fg(theme.text));
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
    draw_variables_footer(frame, app, page.focus, layout);
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

fn draw_variables_footer(
    frame: &mut Frame<'_>,
    app: &App,
    focus: VariablePageFocus,
    layout: VariablesLayout,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    if !layout.apply_button.is_empty() {
        draw_send_button(
            frame,
            layout.apply_button,
            text.apply(),
            true,
            focus == VariablePageFocus::Apply,
            theme,
        );
    }
    if !layout.close_button.is_empty() {
        draw_send_button(
            frame,
            layout.close_button,
            text.close(),
            true,
            focus == VariablePageFocus::Close,
            theme,
        );
    }
}

pub(super) fn handle_variables_mouse(
    app: &mut App,
    event: MouseEvent,
    area: Rect,
    is_double: bool,
) {
    let Some(page) = app.view.variables.as_ref() else {
        return;
    };
    let row_count = page.rows.len();
    let layout = variables_page_layout(area);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left)
            if contains(layout.rows.scrollbar, event.column, event.row) =>
        {
            click_variables_scrollbar(app, event.row, layout);
        }
        MouseEventKind::Drag(MouseButton::Left)
            if contains(layout.rows.scrollbar, event.column, event.row) =>
        {
            drag_variables_scrollbar(app, event.row, layout);
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if contains(layout.apply_button, event.column, event.row) {
                app.click_variables_page_button(VariablePageFocus::Apply);
                return;
            }
            if contains(layout.close_button, event.column, event.row) {
                app.click_variables_page_button(VariablePageFocus::Close);
                return;
            }
            if !contains(layout.rows.content, event.column, event.row) {
                app.cancel_variable_edit();
                return;
            }

            let visible = usize::from(layout.rows.content.height);
            let offset = page.scroll.offset(row_count, visible);
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
            let value_end =
                layout.rows.content.right().saturating_sub(
                    constraint_length(widths[2]).saturating_add(TABLE_COLUMN_SPACING),
                );
            let edit = event.column >= value_start && event.column < value_end;
            let cursor =
                (edit && is_double).then(|| usize::from(event.column.saturating_sub(value_start)));
            app.click_variable_row(index, edit, cursor);
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if contains(layout.rows.content, event.column, event.row) =>
        {
            let direction = if matches!(event.kind, MouseEventKind::ScrollUp) {
                -1
            } else {
                1
            };
            if let Some(page) = app.view.variables.as_mut() {
                page.scroll.move_by(
                    direction,
                    row_count,
                    usize::from(layout.rows.content.height),
                );
            }
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if contains(layout.rows.scrollbar, event.column, event.row) =>
        {
            click_variables_scrollbar(app, event.row, layout);
        }
        _ => {}
    }
}

fn click_variables_scrollbar(app: &mut App, row: u16, layout: VariablesLayout) {
    let visible = usize::from(layout.rows.content.height);
    let Some(page) = app.view.variables.as_ref() else {
        return;
    };
    let count = page.rows.len();
    if count == 0 || visible == 0 {
        return;
    }
    let offset = page.scroll.offset(count, visible);
    let Some(bar) = scrollbar_track_state(layout.rows.scrollbar, count, visible, offset) else {
        return;
    };
    let target = scrollbar_offset_from_track(&bar, row);
    if let Some(page) = app.view.variables.as_mut() {
        page.scroll.set_offset(target, count, visible);
        page.scroll.drag_anchor = Some((row, target));
    }
}

fn drag_variables_scrollbar(app: &mut App, row: u16, layout: VariablesLayout) {
    let visible = usize::from(layout.rows.content.height);
    let Some(page) = app.view.variables.as_mut() else {
        return;
    };
    let count = page.rows.len();
    let offset = page.scroll.offset(count, visible);
    let Some((anchor_row, anchor_offset)) = page.scroll.drag_anchor else {
        return;
    };
    let Some(bar) = scrollbar_track_state(layout.rows.scrollbar, count, visible, offset) else {
        return;
    };
    let target = scrollbar_offset_from_drag(&bar, anchor_row, anchor_offset, row);
    page.scroll.set_offset(target, count, visible);
}
