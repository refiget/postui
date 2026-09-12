use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct DialogLayout {
    pub(super) area: Rect,
    pub(super) table_header: Rect,
    pub(super) rows: ScrollAreas,
    pub(super) add_button: Rect,
    pub(super) apply_button: Rect,
    pub(super) close_button: Rect,
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

pub(super) fn draw_configuration_dropdown(
    frame: &mut Frame<'_>,
    app: &App,
    dialog: &crate::app::ConfigurationsDialog,
    selector: Rect,
) {
    let area = configuration_menu_area(frame.area(), selector, dialog.rows.len());
    if area.is_empty() {
        return;
    }

    let theme = &app.global_config.theme;
    let text = app.text();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent))
            .style(Style::default().bg(theme.surface).fg(theme.text))
            .title(format!(" {} ", text.workspace())),
        area,
    );
    let content = area.inner(Margin::new(1, 1));
    let items = dialog
        .rows
        .iter()
        .map(|configuration| ListItem::new(Line::from(configuration.clone())))
        .collect::<Vec<_>>();
    let list = List::new(items)
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .highlight_style(
            Style::default()
                .bg(theme.selection)
                .fg(theme.text)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");
    let mut state = ListState::default();
    if !dialog.rows.is_empty() {
        state.select(Some(dialog.selected));
    }
    frame.render_stateful_widget(list, content, &mut state);
}

pub(super) fn configuration_menu_area(screen: Rect, selector: Rect, row_count: usize) -> Rect {
    if screen.is_empty() || selector.is_empty() {
        return Rect::default();
    }
    let width = selector.width.max(18).min(screen.width);
    let height = u16::try_from(row_count.saturating_add(2))
        .unwrap_or(u16::MAX)
        .min(screen.height);
    if width == 0 || height == 0 {
        return Rect::default();
    }
    let x = selector.x.min(screen.right().saturating_sub(width));
    let below = selector.y.saturating_add(selector.height);
    let y = if below.saturating_add(height) <= screen.bottom() {
        below
    } else {
        selector.y.saturating_sub(height)
    };
    Rect::new(x, y.max(screen.y), width, height)
}

pub(super) fn variables_page_layout(area: Rect) -> DialogLayout {
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
    DialogLayout {
        area,
        table_header: sections[0],
        rows: inner_scroll_areas(sections[1]),
        add_button: Rect::default(),
        apply_button,
        close_button,
    }
}

