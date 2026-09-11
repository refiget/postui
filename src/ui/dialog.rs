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

pub(super) fn draw_dialog(frame: &mut Frame<'_>, app: &App, dialog: &Dialog) {
    let layout = dialog_layout(frame.area(), dialog);
    if layout.area.is_empty() {
        return;
    }

    let theme = &app.global_config.theme;
    let text = app.text();
    frame.render_widget(Clear, layout.area);
    let title = match dialog {
        Dialog::Variables(_) => Line::from(format!(" {} ", text.variables())),
        Dialog::Headers(_) => Line::from(format!(
            " {} · {} ",
            text.headers(),
            app.current_request().name
        )),
        Dialog::Params(_) => Line::from(format!(
            " {} · {} ",
            text.params(),
            app.current_request().name
        )),
    };
    frame.render_widget(panel_block(title, layout.area, theme), layout.area);

    match dialog {
        Dialog::Variables(dialog) => draw_variables_dialog(frame, app, dialog, layout),
        Dialog::Headers(dialog) => draw_headers_dialog(frame, app, dialog, layout),
        Dialog::Params(dialog) => draw_params_dialog(frame, app, dialog, layout),
    }
}

pub(super) fn dialog_layout(area: Rect, dialog: &Dialog) -> DialogLayout {
    let row_count = match dialog {
        Dialog::Variables(dialog) => dialog.rows.len(),
        Dialog::Headers(dialog) => dialog.rows.len(),
        Dialog::Params(dialog) => dialog.rows.len(),
    };
    let max_width = match dialog {
        Dialog::Variables(_) => 76,
        Dialog::Headers(_) => 92,
        Dialog::Params(_) => 96,
    };
    let desired_height = 8_u16.saturating_add(u16::try_from(row_count.min(12)).unwrap_or(12));
    let dialog_area = centered_rect(
        area,
        area.width.saturating_sub(4).min(max_width),
        area.height.saturating_sub(2).min(desired_height),
    );
    let inner = dialog_area.inner(Margin::new(1, 1));
    let footer_height = inner.height.min(3);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(footer_height),
        ])
        .split(inner);
    let has_add = !matches!(dialog, Dialog::Variables(_));
    let (add_button, apply_button, close_button) = dialog_buttons(sections[2], has_add);
    DialogLayout {
        area: dialog_area,
        table_header: sections[0],
        rows: inner_scroll_areas(sections[1]),
        add_button,
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

pub(super) fn dialog_buttons(area: Rect, has_add: bool) -> (Rect, Rect, Rect) {
    if area.is_empty() {
        return (Rect::default(), Rect::default(), Rect::default());
    }

    if has_add {
        if area.height >= 3 && area.width >= 36 {
            let buttons = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(16),
                    Constraint::Length(10),
                    Constraint::Length(10),
                ])
                .split(area);
            return (buttons[1], buttons[2], buttons[3]);
        }
        let buttons = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(34),
                Constraint::Percentage(33),
                Constraint::Percentage(33),
            ])
            .split(area);
        return (buttons[0], buttons[1], buttons[2]);
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
        return (Rect::default(), buttons[1], buttons[2]);
    }
    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    (Rect::default(), buttons[0], buttons[1])
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
        Table::new(Vec::<Row<'static>>::new(), widths)
            .header(header)
            .column_spacing(1)
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
                    value_cell =
                        value_cell.style(Style::default().add_modifier(Modifier::UNDERLINED));
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
        let table = Table::new(rows, widths)
            .column_spacing(1)
            .row_highlight_style(dialog_row_highlight(
                theme,
                dialog.focus == DialogFocus::Content,
            ))
            .highlight_symbol("▸ ")
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
    draw_dialog_footer(frame, app, dialog.focus, false, layout);
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
    let widths = header_table_widths(layout.rows.content.width);
    let header = Row::new(vec![
        Cell::from(" "),
        Cell::from(text.headers()),
        Cell::from(text.value()),
        Cell::from(text.source()),
    ])
    .style(section_style(theme));
    frame.render_widget(
        Table::new(Vec::<Row<'static>>::new(), widths)
            .header(header)
            .column_spacing(1)
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
                let edited_value = dialog
                    .editor
                    .as_ref()
                    .filter(|_| editing)
                    .map(|editor| editor.value.clone());
                let name = if editing && dialog.field == HeaderField::Name {
                    edited_value.clone().unwrap_or_else(|| row.name.clone())
                } else {
                    row.name.clone()
                };
                let value = if editing && dialog.field == HeaderField::Value {
                    edited_value.unwrap_or_else(|| row.value.clone())
                } else {
                    row.value.clone()
                };
                let row_style = if row.source == HeaderSource::Collection || !row.enabled {
                    Style::default().fg(theme.muted)
                } else {
                    Style::default().fg(theme.text)
                };
                let value_style = if row.source == HeaderSource::Request && row.enabled {
                    row_style.add_modifier(Modifier::UNDERLINED)
                } else {
                    row_style
                };
                let value_cell = Cell::from(highlight::template_line(&value, value_style, theme));
                let source = match row.source {
                    HeaderSource::Collection => text.inherited(),
                    HeaderSource::Request => text.request_scope(),
                };
                Row::new(vec![
                    Cell::from(if row.enabled { "✓" } else { "·" }),
                    Cell::from(highlight::template_line(&name, row_style, theme)),
                    value_cell,
                    Cell::from(source),
                ])
                .style(row_style)
            })
            .collect::<Vec<_>>();
        let table = Table::new(rows, widths)
            .column_spacing(1)
            .row_highlight_style(dialog_row_highlight(
                theme,
                dialog.focus == DialogFocus::Content,
            ))
            .highlight_symbol("▸ ")
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
    draw_dialog_footer(frame, app, dialog.focus, true, layout);
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
    let widths = param_table_widths(layout.rows.content.width);
    let header = Row::new(vec![
        Cell::from(text.source()),
        Cell::from(text.value_type()),
        Cell::from(text.query()),
        Cell::from(text.value()),
    ])
    .style(section_style(theme));
    frame.render_widget(
        Table::new(Vec::<Row<'static>>::new(), widths)
            .header(header)
            .column_spacing(1)
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
                let source = row.source.label(text);
                let is_selected = dialog.selected == index;
                let mut key = row.key.clone();
                let mut value = row.value.clone();
                if is_selected {
                    if let Some(editor) = dialog.editor.as_ref() {
                        match dialog.field {
                            crate::app::HeaderField::Name => key = editor.value.clone(),
                            crate::app::HeaderField::Value => value = editor.value.clone(),
                        }
                    }
                }
                let row_type = row
                    .part_type
                    .map(|part_type| part_type.as_row_type(text))
                    .unwrap_or("");
                let key_style = Style::default();
                let value_style = Style::default().add_modifier(Modifier::UNDERLINED);
                Row::new(vec![
                    Cell::from(highlight::template_line(
                        source,
                        Style::default().fg(theme.text),
                        theme,
                    )),
                    Cell::from(highlight::template_line(
                        row_type,
                        Style::default().fg(theme.text),
                        theme,
                    )),
                    Cell::from(highlight::template_line(&key, key_style, theme)),
                    Cell::from(highlight::template_line(&value, value_style, theme)),
                ])
                .style(Style::default())
            })
            .collect::<Vec<_>>();
        let table = Table::new(rows, widths)
            .column_spacing(1)
            .row_highlight_style(dialog_row_highlight(
                theme,
                dialog.focus == crate::app::DialogFocus::Content,
            ))
            .highlight_symbol("▸ ")
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
    draw_dialog_footer(frame, app, dialog.focus, true, layout);
}

