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
    widgets::{Block, Borders, Clear, Paragraph},
};

const BACKGROUND: Color = Color::Rgb(13, 17, 27);
const SURFACE: Color = Color::Rgb(24, 30, 44);
const SELECTION: Color = Color::Rgb(41, 51, 74);
const TEXT: Color = Color::Rgb(232, 237, 247);
const MUTED: Color = Color::Rgb(127, 138, 163);
const ERROR: Color = Color::Rgb(255, 107, 129);
const ACCENT: Color = Color::Rgb(95, 215, 215);

const ICONS: [(&str, &str, &str); 18] = [
    ("×", "01  Plain cross", "最常用 · 紧凑稳定"),
    ("╳", "02  Terminal cross", "框线体系 · 当前方案"),
    ("✕︎", "03  Tall cross", "强制文本样式 · 轻量"),
    ("✖︎", "04  Heavy cross", "强制文本样式 · 更醒目"),
    ("⨯", "05  Vector cross", "开放几何 · 技术感"),
    ("⊗", "06  Circled cross", "自带容器 · 语义强"),
    ("⌫", "07  Delete key", "键盘隐喻 · 编辑器常见"),
    ("⌦", "08  Forward delete", "键盘隐喻 · 方向明确"),
    ("−", "09  Minus", "列表移除 · 最克制"),
    ("⊖", "10  Circled minus", "列表移除 · 自带轮廓"),
    ("[×]", "11  Bracket action", "CLI 语言 · 命中明确"),
    ("│×", "12  Gutter action", "表格语言 · 靠右收口"),
    ("▌×", "13  Textual edge", "Textual flat · 品牌一致"),
    ("× 删除", "14  Icon + label", "最易理解 · 占用较宽"),
    ("DEL", "15  Key label", "终端快捷键 · 无图标依赖"),
    ("D", "16  Hotkey", "K9s / Lazygit 式"),
    ("🗑︎", "17  Text wastebasket", "可能受终端字体影响"),
    ("󰆴", "18  Material delete", "需 Nerd Font / MDI"),
];

#[derive(Default)]
struct Gallery {
    selected: usize,
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
                    gallery.selected = gallery.selected.saturating_sub(2)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    gallery.selected = (gallery.selected + 2).min(ICONS.len() - 1)
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    gallery.selected = gallery.selected.saturating_sub(1)
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    gallery.selected = (gallery.selected + 1).min(ICONS.len() - 1)
                }
                _ => {}
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, index)) = gallery
                    .hits
                    .iter()
                    .find(|(area, _)| area.contains((mouse.column, mouse.row).into()))
                {
                    gallery.selected = *index;
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
    let area = frame.area().inner(Margin::new(2, 1));
    if area.width < 72 || area.height < 25 {
        frame.render_widget(
            Paragraph::new("终端至少需要 76 × 27")
                .style(Style::default().fg(ERROR))
                .alignment(Alignment::Center),
            area,
        );
        return;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "DELETE CONTROL / DESIGN LANGUAGE",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "18 种常用图标与组合 · 方向键或鼠标选择 · q / Esc 退出",
                Style::default().fg(MUTED),
            )),
        ]),
        sections[0],
    );

    gallery.hits.clear();
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(sections[1]);
    for column in 0..2 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2); 9])
            .split(columns[column]);
        for (row, cell) in rows.iter().enumerate() {
            let index = row * 2 + column;
            draw_icon(frame, *cell, index, gallery.selected == index);
            gallery.hits.push((*cell, index));
        }
    }

    let (icon, name, note) = ICONS[gallery.selected];
    frame.render_widget(Clear, sections[2]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {icon} "),
                Style::default().fg(ERROR).add_modifier(Modifier::BOLD),
            ),
            Span::styled(name, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {note}"), Style::default().fg(MUTED)),
        ]))
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(SELECTION)),
        ),
        sections[2],
    );
}

fn draw_icon(frame: &mut Frame<'_>, area: Rect, index: usize, selected: bool) {
    let (icon, name, note) = ICONS[index];
    let style = Style::default()
        .bg(if selected { SELECTION } else { SURFACE })
        .fg(TEXT);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {icon}  "),
                Style::default()
                    .fg(ERROR)
                    .bg(if selected { SELECTION } else { SURFACE })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{name:<18}"), style.add_modifier(Modifier::BOLD)),
            Span::styled(note, style.fg(MUTED)),
        ]))
        .style(style),
        area,
    );
}
