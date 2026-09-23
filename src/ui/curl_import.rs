use super::{
    dialog::draw_configuration_dropdown,
    widgets::{asset_theme, edit_input_text_style, panel_block, place_cursor, section_style},
};
use crate::app::{App, CurlImportFocus, Dialog};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

const DETAILS_WIDTH: u16 = 31;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct CurlImportLayout {
    pub(super) name: Rect,
    pub(super) workspace: Rect,
    pub(super) description: Rect,
    pub(super) registered_variables: Rect,
    pub(super) command: Rect,
    pub(super) status: Rect,
    heading: Rect,
    flow: Rect,
    wide: bool,
}

impl CurlImportLayout {
    pub(super) fn wide(self) -> bool {
        self.wide
    }
}

pub(super) fn curl_import_layout(area: Rect) -> CurlImportLayout {
    let inner = area.inner(Margin::new(1, 1)).inner(Margin::new(2, 1));
    if inner.width >= 72 && inner.height >= 18 {
        wide_layout(inner)
    } else {
        compact_layout(inner)
    }
}

fn wide_layout(area: Rect) -> CurlImportLayout {
    let columns = Layout::horizontal([Constraint::Length(DETAILS_WIDTH), Constraint::Min(32)])
        .spacing(3)
        .split(area);
    let fields = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(4),
    ])
    .spacing(1)
    .split(columns[0]);
    let editor = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .spacing(1)
    .split(columns[1]);
    CurlImportLayout {
        heading: fields[0],
        name: value_line(fields[1]),
        workspace: value_line(fields[2]),
        description: value_line(fields[3]),
        registered_variables: Rect::new(
            fields[4].x,
            fields[4].y.saturating_add(1),
            fields[4].width,
            fields[4].height.saturating_sub(1),
        ),
        flow: editor[0],
        command: editor[1],
        status: editor[2],
        wide: true,
    }
}

fn compact_layout(area: Rect) -> CurlImportLayout {
    if area.is_empty() {
        return CurlImportLayout::default();
    }
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .spacing(1)
    .split(area);
    let top = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .spacing(2)
        .split(rows[0]);
    CurlImportLayout {
        name: top[0],
        workspace: top[1],
        description: rows[1],
        registered_variables: rows[2],
        command: rows[3],
        status: rows[4],
        ..CurlImportLayout::default()
    }
}

fn value_line(area: Rect) -> Rect {
    Rect::new(area.x, area.y.saturating_add(1), area.width, 1)
}

pub(super) fn draw_curl_import_page(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let text = app.text();
    let theme = &app.global_config.theme;
    let workspace = app.active_configuration().to_string();
    let Some(page) = app.view.curl_import.as_mut() else {
        return;
    };
    let block = panel_block(format!(" {} ", text.curl_import_title()), area, theme)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(block, area);
    let layout = curl_import_layout(area);
    if layout.command.is_empty() {
        return;
    }

    if layout.wide {
        draw_wide_fields(frame, layout, page, &workspace, text, theme);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("cURL", section_style(theme)),
                Span::styled("  ───────▶  ", Style::default().fg(theme.muted)),
                Span::styled(
                    text.curl_import_request(),
                    Style::default().fg(theme.variable),
                ),
            ])),
            layout.flow,
        );
    } else {
        draw_compact_fields(frame, layout, page, &workspace, text, theme);
    }
    draw_command(frame, layout.command, page, !layout.wide, text, theme);
    draw_status(frame, layout.status, page, text, theme);

    if let Some(Dialog::Configurations(dialog)) = &mut app.view.dialog {
        draw_configuration_dropdown(
            frame,
            dialog,
            layout.workspace,
            text.workspace(),
            asset_theme(theme),
        );
    }
}

