use super::*;

pub(super) fn draw_temporary_variables(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let rows = app.temporary_variable_rows();
    let table_height = u16::try_from(rows.len())
        .unwrap_or(u16::MAX)
        .saturating_add(1)
        .min(area.height.saturating_sub(3));
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(table_height),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);
    let table_area = sections[1];
    let body_area = sections[3];
    let name_width = table_area.width.saturating_sub(8).min(24);
    let value_width = usize::from(
        table_area
            .width
            .saturating_sub(name_width)
            .saturating_sub(TABLE_COLUMN_SPACING)
            .saturating_sub(2)
            .max(1),
    );
    let widths = [Constraint::Length(name_width), Constraint::Min(1)];
    let header = Row::new(vec![
        Cell::from(text.variables()).style(highlight::variable_style(Style::default(), theme)),
        Cell::from(text.current_value()).style(Style::default().fg(theme.accent)),
    ])
    .style(section_style(theme));
    let editor = app.temporary_variable_editor();
    let table_rows = rows.iter().map(|row| {
        let editing = editor.is_some_and(|editor| editor.name == row.name);
        let raw_value = editor.filter(|editor| editor.name == row.name).map_or_else(
            || row.value.clone(),
            |editor| editor_view(&editor.input, value_width),
        );
        let value = if row.secret && !editing && !raw_value.is_empty() {
            "••••••".to_string()
        } else {
            raw_value
        };
        let display_value = if value.is_empty() && !editing {
            " "
        } else {
            &value
        };
        let color = if value.is_empty() {
            theme.muted
        } else {
            theme.accent
        };
        let value_style = if editing {
            edit_input_text_style(color)
        } else {
            Style::default()
                .fg(color)
                .add_modifier(Modifier::UNDERLINED)
        };
        Row::new(vec![
            Cell::from(truncate_line(
                Line::styled(
                    row.name.clone(),
                    highlight::variable_style(Style::default(), theme),
                ),
                usize::from(name_width),
            )),
            Cell::from(truncate_line(
                highlight::template_line(display_value, value_style, theme),
                value_width,
            )),
        ])
    });
    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(TABLE_COLUMN_SPACING)
        .cell_highlight_style(super::focus::selection_style(theme, true))
        .highlight_symbol("▸ ")
        .highlight_spacing(HighlightSpacing::Always)
        .style(Style::default().bg(theme.surface).fg(theme.text));
    let selected = app.selected_temporary_variable();
    let mut state = TableState::default().with_selected(selected);
    if editor.is_some_and(|editor| editor.input.mode() == crate::editor::EditMode::Replace) {
        state.select_column(Some(1));
    }
    frame.render_stateful_widget(table, table_area, &mut state);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(text.body(), section_style(theme))),
            Line::from(Span::styled(text.no_content(), label_style(theme))),
        ]),
        body_area,
    );

    let Some(editor) = editor else {
        return;
    };
    let input_area = Rect::new(
        table_area
            .x
            .saturating_add(2)
            .saturating_add(name_width)
            .saturating_add(TABLE_COLUMN_SPACING),
        table_area
            .y
            .saturating_add(1)
            .saturating_add(u16::try_from(selected.unwrap_or_default()).unwrap_or(u16::MAX)),
        table_area
            .width
            .saturating_sub(name_width)
            .saturating_sub(TABLE_COLUMN_SPACING)
            .saturating_sub(2)
            .max(1),
        1,
    )
    .intersection(table_area);
    if !input_area.is_empty() {
        let (value, cursor_width) =
            editor_view_with_cursor(&editor.input, usize::from(input_area.width));
        frame.render_widget(
            Paragraph::new(value).style(edit_input_style(
                &editor.input,
                theme,
                theme.accent,
                theme.background,
            )),
            input_area,
        );
        if let Some(cursor_width) = cursor_width {
            frame.set_cursor_position((
                input_area
                    .x
                    .saturating_add(u16::try_from(cursor_width).unwrap_or(u16::MAX))
                    .min(input_area.right().saturating_sub(1)),
                input_area.y,
            ));
        }
    }
}
