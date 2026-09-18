use std::{env, io, path::PathBuf};

use anyhow::{Context, Result};
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    editor::sanitize_paste,
    i18n::UiText,
    paths::resolve_workspace,
    recent_workspaces::RecentWorkspaces,
    settings::{GlobalConfig, UiTheme},
    terminal::TerminalSession,
};

const WORKSPACE_ITEM_HEIGHT: usize = 3;

struct Picker {
    recent: RecentWorkspaces,
    directory: PathBuf,
    selected: usize,
    list_offset: usize,
    query: String,
    path: String,
    input: Option<Input>,
    notice: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    fn begin_input(&mut self, input: Input) {
        self.input = Some(input);
        self.notice.clear();
    }

    fn append_input(&mut self, value: &str) {
        match self.input {
            Some(Input::Search) => {
                self.query.push_str(value);
                self.reset_list_position();
            }
            Some(Input::Path) => self.path.push_str(value),
            None => {}
        }
    }

    fn push_input(&mut self, character: char) {
        match self.input {
            Some(Input::Search) => {
                self.query.push(character);
                self.reset_list_position();
            }
            Some(Input::Path) => self.path.push(character),
            None => {}
        }
    }

    fn backspace_input(&mut self) {
        match self.input {
            Some(Input::Search) => {
                self.query.pop();
                self.reset_list_position();
            }
            Some(Input::Path) => {
                self.path.pop();
            }
            None => {}
        }
    }

    fn reset_list_position(&mut self) {
        self.selected = 0;
        self.list_offset = 0;
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

    fn submit_path(&mut self, text: UiText) -> Result<Option<PathBuf>> {
        if self.path.trim().is_empty() {
            self.notice = text.workspace_empty_path().into();
            return Ok(None);
        }
        let path =
            if self.path == "~" || self.path.starts_with("~/") || self.path.starts_with("~\\") {
                directories::BaseDirs::new()
                    .context("Cannot determine home directory")?
                    .home_dir()
                    .join(self.path.get(2..).unwrap_or_default())
            } else {
                PathBuf::from(&self.path)
            };
        Ok(self.open_workspace(path))
    }
}

#[derive(Debug, Clone, Copy)]
struct PickerLayout {
    header: Rect,
    directory: Rect,
    recent: Rect,
    notice: Rect,
    footer: Rect,
    search: Rect,
    list_content: Rect,
    list: Rect,
    path: Rect,
}

fn picker_layout(area: Rect, input: Option<Input>, filtered_len: usize) -> Option<PickerLayout> {
    if area.width < 48 || area.height < 20 {
        return None;
    }
    let outer = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(16),
        Constraint::Length(1),
    ])
    .split(area.inner(Margin::new(1, 0)));
    let width = outer[1].width.min(76);
    let card = Rect::new(
        outer[1]
            .x
            .saturating_add(outer[1].width.saturating_sub(width) / 2),
        outer[1]
            .y
            .saturating_add(outer[1].height.saturating_sub(18) / 2),
        width,
        outer[1].height.min(18),
    );
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(12),
        Constraint::Length(2),
    ])
    .split(card);
    let inner = rows[1].inner(Margin::new(2, 1));
    let content = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(if matches!(input, Some(Input::Path)) {
            3
        } else {
            0
        }),
    ])
    .split(inner);
    let list = if filtered_len == 0 {
        Rect::new(
            content[1].x,
            content[1].y.saturating_add(2),
            content[1].width,
            content[1].height.saturating_sub(2),
        )
        .intersection(content[1])
    } else {
        content[1]
    };
    Some(PickerLayout {
        header: outer[0],
        directory: rows[0],
        recent: rows[1],
        notice: rows[2],
        footer: outer[2],
        search: content[0],
        list_content: content[1],
        list,
        path: content[2],
    })
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
}

fn picker_index_at(
    layout: PickerLayout,
    filtered_len: usize,
    list_offset: usize,
    row: u16,
) -> Option<usize> {
    if layout.list.is_empty() || row < layout.list.y || row >= layout.list.bottom() {
        return None;
    }
    let relative = usize::from(row.saturating_sub(layout.list.y));
    let offset = list_offset.min(filtered_len);
    let recent_rows = filtered_len
        .saturating_sub(offset)
        .saturating_mul(WORKSPACE_ITEM_HEIGHT);
    if relative < recent_rows {
        return Some(offset + relative / WORKSPACE_ITEM_HEIGHT);
    }
    (relative == recent_rows).then_some(filtered_len)
}

fn move_selection(picker: &mut Picker, filtered_len: usize, direction: isize) {
    let last = filtered_len;
    let selected = picker.selected.min(last);
    picker.selected = if direction < 0 {
        if selected == 0 { last } else { selected - 1 }
    } else if direction > 0 {
        if selected == last { 0 } else { selected + 1 }
    } else {
        selected
    };
}

