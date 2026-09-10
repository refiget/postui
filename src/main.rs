#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
compile_error!("postui 仅支持 Linux amd64 (x86_64) 和 Windows x86_64");

mod app;
mod cache;
mod clipboard;
mod config;
mod highlight;
mod http;
mod i18n;
mod logging;
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
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{app::App, config::load as load_request_config};

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
    let explicit_global_config = options.config_path.is_some();
    let global_config_path = options
        .config_path
        .as_deref()
        .map(resolve_cli_path)
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
        .as_deref()
        .map(resolve_cli_path)
        .or_else(|| {
            if explicit_global_config {
                None
            } else {
                discover_local_request_config()
            }
        })
        .unwrap_or_else(|| resolve_cli_path(&global_config.request_config));
    tracing::debug!(
        path = %request_config_path.display(),
        explicit_global_config,
        "选择请求配置文件"
    );
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
enum CliCommand {
    Help,
    Init,
    Run(CliOptions),
}

#[derive(Debug)]
struct CliOptions {
    config_path: Option<PathBuf>,
    request_config_path: Option<PathBuf>,
    debug: bool,
    log_file: Option<PathBuf>,
}

fn parse_args() -> Result<CliCommand> {
    let mut args = env::args().skip(1);
    let mut config = None;
    let mut request_config = None;
    let mut debug = false;
    let mut log_file = None;
    let mut init = false;

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => return Ok(CliCommand::Help),
            "init" => init = true,
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

    if init {
        if request_config.is_some() {
            bail!("postui init 不接受 --requests")
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
        config_path: config,
        request_config_path: request_config,
        debug,
        log_file,
    }))
}

fn print_help() {
    print!(
        "用法:\n\
  postui [--config <全局配置>] [--requests <请求配置>] [--debug] [--log-file <路径>]\n\
  postui init\n\n\
全局配置优先级: 显式 --config，其次用户 Home 下的 postui.yaml 或 .postui.yaml，再到平台配置目录；都不存在时使用内置默认配置。\n\
未显式指定 --config 或 --requests 时，优先读取当前目录的 .postui/requests.yaml；否则使用全局配置的 request_config。\n\
请求配置也可以用 --requests 覆盖。\n\
默认 debug 日志: 全局配置所在目录/logs/postui-debug.log\n\
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

fn discover_global_config_path() -> Option<PathBuf> {
    let home = user_home_directory();
    let mut candidates = Vec::new();
    if let Some(home) = home.as_deref() {
        candidates.push(home.join("postui.yaml"));
        candidates.push(home.join(".postui.yaml"));
    }
    #[cfg(not(windows))]
    if home.as_deref() != Some(Path::new("/root")) {
        candidates.push(PathBuf::from("/root/postui.yaml"));
        candidates.push(PathBuf::from("/root/.postui.yaml"));
    }
    let config_home = config_directory(home.as_deref());
    if let Some(config_home) = config_home {
        candidates.push(config_home.join("postui/config.yaml"));
    }
    let found = candidates.into_iter().find(|path| path.is_file());
    if let Some(path) = &found {
        tracing::debug!(path = %path.display(), "自动发现全局配置");
    }
    found
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

fn discover_local_request_config() -> Option<PathBuf> {
    let path = env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".postui/requests.yaml");
    if path.is_file() {
        tracing::debug!(path = %path.display(), "自动发现当前目录请求配置");
        Some(path)
    } else {
        tracing::debug!(path = %path.display(), "当前目录没有请求配置");
        None
    }
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

    #[cfg(unix)]
    #[test]
    fn keeps_absolute_cli_paths() {
        assert_eq!(
            resolve_cli_path(Path::new("/opt/postui/config.yaml")),
            PathBuf::from("/opt/postui/config.yaml")
        );
    }

    #[cfg(windows)]
    #[test]
    fn keeps_absolute_cli_paths() {
        assert_eq!(
            resolve_cli_path(Path::new(r"C:\\PostUI\\config.yaml")),
            PathBuf::from(r"C:\\PostUI\\config.yaml")
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn identifies_supported_shells() {
        use super::{ShellKind, shell_kind};

        assert_eq!(shell_kind(Path::new("/bin/bash")), Some(ShellKind::Bash));
        assert_eq!(shell_kind(Path::new("/usr/bin/zsh")), Some(ShellKind::Zsh));
        assert_eq!(shell_kind(Path::new("/bin/fish")), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn shell_init_block_quotes_paths_and_is_repeatable() {
        use super::{shell_init_block, upsert_shell_init_block};

        let block = shell_init_block(Path::new("/opt/Post UI/bin/o'reilly"));
        assert_eq!(
            block,
            "# >>> postui init >>>\ncase \":${PATH:-}:\" in\n  *:'/opt/Post UI/bin':*) ;;\n  *) export PATH='/opt/Post UI/bin'${PATH:+:$PATH} ;;\nesac\n# <<< postui init <<<\n"
        );
        assert_eq!(upsert_shell_init_block(&block, &block).unwrap(), block);
    }

    #[cfg(not(windows))]
    #[test]
    fn shell_init_block_is_appended_after_existing_config() {
        use super::{shell_init_block, upsert_shell_init_block};

        let block = shell_init_block(Path::new("/opt/postui"));
        assert_eq!(
            upsert_shell_init_block("export EDITOR=vi", &block).unwrap(),
            format!("export EDITOR=vi\n\n{block}")
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn shell_init_block_rejects_incomplete_markers() {
        use super::upsert_shell_init_block;

        let error = upsert_shell_init_block("# >>> postui init >>>\n", "block").unwrap_err();
        assert!(error.to_string().contains("标记不完整"));
    }
}
