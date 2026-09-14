use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct InlineEditorLayout {
    pub(super) table_header: Rect,
    pub(super) rows: ScrollAreas,
    pub(super) add_button: Rect,
}

pub(super) fn draw_configuration_dropdown(
    frame: &mut Frame<'_>,
    dialog: &mut crate::app::ConfigurationsDialog,
    selector: Rect,
    title: &str,
    theme: AssetTheme,
) {
    let area = configuration_menu_area(frame.area(), selector, dialog.rows.len());
    if area.is_empty() {
        return;
    }
    let items = dialog
        .rows
        .iter()
        .map(|configuration| AssetDropdownItem::new(configuration))
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        AssetDropdown::new(title, &items, theme),
        area,
        &mut dialog.state,
    );
}

pub(super) fn configuration_menu_area(screen: Rect, selector: Rect, row_count: usize) -> Rect {
    tui_assets_rust::dropdown_menu_area(screen, selector, row_count, 18)
}

pub(super) fn draw_headers_dialog(
    frame: &mut Frame<'_>,
    app: &App,
    dialog: &crate::app::HeadersDialog,
    layout: InlineEditorLayout,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let visible = usize::from(layout.rows.content.height);
    let offset = dialog.scroll.offset(dialog.rows.len(), visible);
    let widths = inline_table_widths(layout.rows.content.width);
    let header = Row::new(vec![
        Cell::from(text.name()),
        Cell::from(text.value()),
        Cell::from(""),
    ])
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
                let delete_cell = inline_delete_cell(true, dialog.selected == index, theme);
                Row::new(vec![name_cell, value_cell, delete_cell]).style(row_style)
            })
            .collect::<Vec<_>>();
        let table = Table::new(rows, widths)
            .column_spacing(TABLE_COLUMN_SPACING)
            .row_highlight_style(super::focus::selection_style(theme, true))
            .highlight_symbol("▸ ")
            .highlight_spacing(HighlightSpacing::Always)
            .style(Style::default().bg(theme.surface).fg(theme.text));
        let mut state = TableState::default();
        state.select(
            (offset..offset.saturating_add(visible))
                .contains(&dialog.selected)
                .then(|| dialog.selected - offset),
        );
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
    layout: InlineEditorLayout,
) {
    let theme = &app.global_config.theme;
    let text = app.text();
    let visible = usize::from(layout.rows.content.height);
    let offset = dialog.scroll.offset(dialog.rows.len(), visible);
    let widths = inline_table_widths(layout.rows.content.width);
    let header = Row::new(vec![
        Cell::from(text.name()),
        Cell::from(text.value()),
        Cell::from(""),
    ])
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
                let delete_cell = inline_delete_cell(true, is_selected, theme);
                Row::new(vec![key_cell, value_cell, delete_cell]).style(Style::default())
            })
            .collect::<Vec<_>>();
        let table = Table::new(rows, widths)
            .column_spacing(TABLE_COLUMN_SPACING)
            .row_highlight_style(super::focus::selection_style(theme, true))
            .highlight_symbol("▸ ")
            .highlight_spacing(HighlightSpacing::Always)
            .style(Style::default().bg(theme.surface).fg(theme.text));
        let mut state = TableState::default();
        state.select(
            (offset..offset.saturating_add(visible))
                .contains(&dialog.selected)
                .then(|| dialog.selected - offset),
        );
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

fn inline_delete_cell(
    enabled: bool,
    selected: bool,
    theme: &crate::settings::UiTheme,
) -> Cell<'static> {
    Cell::from(Span::styled(
        format!(" {DELETE_ICON} "),
        Style::default()
            .fg(if enabled { theme.error } else { theme.muted })
            .bg(if selected {
                theme.selection
            } else {
                theme.surface
            })
            .add_modifier(Modifier::BOLD),
    ))
}

pub(super) fn inline_table_widths(width: u16) -> [Constraint; 3] {
    let width = width.saturating_sub(
        TABLE_HIGHLIGHT_WIDTH + TABLE_COLUMN_SPACING.saturating_mul(2) + INLINE_DELETE_WIDTH,
    );
    let name = (width * 2 / 5).max(u16::from(width > 1));
    let value = width.saturating_sub(name);
    [
        Constraint::Length(name),
        Constraint::Length(value),
        Constraint::Length(INLINE_DELETE_WIDTH),
    ]
}