fn draw_wide_fields(
    frame: &mut Frame<'_>,
    layout: CurlImportLayout,
    page: &crate::app::CurlImportPage,
    workspace: &str,
    text: crate::i18n::UiText,
    theme: &crate::settings::UiTheme,
) {
    frame.render_widget(
        Paragraph::new(text.curl_import_details()).style(section_style(theme)),
        layout.heading,
    );
    draw_stacked_field(
        frame,
        layout.name,
        field(
            page,
            CurlImportFocus::Name,
            text.curl_import_name(),
            None,
            theme.accent,
        ),
        theme,
    );
    stacked_workspace(
        frame,
        layout.workspace,
        text.curl_import_workspace(),
        workspace,
        page.focused(CurlImportFocus::Workspace),
        theme,
    );
    draw_stacked_field(
        frame,
        layout.description,
        field(
            page,
            CurlImportFocus::Description,
            text.curl_import_description(),
            None,
            theme.secondary,
        ),
        theme,
    );
    draw_stacked_field(
        frame,
        layout.registered_variables,
        field(
            page,
            CurlImportFocus::RegisteredVariables,
            text.curl_import_variables(),
            Some(text.curl_import_variables_hint()),
            theme.variable,
        ),
        theme,
    );
}

struct Field<'a> {
    label: &'a str,
    value: &'a str,
    cursor: usize,
    focused: bool,
    hint: Option<&'a str>,
    color: Color,
}

/// 字段的当前值、光标和聚焦状态。
fn field<'a>(
    page: &'a crate::app::CurlImportPage,
    focus: CurlImportFocus,
    label: &'a str,
    hint: Option<&'a str>,
    color: Color,
) -> Field<'a> {
    Field {
        label,
        value: page.field_value(focus),
        cursor: page.cursor(focus),
        focused: page.focused(focus),
        hint,
        color,
    }
}

fn draw_stacked_field(
    frame: &mut Frame<'_>,
    area: Rect,
    field: Field<'_>,
    theme: &crate::settings::UiTheme,
) {
    frame.render_widget(
        Paragraph::new(field.label).style(Style::default().fg(if field.focused {
            field.color
        } else {
            theme.muted
        })),
        Rect::new(area.x, area.y.saturating_sub(1), area.width, 1),
    );
    let (row, column) = crate::editor::cursor_row_column(field.value, field.cursor);
    let (shown, cursor_column) = if field.value.is_empty() {
        (field.hint.unwrap_or("_").to_string(), column)
    } else if field.focused && row == 0 {
        let width = usize::from(area.width).saturating_sub(2);
        let window = crate::editor::cursor_window(field.value, field.cursor, width);
        (format!("  {}", window.text()), window.column())
    } else {
        (indent_lines(field.value), column)
    };
    let style = Style::default()
        .fg(if field.value.is_empty() && field.hint.is_some() {
            theme.muted
        } else {
            theme.text
        })
        .bg(if field.focused {
            theme.selection
        } else {
            theme.surface
        })
        .add_modifier(if field.focused {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
    frame.render_widget(
        Paragraph::new(shown)
            .style(style)
            .wrap(Wrap { trim: false }),
        area,
    );
    if field.focused {
        place_cursor(frame, area, 2usize.saturating_add(cursor_column), row);
    }
}

fn indent_lines(value: &str) -> String {
    format!("  {}", value.replace('\n', "\n  "))
}

fn stacked_workspace(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    workspace: &str,
    focused: bool,
    theme: &crate::settings::UiTheme,
) {
    frame.render_widget(
        Paragraph::new(label).style(Style::default().fg(if focused {
            theme.secondary
        } else {
            theme.muted
        })),
        Rect::new(area.x, area.y.saturating_sub(1), area.width, 1),
    );
    let style = Style::default()
        .fg(theme.text)
        .bg(if focused {
            theme.selection
        } else {
            theme.surface
        })
        .add_modifier(if focused {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
    frame.render_widget(Paragraph::new(format!("  {workspace}")).style(style), area);
}

fn draw_compact_fields(
    frame: &mut Frame<'_>,
    layout: CurlImportLayout,
    page: &crate::app::CurlImportPage,
    workspace: &str,
    text: crate::i18n::UiText,
    theme: &crate::settings::UiTheme,
) {
    let width = compact_field_label_width(text);
    inline_field(
        frame,
        layout.name,
        field(
            page,
            CurlImportFocus::Name,
            text.curl_import_name(),
            None,
            theme.accent,
        ),
        width,
        theme,
    );
    inline_workspace(
        frame,
        layout.workspace,
        text.curl_import_workspace(),
        workspace,
        page.focused(CurlImportFocus::Workspace),
        width,
        theme,
    );
    inline_field(
        frame,
        layout.description,
        field(
            page,
            CurlImportFocus::Description,
            text.curl_import_description(),
            None,
            theme.secondary,
        ),
        width,
        theme,
    );
    inline_field(
        frame,
        layout.registered_variables,
        field(
            page,
            CurlImportFocus::RegisteredVariables,
            text.curl_import_variables(),
            Some(text.curl_import_variables_hint()),
            theme.variable,
        ),
        width,
        theme,
    );
}

pub(super) fn compact_field_label_width(text: crate::i18n::UiText) -> usize {
    [
        text.curl_import_name(),
        text.curl_import_description(),
        text.curl_import_variables(),
    ]
    .into_iter()
    .map(|label| Line::from(label).width())
    .max()
    .unwrap_or_default()
}

fn inline_workspace(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    workspace: &str,
    focused: bool,
    label_width: usize,
    theme: &crate::settings::UiTheme,
) {
    let padding = " ".repeat(label_width.saturating_sub(Line::from(label).width()));
    let value_style = if focused {
        edit_input_text_style(theme.secondary)
    } else {
        Style::default().fg(theme.secondary)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{label}{padding}: "),
                Style::default().fg(theme.secondary),
            ),
            Span::styled(workspace.to_string(), value_style),
        ])),
        area,
    );
}

