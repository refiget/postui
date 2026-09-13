//! Interactive study for the in-TUI configuration error prompt.
//!
//! Run with:
//!   cargo run --quiet --example config_error_gallery
//!
//! Controls:
//!   1..5      choose a prompt layout
//!   ←/→, h/l  switch layout
//!   d         show or hide diagnostic detail
//!   q, Esc    quit

use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

const BACKGROUND: Color = Color::Rgb(10, 15, 23);
const PANEL: Color = Color::Rgb(20, 29, 41);
const PANEL_ALT: Color = Color::Rgb(25, 37, 53);
const BORDER: Color = Color::Rgb(53, 75, 99);
const TEXT: Color = Color::Rgb(229, 237, 245);
const MUTED: Color = Color::Rgb(128, 148, 171);
const BLUE: Color = Color::Rgb(117, 167, 255);
const CYAN: Color = Color::Rgb(99, 215, 209);
const AMBER: Color = Color::Rgb(243, 184, 95);
const RED: Color = Color::Rgb(255, 114, 136);

const LAYOUTS: [&str; 5] = ["Center", "Bottom", "Card", "Blocking", "Compact"];

struct App {
    layout: usize,
    show_detail: bool,
}

struct Diagnostic {
    code: &'static str,
    summary: &'static str,
    detail: &'static str,
    file: &'static str,
    field: &'static str,
    location: &'static str,
    action: &'static str,
}

const DIAGNOSTIC: Diagnostic = Diagnostic {
    code: "CONFIG_PARSE",
    summary: "Invalid YAML",
    detail: "unknown field `request_config`; expected one of language, theme, max_response_display_bytes, max_response_bytes",
    file: "project/.postui/config.yaml",
    field: "user configuration",
    location: "line 2, column 1",
    action: "Edit the file and reload the configuration",
};

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    let mut app = App {
        layout: 0,
        show_detail: true,
    };

    loop {
        terminal.draw(|frame| draw(frame, &app))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Char('d') | KeyCode::Char('D') => app.show_detail = !app.show_detail,
                KeyCode::Left | KeyCode::Char('h') => {
                    app.layout = if app.layout == 0 {
                        LAYOUTS.len() - 1
                    } else {
                        app.layout - 1
                    };
                }
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
                    app.layout = (app.layout + 1) % LAYOUTS.len();
                }
                KeyCode::Char(value) if ('1'..='5').contains(&value) => {
                    app.layout = usize::from(value as u8 - b'1');
                }
                _ => {}
            }
        }
    }
}

fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(BACKGROUND)),
        area,
    );

    if area.width < 72 || area.height < 22 {
        let message = vec![
            Line::from(Span::styled(
                "CONFIG ERROR GALLERY",
                Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Resize the terminal to at least 72 x 22",
                Style::default().fg(TEXT),
            )),
            Line::from(format!("Current size: {} x {}", area.width, area.height)),
        ];
        frame.render_widget(
            Paragraph::new(message).alignment(Alignment::Center),
            area.inner(Margin::new(2, 1)),
        );
        return;
    }

    let outer = area.inner(Margin::new(2, 1));
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(outer);

    draw_header(frame, sections[0], area, app);
    draw_tabs(frame, sections[1], app.layout);
    match app.layout {
        0 => draw_centered_modal(frame, sections[2], app.show_detail),
        1 => draw_bottom_sheet(frame, sections[2], app.show_detail),
        2 => draw_diagnostic_card(frame, sections[2], app.show_detail),
        3 => draw_blocking_screen(frame, sections[2], app.show_detail),
        _ => draw_compact_prompt(frame, sections[2], app.show_detail),
    }
    draw_footer(frame, sections[3], app);
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, terminal_area: Rect, app: &App) {
    let lines = vec![
        Line::from(vec![
            Span::styled(
                "CONFIG ERROR / UI STUDY",
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("    {} / 5", app.layout + 1),
                Style::default().fg(MUTED),
            ),
        ]),
        Line::from(Span::styled(
            "Config failure stays in TUI; the previous workspace remains visible.",
            Style::default().fg(TEXT),
        )),
        Line::from(Span::styled(
            format!(
                "Terminal: {} x {}    Canvas: {} x {}    Code: {}",
                terminal_area.width, terminal_area.height, area.width, area.height, DIAGNOSTIC.code
            ),
            Style::default().fg(MUTED),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_tabs(frame: &mut Frame<'_>, area: Rect, selected: usize) {
    let tabs = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20); 5])
        .split(area);
    for (index, (tab_area, label)) in tabs.iter().zip(LAYOUTS).enumerate() {
        let style = if index == selected {
            Style::default()
                .fg(BACKGROUND)
                .bg(BLUE)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(MUTED).bg(PANEL)
        };
        frame.render_widget(
            Paragraph::new(format!(" {}  {} ", index + 1, label))
                .alignment(Alignment::Center)
                .style(style)
                .block(
                    Block::default()
                        .borders(Borders::BOTTOM)
                        .border_style(BORDER),
                ),
            *tab_area,
        );
    }
}

fn draw_centered_modal(frame: &mut Frame<'_>, area: Rect, show_detail: bool) {
    let modal = centered(area, 76, 17, 60, 13);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Block::default()
            .title(" Configuration blocked ")
            .title_style(Style::default().fg(RED).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(RED))
            .style(Style::default().bg(PANEL)),
        modal,
    );
    let content = modal.inner(Margin::new(2, 1));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(if show_detail { 3 } else { 1 }),
            Constraint::Length(if show_detail { 4 } else { 0 }),
            Constraint::Length(2),
        ])
        .split(content);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                DIAGNOSTIC.summary,
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "PostUI opened with the last usable workspace state.",
                Style::default().fg(MUTED),
            )),
        ]),
        rows[0],
    );
    draw_detail(frame, rows[1], show_detail);
    if show_detail {
        draw_metadata(frame, rows[2]);
    }
    draw_actions(frame, rows[3], true);
}

