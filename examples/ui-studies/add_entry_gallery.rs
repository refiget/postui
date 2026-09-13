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
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

const BACKGROUND: Color = Color::Rgb(13, 17, 27);
const SURFACE: Color = Color::Rgb(24, 30, 44);
const SURFACE_HIGH: Color = Color::Rgb(41, 51, 74);
const TEXT: Color = Color::Rgb(232, 237, 247);
const MUTED: Color = Color::Rgb(127, 138, 163);
const ACCENT: Color = Color::Rgb(95, 215, 215);
const SECONDARY: Color = Color::Rgb(169, 139, 250);
const WARM: Color = Color::Rgb(255, 184, 108);

const NAMES: [&str; 12] = [
    "Textual flat row",
    "Ghost action row",
    "Dashed placeholder",
    "Table continuation",
    "Lazygit hotkey",
    "K9s command hint",
    "Command palette",
    "Split key / value",
    "Compact chip",
    "Left rail action",
    "Centered invitation",
    "Footer action bar",
];

#[derive(Default)]
struct Gallery {
    selected: usize,
    active: usize,
    hits: Vec<(Rect, usize)>,
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
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
    let mut gallery = Gallery::default();
    loop {
        terminal.draw(|frame| draw(frame, &mut gallery))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Up | KeyCode::Char('k') => {
                    gallery.selected = gallery.selected.saturating_sub(1)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    gallery.selected = (gallery.selected + 1).min(NAMES.len() - 1)
                }
                KeyCode::Enter | KeyCode::Char(' ') => gallery.active = gallery.selected,
                _ => {}
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, index)) = gallery
                    .hits
                    .iter()
                    .find(|(area, _)| area.contains((mouse.column, mouse.row).into()))
                {
                    gallery.selected = *index;
                    gallery.active = *index;
                }
            }
            _ => {}
        }
    }
}

fn draw(frame: &mut Frame<'_>, gallery: &mut Gallery) {
    frame.render_widget(
        Block::default().style(Style::default().bg(BACKGROUND)),
        frame.area(),
    );
    let outer = frame.area().inner(Margin::new(2, 1));
    if outer.width < 70 || outer.height < 20 {
        frame.render_widget(
            Paragraph::new("终端至少需要 74 × 22")
                .style(Style::default().fg(WARM))
                .alignment(Alignment::Center),
            outer,
        );
        return;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(6),
            Constraint::Length(2),
        ])
        .split(outer);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "ADD ENTRY / TUI PATTERN STUDY",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "参数与请求头的末行入口 · ↑↓ / jk 浏览 · Enter 或鼠标选择",
                Style::default().fg(MUTED),
            )),
        ]),
        sections[0],
    );
    draw_context(frame, sections[1]);

    gallery.hits.clear();
    let visible = usize::from(sections[2].height / 4).max(1).min(NAMES.len());
    let start = gallery
        .selected
        .saturating_sub(visible / 2)
        .min(NAMES.len() - visible);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(4); visible])
        .split(sections[2]);
    for (slot, index) in (start..start + visible).enumerate() {
        draw_sample(frame, rows[slot], index, gallery.selected == index);
        gallery.hits.push((rows[slot], index));
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("已选 {:02}  {}", gallery.active + 1, NAMES[gallery.active]),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("    q / Esc 退出", Style::default().fg(MUTED)),
        ])),
        sections[3],
    );
}