fn handle_mouse(picker: &mut Picker, event: MouseEvent, area: Rect) -> Result<Option<PathBuf>> {
    let filtered = picker.filtered_indices();
    let Some(layout) = picker_layout(area, picker.input, filtered.len()) else {
        return Ok(None);
    };
    match event.kind {
        MouseEventKind::Down(MouseButton::Left)
            if contains(layout.search, event.column, event.row) =>
        {
            picker.begin_input(Input::Search);
        }
        MouseEventKind::Down(MouseButton::Left)
            if contains(layout.path, event.column, event.row) =>
        {
            picker.begin_input(Input::Path);
        }
        MouseEventKind::Down(MouseButton::Left)
            if contains(layout.list, event.column, event.row) =>
        {
            picker.input = None;
            if let Some(index) =
                picker_index_at(layout, filtered.len(), picker.list_offset, event.row)
            {
                picker.selected = index;
                if index < filtered.len() {
                    if let Some(path) = picker
                        .open_workspace(picker.recent.workspaces[filtered[index]].path.clone())
                    {
                        return Ok(Some(path));
                    }
                } else {
                    picker.begin_input(Input::Path);
                }
            }
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if contains(layout.list, event.column, event.row) =>
        {
            move_selection(
                picker,
                filtered.len(),
                if matches!(event.kind, MouseEventKind::ScrollUp) {
                    -1
                } else {
                    1
                },
            );
        }
        _ => {}
    }
    Ok(None)
}

pub(crate) fn run(config: &mut GlobalConfig, debug: bool) -> Result<Option<PathBuf>> {
    let text = UiText::new(config.language);
    let mut picker = Picker {
        recent: RecentWorkspaces::load()?,
        directory: env::current_dir().context("Cannot determine current directory")?,
        selected: 0,
        list_offset: 0,
        query: String::new(),
        path: String::new(),
        input: None,
        notice: String::new(),
    };
    crossterm::style::force_color_output(true);
    let _session = TerminalSession::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    loop {
        terminal.draw(|frame| draw(frame, &mut picker, &config.theme, text, debug))?;
        let event = event::read()?;
        if let Event::Paste(value) = event {
            let (value, truncated) = sanitize_paste(&value, false);
            picker.append_input(&value);
            if truncated {
                picker.notice = text.paste_truncated().to_string();
            }
            continue;
        }
        if let Event::Mouse(mouse) = event {
            let size = terminal.size()?;
            let area = Rect::new(0, 0, size.width, size.height);
            if let Some(path) = handle_mouse(&mut picker, mouse, area)? {
                return Ok(Some(path));
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
                        if let Some(path) = picker.submit_path(text)? {
                            return Ok(Some(path));
                        }
                    } else {
                        picker.input = None;
                    }
                }
                KeyCode::Char(character) => picker.push_input(character),
                KeyCode::Backspace => picker.backspace_input(),
                _ => {}
            }
            continue;
        }
        let filtered = picker.filtered_indices();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            KeyCode::Char('/') => {
                picker.begin_input(Input::Search);
            }
            KeyCode::Char('o') => {
                picker.begin_input(Input::Path);
            }
            KeyCode::Char('d') => {
                if let Some(index) = filtered.get(picker.selected).copied() {
                    picker.recent.remove(index)?;
                    picker.selected = picker.selected.min(filtered.len().saturating_sub(1));
                    picker.list_offset = 0;
                    picker.notice.clear();
                }
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                move_selection(&mut picker, filtered.len(), 1);
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                move_selection(&mut picker, filtered.len(), -1);
            }
            KeyCode::Enter => {
                if let Some(index) = filtered.get(picker.selected) {
                    if let Some(path) =
                        picker.open_workspace(picker.recent.workspaces[*index].path.clone())
                    {
                        return Ok(Some(path));
                    }
                } else {
                    picker.begin_input(Input::Path);
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

fn draw(frame: &mut Frame<'_>, picker: &mut Picker, theme: &UiTheme, text: UiText, debug: bool) {
    let area = frame.area();
    let filtered = picker.filtered_indices();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        area,
    );
    let Some(layout) = picker_layout(area, picker.input, filtered.len()) else {
        frame.render_widget(
            Paragraph::new(text.workspace_min_size()).style(Style::default().fg(theme.muted)),
            area,
        );
        return;
    };
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
        layout.header,
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{}: {}",
            picker.directory.display(),
            text.workspace_not_found()
        ))
        .style(Style::default().fg(theme.muted)),
        layout.directory,
    );
    let block = panel(format!(" {} ", text.workspace_recent()), theme, true);
    frame.render_widget(block, layout.recent);
    if !layout.search.is_empty() {
        let value = if picker.query.is_empty() && picker.input.is_none() {
            text.workspace_filter().to_string()
        } else {
            format!(
                "{}{}",
                picker.query,
                if matches!(picker.input, Some(Input::Search)) {
                    "▏"
                } else {
                    ""
                }
            )
        };
        frame.render_widget(
            Paragraph::new(format!("/ {value}")).style(Style::default().fg(
                if picker.input.is_some() || !picker.query.is_empty() {
                    theme.primary
                } else {
                    theme.muted
                },
            )),
            layout.search,
        );
    }
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
    if filtered.is_empty() {
        frame.render_widget(
            Paragraph::new(if picker.recent.workspaces.is_empty() {
                text.workspace_no_recent()
            } else {
                text.workspace_no_match()
            })
            .style(Style::default().fg(theme.muted)),
            layout.list_content,
        );
    }
    let mut state = ListState::default()
        .with_offset(picker.list_offset)
        .with_selected(Some(picker.selected));
    frame.render_stateful_widget(
        List::new(items)
            .style(Style::default().fg(theme.text))
            .highlight_symbol("› ")
            .highlight_style(Style::default().bg(theme.selection)),
        layout.list,
        &mut state,
    );
    picker.list_offset = state.offset();
    if matches!(picker.input, Some(Input::Path)) {
        frame.render_widget(
            Paragraph::new(format!(" {}▏", picker.path)).block(panel(
                format!(" {} ", text.workspace_directory()),
                theme,
                true,
            )),
            layout.path,
        );
    }
    frame.render_widget(
        Paragraph::new(picker.notice.as_str())
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(theme.error)),
        layout.notice,
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
        layout.footer,
    );
}
