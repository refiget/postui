use std::{env, io, path::PathBuf};

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Margin},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    i18n::UiText,
    paths::resolve_workspace,
    recent_workspaces::RecentWorkspaces,
    settings::{GlobalConfig, UiTheme},
    terminal::TerminalSession,
};

struct Picker {
    recent: RecentWorkspaces,
    directory: PathBuf,
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
    fn filtered_indices(&self) -> Vec<usize> {
        let query = self.query.to_lowercase();
        self.recent
            .workspaces
            .iter()
            .enumerate()
            .filter(|(_, workspace)| {
                format!("{} {}", workspace.name, workspace.path.display())
                    .to_lowercase()
                    .contains(&query)
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn open_workspace(&mut self, path: PathBuf) -> Option<PathBuf> {
        match resolve_workspace(&path) {
            Ok(workspace) => Some(workspace.path),
            Err(error) => {
                self.notice = format!("{error:#}");
                None
            }
        }
    }
}

pub(crate) fn run(config: &mut GlobalConfig, debug: bool) -> Result<Option<PathBuf>> {
    let text = UiText::new(config.language);
    let mut picker = Picker {
        recent: RecentWorkspaces::load()?,
        directory: env::current_dir().context("Cannot determine current directory")?,
        selected: 0,
        query: String::new(),
        path: String::new(),
        input: None,
        notice: String::new(),
    };
    crossterm::style::force_color_output(true);
    let _session = TerminalSession::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    loop {
        terminal.draw(|frame| draw(frame, &picker, &config.theme, text, debug))?;
        let event = event::read()?;
        if let Event::Paste(value) = event {
            let value = value
                .chars()
                .filter(|character| !character.is_control())
                .collect::<String>();
            match picker.input {
                Some(Input::Search) => {
                    picker.query.push_str(&value);
                    picker.selected = 0;
                }
                Some(Input::Path) => picker.path.push_str(&value),
                None => {}
            }
            continue;
        }
        let Event::Key(key) = event else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(None);
        }
        if debug && key.code == KeyCode::F(5) {
            config.theme = crate::settings::next_theme(&config.theme.name)?;
            continue;
        }
        if picker.input.is_some() {
            match key.code {
                KeyCode::Esc => {
                    picker.input = None;
                    picker.notice.clear();
                }
                KeyCode::Enter => {
                    if matches!(picker.input, Some(Input::Path)) {
                        if picker.path.trim().is_empty() {
                            picker.notice = text.workspace_empty_path().into();
                            continue;
                        }
                        let path = if picker.path == "~"
                            || picker.path.starts_with("~/")
                            || picker.path.starts_with("~\\")
                        {
                            directories::BaseDirs::new()
                                .context("Cannot determine home directory")?
                                .home_dir()
                                .join(picker.path.get(2..).unwrap_or_default())
                        } else {
                            PathBuf::from(&picker.path)
                        };
                        if let Some(path) = picker.open_workspace(path) {
                            return Ok(Some(path));
                        }
                    } else {
                        picker.input = None;
                    }
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
        let filtered = picker.filtered_indices();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            KeyCode::Char('/') => {
                picker.input = Some(Input::Search);
                picker.notice.clear();
            }
            KeyCode::Char('o') => {
                picker.input = Some(Input::Path);
                picker.notice.clear();
            }
            KeyCode::Char('d') => {
                if let Some(index) = filtered.get(picker.selected).copied() {
                    picker.recent.remove(index)?;
                    picker.selected = picker.selected.min(filtered.len().saturating_sub(1));
                    picker.notice.clear();
                }
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                picker.selected = (picker.selected + 1) % (filtered.len() + 1);
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                let count = filtered.len() + 1;
                picker.selected = (picker.selected + count - 1) % count;
            }
            KeyCode::Enter => {
                if let Some(index) = filtered.get(picker.selected) {
                    if let Some(path) =
                        picker.open_workspace(picker.recent.workspaces[*index].path.clone())
                    {
                        return Ok(Some(path));
                    }
                } else {
                    picker.input = Some(Input::Path);
                    picker.notice.clear();
                }
            }
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

fn draw(frame: &mut Frame<'_>, picker: &Picker, theme: &UiTheme, text: UiText, debug: bool) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        area,
    );
    if area.width < 48 || area.height < 20 {
        frame.render_widget(
            Paragraph::new(text.workspace_min_size()).style(Style::default().fg(theme.muted)),
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
            Span::styled(
                format!("  {}", text.workspace_picker_title()),
                Style::default().fg(theme.text),
            ),
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
        Paragraph::new(format!(
            "{}: {}",
            picker.directory.display(),
            text.workspace_not_found()
        ))
        .style(Style::default().fg(theme.muted)),
        rows[0],
    );
    let block = panel(format!(" {} ", text.workspace_recent()), theme, true);
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
    let filtered = picker.filtered_indices();
    let mut items: Vec<ListItem<'_>> = filtered
        .iter()
        .map(|index| {
            let workspace = &picker.recent.workspaces[*index];
            let available = workspace.path.is_dir();
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(workspace.name.as_str(), Style::default().fg(theme.text)),
                    Span::styled(
                        if available {
                            ""
                        } else {
                            text.workspace_path_missing()
                        },
                        Style::default().fg(theme.warning),
                    ),
                ]),
                Line::from(Span::styled(
                    workspace.path.display().to_string(),
                    Style::default().fg(theme.muted),
                )),
                Line::from(""),
            ])
        })
        .collect();
    items.push(ListItem::new(text.workspace_other()));
    let list_area = if filtered.is_empty() {
        frame.render_widget(
            Paragraph::new(if picker.recent.workspaces.is_empty() {
                text.workspace_no_recent()
            } else {
                text.workspace_no_match()
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
            Paragraph::new(format!(" {}▏", picker.path)).block(panel(
                format!(" {} ", text.workspace_directory()),
                theme,
                true,
            )),
            content[2],
        );
    }
    frame.render_widget(
        Paragraph::new(picker.notice.as_str())
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(theme.error)),
        rows[2],
    );
    let hint = if picker.input.is_some() {
        text.workspace_input_hint()
    } else {
        text.workspace_picker_hint()
    };
    let hint = if debug {
        format!("{hint}  {}", text.workspace_theme_hint())
    } else {
        hint.to_string()
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(theme.muted)),
        outer[2],
    );
}
