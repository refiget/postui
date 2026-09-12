#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
compile_error!("postui 仅支持 Linux amd64 (x86_64) 和 Windows x86_64");

mod app;
mod cache;
mod clipboard;
mod config;
mod editor;
mod highlight;
mod http;
mod i18n;
mod logging;
mod request_executor;
mod request_file;
mod response_output;
mod settings;
mod template;
mod ui;

use std::{
    env,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(windows)]
use std::process::Command;

use anyhow::{Context, Result, bail};
use crossterm::{
    cursor::Show,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{
    app::App, config::load as load_request_config, http::HttpClient,
    request_executor::RequestExecutor,
};

#[cfg(not(windows))]
const INIT_BLOCK_START: &str = "# >>> postui init >>>";
#[cfg(not(windows))]
const INIT_BLOCK_END: &str = "# <<< postui init <<<";

fn main() -> Result<()> {
    let command = parse_args()?;
    match command {
        CliCommand::Help => {
            print_help();
            Ok(())
        }
        CliCommand::Init => init_shell_integration(),
        CliCommand::Run(options) => run_app(options),
    }
}

fn run_app(options: CliOptions) -> Result<()> {
    let project_path = options
        .project_path
        .as_deref()
        .map(resolve_cli_path)
        .or_else(discover_project_path)
        .ok_or_else(|| {
            anyhow::anyhow!("未找到 PostUI 项目；请在包含 .postui 的目录中运行，或传入项目路径")
        })?;
    let workspace_path = project_path.join(".postui");
    let global_config_path = discover_user_config_path();
    let log_base = workspace_path.join("postui.yaml");
    let log_path = options
        .log_file
        .unwrap_or_else(|| default_log_path(&log_base));
    logging::init(options.debug, &log_path)
        .with_context(|| format!("初始化 debug 日志失败: {}", log_path.display()))?;
    tracing::debug!(
        config_path = global_config_path
            .as_deref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<内置界面配置>".to_string()),
        project_path = %project_path.display(),
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
                    "用户界面配置加载失败"
                );
                return Err(error.context(format!("加载用户界面配置失败: {}", path.display())));
            }
        },
        None => settings::default_config(),
    };
    tracing::debug!(
        path = %workspace_path.display(),
        "选择 PostUI 工作区"
    );
    let request_config = match load_request_config(&workspace_path) {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(
                path = %workspace_path.display(),
                error = ?error,
                "PostUI 工作区加载失败"
            );
            return Err(error.context(format!(
                "加载 PostUI 工作区失败: {}",
                workspace_path.display()
            )));
        }
    };
    let http_client = HttpClient::new().context("初始化 HTTP 客户端失败")?;
    let request_executor = RequestExecutor::new(http_client);
    let mut app = App::new(
        request_config,
        workspace_path,
        global_config,
        request_executor,
    );

    let terminal_session = TerminalSession::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).context("创建终端失败")?;
    tracing::debug!("终端界面已初始化");

    let result = run(&mut terminal, &mut app);
    if let Err(error) = &result {
        tracing::error!(error = ?error, "TUI 事件循环异常退出");
    }

    drop(terminal);
    drop(terminal_session);
    tracing::debug!("终端界面已恢复");
    result
}

struct TerminalSession;

impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("启用终端 raw 模式失败")?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
            disable_raw_mode().ok();
            return Err(error).context("初始化终端界面失败");
        }
        Ok(Self)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        disable_raw_mode().ok();
        execute!(
            io::stdout(),
            DisableMouseCapture,
            LeaveAlternateScreen,
            Show
        )
        .ok();
    }
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    tracing::debug!("进入 TUI 事件循环");
    while !app.should_quit {
        app.advance_animation();
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
enum CliCommand {
    Help,
    Init,
    Run(CliOptions),
}

#[derive(Debug)]
struct CliOptions {
    project_path: Option<PathBuf>,
    debug: bool,
    log_file: Option<PathBuf>,
}

fn parse_args() -> Result<CliCommand> {
    parse_args_from(env::args().skip(1))
}

fn parse_args_from(mut args: impl Iterator<Item = String>) -> Result<CliCommand> {
    let mut project = None;
    let mut debug = false;
    let mut log_file = None;
    let mut init = false;

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => return Ok(CliCommand::Help),
            "init" => {
                if init {
                    bail!("postui init 只能指定一次")
                }
                init = true;
            }
            "--debug" => debug = true,
            "--log-file" => {
                let Some(path) = args.next() else {
                    bail!("--log-file 需要一个文件路径")
                };
                if log_file.replace(PathBuf::from(path)).is_some() {
                    bail!("日志文件只能指定一次")
                }
            }
            value if value.starts_with('-') => bail!("未知参数: {value}"),
            path => {
                if project.replace(PathBuf::from(path)).is_some() {
                    bail!("项目路径只能指定一次")
                }
            }
        }
    }

    if log_file.is_some() && !debug {
        bail!("--log-file 只能和 --debug 一起使用")
    }

    if init {
        if project.is_some() {
            bail!("postui init 不接受项目路径")
        }
        if debug {
            bail!("postui init 不接受 --debug")
        }
        if log_file.is_some() {
            bail!("postui init 不接受 --log-file")
        }
        return Ok(CliCommand::Init);
    }

    Ok(CliCommand::Run(CliOptions {
        project_path: project,
        debug,
        log_file,
    }))
}

fn print_help() {
    print!(
        "用法:\n\
  postui [项目目录] [--debug] [--log-file <路径>]\n\
  postui init\n\n\
不传项目目录时，从当前目录向上查找 .postui。项目配置位于 .postui/postui.yaml。\n\
个人语言和主题配置位于用户配置目录的 postui/config.yaml。\n\
默认 debug 日志: .postui/logs/postui-debug.log\n\
--debug 仅在 debug 构建中可用。\n\
postui init 会在 Linux 更新 ~/.zshrc 或 ~/.bashrc；Windows 更新当前用户 PATH。两者都不会写入系统级配置。\n"
    );
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellKind {
    Bash,
    Zsh,
}

#[cfg(not(windows))]
impl ShellKind {
    fn rc_name(self) -> &'static str {
        match self {
            Self::Bash => ".bashrc",
            Self::Zsh => ".zshrc",
        }
    }
}

#[cfg(not(windows))]
fn init_shell_integration() -> Result<()> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("无法确定用户 Home 目录，请设置 HOME 后重试"))?;
    let shell = env::var_os("SHELL");
    let (shell_kind, rc_path) = select_shell_rc(&home, shell.as_deref())?;
    let launch_path = runtime_launch_path()?;
    let block = shell_init_block(&launch_path);
    let current = match fs::read_to_string(&rc_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("无法读取 shell 配置文件: {}", rc_path.display()));
        }
    };
    let updated = upsert_shell_init_block(&current, &block)?;
    if updated != current {
        fs::write(&rc_path, updated)
            .with_context(|| format!("无法写入 shell 配置文件: {}", rc_path.display()))?;
        println!(
            "已将 PostUI 启动命令写入 {} ({})",
            rc_path.display(),
            shell_kind.rc_name()
        );
    } else {
        println!("PostUI 启动命令已存在: {}", rc_path.display());
    }
    println!("请执行 `source {}` 或重新打开终端。", rc_path.display());
    Ok(())
}

#[cfg(windows)]
fn init_shell_integration() -> Result<()> {
    let launch_path = runtime_launch_path()?;
    let launch_directory = launch_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let added = add_user_path(launch_directory)?;

    if added {
        println!(
            "已将 PostUI 所在目录加入当前用户的 PATH: {}",
            launch_directory.display()
        );
    } else {
        println!(
            "PostUI 所在目录已经在当前用户的 PATH 中: {}",
            launch_directory.display()
        );
    }
    println!("请关闭并重新打开 PowerShell，使新的 PATH 生效。无需管理员权限。\n");
    Ok(())
}

