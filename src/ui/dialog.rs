use super::{
    DELETE_ICON, INLINE_DELETE_WIDTH, TABLE_COLUMN_SPACING, TABLE_HIGHLIGHT_WIDTH,
    layout::ScrollAreas,
    widgets::{
        constraint_length, draw_scrollbar, edit_input_style, editor_view, label_style,
        section_style, styled_list_table, truncate,
    },
};
use crate::{
    app::{App, HeaderRow, HeaderSource, InlineRow, InlineTable, KeyValueField, ParamsDialogRow},
    highlight,
    settings::UiTheme,
};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Cell, Paragraph, Row, Table, TableState},
};
use tui_assets_rust::{
    Dropdown as AssetDropdown, DropdownItem as AssetDropdownItem, Theme as AssetTheme,
};

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

/// 表格行的名称列、值列和整行样式；参数和请求头表格共用。
pub(super) trait InlineRowView: InlineRow {
    fn name_style(&self, theme: &UiTheme) -> Style;
    fn value_style(&self, theme: &UiTheme) -> Style;
    fn row_style(&self, theme: &UiTheme) -> Style;
}

impl InlineRowView for HeaderRow {
    fn name_style(&self, theme: &UiTheme) -> Style {
        if self.source == HeaderSource::Collection || !self.enabled {
            Style::default().fg(theme.muted)
        } else {
            Style::default().fg(theme.text)
        }
    }

    fn value_style(&self, theme: &UiTheme) -> Style {
        if self.source == HeaderSource::Request && self.enabled {
            self.name_style(theme).fg(theme.accent)
        } else {
            self.name_style(theme)
        }
    }

    fn row_style(&self, theme: &UiTheme) -> Style {
        self.name_style(theme)
    }
}

impl InlineRowView for ParamsDialogRow {
    fn name_style(&self, theme: &UiTheme) -> Style {
        Style::default().fg(theme.text)
    }

    fn value_style(&self, theme: &UiTheme) -> Style {
        Style::default().fg(theme.accent)
    }

    fn row_style(&self, _theme: &UiTheme) -> Style {
        Style::default()
    }
}

pub(super) fn draw_inline_table<R: InlineRowView>(
    frame: &mut Frame<'_>,
    app: &App,
    rows: &[R],
    table_state: &InlineTable,
    empty_label: &str,
    layout: InlineEditorLayout,
) {
    let theme = &app.global_config.theme;
    let visible = usize::from(layout.rows.content.height);
    let offset = table_state.scroll.offset(rows.len(), visible);
    let widths = inline_table_widths(layout.rows.content.width);
    let name_width = usize::from(constraint_length(widths[0]));
    let value_width = usize::from(constraint_length(widths[1]));
    let header = Row::new(vec![
        Cell::from(app.text().name()),
        Cell::from(app.text().value()),
        Cell::from(""),
    ])
    .style(section_style(theme));
    frame.render_widget(
        styled_list_table(
            Table::new(Vec::<Row<'static>>::new(), widths).header(header),
            theme,
        ),
        layout.table_header,
    );

    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(empty_label).style(label_style(theme)),
            layout.rows.content,
        );
    } else {
        let items = rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible)
            .map(|(index, row)| {
                let selected = table_state.selected == index;
                let editor = table_state.editor.as_ref().filter(|_| selected);
                let editor_column = |field: KeyValueField, width: usize| match editor {
                    Some(editor) if table_state.field == field => editor_view(editor, width),
                    _ => truncate(row.column(field), width),
                };
                let mut name_cell = Cell::from(highlight::template_line(
                    &editor_column(KeyValueField::Name, name_width),
                    row.name_style(theme),
                    theme,
                ));
                let mut value_cell = Cell::from(highlight::template_line(
                    &editor_column(KeyValueField::Value, value_width),
                    row.value_style(theme),
                    theme,
                ));
                if let Some(editor) = editor {
                    let style = edit_input_style(editor, theme, theme.accent, theme.surface);
                    match table_state.field {
                        KeyValueField::Name => name_cell = name_cell.style(style),
                        KeyValueField::Value => value_cell = value_cell.style(style),
                    }
                }
                let delete_cell = inline_delete_cell(true, selected, theme);
                Row::new(vec![name_cell, value_cell, delete_cell]).style(row.row_style(theme))
            })
            .collect::<Vec<_>>();
        let table = styled_list_table(
            Table::new(items, widths)
                .row_highlight_style(super::focus::selection_style(theme, true)),
            theme,
        );
        let mut state = TableState::default();
        state.select(
            (offset..offset.saturating_add(visible))
                .contains(&table_state.selected)
                .then(|| table_state.selected - offset),
        );
        frame.render_stateful_widget(table, layout.rows.content, &mut state);
    }
    draw_scrollbar(
        frame,
        layout.rows.scrollbar,
        rows.len(),
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
    let name = u16::try_from(u32::from(width) * 2 / 5)
        .unwrap_or(u16::MAX)
        .max(u16::from(width > 1));
    let value = width.saturating_sub(name);
    [
        Constraint::Length(name),
        Constraint::Length(value),
        Constraint::Length(INLINE_DELETE_WIDTH),
    ]
}
