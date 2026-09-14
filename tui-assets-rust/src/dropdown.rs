use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    buffer::Buffer,
    layout::{Margin, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, StatefulWidget, Widget},
};

use crate::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropdownItem<'a> {
    pub label: &'a str,
    pub symbol: Option<&'a str>,
}

impl<'a> DropdownItem<'a> {
    pub const fn new(label: &'a str) -> Self {
        Self {
            label,
            symbol: None,
        }
    }

    pub const fn symbol(mut self, symbol: &'a str) -> Self {
        self.symbol = Some(symbol);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropdownEvent {
    None,
    Opened,
    Closed,
    SelectionChanged(usize),
    Selected(usize),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DropdownState {
    value: usize,
    open: bool,
    hovered: Option<usize>,
    list: ListState,
}

impl DropdownState {
    pub const fn selected(&self) -> usize {
        self.value
    }

    pub fn active(&self) -> usize {
        self.list.selected().unwrap_or(self.value)
    }

    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub const fn hovered(&self) -> Option<usize> {
        self.hovered
    }

    pub fn open(&mut self) {
        self.open = true;
        self.list.select(Some(self.value));
    }

    pub fn close(&mut self) {
        self.open = false;
        self.hovered = None;
    }

    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn select(&mut self, selected: usize, item_count: usize) {
        self.value = selected.min(item_count.saturating_sub(1));
        self.list.select((item_count > 0).then_some(self.value));
    }

    pub fn next(&mut self, item_count: usize) {
        if item_count > 0 {
            self.list.select_next();
            if self.active() >= item_count {
                self.list.select_first();
            }
        }
    }

    pub fn previous(&mut self, item_count: usize) {
        if item_count > 0 {
            self.list.select_previous();
            if self.active() >= item_count {
                self.list.select_last();
            }
        }
    }

    pub fn handle_key(&mut self, event: KeyEvent, item_count: usize) -> DropdownEvent {
        if event.kind != KeyEventKind::Press {
            return DropdownEvent::None;
        }
        match event.code {
            KeyCode::Enter | KeyCode::Char(' ') if self.open => {
                self.value = self.active().min(item_count.saturating_sub(1));
                self.close();
                DropdownEvent::Selected(self.value)
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.open();
                DropdownEvent::Opened
            }
            KeyCode::Esc if self.open => {
                self.close();
                DropdownEvent::Closed
            }
            KeyCode::Down if self.open => {
                self.next(item_count);
                DropdownEvent::SelectionChanged(self.active())
            }
            KeyCode::Up if self.open => {
                self.previous(item_count);
                DropdownEvent::SelectionChanged(self.active())
            }
            KeyCode::Home if self.open && item_count > 0 => {
                self.list.select_first();
                DropdownEvent::SelectionChanged(self.active())
            }
            KeyCode::End if self.open && item_count > 0 => {
                self.list.select(Some(item_count - 1));
                DropdownEvent::SelectionChanged(self.active())
            }
            _ => DropdownEvent::None,
        }
    }

    pub fn handle_mouse(
        &mut self,
        event: MouseEvent,
        trigger: Rect,
        menu: Rect,
        item_count: usize,
    ) -> DropdownEvent {
        let position = Position::new(event.column, event.row);
        let trigger_hit = trigger.contains(position);
        let content = menu.inner(Margin::new(1, 1));
        let item = content
            .contains(position)
            .then(|| {
                self.list
                    .offset()
                    .saturating_add(usize::from(event.row.saturating_sub(content.y)))
            })
            .filter(|index| *index < item_count);

        match event.kind {
            MouseEventKind::Moved if self.open => {
                self.hovered = item;
                if let Some(index) = item
                    && index != self.active()
                {
                    self.list.select(Some(index));
                    return DropdownEvent::SelectionChanged(index);
                }
            }
            MouseEventKind::Down(MouseButton::Left) if trigger_hit => {
                self.toggle();
                return if self.open {
                    DropdownEvent::Opened
                } else {
                    DropdownEvent::Closed
                };
            }
            MouseEventKind::Down(MouseButton::Left) if self.open => {
                if let Some(index) = item {
                    self.value = index;
                    self.list.select(Some(index));
                    self.close();
                    return DropdownEvent::Selected(index);
                }
                self.close();
                return DropdownEvent::Closed;
            }
            MouseEventKind::ScrollUp if self.open && content.contains(position) => {
                self.previous(item_count);
                return DropdownEvent::SelectionChanged(self.active());
            }
            MouseEventKind::ScrollDown if self.open && content.contains(position) => {
                self.next(item_count);
                return DropdownEvent::SelectionChanged(self.active());
            }
            _ => {}
        }
        DropdownEvent::None
    }
}

#[derive(Debug, Clone)]
pub struct Dropdown<'a> {
    title: &'a str,
    items: &'a [DropdownItem<'a>],
    theme: Theme,
    border_color: Color,
    highlight_style: Style,
}

impl<'a> Dropdown<'a> {
    pub const fn new(title: &'a str, items: &'a [DropdownItem<'a>], theme: Theme) -> Self {
        Self {
            title,
            items,
            theme,
            border_color: theme.secondary,
            highlight_style: Style::new()
                .bg(theme.selection)
                .fg(theme.secondary)
                .add_modifier(Modifier::BOLD),
        }
    }

    pub const fn border_color(mut self, color: Color) -> Self {
        self.border_color = color;
        self
    }

    pub const fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }
}

impl StatefulWidget for &Dropdown<'_> {
    type State = DropdownState;

