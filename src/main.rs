#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
compile_error!("postui 仅支持 Linux amd64 (x86_64)");

mod app;
mod clipboard;
mod config;
mod highlight;
mod http;
mod logging;
mod settings;
mod template;
mod ui;

use std::{
    env, io,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{app::App, config::load as load_request_config};

fn main() -> Result<()> {
    let Some(options) = parse_args()? else {
        print_help();
        return Ok(());
    };
    let global_config_path = options
        .config_path
        .clone()
        .or_else(discover_global_config_path);
    let log_base = global_config_path
        .as_deref()
        .unwrap_or_else(|| Path::new("postui.yaml"));
    let log_path = options
        .log_file
        .unwrap_or_else(|| default_log_path(log_base));
    logging::init(options.debug, &log_path)
        .with_context(|| format!("初始化 debug 日志失败: {}", log_path.display()))?;
    tracing::debug!(
        config_path = global_config_path
            .as_deref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<内置默认配置>".to_string()),
        debug = options.debug,
        log_file = %log_path.display(),
        "启动 PostUI"
    );

    let global_config = match global_config_path.as_deref() {
        Some(path) => match settings::load(path) {
            Ok(config) => config,
            Err(error) => {
                tracing::error!(
                    path = %path.display(),
                    error = ?error,
                    "全局配置加载失败"
                );
                return Err(error.context(format!("加载全局配置失败: {}", path.display())));
            }
        },
        None => settings::default_config(),
    };
    let request_config_path = options
        .request_config_path
        .map(|path| resolve_cli_path(&path))
        .unwrap_or_else(|| global_config.request_config.clone());
    let request_config = match load_request_config(&request_config_path) {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(
                path = %request_config_path.display(),
                error = ?error,
                "请求配置加载失败"
            );
            return Err(error.context(format!(
                "加载请求配置失败: {}",
                request_config_path.display()
            )));
        }
    };
    let mut app = App::new(request_config, request_config_path, global_config);

    enable_raw_mode().context("启用终端 raw 模式失败")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).context("初始化终端界面失败")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("创建终端失败")?;
    tracing::debug!("终端界面已初始化");

    let result = run(&mut terminal, &mut app);
    if let Err(error) = &result {
        tracing::error!(error = ?error, "TUI 事件循环异常退出");
    }

    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )
    .ok();
    terminal.show_cursor().ok();
    tracing::debug!("终端界面已恢复");
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    tracing::debug!("进入 TUI 事件循环");
    while !app.should_quit {
        app.poll_messages();
        terminal.draw(|frame| ui::draw(frame, app))?;

        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            match &event {
                Event::Key(key) => tracing::debug!(
                    key_kind = app::key_kind(key.code),
                    modifiers = ?key.modifiers,
                    "收到键盘事件"
                ),
                Event::Mouse(mouse)
                    if matches!(
                        mouse.kind,
                        crossterm::event::MouseEventKind::Down(_)
                            | crossterm::event::MouseEventKind::ScrollUp
                            | crossterm::event::MouseEventKind::ScrollDown
                            | crossterm::event::MouseEventKind::ScrollLeft
                            | crossterm::event::MouseEventKind::ScrollRight
                    ) =>
                {
                    tracing::debug!(
                        kind = ?mouse.kind,
                        column = mouse.column,
                        row = mouse.row,
                        "收到鼠标操作事件"
                    )
                }
                Event::Mouse(_) => {}
                Event::Resize(width, height) => {
                    tracing::debug!(width, height, "收到终端尺寸变化事件")
                }
                _ => tracing::debug!(event = ?event, "收到未处理的终端事件"),
            }
            match event {
                Event::Key(key) => app.handle_key(key),
                Event::Mouse(mouse) => {
                    let size = terminal.size()?;
                    let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
                    ui::handle_mouse(app, mouse, area);
                }
                _ => {}
            }
        }
    }
    tracing::debug!("TUI 事件循环结束");
    Ok(())
}

#[derive(Debug)]
struct CliOptions {
    config_path: Option<PathBuf>,
    request_config_path: Option<PathBuf>,
    debug: bool,
    log_file: Option<PathBuf>,
}

fn parse_args() -> Result<Option<CliOptions>> {
    let mut args = env::args().skip(1);
    let mut config = None;
    let mut request_config = None;
    let mut debug = false;
    let mut log_file = None;

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => return Ok(None),
            "--debug" => debug = true,
            "-c" | "--config" => {
                let Some(path) = args.next() else {
                    bail!("--config 需要一个文件路径")
                };
                config = Some(PathBuf::from(path));
            }
            "-r" | "--requests" => {
                let Some(path) = args.next() else {
                    bail!("--requests 需要一个文件路径")
                };
                request_config = Some(PathBuf::from(path));
            }
            "--log-file" => {
                let Some(path) = args.next() else {
                    bail!("--log-file 需要一个文件路径")
                };
                log_file = Some(PathBuf::from(path));
            }
            value if value.starts_with('-') => bail!("未知参数: {value}"),
            path => config = Some(PathBuf::from(path)),
        }
    }

    if log_file.is_some() && !debug {
        bail!("--log-file 只能和 --debug 一起使用")
    }

    Ok(Some(CliOptions {
        config_path: config,
        request_config_path: request_config,
        debug,
        log_file,
    }))
}

fn print_help() {
    print!(
        "用法: postui [--config <全局配置>] [--requests <请求配置>] [--debug] [--log-file <路径>]\n\n\
全局配置优先级: 显式 --config，其次 Home/root 下的 postui.yaml 或 .postui.yaml，最后 ~/.config/postui/config.yaml；都不存在时使用内置默认配置。\n\
请求配置默认由全局配置的 request_config 指定，也可以用 --requests 覆盖。\n\
默认 debug 日志: 全局配置所在目录/logs/postui-debug.log\n\
--debug 仅在 debug 构建中可用\n"
    );
}

fn discover_global_config_path() -> Option<PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from);
    let mut candidates = Vec::new();
    if let Some(home) = home.as_deref() {
        candidates.push(home.join("postui.yaml"));
        candidates.push(home.join(".postui.yaml"));
    }
    if home.as_deref() != Some(Path::new("/root")) {
        candidates.push(PathBuf::from("/root/postui.yaml"));
        candidates.push(PathBuf::from("/root/.postui.yaml"));
    }
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_deref().map(|path| path.join(".config")));
    if let Some(config_home) = config_home {
        candidates.push(config_home.join("postui/config.yaml"));
    }

    let found = candidates.into_iter().find(|path| path.is_file());
    if let Some(path) = &found {
        tracing::debug!(path = %path.display(), "自动发现全局配置");
    }
    found
}

fn resolve_cli_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn default_log_path(config_path: &Path) -> PathBuf {
    let directory = config_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    directory.join("logs/postui-debug.log")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::resolve_cli_path;

    #[test]
    fn keeps_absolute_cli_paths() {
        assert_eq!(
            resolve_cli_path(Path::new("/opt/postui/config.yaml")),
            PathBuf::from("/opt/postui/config.yaml")
        );
    }
}
