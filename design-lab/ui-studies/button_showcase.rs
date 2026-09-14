//! Standalone TUI button showcase using existing dependencies:
//! crossterm + ratatui
//!
//! Run:
//!   cargo run --quiet --example button_showcase
//!
//! Controls:
//! - Tab / ← / →: switch focus
//! - Enter / Space: press focused button
//! - Left click: click button
//! - d: toggle enable/disable focus button
//! - t: toggle state for TOGGLE
//! - r: reset action log
//! - q / Esc: exit

use std::collections::VecDeque;
use std::io::{Stdout, stdout};
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ButtonSkin {
    Primary,
    Soft,
    Outline,
    Ghost,
    Danger,
    Toggle,
    Link,
}

#[derive(Clone, Copy)]
struct ButtonState {
    label: &'static str,
    skin: ButtonSkin,
    enabled: bool,
    checked: bool,
}

struct App {
    buttons: Vec<ButtonState>,
    focused: usize,
    pressed: Option<(usize, Instant)>,
    log: VecDeque<String>,
    button_areas: Vec<Rect>,
}

fn main() -> std::io::Result<()> {
    let mut stdout = stdout();
    enable_raw_mode()?;
    execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        EnableMouseCapture
    )?;

    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;
    let _ = terminal.clear();
    let _ = terminal.show_cursor();

    let mut app = App {
        buttons: vec![
            ButtonState {
                label: "Primary",
                skin: ButtonSkin::Primary,
                enabled: true,
                checked: false,
            },
            ButtonState {
                label: "Soft",
                skin: ButtonSkin::Soft,
                enabled: true,
                checked: false,
            },
            ButtonState {
                label: "Outline",
                skin: ButtonSkin::Outline,
                enabled: true,
                checked: false,
            },
            ButtonState {
                label: "Ghost",
                skin: ButtonSkin::Ghost,
                enabled: true,
                checked: false,
            },
            ButtonState {
                label: "Danger",
                skin: ButtonSkin::Danger,
                enabled: true,
                checked: false,
            },
            ButtonState {
                label: "Toggle",
                skin: ButtonSkin::Toggle,
                enabled: true,
                checked: false,
            },
            ButtonState {
                label: "Link",
                skin: ButtonSkin::Link,
                enabled: true,
                checked: false,
            },
        ],
        focused: 0,
        pressed: None,
        log: VecDeque::new(),
        button_areas: Vec::new(),
    };
    app.push_log("Button showcase started, press Enter or click to test.".to_string());

    let result = run(&mut terminal, &mut app);

    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();

    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<&mut Stdout>>,
    app: &mut App,
) -> std::io::Result<()> {
    if app.buttons.is_empty() {
        return Ok(());
    }

    let tick = Duration::from_millis(16);
    loop {
        let now = Instant::now();
        if let Some((_, started_at)) = app.pressed {
            if now.duration_since(started_at) > Duration::from_millis(160) {
                app.pressed = None;
            }
        }

        terminal.draw(|frame| ui(frame, app))?;

        if event::poll(tick)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                        break;
                    }
                    match key.code {
                        KeyCode::Tab => app.focused = (app.focused + 1) % app.buttons.len(),
                        KeyCode::Left => {
                            app.focused = if app.focused == 0 {
                                app.buttons.len() - 1
                            } else {
                                app.focused - 1
                            }
                        }
                        KeyCode::Right => app.focused = (app.focused + 1) % app.buttons.len(),
                        KeyCode::Enter | KeyCode::Char(' ') => activate(app, app.focused),
                        KeyCode::Char('d') | KeyCode::Char('D') => {
                            app.buttons[app.focused].enabled = !app.buttons[app.focused].enabled;
                            let state = if app.buttons[app.focused].enabled {
                                "enabled"
                            } else {
                                "disabled"
                            };
                            app.push_log(format!("{}: {}", app.buttons[app.focused].label, state));
                        }
                        KeyCode::Char('t') | KeyCode::Char('T') => {
                            if app.buttons[app.focused].skin == ButtonSkin::Toggle {
                                app.buttons[app.focused].checked =
                                    !app.buttons[app.focused].checked;
                                app.push_log(format!(
                                    "{} -> {}",
                                    app.buttons[app.focused].label,
                                    if app.buttons[app.focused].checked {
                                        "ON"
                                    } else {
                                        "OFF"
                                    }
                                ));
                            } else {
                                app.push_log("T: only TOGGLE button uses state.".to_string());
                            }
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            app.log.clear();
                            app.push_log("Action log cleared.".to_string());
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => {
                    if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                        if let Some((index, _)) = app
                            .button_areas
                            .iter()
                            .enumerate()
                            .find(|(_, rect)| contains(**rect, mouse.column, mouse.row))
                        {
                            app.focused = index;
                            activate(app, index);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

struct HeaderInfo {
    focused: &'static str,
    skins: usize,
}

fn ui(frame: &mut ratatui::Frame<'_>, app: &mut App) {
    let size = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(8),
        ])
        .split(size);

    let header = HeaderInfo {
        focused: app.buttons[app.focused].label,
        skins: app.buttons.len(),
    };
    frame.render_widget(header_widget(&header), layout[0]);

    let rows = layout_button_rects(layout[1], app.buttons.len());
    app.button_areas = rows.clone();
    for (index, rect) in rows.iter().enumerate() {
        let button = app.buttons[index];
        let focused = index == app.focused;
        let pressed = app.pressed.is_some_and(|(i, _)| i == index);
        render_button(frame, *rect, button, focused, pressed);
    }

    let mut log_lines = vec![Line::from("Actions:"), Line::from("")];
    log_lines.extend(app.log.iter().map(|line| Line::from(line.as_str())));

    frame.render_widget(
        Paragraph::new(log_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Event Log")
                .border_type(BorderType::Rounded),
        ),
        layout[2],
    );
}

fn header_widget(info: &HeaderInfo) -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::from(format!(
            "Button Style Gallery | Focus: {} | skins: {}",
            info.focused, info.skins
        )),
        Line::from(""),
        Line::from(
            "Tab/←/→: switch • Enter/Space: press • Mouse click • d: disable • t: toggle • r: reset • q/esc: quit",
        ),
    ])
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::White))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Showcase "),
    )
}