    fn render(self, area: Rect, buffer: &mut Buffer, state: &mut Self::State) {
        render_dropdown(self, area, buffer, state);
    }
}

impl StatefulWidget for Dropdown<'_> {
    type State = DropdownState;

    fn render(self, area: Rect, buffer: &mut Buffer, state: &mut Self::State) {
        render_dropdown(&self, area, buffer, state);
    }
}

fn render_dropdown(
    dropdown: &Dropdown<'_>,
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DropdownState,
) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    Clear.render(area, buffer);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(dropdown.border_color))
        .style(
            Style::default()
                .bg(dropdown.theme.surface)
                .fg(dropdown.theme.text),
        );
    let block = if dropdown.title.is_empty() {
        block
    } else {
        block.title(Span::styled(
            format!(" {} ", dropdown.title),
            Style::default()
                .fg(dropdown.border_color)
                .add_modifier(Modifier::BOLD),
        ))
    };
    block.render(area, buffer);

    let list_items = dropdown
        .items
        .iter()
        .map(|item| {
            let line = match item.symbol {
                Some(symbol) => Line::from(format!("{symbol}  {}", item.label)),
                None => Line::from(item.label),
            };
            ListItem::new(line)
        })
        .collect::<Vec<_>>();
    let list = List::new(list_items)
        .style(
            Style::default()
                .bg(dropdown.theme.surface)
                .fg(dropdown.theme.text),
        )
        .highlight_style(dropdown.highlight_style)
        .highlight_symbol("▸ ");
    if dropdown.items.is_empty() {
        state.list.select(None);
    } else if state.active() >= dropdown.items.len() {
        state
            .list
            .select(Some(state.value.min(dropdown.items.len() - 1)));
    }
    StatefulWidget::render(list, area.inner(Margin::new(1, 1)), buffer, &mut state.list);
}

pub fn dropdown_menu_area(screen: Rect, trigger: Rect, item_count: usize, min_width: u16) -> Rect {
    if screen.is_empty() || trigger.is_empty() {
        return Rect::default();
    }
    let width = trigger.width.max(min_width).min(screen.width);
    let height = u16::try_from(item_count.saturating_add(2))
        .unwrap_or(u16::MAX)
        .min(screen.height);
    if width < 3 || height < 3 {
        return Rect::default();
    }
    let x = trigger.x.min(screen.right().saturating_sub(width));
    let below = trigger.bottom();
    let y = if below.saturating_add(height) <= screen.bottom() {
        below
    } else {
        trigger.y.saturating_sub(height)
    };
    Rect::new(x, y.max(screen.y), width, height)
}