pub(super) fn variable_table_widths(width: u16) -> [Constraint; 3] {
    let name = width.min(20);
    let default = width.saturating_sub(name).min(22);
    [
        Constraint::Length(name),
        Constraint::Min(0),
        Constraint::Length(default),
    ]
}

pub(super) fn header_table_widths(width: u16) -> [Constraint; 4] {
    let available = width.saturating_sub(5);
    let enabled = available.min(2);
    let source = available.saturating_sub(enabled).min(8);
    let fields = available.saturating_sub(enabled.saturating_add(source));
    let name = fields.saturating_mul(2).saturating_div(5).min(24);
    let value = fields.saturating_sub(name);
    [
        Constraint::Length(enabled),
        Constraint::Length(name),
        Constraint::Length(value),
        Constraint::Length(source),
    ]
}

pub(super) fn param_table_widths(width: u16) -> [Constraint; 4] {
    let available = width.saturating_sub(5);
    let source = available.min(6);
    let row_type = available.saturating_sub(source).min(7);
    let fields = available.saturating_sub(source.saturating_add(row_type));
    let key = fields.saturating_div(3).min(24);
    let value = fields.saturating_sub(key);
    [
        Constraint::Length(source),
        Constraint::Length(row_type),
        Constraint::Length(key),
        Constraint::Length(value),
    ]
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

pub(super) fn draw_dialog_footer(
    frame: &mut Frame<'_>,
    app: &App,
    focus: DialogFocus,
    has_add: bool,
    layout: DialogLayout,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    if has_add && !layout.add_button.is_empty() {
        let state = dialog_button_state(focus, DialogFocus::Add);
        frame.render_widget(
            dialog_button_widget(text.add_row(), &state, theme, layout.add_button),
            layout.add_button,
        );
    }
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
    let variant = if area.height >= 3 && area.width >= label.chars().count() as u16 + 4 {
        ButtonVariant::Block
    } else {
        ButtonVariant::SingleLine
    };
    button_widget(label, state, secondary_button_style(theme), variant)
}

pub(super) fn compact_button_widget<'a>(
    label: &'a str,
    state: &'a ButtonState,
    theme: &crate::settings::UiTheme,
) -> Button<'a> {
    button_widget(
        label,
        state,
        secondary_button_style(theme),
        ButtonVariant::Block,
    )
}