fn layout_button_rects(area: Rect, count: usize) -> Vec<Rect> {
    if area.is_empty() || count == 0 {
        return Vec::new();
    }

    let mut constraints = vec![Constraint::Length(4); count];
    constraints.push(Constraint::Min(0));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    chunks
        .iter()
        .take(chunks.len().saturating_sub(1))
        .map(|row| {
            let width = 34.min(row.width.saturating_sub(4)).min(row.width).max(1);
            let x = row.x + row.width.saturating_sub(width) / 2;
            Rect::new(x, row.y + 1, width, row.height.saturating_sub(2).max(1))
        })
        .collect()
}

fn contains(area: Rect, x: u16, y: u16) -> bool {
    let right = area.x.saturating_add(area.width);
    let bottom = area.y.saturating_add(area.height);
    x >= area.x && x < right && y >= area.y && y < bottom
}

impl App {
    fn push_log(&mut self, message: String) {
        self.log.push_front(format!("[{}] {}", now_ms(), message));
        while self.log.len() > 6 {
            self.log.pop_back();
        }
    }
}

fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis())
        % 1_000_000
}

fn activate(app: &mut App, index: usize) {
    if index >= app.buttons.len() {
        return;
    }
    if !app.buttons[index].enabled {
        app.push_log(format!(
            "{} is disabled, ignored.",
            app.buttons[index].label
        ));
        return;
    }

    app.pressed = Some((index, Instant::now()));
    let button = app.buttons[index];

    let state = if button.skin == ButtonSkin::Toggle {
        app.buttons[index].checked = !app.buttons[index].checked;
        if app.buttons[index].checked {
            "ON"
        } else {
            "OFF"
        }
        .to_string()
    } else {
        "clicked".to_string()
    };

    app.push_log(format!(
        "{} [{}] {}",
        button.label,
        style_name(button.skin),
        state
    ));
}

fn style_name(skin: ButtonSkin) -> &'static str {
    match skin {
        ButtonSkin::Primary => "primary",
        ButtonSkin::Soft => "soft",
        ButtonSkin::Outline => "outline",
        ButtonSkin::Ghost => "ghost",
        ButtonSkin::Danger => "danger",
        ButtonSkin::Toggle => "toggle",
        ButtonSkin::Link => "link",
    }
}

#[derive(Clone)]
struct ButtonPaint {
    block_style: Style,
    text_style: Style,
    border_style: Style,
    border_set: BorderType,
    has_border: bool,
    prefix: &'static str,
    suffix: &'static str,
}

