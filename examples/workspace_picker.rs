//! Run: cargo run --example workspace_picker
//! F2 switches between recent workspaces and an empty list. All paths are sample data.

use std::io;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use postui_core::settings::UiTheme;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Margin},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

const WORKSPACES: [(&str, &str, bool); 3] = [
    ("订单服务", "/work/orders/.postui", true),
    ("用户服务", "/work/users/.postui", true),
    ("内部工具", "/work/tools/.postui", false),
];

#[derive(Default)]
struct Picker {
    empty: bool,
    selected: usize,
    query: String,
    path: String,
    input: Option<Input>,
    notice: String,
}

enum Input {
    Search,
    Path,
}

impl Picker {
    fn visible(&self) -> Vec<usize> {
        WORKSPACES
            .iter()
            .enumerate()
            .filter(|(_, (name, path, _))| {
                !self.empty
                    && format!("{name} {path}")
                        .to_lowercase()
                        .contains(&self.query.to_lowercase())
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn activate(&mut self) {
        let visible = self.visible();
        if let Some(index) = visible.get(self.selected) {
            let (name, path, available) = WORKSPACES[*index];
            self.notice = if available {
                format!("已选择：{name}")
            } else {
                format!("路径不存在：{path}")
            };
        } else {
            self.input = Some(Input::Path);
            self.notice.clear();
        }
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn main() -> io::Result<()> {
    crossterm::style::force_color_output(true);
    enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut picker = Picker::default();
    let config_path = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .or_else(|| {
            directories::ProjectDirs::from("", "", "postui")
                .map(|dirs| dirs.config_dir().join("config.yaml"))
        });
    let theme = match config_path {
        Some(path) => match postui_core::settings::load(&path) {
            Ok(config) => config.theme,
            Err(error)
                if std::env::args_os().nth(1).is_none()
                    && postui_core::settings::is_not_found(&error) =>
            {
                UiTheme::default()
            }
            Err(error) => return Err(io::Error::other(format!("{error:#}"))),
        },
        None => UiTheme::default(),
    };
    loop {
        terminal.draw(|frame| draw(frame, &picker, &theme))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if picker.input.is_some() {
            match key.code {
                KeyCode::Esc => picker.input = None,
                KeyCode::Enter => {
                    if matches!(picker.input, Some(Input::Path)) {
                        picker.notice = if picker.path.trim().is_empty() {
                            "目录不能为空".into()
                        } else {
                            format!("目录：{}", picker.path)
                        };
                    }
                    picker.input = None;
                }
                KeyCode::Char(character) => match picker.input {
                    Some(Input::Search) => {
                        picker.query.push(character);
                        picker.selected = 0;
                    }
                    Some(Input::Path) => picker.path.push(character),
                    None => unreachable!(),
                },
                KeyCode::Backspace => match picker.input {
                    Some(Input::Search) => {
                        picker.query.pop();
                        picker.selected = 0;
                    }
                    Some(Input::Path) => {
                        picker.path.pop();
                    }
                    None => unreachable!(),
                },
                _ => {}
            }
            continue;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            KeyCode::F(2) => {
                picker.empty = !picker.empty;
                picker.selected = 0;
                picker.query.clear();
                picker.notice.clear();
            }
            KeyCode::Char('/') => picker.input = Some(Input::Search),
            KeyCode::Char('o') => picker.input = Some(Input::Path),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                picker.selected = (picker.selected + 1) % (picker.visible().len() + 1);
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                let count = picker.visible().len() + 1;
                picker.selected = (picker.selected + count - 1) % count;
            }
            KeyCode::Enter => picker.activate(),
            _ => {}
        }
    }
}

fn panel(title: String, theme: &UiTheme, focused: bool) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { theme.primary } else { theme.muted }))
        .style(Style::default().bg(theme.surface).fg(theme.text))
}

fn draw(frame: &mut Frame<'_>, picker: &Picker, theme: &UiTheme) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        area,
    );
    if area.width < 48 || area.height < 20 {
        frame.render_widget(
            Paragraph::new("窗口最小尺寸：48 × 20").style(Style::default().fg(theme.muted)),
            area,
        );
        return;
    }
    let outer = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(16),
        Constraint::Length(1),
    ])
    .split(area.inner(Margin::new(1, 0)));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " POSTUI ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  打开工作区", Style::default().fg(theme.text)),
        ]))
        .block(panel(String::new(), theme, false)),
        outer[0],
    );
    let width = outer[1].width.min(76);
    let card = ratatui::layout::Rect::new(
        outer[1].x + (outer[1].width - width) / 2,
        outer[1].y + outer[1].height.saturating_sub(18) / 2,
        width,
        outer[1].height.min(18),
    );
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(12),
        Constraint::Length(2),
    ])
    .split(card);
    frame.render_widget(
        Paragraph::new("/work：未找到 .postui").style(Style::default().fg(theme.muted)),
        rows[0],
    );
    let block = panel(" 最近工作区 ".into(), theme, true);
    let inner = block.inner(rows[1]).inner(Margin::new(1, 0));
    frame.render_widget(block, rows[1]);
    let content = Layout::vertical([
        Constraint::Length(
            u16::from(matches!(picker.input, Some(Input::Search)) || !picker.query.is_empty()) * 2,
        ),
        Constraint::Min(1),
        Constraint::Length(if matches!(picker.input, Some(Input::Path)) {
            3
        } else {
            0
        }),
    ])
    .split(inner);
    if content[0].height > 0 {
        frame.render_widget(
            Paragraph::new(format!(
                "/ {}{}",
                picker.query,
                if matches!(picker.input, Some(Input::Search)) {
                    "▏"
                } else {
                    ""
                }
            ))
            .style(Style::default().fg(theme.primary)),
            content[0],
        );
    }
    let visible = picker.visible();
    let mut items: Vec<ListItem<'_>> = visible
        .iter()
        .map(|index| {
            let (name, path, available) = WORKSPACES[*index];
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(name, Style::default().fg(theme.text)),
                    Span::styled(
                        if available { "" } else { "  路径不存在" },
                        Style::default().fg(theme.warning),
                    ),
                ]),
                Line::from(Span::styled(path, Style::default().fg(theme.muted))),
                Line::from(""),
            ])
        })
        .collect();
    items.push(ListItem::new("打开其他目录…"));
    let list_area = if visible.is_empty() {
        frame.render_widget(
            Paragraph::new(if picker.empty {
                "暂无记录"
            } else {
                "没有匹配项"
            })
            .style(Style::default().fg(theme.muted)),
            content[1],
        );
        ratatui::layout::Rect {
            y: content[1].y + 2,
            height: content[1].height.saturating_sub(2),
            ..content[1]
        }
    } else {
        content[1]
    };
    frame.render_stateful_widget(
        List::new(items)
            .style(Style::default().fg(theme.text))
            .highlight_symbol("› ")
            .highlight_style(Style::default().bg(theme.selection)),
        list_area,
        &mut ListState::default().with_selected(Some(picker.selected)),
    );
    if matches!(picker.input, Some(Input::Path)) {
        frame.render_widget(
            Paragraph::new(format!(" {}▏", picker.path)).block(panel(" 目录 ".into(), theme, true)),
            content[2],
        );
    }
    frame.render_widget(
        Paragraph::new(picker.notice.as_str()).style(Style::default().fg(theme.text)),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(if picker.input.is_some() {
            " Enter 确认  Esc 返回"
        } else {
            " ↑↓ 选择  Enter 打开  / 筛选  o 输入目录  q 退出"
        })
        .style(Style::default().fg(theme.muted)),
        outer[2],
    );
}
