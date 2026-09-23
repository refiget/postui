use super::{
    TABLE_COLUMN_SPACING, TABLE_HIGHLIGHT_WIDTH, contains,
    dialog::{InlineEditorLayout, draw_inline_table, inline_table_widths},
    layout::inner_scroll_areas,
    widgets::{
        constraint_length, scrollbar_offset_from_drag, scrollbar_offset_from_track,
        scrollbar_track_state,
    },
};
use crate::app::{
    App, Dialog, HEADER_PRESETS, HeaderSource, KeyValueField, PreviewTab, ScrollDragTarget,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

pub(super) fn inline_dialog_layout(area: Rect, row_count: usize) -> InlineEditorLayout {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(area.height.min(1)), Constraint::Min(0)])
        .split(area);
    let content = sections[1];
    let add_height = content.height.min(3);
    let add_y = content.y.saturating_add(
        u16::try_from(row_count)
            .unwrap_or(u16::MAX)
            .min(content.height.saturating_sub(add_height)),
    );
    let add_button = if content.is_empty() {
        Rect::default()
    } else {
        Rect::new(content.x, add_y, content.width, add_height)
    };
    let table_area = Rect::new(
        content.x,
        content.y,
        content.width,
        add_y.saturating_sub(content.y),
    );
    InlineEditorLayout {
        table_header: sections[0],
        rows: inner_scroll_areas(table_area),
        add_button,
    }
}

pub(super) fn draw_inline_editor(frame: &mut Frame<'_>, area: Rect, app: &App, dialog: &Dialog) {
    let row_count = dialog.table_row_count().unwrap_or_default();
    let layout = inline_dialog_layout(area, row_count);
    match dialog {
        Dialog::Configurations(_) => {}
        Dialog::Headers(dialog) => draw_inline_table(
            frame,
            app,
            &dialog.rows,
            &dialog.table,
            app.text().no_headers(),
            layout,
        ),
        Dialog::Params(dialog) => draw_inline_table(
            frame,
            app,
            &dialog.rows,
            &dialog.table,
            app.text().no_params(),
            layout,
        ),
    }
    if !layout.add_button.is_empty() && !matches!(dialog, Dialog::Configurations(_)) {
        let theme = &app.global_config.theme;
        let background = Style::default().bg(theme.surface);
        frame.render_widget(Block::default().style(background), layout.add_button);
        let label_area = Rect::new(
            layout.add_button.x,
            layout
                .add_button
                .y
                .saturating_add(layout.add_button.height.saturating_sub(1) / 2),
            layout.add_button.width,
            layout.add_button.height.min(1),
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "  +  ",
                    background.fg(theme.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    app.text().add_entry(),
                    background.fg(theme.text).add_modifier(Modifier::BOLD),
                ),
            ]))
            .style(background),
            label_area,
        );
    }
    if let Dialog::Headers(headers) = dialog
        && let Some(selected) = headers.preset_selection
    {
        draw_header_presets(frame, area, app, selected);
    }
}

fn draw_header_presets(frame: &mut Frame<'_>, area: Rect, app: &App, selected: usize) {
    let theme = &app.global_config.theme;
    let mut labels = HEADER_PRESETS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    labels.push(app.text().custom_header());
    let width = labels
        .iter()
        .map(|label| crate::editor::terminal_width(label))
        .max()
        .unwrap_or_default()
        .saturating_add(4);
    let width = u16::try_from(width).unwrap_or(u16::MAX).min(area.width);
    let height = u16::try_from(labels.len().saturating_add(2))
        .unwrap_or(u16::MAX)
        .min(area.height);
    let popup = Rect::new(area.x, area.bottom().saturating_sub(height), width, height);
    let items = labels.into_iter().enumerate().map(|(index, label)| {
        let style = if index == selected {
            Style::default()
                .fg(theme.text)
                .bg(theme.selection)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text).bg(theme.surface)
        };
        ListItem::new(format!(" {label}")).style(style)
    });
    frame.render_widget(Clear, popup);
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .title(app.text().header_preset_title())
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent))
                .style(Style::default().bg(theme.surface)),
        ),
        popup,
    );
}

pub(super) fn drag_inline_editor_scrollbar(app: &mut App, row: u16, area: Rect) -> bool {
    let Some(row_count) = app.view.dialog.as_ref().and_then(Dialog::table_row_count) else {
        return false;
    };
    let layout = inline_dialog_layout(area, row_count);
    let visible = usize::from(layout.rows.content.height);
    let offset = inline_scroll(app, row_count, visible);
    let anchor = app
        .view
        .dialog
        .as_ref()
        .and_then(Dialog::table)
        .and_then(|table| table.scroll.drag_anchor);
    let Some((anchor_row, anchor_offset)) = anchor else {
        return false;
    };
    let Some(bar) = scrollbar_track_state(layout.rows.scrollbar, row_count, visible, offset) else {
        return false;
    };
    let target = scrollbar_offset_from_drag(&bar, anchor_row, anchor_offset, row);
    set_inline_scroll(app, target, row_count, visible, None);
    true
}

pub(super) fn scroll_inline_editor(app: &mut App, direction: isize, area: Rect) -> bool {
    let Some(row_count) = app.view.dialog.as_ref().and_then(Dialog::table_row_count) else {
        return false;
    };
    let Some(table) = app.view.dialog.as_mut().and_then(Dialog::table_mut) else {
        return false;
    };
    let visible = usize::from(inline_dialog_layout(area, row_count).rows.content.height);
    table.scroll.move_by(direction, row_count, visible);
    true
}

