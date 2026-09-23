use crate::{
    app::{self, App},
    ui,
};
use anyhow::{Context, Result};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event,
    },
    execute,
    style::force_color_output,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io,
    time::{Duration, Instant},
};

pub(crate) fn run_app(app: &mut App) -> Result<()> {
    force_color_output(true);
    let mut terminal_session = TerminalSession::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).context("创建终端失败")?;
    tracing::debug!("终端界面已初始化");

    let result = run(&mut terminal_session, &mut terminal, app);
    if let Err(error) = &result {
        tracing::error!(error = ?error, "TUI 事件循环异常退出");
    }

    drop(terminal);
    drop(terminal_session);
    tracing::debug!("终端界面已恢复");
    result
}

pub(crate) struct TerminalSession;

impl TerminalSession {
    pub(crate) fn enter() -> Result<Self> {
        enable_raw_mode().context("启用终端 raw 模式失败")?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        ) {
            disable_raw_mode().ok();
            return Err(error).context("初始化终端界面失败");
        }
        Ok(Self)
    }

    fn suspend(&mut self) -> Result<()> {
        disable_raw_mode().context("Disable terminal raw mode failed")?;
        execute!(
            io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen,
            Show
        )
        .context("Leave TUI screen failed")?;
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        enable_raw_mode().context("Enable terminal raw mode failed")?;
        if let Err(error) = execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        ) {
            disable_raw_mode().ok();
            return Err(error).context("Restore TUI screen failed");
        }
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        disable_raw_mode().ok();
        execute!(
            io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen,
            Show
        )
        .ok();
    }
}

fn run(
    terminal_session: &mut TerminalSession,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    const MAX_EVENT_BATCH: usize = 32;
    const ANIMATION_INTERVAL: Duration = Duration::from_millis(100);

    tracing::debug!("进入 TUI 事件循环");
    let mut redraw = true;
    let mut next_animation = Instant::now();
    let mut frame_metrics = (Instant::now(), 0_u64, 0_u64, 0_u64);
    while !app.should_quit {
        redraw |= app.poll_request_results();
        redraw |= app.poll_response_actions();
        redraw |= app.poll_response_search();
        redraw |= app.poll_workspace_reload();
        redraw |= app.poll_curl_import();
        redraw |= crate::highlight::take_response_highlight_change();

        let now = Instant::now();
        let animating = app.is_animating();
        if animating && now >= next_animation {
            app.advance_animation();
            next_animation = now + ANIMATION_INTERVAL;
            redraw = true;
        } else if !animating {
            next_animation = now;
        }

        if redraw {
            let started = Instant::now();
            terminal.draw(|frame| ui::draw(frame, app))?;
            if tracing::enabled!(target: "postui::perf", tracing::Level::DEBUG) {
                let elapsed_us = started.elapsed().as_micros() as u64;
                frame_metrics.1 += 1;
                frame_metrics.2 += elapsed_us;
                frame_metrics.3 = frame_metrics.3.max(elapsed_us);
                if elapsed_us >= 16_000 {
                    tracing::debug!(target: "postui::perf", elapsed_us, "ui_slow_frame");
                }
                if frame_metrics.0.elapsed() >= Duration::from_secs(1) {
                    tracing::debug!(target: "postui::perf", frames = frame_metrics.1,
                        window_ms = frame_metrics.0.elapsed().as_millis() as u64,
                        mean_us = frame_metrics.2 / frame_metrics.1,
                        max_us = frame_metrics.3, "ui_frames");
                    frame_metrics = (Instant::now(), 0, 0, 0);
                }
            }
            redraw = false;
        }

        let poll_timeout = if animating {
            next_animation
                .saturating_duration_since(Instant::now())
                .min(ANIMATION_INTERVAL)
        } else {
            ANIMATION_INTERVAL
        };
        let poll_timeout = if app.has_pending_background_work() {
            poll_timeout.min(Duration::from_millis(16))
        } else {
            poll_timeout
        };
        if event::poll(poll_timeout)? {
            for _ in 0..MAX_EVENT_BATCH {
                redraw |= handle_terminal_event(terminal_session, terminal, app, event::read()?)?;
                if app.should_quit || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
    }
    tracing::debug!("TUI 事件循环结束");
    Ok(())
}

fn handle_terminal_event(
    terminal_session: &mut TerminalSession,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    event: Event,
) -> Result<bool> {
    let started = Instant::now();
    let event_kind = match &event {
        Event::Key(_) => "key",
        Event::Mouse(_) => "mouse",
        Event::Resize(_, _) => "resize",
        _ => "other",
    };
    let redraw = match event {
        Event::Key(key) => {
            app.view.cancel_scroll_drag();
            tracing::trace!(
                key_kind = app::key_kind(key.code),
                modifiers = ?key.modifiers,
                "收到键盘事件"
            );
            app.handle_key(key);
            if let Some(request) = app.take_editor_request() {
                terminal_session.suspend()?;
                let editor_result = crate::editor::open_file(&request.path);
                let resume_result = terminal_session.resume();
                app.finish_editor(&request, editor_result);
                resume_result?;
                terminal.clear()?;
            }
            true
        }
        Event::Mouse(mouse) => {
            tracing::trace!(
                kind = ?mouse.kind,
                column = mouse.column,
                row = mouse.row,
                "收到鼠标事件"
            );
            let size = terminal.size()?;
            let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
            let redraw = matches!(
                mouse.kind,
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
                    | crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left)
                    | crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left)
                    | crossterm::event::MouseEventKind::ScrollUp
                    | crossterm::event::MouseEventKind::ScrollDown
            ) || matches!(mouse.kind, crossterm::event::MouseEventKind::Moved);
            ui::handle_mouse(app, mouse, area);
            redraw
        }
        Event::Resize(width, height) => {
            app.view.cancel_scroll_drag();
            tracing::debug!(width, height, "终端尺寸变化");
            true
        }
        Event::Paste(value) => {
            app.handle_paste(&value);
            true
        }
        _ => false,
    };
    let elapsed_us = started.elapsed().as_micros() as u64;
    if elapsed_us >= 4_000 {
        tracing::debug!(target: "postui::perf", event_kind, elapsed_us, "ui_slow_event");
    }
    Ok(redraw)
}