pub(super) fn handle_dialog_mouse(app: &mut App, event: MouseEvent, area: Rect) {
    let Some(dialog) = app.dialog.as_ref() else {
        return;
    };
    let layout = dialog_layout(area, dialog);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if contains(layout.add_button, event.column, event.row) {
                app.click_dialog_button(DialogFocus::Add);
                return;
            }
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

            let row_count = match app.dialog.as_ref() {
                Some(Dialog::Variables(dialog)) => dialog.rows.len(),
                Some(Dialog::Headers(dialog)) => dialog.rows.len(),
                Some(Dialog::Params(dialog)) => dialog.rows.len(),
                None => return,
            };
            let visible = usize::from(layout.rows.content.height);
            let selected = match app.dialog.as_ref() {
                Some(Dialog::Variables(dialog)) => dialog.selected,
                Some(Dialog::Headers(dialog)) => dialog.selected,
                Some(Dialog::Params(dialog)) => dialog.selected,
                None => return,
            };
            let offset = request_list_offset(selected, row_count, visible);
            let index = offset.saturating_add(usize::from(event.row - layout.rows.content.y));
            if index >= row_count {
                return;
            }

            match app.dialog.as_ref() {
                Some(Dialog::Variables(_)) => {
                    let widths = variable_table_widths(layout.rows.content.width);
                    let name_width = constraint_length(widths[0]);
                    let edit = event.column
                        >= layout
                            .rows
                            .content
                            .x
                            .saturating_add(name_width.saturating_add(1));
                    app.click_variable_row(index, edit);
                }
                Some(Dialog::Headers(dialog)) => {
                    let widths = header_table_widths(layout.rows.content.width);
                    let enabled_width = constraint_length(widths[0]);
                    let name_width = constraint_length(widths[1]);
                    let name_start = layout
                        .rows
                        .content
                        .x
                        .saturating_add(enabled_width)
                        .saturating_add(1);
                    let value_start = name_start.saturating_add(name_width).saturating_add(1);
                    if event.column < name_start {
                        app.toggle_header_row(index);
                    } else if event.column < value_start {
                        let editable = dialog
                            .rows
                            .get(index)
                            .is_some_and(|row| row.source == HeaderSource::Request);
                        app.click_header_row(index, HeaderField::Name, editable);
                    } else {
                        let value_end = value_start.saturating_add(constraint_length(widths[2]));
                        let in_value = event.column < value_end;
                        let editable = in_value
                            && dialog
                                .rows
                                .get(index)
                                .is_some_and(|row| row.source == HeaderSource::Request);
                        app.click_header_row(index, HeaderField::Value, editable);
                    }
                }
                Some(Dialog::Params(dialog)) => {
                    let widths = param_table_widths(layout.rows.content.width);
                    let source_width = constraint_length(widths[0]);
                    let type_width = constraint_length(widths[1]);
                    let key_width = constraint_length(widths[2]);
                    let key_start = layout
                        .rows
                        .content
                        .x
                        .saturating_add(source_width)
                        .saturating_add(type_width)
                        .saturating_add(2);
                    let value_start = key_start.saturating_add(key_width).saturating_add(1);
                    let field = if event.column < value_start {
                        HeaderField::Name
                    } else {
                        HeaderField::Value
                    };
                    let editable = dialog.rows.get(index).is_some();
                    app.click_param_row(index, field, editable);
                }
                None => {}
            }
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
