use std::{io, time::Duration};

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
        MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

const ACCENT: Color = Color::Rgb(255, 184, 77);
const SURFACE: Color = Color::Rgb(38, 42, 48);
const MUTED: Color = Color::Rgb(119, 128, 140);
const TEXT: Color = Color::Rgb(229, 232, 236);
const PRIMARY: Color = Color::Rgb(0, 122, 204);
const SUCCESS: Color = Color::Rgb(46, 160, 67);
const WARNING: Color = Color::Rgb(210, 153, 34);
const ERROR: Color = Color::Rgb(218, 54, 51);
const CHARM: Color = Color::Rgb(255, 92, 168);

const NAMES: [&str; 16] = [
    "Textual primary",
    "Textual default",
    "Textual success",
    "Textual warning",
    "Textual error",
    "Textual flat",
    "Charm / Gum",
    "Lip Gloss rounded",
    "Lip Gloss outline",
    "Lazygit action",
    "K9s hotkey",
    "Zellij tab",
    "Helix tab",
    "GitHub CLI keycap",
    "Segmented control",
    "Classic dialog",
];

#[derive(Default)]
struct App {
    row: usize,
    choice: usize,
    active_row: usize,
    active_choice: usize,
    hits: Vec<(Rect, usize, usize)>,
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
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

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::default();
    loop {
        terminal.draw(|frame| draw(frame, &mut app))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Up | KeyCode::Char('k') => app.row = app.row.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => app.row = (app.row + 1).min(NAMES.len() - 1),
                KeyCode::Left | KeyCode::Char('h') => app.choice = 0,
                KeyCode::Right | KeyCode::Char('l') => app.choice = 1,
                KeyCode::Tab => app.choice = 1 - app.choice,
                KeyCode::Enter | KeyCode::Char(' ') => {
                    app.active_row = app.row;
                    app.active_choice = app.choice;
                }
                _ => {}
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, row, choice)) = app
                    .hits
                    .iter()
                    .find(|(rect, _, _)| rect.contains((mouse.column, mouse.row).into()))
                {
                    app.row = *row;
                    app.choice = *choice;
                    app.active_row = *row;
                    app.active_choice = *choice;
                }
            }
            _ => {}
        }
    }
}

fn draw(frame: &mut Frame<'_>, app: &mut App) {
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(20, 22, 26))),
        frame.area(),
    );
    let outer = frame.area().inner(ratatui::layout::Margin::new(2, 1));
    if outer.width < 54 || outer.height < 16 {
        frame.render_widget(
            Paragraph::new("Terminal too small - use at least 58 x 18")
                .style(Style::default().fg(ACCENT))
                .alignment(Alignment::Center),
            outer,
        );
        return;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(outer);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "BUTTON STUDY / 16",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Short controls for the response toolbar",
                Style::default().fg(MUTED),
            )),
        ]),
        sections[0],
    );
    frame.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(SURFACE)),
        sections[1],
    );

    app.hits.clear();
    let visible_count = usize::from(sections[2].height / 3).max(1).min(NAMES.len());
    let start = app
        .row
        .saturating_sub(visible_count / 2)
        .min(NAMES.len() - visible_count);
    let sample_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(3); visible_count])
        .split(sections[2]);
    for (slot, row) in (start..start + visible_count).enumerate() {
        draw_sample(frame, sample_rows[slot], row, app);
    }

    let selected = format!(
        "Selected: {} / {}",
        NAMES[app.active_row],
        if app.active_choice == 0 {
            "Formatted"
        } else {
            "Raw"
        }
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                selected,
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "    arrows move  enter select  mouse click  q quit",
                Style::default().fg(MUTED),
            ),
        ])),
        sections[3],
    );
}