pub(super) fn dialog_buttons(area: Rect) -> (Rect, Rect) {
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

pub(super) fn draw_variables_table(
    frame: &mut Frame<'_>,
    app: &App,
    page: &crate::app::VariablesPage,
    layout: DialogLayout,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let visible = usize::from(layout.rows.content.height);
    let offset = request_list_offset(page.selected, page.rows.len(), visible);
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
                let value = page
                    .editor
                    .as_ref()
                    .filter(|_| editing)
                    .map(|editor| editor_view(editor, usize::from(constraint_length(widths[1]))))
                    .unwrap_or_else(|| row.value.clone());
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
        state.select(Some(page.selected.saturating_sub(offset)));
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

pub(super) fn draw_headers_dialog(
    frame: &mut Frame<'_>,
    app: &App,
    dialog: &crate::app::HeadersDialog,
    layout: DialogLayout,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let visible = usize::from(layout.rows.content.height);
    let offset = request_list_offset(dialog.selected, dialog.rows.len(), visible);
    let widths = inline_header_table_widths(layout.rows.content.width);
    let header = Row::new(vec![Cell::from(text.name()), Cell::from(text.value())])
        .style(section_style(theme));
    let name_width = constraint_length(widths[0]);
    let value_width = constraint_length(widths[1]);
    frame.render_widget(
        Table::new(Vec::<Row<'static>>::new(), widths)
            .header(header)
            .column_spacing(TABLE_COLUMN_SPACING)
            .highlight_symbol("▸ ")
            .highlight_spacing(HighlightSpacing::Always)
            .style(Style::default().bg(theme.surface).fg(theme.text)),
        layout.table_header,
    );

    if dialog.rows.is_empty() {
        frame.render_widget(
            Paragraph::new(text.no_headers()).style(label_style(theme)),
            layout.rows.content,
        );
    } else {
        let rows = dialog
            .rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible)
            .map(|(index, row)| {
                let editing = dialog.editor.is_some() && dialog.selected == index;
                let name = if editing && dialog.field == KeyValueField::Name {
                    editor_view(dialog.editor.as_ref().unwrap(), usize::from(name_width))
                } else {
                    row.name.clone()
                };
                let value = if editing && dialog.field == KeyValueField::Value {
                    editor_view(dialog.editor.as_ref().unwrap(), usize::from(value_width))
                } else {
                    row.value.clone()
                };
                let name = if editing && dialog.field == KeyValueField::Name {
                    name
                } else {
                    truncate(&name, usize::from(name_width))
                };
                let value = if editing && dialog.field == KeyValueField::Value {
                    value
                } else {
                    truncate(&value, usize::from(value_width))
                };
                let row_style = if row.source == HeaderSource::Collection || !row.enabled {
                    Style::default().fg(theme.muted)
                } else {
                    Style::default().fg(theme.text)
                };
                let value_style = if row.source == HeaderSource::Request && row.enabled {
                    row_style.fg(theme.accent)
                } else {
                    row_style
                };
                let mut name_cell = Cell::from(highlight::template_line(&name, row_style, theme));
                let mut value_cell =
                    Cell::from(highlight::template_line(&value, value_style, theme));
                if editing {
                    let editor = dialog.editor.as_ref().unwrap();
                    match dialog.field {
                        KeyValueField::Name => {
                            name_cell = name_cell.style(edit_input_style(
                                editor,
                                theme,
                                theme.accent,
                                theme.surface,
                            ))
                        }
                        KeyValueField::Value => {
                            value_cell = value_cell.style(edit_input_style(
                                editor,
                                theme,
                                theme.accent,
                                theme.surface,
                            ));
                        }
                    }
                }
                Row::new(vec![name_cell, value_cell]).style(row_style)
            })
            .collect::<Vec<_>>();
        let table = Table::new(rows, widths)
            .column_spacing(TABLE_COLUMN_SPACING)
            .row_highlight_style(dialog_row_highlight(theme, true))
            .highlight_symbol("▸ ")
            .highlight_spacing(HighlightSpacing::Always)
            .style(Style::default().bg(theme.surface).fg(theme.text));
        let mut state = TableState::default();
        state.select(Some(dialog.selected.saturating_sub(offset)));
        frame.render_stateful_widget(table, layout.rows.content, &mut state);
    }
    draw_scrollbar(
        frame,
        layout.rows.scrollbar,
        dialog.rows.len(),
        visible,
        offset,
        theme,
    );
}

pub(super) fn draw_params_dialog(
    frame: &mut Frame<'_>,
    app: &App,
    dialog: &crate::app::ParamsDialog,
    layout: DialogLayout,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let visible = usize::from(layout.rows.content.height);
    let offset = request_list_offset(dialog.selected, dialog.rows.len(), visible);
    let widths = inline_param_table_widths(layout.rows.content.width);
    let header = Row::new(vec![Cell::from(text.name()), Cell::from(text.value())])
        .style(section_style(theme));
    let key_width = constraint_length(widths[0]);
    let value_width = constraint_length(widths[1]);
    frame.render_widget(
        Table::new(Vec::<Row<'static>>::new(), widths)
            .header(header)
            .column_spacing(TABLE_COLUMN_SPACING)
            .highlight_symbol("▸ ")
            .highlight_spacing(HighlightSpacing::Always)
            .style(Style::default().bg(theme.surface).fg(theme.text)),
        layout.table_header,
    );

    if dialog.rows.is_empty() {
        frame.render_widget(
            Paragraph::new(text.no_params()).style(label_style(theme)),
            layout.rows.content,
        );
    } else {
        let rows = dialog
            .rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible)
            .map(|(index, row)| {
                let is_selected = dialog.selected == index;
                let mut key = row.key.clone();
                let mut value = row.value.clone();
                if is_selected {
                    if let Some(editor) = dialog.editor.as_ref() {
                        match dialog.field {
                            crate::app::KeyValueField::Name => {
                                key = editor_view(editor, usize::from(key_width));
                            }
                            crate::app::KeyValueField::Value => {
                                value = editor_view(editor, usize::from(value_width));
                            }
                        }
                    }
                }
                if dialog.editor.is_none() || !is_selected || dialog.field != KeyValueField::Name {
                    key = truncate(&key, usize::from(key_width));
                }
                if dialog.editor.is_none() || !is_selected || dialog.field != KeyValueField::Value {
                    value = truncate(&value, usize::from(value_width));
                }
                let key_style = Style::default().fg(theme.text);
                let value_style = Style::default().fg(theme.accent);
                let mut key_cell = Cell::from(highlight::template_line(&key, key_style, theme));
                let mut value_cell =
                    Cell::from(highlight::template_line(&value, value_style, theme));
                if let Some(editor) = dialog.editor.as_ref().filter(|_| is_selected) {
                    match dialog.field {
                        KeyValueField::Name => {
                            key_cell = key_cell.style(edit_input_style(
                                editor,
                                theme,
                                theme.accent,
                                theme.surface,
                            ))
                        }
                        KeyValueField::Value => {
                            value_cell = value_cell.style(edit_input_style(
                                editor,
                                theme,
                                theme.accent,
                                theme.surface,
                            ));
                        }
                    }
                }
                Row::new(vec![key_cell, value_cell]).style(Style::default())
            })
            .collect::<Vec<_>>();
        let table = Table::new(rows, widths)
            .column_spacing(TABLE_COLUMN_SPACING)
            .row_highlight_style(dialog_row_highlight(theme, true))
            .highlight_symbol("▸ ")
            .highlight_spacing(HighlightSpacing::Always)
            .style(Style::default().bg(theme.surface).fg(theme.text));
        let mut state = TableState::default();
        state.select(Some(dialog.selected.saturating_sub(offset)));
        frame.render_stateful_widget(table, layout.rows.content, &mut state);
    }
    draw_scrollbar(
        frame,
        layout.rows.scrollbar,
        dialog.rows.len(),
        visible,
        offset,
        theme,
    );
}

pub(super) fn variable_table_widths(width: u16) -> [Constraint; 3] {
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

pub(super) fn inline_header_table_widths(width: u16) -> [Constraint; 2] {
    let width = width.saturating_sub(TABLE_HIGHLIGHT_WIDTH + TABLE_COLUMN_SPACING);
    let name = (width * 2 / 5).max(u16::from(width > 1));
    let value = width.saturating_sub(name);
    [Constraint::Length(name), Constraint::Length(value)]
}

pub(super) fn inline_param_table_widths(width: u16) -> [Constraint; 2] {
    let width = width.saturating_sub(TABLE_HIGHLIGHT_WIDTH + TABLE_COLUMN_SPACING);
    let key = (width * 2 / 5).max(u16::from(width > 1));
    let value = width.saturating_sub(key);
    [Constraint::Length(key), Constraint::Length(value)]
}

pub(super) fn dialog_row_highlight(
    theme: &crate::settings::UiTheme,
    content_focused: bool,
) -> Style {
    super::focus::selection_style(theme, content_focused)
}

pub(super) fn draw_variables_footer(
    frame: &mut Frame<'_>,
    app: &App,
    focus: VariablePageFocus,
    layout: DialogLayout,
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
    let selected = page.selected;
    let layout = variables_page_layout(area);
    match event.kind {
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
            let offset = request_list_offset(selected, row_count, visible);
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
            app.move_variable_selection(direction);
        }
        _ => {}
    }
}
