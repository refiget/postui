use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Disabled,
    Idle,
    Hovered,
    Focused,
    Pressed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEvent {
    None,
    Clicked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonInteraction {
    enabled: bool,
    focused: bool,
    hovered: bool,
    pressed: bool,
}

impl Default for ButtonInteraction {
    fn default() -> Self {
        Self {
            enabled: true,
            focused: false,
            hovered: false,
            pressed: false,
        }
    }
}

impl ButtonInteraction {
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub const fn focused(&self) -> bool {
        self.focused
    }

    pub const fn hovered(&self) -> bool {
        self.hovered
    }

    pub const fn pressed(&self) -> bool {
        self.pressed
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.hovered = false;
            self.pressed = false;
        }
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub const fn visual_state(&self) -> ButtonState {
        if !self.enabled {
            ButtonState::Disabled
        } else if self.pressed {
            ButtonState::Pressed
        } else if self.focused {
            ButtonState::Focused
        } else if self.hovered {
            ButtonState::Hovered
        } else {
            ButtonState::Idle
        }
    }

    pub fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> ButtonEvent {
        let inside = area.contains(Position::new(event.column, event.row));
        match event.kind {
            MouseEventKind::Moved => self.hovered = self.enabled && inside,
            MouseEventKind::Down(MouseButton::Left) => {
                self.hovered = self.enabled && inside;
                self.pressed = self.enabled && inside;
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                self.hovered = self.enabled && inside;
                if !inside {
                    self.pressed = false;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let clicked = self.enabled && self.pressed && inside;
                self.pressed = false;
                self.hovered = self.enabled && inside;
                if clicked {
                    return ButtonEvent::Clicked;
                }
            }
            _ => {}
        }
        ButtonEvent::None
    }

    pub fn handle_key(&self, event: KeyEvent) -> ButtonEvent {
        if self.enabled
            && self.focused
            && event.kind == KeyEventKind::Press
            && matches!(event.code, KeyCode::Enter | KeyCode::Char(' '))
        {
            ButtonEvent::Clicked
        } else {
            ButtonEvent::None
        }
    }
}

impl ButtonState {
    pub const fn new(enabled: bool, focused: bool) -> Self {
        match (enabled, focused) {
            (false, _) => Self::Disabled,
            (true, true) => Self::Focused,
            (true, false) => Self::Idle,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Button<'a> {
    label: &'a str,
    state: ButtonState,
    color: Color,
    theme: Theme,
    alignment: Alignment,
}

impl<'a> Button<'a> {
    pub const fn new(label: &'a str, theme: Theme) -> Self {
        Self {
            label,
            state: ButtonState::Idle,
            color: theme.accent,
            theme,
            alignment: Alignment::Center,
        }
    }

    pub const fn state(mut self, state: ButtonState) -> Self {
        self.state = state;
        self
    }

    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub const fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
}

impl Widget for &Button<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        render_button(self, area, buffer);
    }
}

impl Widget for Button<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        render_button(&self, area, buffer);
    }
}

fn render_button(button: &Button<'_>, area: Rect, buffer: &mut Buffer) {
    if area.is_empty() {
        return;
    }
    let line = match button.state {
        ButtonState::Disabled => Line::from(vec![
            Span::styled("│", Style::default().fg(button.theme.muted)),
            Span::styled(
                format!(" {} ", button.label),
                Style::default()
                    .fg(button.theme.muted)
                    .underline_color(button.theme.selection)
                    .add_modifier(Modifier::DIM | Modifier::UNDERLINED),
            ),
        ]),
        ButtonState::Idle | ButtonState::Hovered | ButtonState::Focused | ButtonState::Pressed => {
            let fill = match button.state {
                ButtonState::Focused => button.theme.text,
                ButtonState::Hovered => blend_rgb(button.theme.text, button.color, 25),
                ButtonState::Pressed => blend_rgb(button.color, button.theme.background, 75),
                ButtonState::Idle | ButtonState::Disabled => button.color,
            };
            let edge = blend_rgb(button.color, button.theme.surface, 65);
            let style = Style::default()
                .fg(button.theme.background)
                .bg(fill)
                .underline_color(edge)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            Line::from(vec![
                Span::styled("▌", Style::default().fg(edge).bg(fill)),
                Span::styled(format!(" {} ", button.label), style),
            ])
        }
    };
    Paragraph::new(line)
        .alignment(button.alignment)
        .render(area, buffer);
}

pub fn blend_rgb(foreground: Color, background: Color, foreground_percent: u16) -> Color {
    let (Color::Rgb(fr, fg, fb), Color::Rgb(br, bg, bb)) = (foreground, background) else {
        return foreground;
    };
    let background_percent = 100 - foreground_percent;
    let blend = |front: u8, back: u8| {
        ((u16::from(front) * foreground_percent + u16::from(back) * background_percent) / 100) as u8
    };
    Color::Rgb(blend(fr, br), blend(fg, bg), blend(fb, bb))
}
