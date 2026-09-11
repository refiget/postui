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

pub(super) fn draw_dialog(frame: &mut Frame<'_>, app: &App, dialog: &crate::app::VariablesDialog) {
    let layout = dialog_layout(frame.area(), dialog.rows.len());
    if layout.area.is_empty() {
        return;
    }

    let theme = &app.global_config.theme;
    let text = app.text();
    frame.render_widget(Clear, layout.area);
    let title = Line::from(format!(" {} ", text.variables()));
    frame.render_widget(dialog_block(title, layout.area, theme), layout.area);

    draw_variables_dialog(frame, app, dialog, layout);
}

pub(super) fn dialog_layout(area: Rect, row_count: usize) -> DialogLayout {
    let desired_height = 10_u16.saturating_add(u16::try_from(row_count.min(16)).unwrap_or(16));
    let dialog_area = centered_rect(
        area,
        area.width.saturating_sub(2).min(96),
        area.height.saturating_sub(2).min(desired_height),
    );
    let inner = dialog_area.inner(Margin::new(u16::from(dialog_area.width >= 48) + 1, 1));
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
        area: dialog_area,
        table_header: sections[0],
        rows: inner_scroll_areas(sections[1]),
        add_button: Rect::default(),
        apply_button,
        close_button,
    }
}

pub(super) fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x.saturating_add(area.width.saturating_sub(width) / 2),
        area.y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    )
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