fn inline_field(
    frame: &mut Frame<'_>,
    area: Rect,
    field: Field<'_>,
    label_width: usize,
    theme: &crate::settings::UiTheme,
) {
    let value_width = usize::from(area.width).saturating_sub(label_width.saturating_add(2));
    let (shown, cursor_column) = if field.value.is_empty() {
        (field.hint.unwrap_or("_").to_string(), 0)
    } else if field.focused {
        let window = crate::editor::cursor_window(field.value, field.cursor, value_width);
        (window.text(), window.column())
    } else {
        (field.value.to_string(), 0)
    };
    let padding = " ".repeat(label_width.saturating_sub(Line::from(field.label).width()));
    let style = if field.value.is_empty() && field.hint.is_some() {
        Style::default().fg(theme.muted)
    } else if field.focused {
        edit_input_text_style(field.color)
    } else {
        Style::default().fg(field.color)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{}{padding}: ", field.label),
                Style::default().fg(field.color),
            ),
            Span::styled(shown, style),
        ])),
        area,
    );
    if field.focused {
        let column = label_width.saturating_add(2).saturating_add(cursor_column);
        place_cursor(frame, area, column, 0);
    }
}

fn draw_status(
    frame: &mut Frame<'_>,
    area: Rect,
    page: &crate::app::CurlImportPage,
    text: crate::i18n::UiText,
    theme: &crate::settings::UiTheme,
) {
    if let Some((status, error)) = page.status(text) {
        frame.render_widget(
            Paragraph::new(status).style(Style::default().fg(if error {
                theme.error
            } else {
                theme.primary
            })),
            area,
        );
    }
}

fn draw_command(
    frame: &mut Frame<'_>,
    area: Rect,
    page: &crate::app::CurlImportPage,
    titled: bool,
    text: crate::i18n::UiText,
    theme: &crate::settings::UiTheme,
) {
    let focused = page.focused(CurlImportFocus::Command);
    let empty = page.command().is_empty();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(if focused {
            theme.accent
        } else if empty {
            theme.secondary
        } else {
            theme.muted
        }))
        .style(Style::default().bg(theme.surface));
    let block = if titled {
        block.title(format!(" {} ", text.curl_import_command()))
    } else {
        block
    };
    let inner = block.inner(area).inner(Margin::new(1, 1));
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(if empty { "_" } else { page.command() })
            .style(Style::default().fg(if empty { theme.secondary } else { theme.text }))
            .wrap(Wrap { trim: false }),
        inner,
    );
    if focused {
        let (row, column) =
            crate::editor::cursor_row_column(page.command(), page.cursor(CurlImportFocus::Command));
        place_cursor(frame, inner, column, row);
    }
}