fn paint_for(button: ButtonState, focused: bool, pressed: bool) -> ButtonPaint {
    if !button.enabled {
        return ButtonPaint {
            block_style: Style::default().fg(Color::DarkGray).bg(Color::Black),
            text_style: Style::default().fg(Color::DarkGray),
            border_style: Style::default().fg(Color::DarkGray),
            border_set: BorderType::Plain,
            has_border: true,
            prefix: " ! ",
            suffix: " ! ",
        };
    }

    match button.skin {
        ButtonSkin::Primary => {
            let base = if pressed {
                Color::Rgb(220, 236, 220)
            } else if focused {
                Color::Rgb(90, 190, 120)
            } else {
                Color::Rgb(80, 150, 100)
            };
            ButtonPaint {
                block_style: Style::default()
                    .bg(base)
                    .fg(Color::Black)
                    .add_modifier(if focused {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                text_style: Style::default().fg(Color::Black).add_modifier(if focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
                border_style: Style::default().fg(base),
                border_set: BorderType::Rounded,
                has_border: true,
                prefix: "▶",
                suffix: if focused { "◀" } else { "" },
            }
        }
        ButtonSkin::Soft => ButtonPaint {
            block_style: Style::default().bg(Color::Rgb(60, 60, 70)).fg(Color::Gray),
            text_style: Style::default().fg(Color::White).add_modifier(if focused {
                Modifier::BOLD | Modifier::UNDERLINED
            } else {
                Modifier::empty()
            }),
            border_style: Style::default().fg(if focused {
                Color::Rgb(130, 170, 240)
            } else {
                Color::DarkGray
            }),
            border_set: BorderType::Rounded,
            has_border: true,
            prefix: "●",
            suffix: "●",
        },
        ButtonSkin::Outline => ButtonPaint {
            block_style: Style::default().fg(Color::Rgb(170, 220, 255)),
            text_style: Style::default().fg(Color::Rgb(170, 220, 255)),
            border_style: Style::default().fg(if focused {
                Color::Rgb(170, 220, 255)
            } else {
                Color::Rgb(90, 120, 160)
            }),
            border_set: BorderType::Double,
            has_border: true,
            prefix: "[",
            suffix: "]",
        },
        ButtonSkin::Ghost => ButtonPaint {
            block_style: Style::default().fg(Color::Rgb(120, 190, 255)),
            text_style: Style::default().fg(Color::Rgb(120, 190, 255)),
            border_style: Style::default().fg(if focused {
                Color::Rgb(120, 190, 255)
            } else {
                Color::Gray
            }),
            border_set: BorderType::Plain,
            has_border: false,
            prefix: "· ",
            suffix: " ·",
        },
        ButtonSkin::Danger => ButtonPaint {
            block_style: Style::default()
                .bg(if pressed {
                    Color::Rgb(255, 120, 120)
                } else if focused {
                    Color::Rgb(210, 100, 100)
                } else {
                    Color::Black
                })
                .fg(if focused {
                    Color::Black
                } else {
                    Color::Rgb(240, 120, 120)
                })
                .add_modifier(Modifier::BOLD),
            text_style: Style::default().fg(if focused {
                Color::Black
            } else {
                Color::Rgb(240, 120, 120)
            }),
            border_style: Style::default().fg(if focused {
                Color::Rgb(255, 130, 130)
            } else {
                Color::Rgb(200, 90, 90)
            }),
            border_set: BorderType::Thick,
            has_border: true,
            prefix: "!",
            suffix: "!",
        },
        ButtonSkin::Toggle => {
            let checked = if button.checked { "ON" } else { "OFF" };
            ButtonPaint {
                block_style: Style::default().bg(if focused {
                    Color::Rgb(80, 80, 110)
                } else {
                    Color::Rgb(55, 55, 75)
                }),
                text_style: Style::default().fg(if button.checked {
                    Color::Rgb(140, 250, 170)
                } else {
                    Color::Rgb(220, 220, 220)
                }),
                border_style: Style::default().fg(if focused {
                    Color::Rgb(160, 180, 255)
                } else {
                    Color::Rgb(90, 110, 150)
                }),
                border_set: BorderType::Rounded,
                has_border: true,
                prefix: checked,
                suffix: if checked == "ON" { "●" } else { "○" },
            }
        }
        ButtonSkin::Link => ButtonPaint {
            block_style: Style::default(),
            text_style: Style::default().fg(Color::Rgb(160, 200, 255)).add_modifier(
                Modifier::UNDERLINED
                    | if focused {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    },
            ),
            border_style: Style::default().fg(Color::Rgb(80, 120, 200)),
            border_set: BorderType::Plain,
            has_border: false,
            prefix: "↗",
            suffix: "↘",
        },
    }
}

fn render_button(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    button: ButtonState,
    focused: bool,
    pressed: bool,
) {
    let paint = paint_for(button, focused, pressed);

    let mut block = Block::default().style(paint.block_style);
    if paint.has_border {
        block = block
            .borders(Borders::ALL)
            .border_type(paint.border_set)
            .border_style(paint.border_style);
    }

    let toggle_label = if button.skin == ButtonSkin::Toggle {
        format!("[{}] ", if button.checked { "ON" } else { "OFF" })
    } else {
        String::new()
    };

    let label = format!(
        "{} {} {}{}",
        paint.prefix, button.label, toggle_label, paint.suffix
    );

    let mut style = paint.text_style;
    if focused {
        style = style.add_modifier(Modifier::BOLD);
    }
    if pressed {
        style = style.add_modifier(Modifier::REVERSED);
    }
    if !button.enabled {
        style = Style::default().fg(Color::DarkGray);
    }

    let widget = Paragraph::new(Line::from(label))
        .alignment(Alignment::Center)
        .style(style)
        .wrap(Wrap { trim: true })
        .block(block);
    frame.render_widget(widget, area);
}