fn draw_sample(frame: &mut Frame<'_>, area: Rect, row: usize, app: &mut App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(24),
            Constraint::Length(16),
            Constraint::Length(14),
            Constraint::Min(0),
        ])
        .split(area);
    let selected = app.row == row;
    let marker = if selected { ">" } else { " " };
    frame.render_widget(
        Paragraph::new(format!("{marker} {}  {}", row + 1, NAMES[row])).style(
            Style::default()
                .fg(if selected { TEXT } else { MUTED })
                .add_modifier(if selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        cols[0],
    );

    for (choice, cell) in [(0, cols[1]), (1, cols[2])] {
        let focused = selected && app.choice == choice;
        let active = app.active_row == row && app.active_choice == choice;
        let label = if choice == 0 { "Formatted" } else { "Raw" };
        draw_button(frame, cell, row, label, focused, active);
        app.hits.push((cell, row, choice));
    }
}

fn draw_button(
    frame: &mut Frame<'_>,
    area: Rect,
    style: usize,
    label: &str,
    focused: bool,
    active: bool,
) {
    frame.render_widget(Clear, area);

    if style == 7 || style == 8 {
        let button_width = (label.len() as u16 + 4).min(area.width);
        let button_area = Rect::new(area.x, area.y, button_width, area.height.min(3));
        let color = if style == 7 { CHARM } else { PRIMARY };
        let border_type = if style == 7 {
            BorderType::Rounded
        } else {
            BorderType::Plain
        };
        frame.render_widget(
            Paragraph::new(if active {
                format!("{label} ✓")
            } else {
                label.to_owned()
            })
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(if focused { Color::Black } else { Color::White })
                    .bg(if focused { ACCENT } else { color })
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(border_type)
                    .border_style(
                        Style::default()
                            .fg(if focused { Color::White } else { color })
                            .add_modifier(Modifier::BOLD),
                    ),
            ),
            button_area,
        );
        return;
    }

    let color = match style {
        0 | 11 => PRIMARY,
        1 | 13 => Color::Rgb(91, 101, 115),
        2 => SUCCESS,
        3 => WARNING,
        4 => ERROR,
        5 | 12 => Color::Rgb(32, 135, 190),
        6 => CHARM,
        9 => Color::Rgb(230, 126, 34),
        10 => Color::Rgb(0, 150, 136),
        14 => ACCENT,
        _ => Color::Rgb(76, 110, 245),
    };
    let foreground = if matches!(style, 3 | 6 | 14) {
        Color::Black
    } else {
        Color::White
    };
    let fill = Style::default()
        .fg(foreground)
        .bg(color)
        .add_modifier(Modifier::BOLD);
    let focus = Style::default()
        .fg(Color::Black)
        .bg(Color::White)
        .add_modifier(Modifier::BOLD);
    let button = if focused { focus } else { fill };
    let state = if active { " ✓" } else { "" };

    let spans = match style {
        0 => vec![
            Span::styled(
                if focused { "▶ " } else { "  " },
                Style::default().fg(ACCENT),
            ),
            Span::styled(format!(" {label}{state} "), button),
        ],
        5 => vec![Span::styled(
            format!("▌ {label}{state} "),
            button.add_modifier(Modifier::UNDERLINED),
        )],
        6 => vec![Span::styled(format!("  {label}{state}  "), button)],
        9 => vec![
            Span::styled(
                " ENTER ",
                Style::default()
                    .fg(Color::Black)
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {label}{state} "), button),
        ],
        10 => vec![Span::styled(
            format!(" {}  {label}{state} ", &label[0..1]),
            button,
        )],
        11 => vec![
            Span::styled(
                "",
                Style::default().fg(if focused { Color::White } else { color }),
            ),
            Span::styled(format!(" {label}{state} "), button),
            Span::styled(
                "",
                Style::default().fg(if focused { Color::White } else { color }),
            ),
        ],
        12 => vec![Span::styled(format!("▔ {label}{state} ▔"), button)],
        13 => vec![
            Span::styled("[", Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {label}{state} "), button),
            Span::styled("]", Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ],
        14 => vec![
            Span::styled(format!(" {label}{state} "), button),
            Span::styled("│", Style::default().fg(Color::Rgb(20, 22, 26)).bg(color)),
        ],
        15 => vec![
            Span::styled("< ", button),
            Span::styled(format!("{label}{state}"), button),
            Span::styled(" >", button),
        ],
        _ => vec![
            Span::styled(
                if focused { "▶" } else { "▐" },
                Style::default().fg(if focused { Color::White } else { color }),
            ),
            Span::styled(format!(" {label}{state} "), button),
            Span::styled(
                "▌",
                Style::default().fg(if focused { Color::White } else { color }),
            ),
        ],
    };

    let line_area = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(1) / 2,
        area.width,
        1,
    );
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Left),
        line_area,
    );
}