fn draw_bottom_sheet(frame: &mut Frame<'_>, area: Rect, show_detail: bool) {
    let sheet_height = if show_detail { 12 } else { 8 };
    let sheet = bottom_aligned(area, sheet_height, 8);
    frame.render_widget(Clear, sheet);
    frame.render_widget(
        Block::default()
            .title(" Error details ")
            .title_style(Style::default().fg(RED).add_modifier(Modifier::BOLD))
            .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
            .border_style(Style::default().fg(RED))
            .style(Style::default().bg(PANEL)),
        sheet,
    );
    let content = sheet.inner(Margin::new(2, 1));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(content);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!("{}  ·  {}", DIAGNOSTIC.code, DIAGNOSTIC.summary),
                Style::default().fg(RED).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "The TUI is ready. Fix the file, then press R to reload.",
                Style::default().fg(TEXT),
            )),
        ]),
        rows[0],
    );
    if show_detail {
        draw_detail(frame, rows[1], true);
    } else {
        frame.render_widget(
            Paragraph::new(Span::styled(DIAGNOSTIC.action, Style::default().fg(MUTED))),
            rows[1],
        );
    }
    draw_actions(frame, rows[2], false);
}

fn draw_diagnostic_card(frame: &mut Frame<'_>, area: Rect, show_detail: bool) {
    let card = centered(area, 88, 16, 64, 13);
    frame.render_widget(Clear, card);
    frame.render_widget(
        Block::default()
            .title(" Diagnostic ")
            .title_style(Style::default().fg(AMBER).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(AMBER))
            .style(Style::default().bg(PANEL)),
        card,
    );
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(card.inner(Margin::new(2, 1)));

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                DIAGNOSTIC.code,
                Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                DIAGNOSTIC.summary,
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "The app is still running.",
                Style::default().fg(MUTED),
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(BORDER),
        ),
        columns[0],
    );
    let detail_area = columns[1].inner(Margin::new(1, 0));
    if show_detail {
        draw_metadata(frame, detail_area);
    } else {
        frame.render_widget(
            Paragraph::new(DIAGNOSTIC.action).style(Style::default().fg(TEXT)),
            detail_area,
        );
    }
}

