use std::process::Command;

use arboard::Clipboard;

#[derive(Default)]
pub(crate) struct SystemClipboard {
    clipboard: Option<Clipboard>,
}

#[derive(Clone, Copy, Debug)]
enum CommandClipboard {
    #[cfg(not(windows))]
    Wayland,
    #[cfg(not(windows))]
    Xclip,
    #[cfg(not(windows))]
    Xsel,
    #[cfg(windows)]
    PowerShell,
}

impl CommandClipboard {
    fn command(self) -> (&'static str, &'static [&'static str], &'static str) {
        match self {
            #[cfg(not(windows))]
            Self::Wayland => (
                "wl-paste",
                &["--no-newline", "--type", "text/plain;charset=utf-8"],
                "wl-clipboard",
            ),
            #[cfg(not(windows))]
            Self::Xclip => ("xclip", &["-selection", "clipboard", "-out"], "xclip"),
            #[cfg(not(windows))]
            Self::Xsel => ("xsel", &["--clipboard", "--output"], "xsel"),
            #[cfg(windows)]
            Self::PowerShell => (
                "powershell.exe",
                &[
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "[Console]::OutputEncoding = [Text.Encoding]::UTF8; [Console]::Out.Write((Get-Clipboard -Raw))",
                ],
                "PowerShell",
            ),
        }
    }
}

impl SystemClipboard {
    pub(crate) fn get_text(&mut self) -> Result<String, String> {
        tracing::debug!(session = %desktop_session(), "读取系统剪贴板");
        match self.get_text_native() {
            Ok(value) => {
                tracing::debug!(
                    value_bytes = value.len(),
                    backend = "arboard",
                    "系统剪贴板读取完成"
                );
                Ok(value)
            }
            Err(native_error) => {
                tracing::warn!(error = %native_error, "原生剪贴板读取失败，尝试命令行回退");
                self.get_text_command().map_err(|command_error| {
                    let error = clipboard_error(&native_error, &command_error);
                    tracing::error!(error = %error, "系统剪贴板读取失败");
                    error
                })
            }
        }
    }

    fn get_text_native(&mut self) -> Result<String, String> {
        self.get_or_init()?
            .get_text()
            .map_err(|error| format!("原生剪贴板读取失败: {error}"))
    }

    fn get_text_command(&self) -> Result<String, String> {
        let mut errors = Vec::new();
        for backend in command_backends() {
            let (program, arguments, label) = backend.command();
            match read_command(program, arguments) {
                Ok(value) => {
                    tracing::debug!(
                        backend = label,
                        value_bytes = value.len(),
                        "命令行剪贴板读取完成"
                    );
                    return Ok(value);
                }
                Err(error) => {
                    tracing::debug!(backend = label, error = %error, "命令行剪贴板读取不可用");
                    errors.push(format!("{label}: {error}"));
                }
            }
        }
        Err(command_error_message(errors))
    }

    fn get_or_init(&mut self) -> Result<&mut Clipboard, String> {
        if self.clipboard.is_none() {
            tracing::debug!(session = %desktop_session(), "初始化原生剪贴板连接");
            self.clipboard = Some(Clipboard::new().map_err(|error| {
                let message = format!("初始化原生剪贴板失败: {error}");
                tracing::debug!(error = %message, "原生剪贴板不可用");
                message
            })?);
        }
        self.clipboard
            .as_mut()
            .ok_or_else(|| "原生剪贴板不可用".to_string())
    }
}

fn command_backends() -> Vec<CommandClipboard> {
    #[cfg(windows)]
    {
        vec![CommandClipboard::PowerShell]
    }

    #[cfg(not(windows))]
    {
        let mut backends = Vec::with_capacity(3);
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            backends.push(CommandClipboard::Wayland);
        }
        if std::env::var_os("DISPLAY").is_some() {
            backends.extend([CommandClipboard::Xclip, CommandClipboard::Xsel]);
        }
        if backends.is_empty() {
            backends.extend([
                CommandClipboard::Wayland,
                CommandClipboard::Xclip,
                CommandClipboard::Xsel,
            ]);
        }
        backends
    }
}

fn desktop_session() -> &'static str {
    #[cfg(windows)]
    {
        "windows"
    }

    #[cfg(not(windows))]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            "wayland"
        } else if std::env::var_os("DISPLAY").is_some() {
            "x11"
        } else {
            "unknown"
        }
    }
}

fn read_command(program: &str, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| format!("无法启动 {program}: {error}"))?;
    if !output.status.success() {
        return Err(command_status_error(program, &output.stderr));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("{program} 返回的剪贴板不是 UTF-8 文本: {error}"))
}

fn command_status_error(program: &str, stderr: &[u8]) -> String {
    let detail = String::from_utf8_lossy(stderr).trim().to_string();
    if detail.is_empty() {
        format!("{program} 退出状态异常")
    } else {
        format!("{program} 执行失败: {detail}")
    }
}

fn command_error_message(errors: Vec<String>) -> String {
    let attempts = if errors.is_empty() {
        "未找到可用命令".to_string()
    } else {
        errors.join("；")
    };
    #[cfg(windows)]
    {
        format!("命令行剪贴板不可用: {attempts}。Windows 10 请确认 PowerShell 可用")
    }
    #[cfg(not(windows))]
    {
        format!(
            "命令行剪贴板不可用: {attempts}。麒麟/统信 Wayland 请安装 wl-clipboard；X11 请安装 xclip 或 xsel"
        )
    }
}

fn clipboard_error(native_error: &str, command_error: &str) -> String {
    format!("系统剪贴板不可用: {native_error}；{command_error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn reports_clipboard_installation_guidance() {
        let message = command_error_message(vec!["xclip: 未安装".to_string()]);
        assert!(message.contains("wl-clipboard"));
        assert!(message.contains("xclip"));
        assert!(message.contains("xsel"));
    }

    #[cfg(windows)]
    #[test]
    fn reports_windows_clipboard_installation_guidance() {
        let message = command_error_message(vec!["PowerShell: 未安装".to_string()]);
        assert!(message.contains("PowerShell"));
    }

    #[test]
    fn formats_command_failure_without_stderr() {
        assert_eq!(command_status_error("xclip", b"\n"), "xclip 退出状态异常");
    }
}