fn draw_context(frame: &mut Frame<'_>, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1); 4])
        .split(area);
    frame.render_widget(
        Paragraph::new("  Name                         Value                         ")
            .style(Style::default().fg(MUTED).bg(SURFACE)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new("  Authorization                Bearer {{token}}               − ")
            .style(Style::default().fg(TEXT).bg(SURFACE_HIGH)),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new("  Accept                       application/json               − ")
            .style(Style::default().fg(TEXT).bg(SURFACE)),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new("  下方展示新增入口在真实表格语境中的形态")
            .style(Style::default().fg(MUTED)),
        rows[3],
    );
}

fn draw_sample(frame: &mut Frame<'_>, area: Rect, index: usize, focused: bool) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(25), Constraint::Min(36)])
        .split(area);
    let marker = if focused { "›" } else { " " };
    frame.render_widget(
        Paragraph::new(format!("{marker} {:02}  {}", index + 1, NAMES[index])).style(
            Style::default()
                .fg(if focused { TEXT } else { MUTED })
                .add_modifier(if focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        columns[0],
    );
    draw_add_control(frame, columns[1], index, focused);
}

fn draw_add_control(frame: &mut Frame<'_>, area: Rect, style: usize, focused: bool) {
    frame.render_widget(Clear, area);
    let bg = if focused { SURFACE_HIGH } else { SURFACE };
    let accent = if focused { WARM } else { ACCENT };
    let base = Style::default().fg(TEXT).bg(bg);
    let muted = Style::default().fg(MUTED).bg(bg);
    let strong = Style::default()
        .fg(accent)
        .bg(bg)
        .add_modifier(Modifier::BOLD);

    match style {
        0 => frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  +  ", strong),
                Span::styled("Add entry", base.add_modifier(Modifier::BOLD)),
            ]))
            .style(base),
            Rect::new(area.x, area.y + 1, area.width, 1),
        ),
        1 => frame.render_widget(
            Paragraph::new("＋  Add another entry")
                .style(strong)
                .alignment(Alignment::Center),
            Rect::new(area.x, area.y + 1, area.width, 1),
        ),
        2 => frame.render_widget(
            Paragraph::new("  · · · · · ·  + Add entry  · · · · · ·  ")
                .style(muted.fg(accent))
                .alignment(Alignment::Center),
            Rect::new(area.x, area.y + 1, area.width, 1),
        ),
        3 => frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  +  ", strong),
                Span::styled("name", muted),
                Span::styled("                    value", muted),
            ]))
            .style(base),
            Rect::new(area.x, area.y + 1, area.width, 1),
        ),
        4 => frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" a ", Style::default().fg(BACKGROUND).bg(WARM)),
                Span::styled("  add entry", base.add_modifier(Modifier::BOLD)),
            ])),
            Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(2), 1),
        ),
        5 => frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("[a]", strong),
                Span::styled(" Add entry", base),
                Span::styled("   [i] Import", muted),
            ]))
            .alignment(Alignment::Center),
            Rect::new(area.x, area.y + 1, area.width, 1),
        ),
        6 => frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  > ", strong.fg(SECONDARY)),
                Span::styled("Add parameter or header…", base),
                Span::styled("       Ctrl+N ", muted),
            ]))
            .style(base),
            Rect::new(area.x, area.y + 1, area.width, 1),
        ),
        7 => {
            let cells = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(45),
                    Constraint::Percentage(45),
                    Constraint::Min(3),
                ])
                .split(Rect::new(area.x, area.y + 1, area.width, 1));
            frame.render_widget(Paragraph::new(" + name").style(muted), cells[0]);
            frame.render_widget(Paragraph::new("value").style(muted), cells[1]);
            frame.render_widget(Paragraph::new("↵").style(strong), cells[2]);
        }
        8 => frame.render_widget(
            Paragraph::new(" + Add ")
                .style(strong.bg(if focused { SECONDARY } else { SURFACE_HIGH }))
                .alignment(Alignment::Center),
            Rect::new(area.x + 2, area.y + 1, 9, 1),
        ),
        9 => frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("▌", strong),
                Span::styled("  + Add entry", base.add_modifier(Modifier::BOLD)),
            ]))
            .style(base),
            Rect::new(area.x, area.y, area.width, 3),
        ),
        10 => frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("＋", strong)),
                Line::from(Span::styled("Add the next entry", muted)),
            ])
            .style(base)
            .alignment(Alignment::Center),
            Rect::new(area.x, area.y, area.width, 3),
        ),
        _ => frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Entries", muted),
                Span::styled("                         + Add  ", strong),
            ]))
            .style(base)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_type(BorderType::Plain)
                    .border_style(Style::default().fg(SECONDARY).bg(bg)),
            ),
            Rect::new(area.x, area.y, area.width, 2),
        ),
    }
}
