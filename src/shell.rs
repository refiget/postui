#[cfg(not(windows))]
use std::io;
#[cfg(windows)]
use std::process::Command;
use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

#[cfg(not(windows))]
const INIT_BLOCK_START: &str = "# >>> postui init >>>";
#[cfg(not(windows))]
const INIT_BLOCK_END: &str = "# <<< postui init <<<";

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
pub(crate) fn init_shell_integration() -> Result<()> {
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
pub(crate) fn init_shell_integration() -> Result<()> {
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