fn draw_blocking_screen(frame: &mut Frame<'_>, area: Rect, show_detail: bool) {
    let panel = area.inner(Margin::new(4, 1));
    frame.render_widget(
        Block::default()
            .title(" PostUI is running ")
            .title_style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(PANEL)),
        panel,
    );
    let content = panel.inner(Margin::new(2, 1));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(if show_detail { 4 } else { 2 }),
            Constraint::Min(2),
            Constraint::Length(2),
        ])
        .split(content);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Configuration needs attention",
                Style::default().fg(RED).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "The failed file was not applied. Existing workspace data remains available.",
                Style::default().fg(TEXT),
            )),
        ]),
        rows[0],
    );
    if show_detail {
        draw_metadata(frame, rows[1]);
    } else {
        frame.render_widget(
            Paragraph::new(DIAGNOSTIC.action).style(Style::default().fg(MUTED)),
            rows[1],
        );
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Activity",
                Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "  • TUI initialized",
                Style::default().fg(MUTED),
            )),
            Line::from(Span::styled(
                "  • Previous usable configuration retained",
                Style::default().fg(MUTED),
            )),
            Line::from(Span::styled(
                "  • Reload is waiting for a corrected file",
                Style::default().fg(MUTED),
            )),
        ])
        .block(Block::default().borders(Borders::TOP).border_style(BORDER)),
        rows[2],
    );
    draw_actions(frame, rows[3], true);
}

fn draw_compact_prompt(frame: &mut Frame<'_>, area: Rect, show_detail: bool) {
    let prompt = centered(area, 68, if show_detail { 11 } else { 8 }, 54, 8);
    frame.render_widget(Clear, prompt);
    frame.render_widget(
        Block::default()
            .title(" ×  Configuration error ")
            .title_style(Style::default().fg(RED).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(RED))
            .style(Style::default().bg(PANEL_ALT)),
        prompt,
    );
    let content = prompt.inner(Margin::new(2, 1));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(content);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                DIAGNOSTIC.summary,
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "TUI is open; configuration was not applied.",
                Style::default().fg(MUTED),
            )),
        ]),
        rows[0],
    );
    if show_detail {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    DIAGNOSTIC.location,
                    Style::default().fg(AMBER),
                )),
                Line::from(Span::styled(DIAGNOSTIC.detail, Style::default().fg(TEXT))),
            ])
            .wrap(Wrap { trim: false }),
            rows[1],
        );
    } else {
        frame.render_widget(
            Paragraph::new(DIAGNOSTIC.action).style(Style::default().fg(TEXT)),
            rows[1],
        );
    }
    draw_actions(frame, rows[2], false);
}

fn draw_detail(frame: &mut Frame<'_>, area: Rect, show_detail: bool) {
    if !show_detail {
        return;
    }
    frame.render_widget(
        Paragraph::new(DIAGNOSTIC.detail)
            .style(Style::default().fg(TEXT))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_metadata(frame: &mut Frame<'_>, area: Rect) {
    let lines = vec![
        metadata_line("file", DIAGNOSTIC.file),
        metadata_line("field", DIAGNOSTIC.field),
        metadata_line("location", DIAGNOSTIC.location),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(MUTED))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn metadata_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<9}"),
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(TEXT)),
    ])
}

fn draw_actions(frame: &mut Frame<'_>, area: Rect, emphasize_reload: bool) {
    let reload_style = if emphasize_reload {
        Style::default()
            .fg(BACKGROUND)
            .bg(CYAN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" [R] Reload ", reload_style),
            Span::styled("  [E] Edit file  ", Style::default().fg(BLUE)),
            Span::styled("  [Esc] Keep working  ", Style::default().fg(MUTED)),
        ])),
        area,
    );
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" Layout: {} ", LAYOUTS[app.layout]),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  1..5 choose  ←/→ switch  d detail  q quit",
                Style::default().fg(MUTED),
            ),
        ])),
        area,
    );
}

fn centered(
    area: Rect,
    percent: u16,
    preferred_height: u16,
    min_width: u16,
    min_height: u16,
) -> Rect {
    let available_width = area.width.saturating_sub(2).max(1);
    let available_height = area.height.saturating_sub(2).max(1);
    let width = (u32::from(area.width) * u32::from(percent) / 100)
        .try_into()
        .unwrap_or(u16::MAX)
        .max(min_width)
        .min(available_width);
    let height = preferred_height.max(min_height).min(available_height);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn bottom_aligned(area: Rect, preferred_height: u16, min_height: u16) -> Rect {
    let height = preferred_height
        .max(min_height)
        .min(area.height.saturating_sub(1).max(1));
    Rect::new(
        area.x,
        area.bottom().saturating_sub(height),
        area.width,
        height,
    )
}