pub(super) fn draw_variables_dialog(
    frame: &mut Frame<'_>,
    app: &App,
    dialog: &crate::app::VariablesDialog,
    layout: DialogLayout,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let visible = usize::from(layout.rows.content.height);
    let offset = request_list_offset(dialog.selected, dialog.rows.len(), visible);
    let widths = variable_table_widths(layout.rows.content.width);
    let header = Row::new(vec![
        Cell::from(text.variables()),
        Cell::from(text.current_value()),
        Cell::from(text.default_value()),
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

    if dialog.rows.is_empty() {
        frame.render_widget(
            Paragraph::new(text.no_variables()).style(label_style(theme)),
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
                let value = dialog
                    .editor
                    .as_ref()
                    .filter(|_| editing)
                    .map(|editor| editor.value.clone())
                    .unwrap_or_else(|| row.value.clone());
                let mut value_cell = Cell::from(highlight::template_line(
                    &value,
                    highlight::plain_style(theme),
                    theme,
                ));
                if editing {
                    value_cell = value_cell.style(active_editor_style(theme));
                }
                let default = app
                    .config
                    .variables
                    .get(&row.name)
                    .and_then(|definition| definition.default.as_ref())
                    .map(crate::config::value_to_string)
                    .unwrap_or_else(|| "—".to_string());
                Row::new(vec![
                    Cell::from(row.name.clone()),
                    value_cell,
                    Cell::from(default),
                ])
                .style(Style::default().fg(theme.text))
            })
            .collect::<Vec<_>>();
        let table = Table::new(rows, widths.as_slice())
            .column_spacing(TABLE_COLUMN_SPACING)
            .row_highlight_style(dialog_row_highlight(
                theme,
                dialog.focus == DialogFocus::Content,
            ))
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
    draw_dialog_footer(frame, app, dialog.focus, layout);
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
                let name = if editing && dialog.field == HeaderField::Name {
                    editor_view(dialog.editor.as_ref().unwrap(), usize::from(name_width))
                } else {
                    crate::template::resolve_text(&row.name, &app.collection_state.variables)
                };
                let value = if editing && dialog.field == HeaderField::Value {
                    editor_view(dialog.editor.as_ref().unwrap(), usize::from(value_width))
                } else {
                    crate::template::resolve_text(&row.value, &app.collection_state.variables)
                };
                let name = if editing && dialog.field == HeaderField::Name {
                    name
                } else {
                    truncate(&name, usize::from(name_width))
                };
                let value = if editing && dialog.field == HeaderField::Value {
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
                    match dialog.field {
                        HeaderField::Name => {
                            name_cell = name_cell.style(active_editor_style(theme))
                        }
                        HeaderField::Value => {
                            value_cell = value_cell.style(active_editor_style(theme));
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
                let mut key =
                    crate::template::resolve_text(&row.key, &app.collection_state.variables);
                let mut value =
                    crate::template::resolve_text(&row.value, &app.collection_state.variables);
                if is_selected {
                    if let Some(editor) = dialog.editor.as_ref() {
                        match dialog.field {
                            crate::app::HeaderField::Name => {
                                key = editor_view(editor, usize::from(key_width));
                            }
                            crate::app::HeaderField::Value => {
                                value = editor_view(editor, usize::from(value_width));
                            }
                        }
                    }
                }
                if dialog.editor.is_none() || !is_selected || dialog.field != HeaderField::Name {
                    key = truncate(&key, usize::from(key_width));
                }
                if dialog.editor.is_none() || !is_selected || dialog.field != HeaderField::Value {
                    value = truncate(&value, usize::from(value_width));
                }
                let key_style = Style::default().fg(theme.text);
                let value_style = Style::default().fg(theme.accent);
                let mut key_cell = Cell::from(highlight::template_line(&key, key_style, theme));
                let mut value_cell =
                    Cell::from(highlight::template_line(&value, value_style, theme));
                if dialog.editor.is_some() && is_selected {
                    match dialog.field {
                        HeaderField::Name => key_cell = key_cell.style(active_editor_style(theme)),
                        HeaderField::Value => {
                            value_cell = value_cell.style(active_editor_style(theme));
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
    [
        Constraint::Length(name),
        Constraint::Min(0),
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
    Style::default()
        .bg(if content_focused {
            theme.selection
        } else {
            theme.surface
        })
        .fg(theme.text)
}

fn active_editor_style(theme: &crate::settings::UiTheme) -> Style {
    Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

pub(super) fn draw_dialog_footer(
    frame: &mut Frame<'_>,
    app: &App,
    focus: DialogFocus,
    layout: DialogLayout,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    if !layout.apply_button.is_empty() {
        let state = dialog_button_state(focus, DialogFocus::Apply);
        frame.render_widget(
            dialog_button_widget(text.apply(), &state, theme, layout.apply_button),
            layout.apply_button,
        );
    }
    if !layout.close_button.is_empty() {
        let state = dialog_button_state(focus, DialogFocus::Close);
        frame.render_widget(
            dialog_button_widget(text.close(), &state, theme, layout.close_button),
            layout.close_button,
        );
    }
}

pub(super) fn dialog_button_state(current: DialogFocus, focus: DialogFocus) -> ButtonState {
    let focused = current == focus;
    let mut state = ButtonState::enabled();
    state.set_focused(focused);
    state
}

pub(super) fn dialog_button_widget<'a>(
    label: &'a str,
    state: &'a ButtonState,
    theme: &crate::settings::UiTheme,
    area: Rect,
) -> Button<'a> {
    let label_width = u16::try_from(crate::editor::terminal_width(label)).unwrap_or(u16::MAX);
    let variant = if area.height >= 3 && area.width >= label_width.saturating_add(4) {
        ButtonVariant::Block
    } else {
        ButtonVariant::SingleLine
    };
    button_widget(label, state, secondary_button_style(theme), variant)
}

pub(super) fn handle_dialog_mouse(app: &mut App, event: MouseEvent, area: Rect) {
    let Some(Dialog::Variables(dialog)) = app.dialog.as_ref() else {
        return;
    };
    let row_count = dialog.rows.len();
    let selected = dialog.selected;
    let layout = dialog_layout(area, row_count);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if contains(layout.apply_button, event.column, event.row) {
                app.click_dialog_button(DialogFocus::Apply);
                return;
            }
            if contains(layout.close_button, event.column, event.row) {
                app.click_dialog_button(DialogFocus::Close);
                return;
            }
            if !contains(layout.rows.content, event.column, event.row) {
                return;
            }

            let visible = usize::from(layout.rows.content.height);
            let offset = request_list_offset(selected, row_count, visible);
            let index = offset.saturating_add(usize::from(event.row - layout.rows.content.y));
            if index >= row_count {
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
            app.click_variable_row(index, edit);
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if contains(layout.rows.content, event.column, event.row) =>
        {
            let direction = if matches!(event.kind, MouseEventKind::ScrollUp) {
                -1
            } else {
                1
            };
            app.move_dialog_selection(direction);
        }
        _ => {}
    }
}