#[cfg(windows)]
fn add_user_path(directory: &Path) -> Result<bool> {
    let directory = directory.to_string_lossy().into_owned();
    let current = read_user_path()?;
    if current
        .split(';')
        .any(|entry| same_windows_path(entry, &directory))
    {
        return Ok(false);
    }

    let updated = if current.trim().is_empty() {
        directory
    } else {
        format!("{current};{directory}")
    };
    let output = Command::new(reg_executable())
        .args([
            "ADD",
            r"HKCU\Environment",
            "/v",
            "Path",
            "/t",
            "REG_EXPAND_SZ",
            "/d",
        ])
        .arg(&updated)
        .arg("/f")
        .output()
        .context("无法启动 Windows reg.exe 更新用户 PATH")?;
    if !output.status.success() {
        bail!("更新当前用户 PATH 失败: {}", command_error(&output));
    }
    Ok(true)
}

#[cfg(windows)]
fn read_user_path() -> Result<String> {
    let output = Command::new(reg_executable())
        .args(["QUERY", r"HKCU\Environment", "/v", "Path"])
        .output()
        .context("无法启动 Windows reg.exe 读取用户 PATH")?;
    if !output.status.success() {
        if output.status.code() == Some(1) {
            return Ok(String::new());
        }
        bail!("读取当前用户 PATH 失败: {}", command_error(&output));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let mut fields = line.split_whitespace();
        if !fields
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("Path"))
        {
            continue;
        }
        let Some(value_type) = fields.next() else {
            continue;
        };
        if !matches!(value_type, "REG_SZ" | "REG_EXPAND_SZ") {
            continue;
        }
        let value = fields.collect::<Vec<_>>().join(" ");
        return Ok(value);
    }
    Ok(String::new())
}

#[cfg(windows)]
fn reg_executable() -> PathBuf {
    env::var_os("SystemRoot")
        .map(PathBuf::from)
        .map(|root| root.join("System32/reg.exe"))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("reg.exe"))
}

#[cfg(windows)]
fn command_error(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("进程退出码 {:?}", output.status.code())
    } else {
        stderr
    }
}

#[cfg(windows)]
fn same_windows_path(left: &str, right: &str) -> bool {
    let normalize = |value: &str| {
        value
            .trim()
            .trim_matches('"')
            .trim_end_matches(['\\', '/'])
            .to_ascii_lowercase()
    };
    normalize(left) == normalize(right)
}

#[cfg(not(windows))]
fn select_shell_rc(home: &Path, shell: Option<&OsStr>) -> Result<(ShellKind, PathBuf)> {
    if let Some(shell_kind) = shell.and_then(|value| shell_kind(value.as_ref())) {
        return Ok((shell_kind, home.join(shell_kind.rc_name())));
    }

    let zshrc = home.join(".zshrc");
    let bashrc = home.join(".bashrc");
    match (zshrc.is_file(), bashrc.is_file()) {
        (true, false) => Ok((ShellKind::Zsh, zshrc)),
        (false, true) => Ok((ShellKind::Bash, bashrc)),
        (true, true) => bail!(
            "无法从 SHELL 判断当前 shell，且 ~/.zshrc 与 ~/.bashrc 都存在；请设置 SHELL=/bin/zsh 或 SHELL=/bin/bash 后重试"
        ),
        (false, false) => {
            bail!("无法识别当前 shell；请设置 SHELL=/bin/zsh 或 SHELL=/bin/bash 后重试")
        }
    }
}

#[cfg(not(windows))]
fn shell_kind(path: &Path) -> Option<ShellKind> {
    match path.file_name().and_then(OsStr::to_str) {
        Some("bash") => Some(ShellKind::Bash),
        Some("zsh") => Some(ShellKind::Zsh),
        _ => None,
    }
}

