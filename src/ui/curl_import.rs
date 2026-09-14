use super::*;
use crate::app::CurlImportFocus;

const DETAILS_WIDTH: u16 = 31;
const IMPORT_WIDTH: u16 = 16;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct CurlImportLayout {
    pub(super) name: Rect,
    pub(super) workspace: Rect,
    pub(super) description: Rect,
    pub(super) registered_variables: Rect,
    pub(super) command: Rect,
    pub(super) status: Rect,
    pub(super) confirm: Rect,
    pub(super) cancel: Rect,
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
    let actions = action_layout(editor[2]);
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
        status: actions[0],
        cancel: actions[1],
        confirm: actions[2],
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
    let actions = action_layout(rows[4]);
    CurlImportLayout {
        name: top[0],
        workspace: top[1],
        description: rows[1],
        registered_variables: rows[2],
        command: rows[3],
        status: actions[0],
        cancel: actions[1],
        confirm: actions[2],
        ..CurlImportLayout::default()
    }
}

fn action_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(10),
        Constraint::Length(IMPORT_WIDTH.min(area.width / 2)),
    ])
    .spacing(1)
    .split(area)
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
    page.sync_buttons();
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
    draw_actions(frame, layout, page, text, theme);

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
        Field {
            label: text.curl_import_name(),
            value: page.name(),
            cursor: page.cursor(),
            focused: page.focused(CurlImportFocus::Name),
            hint: None,
            color: theme.accent,
        },
        theme,
    );
    stacked_workspace(
        frame,
        layout.workspace,
        text.curl_import_workspace(),
        workspace,
        page.button_state(CurlImportFocus::Workspace),
        theme,
    );
    draw_stacked_field(
        frame,
        layout.description,
        Field {
            label: text.curl_import_description(),
            value: page.description(),
            cursor: page.cursor(),
            focused: page.focused(CurlImportFocus::Description),
            hint: None,
            color: theme.secondary,
        },
        theme,
    );
    draw_stacked_field(
        frame,
        layout.registered_variables,
        Field {
            label: text.curl_import_variables(),
            value: page.registered_variables(),
            cursor: page.cursor(),
            focused: page.focused(CurlImportFocus::RegisteredVariables),
            hint: Some(text.curl_import_variables_hint()),
            color: theme.variable,
        },
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
    let shown = if field.value.is_empty() {
        field.hint.unwrap_or("_")
    } else {
        field.value
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
        Paragraph::new(indent_lines(shown))
            .style(style)
            .wrap(Wrap { trim: false }),
        area,
    );
    if field.focused {
        let before = &field.value[..field.cursor];
        let row = before.bytes().filter(|byte| *byte == b'\n').count();
        let width = before
            .rsplit('\n')
            .next()
            .map(Line::from)
            .map_or(0, |line| line.width());
        let x = area
            .x
            .saturating_add(2)
            .saturating_add(u16::try_from(width).unwrap_or(u16::MAX));
        let y = area
            .y
            .saturating_add(u16::try_from(row).unwrap_or(u16::MAX));
        if x < area.right() && y < area.bottom() {
            frame.set_cursor_position((x, y));
        }
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
    state: FlatButtonState,
    theme: &crate::settings::UiTheme,
) {
    frame.render_widget(
        Paragraph::new(label).style(Style::default().fg(theme.muted)),
        Rect::new(area.x, area.y.saturating_sub(1), area.width, 1),
    );
    draw_flat_button_colored(
        frame,
        area,
        &format!("  {workspace}  ▾"),
        state,
        theme.secondary,
        theme,
        Alignment::Left,
    );
}

fn draw_compact_fields(
    frame: &mut Frame<'_>,
    layout: CurlImportLayout,
    page: &crate::app::CurlImportPage,
    workspace: &str,
    text: crate::i18n::UiText,
    theme: &crate::settings::UiTheme,
) {
    let width = [
        text.curl_import_name(),
        text.curl_import_description(),
        text.curl_import_variables(),
    ]
    .into_iter()
    .map(|label| Line::from(label).width())
    .max()
    .unwrap_or_default();
    inline_field(
        frame,
        layout.name,
        Field {
            label: text.curl_import_name(),
            value: page.name(),
            cursor: page.cursor(),
            focused: page.focused(CurlImportFocus::Name),
            hint: None,
            color: theme.accent,
        },
        width,
        theme,
    );
    draw_flat_button_colored(
        frame,
        layout.workspace,
        &format!("{}: {workspace} ▾", text.curl_import_workspace()),
        page.button_state(CurlImportFocus::Workspace),
        theme.secondary,
        theme,
        Alignment::Left,
    );
    inline_field(
        frame,
        layout.description,
        Field {
            label: text.curl_import_description(),
            value: page.description(),
            cursor: page.cursor(),
            focused: page.focused(CurlImportFocus::Description),
            hint: None,
            color: theme.secondary,
        },
        width,
        theme,
    );
    inline_field(
        frame,
        layout.registered_variables,
        Field {
            label: text.curl_import_variables(),
            value: page.registered_variables(),
            cursor: page.cursor(),
            focused: page.focused(CurlImportFocus::RegisteredVariables),
            hint: Some(text.curl_import_variables_hint()),
            color: theme.variable,
        },
        width,
        theme,
    );
}

fn inline_field(
    frame: &mut Frame<'_>,
    area: Rect,
    field: Field<'_>,
    label_width: usize,
    theme: &crate::settings::UiTheme,
) {
    let shown = if field.value.is_empty() {
        field.hint.unwrap_or("_")
    } else {
        field.value
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
            Span::styled(shown.to_string(), style),
        ])),
        area,
    );
    if field.focused {
        let width = label_width
            .saturating_add(2)
            .saturating_add(Line::from(&field.value[..field.cursor]).width());
        let x = area
            .x
            .saturating_add(u16::try_from(width).unwrap_or(u16::MAX));
        if x < area.right() {
            frame.set_cursor_position((x, area.y));
        }
    }
}

fn draw_actions(
    frame: &mut Frame<'_>,
    layout: CurlImportLayout,
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
            layout.status,
        );
    }
    draw_flat_button_colored(
        frame,
        layout.cancel,
        text.curl_import_cancel(),
        page.button_state(CurlImportFocus::Cancel),
        theme.muted,
        theme,
        Alignment::Center,
    );
    draw_flat_button_colored(
        frame,
        layout.confirm,
        text.curl_import_confirm(),
        page.button_state(CurlImportFocus::Confirm),
        theme.primary,
        theme,
        Alignment::Center,
    );
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
        let before = &page.command()[..page.cursor()];
        let row = before.bytes().filter(|byte| *byte == b'\n').count();
        let column = before
            .rsplit('\n')
            .next()
            .map(Line::from)
            .map_or(0, |line| line.width());
        let x = inner
            .x
            .saturating_add(u16::try_from(column).unwrap_or(u16::MAX));
        let y = inner
            .y
            .saturating_add(u16::try_from(row).unwrap_or(u16::MAX));
        if x < inner.right() && y < inner.bottom() {
            frame.set_cursor_position((x, y));
        }
    }
}
