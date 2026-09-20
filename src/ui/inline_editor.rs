use super::{
    TABLE_COLUMN_SPACING, TABLE_HIGHLIGHT_WIDTH, contains,
    dialog::{InlineEditorLayout, draw_headers_dialog, draw_params_dialog, inline_table_widths},
    layout::inner_scroll_areas,
    widgets::{
        constraint_length, scrollbar_offset_from_drag, scrollbar_offset_from_track,
        scrollbar_track_state,
    },
};
use crate::app::{App, Dialog, HeaderSource, KeyValueField, PreviewTab, ScrollDragTarget};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
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
    let row_count = match dialog {
        Dialog::Configurations(_) => 0,
        Dialog::Headers(dialog) => dialog.rows.len(),
        Dialog::Params(dialog) => dialog.rows.len(),
    };
    let layout = inline_dialog_layout(area, row_count);
    match dialog {
        Dialog::Configurations(_) => {}
        Dialog::Headers(dialog) => draw_headers_dialog(frame, app, dialog, layout),
        Dialog::Params(dialog) => draw_params_dialog(frame, app, dialog, layout),
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
}

pub(super) fn drag_inline_editor_scrollbar(app: &mut App, row: u16, area: Rect) -> bool {
    let row_count = match app.view.dialog.as_ref() {
        Some(Dialog::Headers(dialog)) => dialog.rows.len(),
        Some(Dialog::Params(dialog)) => dialog.rows.len(),
        _ => return false,
    };
    let layout = inline_dialog_layout(area, row_count);
    let visible = usize::from(layout.rows.content.height);
    let offset = inline_scroll(app, row_count, visible);
    let anchor = match app.view.dialog.as_ref() {
        Some(Dialog::Headers(dialog)) => dialog.scroll.drag_anchor,
        Some(Dialog::Params(dialog)) => dialog.scroll.drag_anchor,
        _ => None,
    };
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
    let Some(dialog) = app.view.dialog.as_mut() else {
        return false;
    };
    let (row_count, scroll) = match dialog {
        Dialog::Headers(dialog) => (dialog.rows.len(), &mut dialog.scroll),
        Dialog::Params(dialog) => (dialog.rows.len(), &mut dialog.scroll),
        Dialog::Configurations(_) => return false,
    };
    let visible = usize::from(inline_dialog_layout(area, row_count).rows.content.height);
    scroll.move_by(direction, row_count, visible);
    true
}

fn inline_scroll(app: &App, row_count: usize, visible: usize) -> usize {
    match app.view.dialog.as_ref() {
        Some(Dialog::Headers(dialog)) => dialog.scroll.offset(row_count, visible),
        Some(Dialog::Params(dialog)) => dialog.scroll.offset(row_count, visible),
        _ => 0,
    }
}

fn set_inline_scroll(
    app: &mut App,
    offset: usize,
    row_count: usize,
    visible: usize,
    drag_anchor: Option<(u16, usize)>,
) {
    let scroll = match app.view.dialog.as_mut() {
        Some(Dialog::Headers(dialog)) => &mut dialog.scroll,
        Some(Dialog::Params(dialog)) => &mut dialog.scroll,
        _ => return,
    };
    scroll.set_offset(offset, row_count, visible);
    if drag_anchor.is_some() {
        scroll.drag_anchor = drag_anchor;
    }
}