fn runtime_launch_path() -> Result<PathBuf> {
    let executable = env::current_exe().context("无法确定当前运行程序的位置")?;
    #[cfg(not(windows))]
    let executable = fs::canonicalize(&executable).unwrap_or(executable);

    if executable.file_name().and_then(OsStr::to_str) == Some("postui.bin") {
        if let Some(parent) = executable.parent() {
            let wrapper = parent.join("postui");
            if wrapper.is_file() {
                return Ok(fs::canonicalize(&wrapper).unwrap_or(wrapper));
            }
        }
    }

    Ok(executable)
}

#[cfg(not(windows))]
fn shell_init_block(launch_path: &Path) -> String {
    let launch_directory = launch_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let quoted_directory = shell_quote(launch_directory);
    format!(
        "{INIT_BLOCK_START}\ncase \":${{PATH:-}}:\" in\n  *:{quoted_directory}:*) ;;\n  *) export PATH={quoted_directory}${{PATH:+:$PATH}} ;;\nesac\n{INIT_BLOCK_END}\n"
    )
}

#[cfg(not(windows))]
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(not(windows))]
fn upsert_shell_init_block(current: &str, block: &str) -> Result<String> {
    let start_count = current.matches(INIT_BLOCK_START).count();
    let end_count = current.matches(INIT_BLOCK_END).count();
    match (start_count, end_count) {
        (0, 0) => {
            if current.is_empty() {
                Ok(block.to_string())
            } else {
                let mut updated = current.to_string();
                if !updated.ends_with('\n') {
                    updated.push('\n');
                }
                updated.push('\n');
                updated.push_str(block);
                Ok(updated)
            }
        }
        (1, 1) => {
            let start = current
                .find(INIT_BLOCK_START)
                .expect("marker count guarantees a start marker");
            let end = current
                .find(INIT_BLOCK_END)
                .expect("marker count guarantees an end marker")
                + INIT_BLOCK_END.len();
            if start > end {
                bail!("shell 配置中的 PostUI 初始化标记顺序无效")
            }
            let mut updated = String::with_capacity(current.len() + block.len());
            updated.push_str(&current[..start]);
            updated.push_str(block);
            let suffix = current[end..].strip_prefix('\n').unwrap_or(&current[end..]);
            updated.push_str(suffix);
            Ok(updated)
        }
        _ => bail!("shell 配置中的 PostUI 初始化标记不完整或重复，请手动整理后重试"),
    }
}

fn discover_user_config_path() -> Option<PathBuf> {
    let home = user_home_directory();
    let found = config_directory(home.as_deref())
        .map(|directory| directory.join("postui/config.yaml"))
        .filter(|path| path.is_file());
    if let Some(path) = &found {
        tracing::debug!(path = %path.display(), "自动发现用户界面配置");
    }
    found
}

fn find_project_path(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|directory| directory.join(".postui").is_dir())
        .map(Path::to_path_buf)
}

fn user_home_directory() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(PathBuf::from))
    }
    #[cfg(not(windows))]
    {
        env::var_os("HOME").map(PathBuf::from)
    }
}

fn config_directory(home: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
        return Some(path);
    }
    #[cfg(windows)]
    if let Some(path) = env::var_os("APPDATA").map(PathBuf::from) {
        return Some(path);
    }
    home.map(|path| path.join(".config"))
}

fn discover_project_path() -> Option<PathBuf> {
    let current_directory = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let path = find_project_path(&current_directory);
    match path.as_deref() {
        Some(path) => tracing::debug!(
            start = %current_directory.display(),
            path = %path.display(),
            "自动发现 PostUI 项目"
        ),
        None => tracing::debug!(
            start = %current_directory.display(),
            "当前目录及父目录没有 PostUI 项目"
        ),
    }
    path
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