fn inline_scroll(app: &App, row_count: usize, visible: usize) -> usize {
    app.view
        .dialog
        .as_ref()
        .and_then(Dialog::table)
        .map_or(0, |table| table.scroll.offset(row_count, visible))
}

fn set_inline_scroll(
    app: &mut App,
    offset: usize,
    row_count: usize,
    visible: usize,
    drag_anchor: Option<(u16, usize)>,
) {
    let Some(table) = app.view.dialog.as_mut().and_then(Dialog::table_mut) else {
        return;
    };
    table.scroll.set_offset(offset, row_count, visible);
    if drag_anchor.is_some() {
        table.scroll.drag_anchor = drag_anchor;
    }
}

pub(super) fn place_inline_editor_cursor(app: &mut App, column: u16, row: u16, area: Rect) -> bool {
    let Some(row_count) = app.view.dialog.as_ref().and_then(Dialog::table_row_count) else {
        return false;
    };
    let Some(table) = app.view.dialog.as_ref().and_then(Dialog::table) else {
        return false;
    };
    let selected = table.selected;
    let field = table.field;
    let Some(table) = app.view.dialog.as_mut().and_then(Dialog::table_mut) else {
        return false;
    };
    let Some(editor) = table.editor.as_mut() else {
        return false;
    };
    let layout = inline_dialog_layout(area, row_count);
    if !contains(layout.rows.content, column, row) {
        return false;
    }
    let offset = table
        .scroll
        .offset(row_count, usize::from(layout.rows.content.height));
    if offset.saturating_add(usize::from(row - layout.rows.content.y)) != selected {
        return false;
    }
    let (start, width) = inline_columns(layout.rows.content).field(field);
    if !(start..start.saturating_add(width)).contains(&column) {
        return false;
    }
    let column = editor.visible_column(usize::from(width), usize::from(column - start));
    editor.place_cursor(column);
    true
}

pub(super) fn handle_inline_editor_click(
    app: &mut App,
    column: u16,
    row: u16,
    area: Rect,
    is_double: bool,
) {
    let Some(row_count) = app.view.dialog.as_ref().and_then(Dialog::table_row_count) else {
        return;
    };
    let layout = inline_dialog_layout(area, row_count);
    if contains(layout.rows.scrollbar, column, row) {
        let visible = usize::from(layout.rows.content.height);
        let offset = inline_scroll(app, row_count, visible);
        let Some(bar) = scrollbar_track_state(layout.rows.scrollbar, row_count, visible, offset)
        else {
            return;
        };
        let target = scrollbar_offset_from_track(&bar, row);
        set_inline_scroll(app, target, row_count, visible, Some((row, target)));
        app.view.scroll_drag_target = Some(ScrollDragTarget::Preview);
        return;
    }
    if contains(layout.add_button, column, row) {
        app.add_preview_row(app.view.preview.active_tab);
        return;
    }
    if !contains(layout.rows.content, column, row) {
        return;
    }
    let visible = usize::from(layout.rows.content.height);
    let offset = inline_scroll(app, row_count, visible);
    let index = offset.saturating_add(usize::from(row - layout.rows.content.y));
    if index >= row_count {
        return;
    }
    let columns = inline_columns(layout.rows.content);
    match app.view.dialog.as_ref() {
        Some(Dialog::Headers(_)) => {
            let request_row = app
                .view
                .dialog
                .as_ref()
                .and_then(|dialog| match dialog {
                    Dialog::Headers(dialog) => dialog.rows.get(index),
                    _ => None,
                })
                .is_some_and(|row| row.source == HeaderSource::Request);
            if column >= columns.delete() {
                app.remove_preview_row(PreviewTab::Headers, index);
            } else if column < columns.value {
                if request_row {
                    let cursor =
                        is_double.then(|| usize::from(column.saturating_sub(columns.name)));
                    app.click_preview_row(
                        PreviewTab::Headers,
                        index,
                        KeyValueField::Name,
                        true,
                        cursor,
                    );
                } else {
                    app.toggle_header_row(index);
                }
            } else {
                let cursor = is_double.then(|| usize::from(column.saturating_sub(columns.value)));
                app.click_preview_row(
                    PreviewTab::Headers,
                    index,
                    KeyValueField::Value,
                    true,
                    cursor,
                );
            }
        }
        Some(Dialog::Params(_)) => {
            if column >= columns.delete() {
                app.remove_preview_row(PreviewTab::Params, index);
                return;
            }
            let (field, start) = if column < columns.value {
                (KeyValueField::Name, columns.name)
            } else {
                (KeyValueField::Value, columns.value)
            };
            let cursor = is_double.then(|| usize::from(column.saturating_sub(start)));
            app.click_preview_row(PreviewTab::Params, index, field, true, cursor);
        }
        _ => {}
    }
}

struct InlineColumns {
    name: u16,
    value: u16,
    name_width: u16,
    value_width: u16,
}

impl InlineColumns {
    fn field(&self, field: KeyValueField) -> (u16, u16) {
        match field {
            KeyValueField::Name => (self.name, self.name_width),
            KeyValueField::Value => (self.value, self.value_width),
        }
    }

    fn delete(&self) -> u16 {
        self.value
            .saturating_add(self.value_width)
            .saturating_add(TABLE_COLUMN_SPACING)
    }
}

fn inline_columns(area: Rect) -> InlineColumns {
    let widths = inline_table_widths(area.width);
    let name = area.x.saturating_add(TABLE_HIGHLIGHT_WIDTH);
    let name_width = constraint_length(widths[0]);
    InlineColumns {
        name,
        value: name
            .saturating_add(name_width)
            .saturating_add(TABLE_COLUMN_SPACING),
        name_width,
        value_width: constraint_length(widths[1]),
    }
}