pub(super) fn place_inline_editor_cursor(app: &mut App, column: u16, row: u16, area: Rect) -> bool {
    let (row_count, selected, field, scroll, editor) = match app.view.dialog.as_mut() {
        Some(Dialog::Headers(dialog)) => (
            dialog.rows.len(),
            dialog.selected,
            dialog.field,
            &dialog.scroll,
            dialog.editor.as_mut(),
        ),
        Some(Dialog::Params(dialog)) => (
            dialog.rows.len(),
            dialog.selected,
            dialog.field,
            &dialog.scroll,
            dialog.editor.as_mut(),
        ),
        _ => return false,
    };
    let Some(editor) = editor else {
        return false;
    };
    let layout = inline_dialog_layout(area, row_count);
    if !contains(layout.rows.content, column, row) {
        return false;
    }
    let offset = scroll.offset(row_count, usize::from(layout.rows.content.height));
    if offset.saturating_add(usize::from(row - layout.rows.content.y)) != selected {
        return false;
    }
    let widths = inline_table_widths(layout.rows.content.width);
    let name_start = layout.rows.content.x.saturating_add(TABLE_HIGHLIGHT_WIDTH);
    let (start, width) = match field {
        KeyValueField::Name => (name_start, constraint_length(widths[0])),
        KeyValueField::Value => (
            name_start
                .saturating_add(constraint_length(widths[0]))
                .saturating_add(TABLE_COLUMN_SPACING),
            constraint_length(widths[1]),
        ),
    };
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
    let row_count = match app.view.dialog.as_ref() {
        Some(Dialog::Headers(dialog)) => dialog.rows.len(),
        Some(Dialog::Params(dialog)) => dialog.rows.len(),
        _ => return,
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
    let (row_count, offset) = match app.view.dialog.as_ref() {
        Some(Dialog::Headers(dialog)) => (
            dialog.rows.len(),
            dialog.scroll.offset(dialog.rows.len(), visible),
        ),
        Some(Dialog::Params(dialog)) => (
            dialog.rows.len(),
            dialog.scroll.offset(dialog.rows.len(), visible),
        ),
        _ => return,
    };
    let index = offset.saturating_add(usize::from(row - layout.rows.content.y));
    if index >= row_count {
        return;
    }
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
            let widths = inline_table_widths(layout.rows.content.width);
            let value_start = layout
                .rows
                .content
                .x
                .saturating_add(TABLE_HIGHLIGHT_WIDTH)
                .saturating_add(constraint_length(widths[0]))
                .saturating_add(TABLE_COLUMN_SPACING);
            let delete_start = value_start
                .saturating_add(constraint_length(widths[1]))
                .saturating_add(TABLE_COLUMN_SPACING);
            let name_start = layout.rows.content.x.saturating_add(TABLE_HIGHLIGHT_WIDTH);
            if column >= delete_start {
                app.remove_preview_row(PreviewTab::Headers, index);
            } else if column < value_start {
                if request_row {
                    let cursor = is_double.then(|| usize::from(column.saturating_sub(name_start)));
                    app.click_header_row(index, KeyValueField::Name, true, cursor);
                } else {
                    app.toggle_header_row(index);
                }
            } else {
                let cursor = is_double.then(|| usize::from(column.saturating_sub(value_start)));
                app.click_header_row(index, KeyValueField::Value, true, cursor);
            }
        }
        Some(Dialog::Params(_)) => {
            let widths = inline_table_widths(layout.rows.content.width);
            let value_start = layout
                .rows
                .content
                .x
                .saturating_add(TABLE_HIGHLIGHT_WIDTH)
                .saturating_add(constraint_length(widths[0]))
                .saturating_add(TABLE_COLUMN_SPACING);
            let delete_start = value_start
                .saturating_add(constraint_length(widths[1]))
                .saturating_add(TABLE_COLUMN_SPACING);
            if column >= delete_start {
                app.remove_preview_row(PreviewTab::Params, index);
                return;
            }
            let field = if column < value_start {
                KeyValueField::Name
            } else {
                KeyValueField::Value
            };
            let field_start = if field == KeyValueField::Name {
                layout.rows.content.x.saturating_add(TABLE_HIGHLIGHT_WIDTH)
            } else {
                value_start
            };
            let cursor = is_double.then(|| usize::from(column.saturating_sub(field_start)));
            app.click_param_row(index, field, true, cursor);
        }
        _ => {}
    }
}
