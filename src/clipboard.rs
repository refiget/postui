use std::{
    io::Write,
    process::{Command, Stdio},
};

use arboard::Clipboard;

#[derive(Default)]
pub(crate) struct SystemClipboard {
    clipboard: Option<Clipboard>,
}

#[derive(Clone, Copy, Debug)]
enum CommandClipboard {
    Wayland,
    Xclip,
    Xsel,
}

impl CommandClipboard {
    fn copy_command(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Wayland => ("wl-copy", &["--type", "text/plain;charset=utf-8"]),
            Self::Xclip => ("xclip", &["-selection", "clipboard", "-in"]),
            Self::Xsel => ("xsel", &["--clipboard", "--input"]),
        }
    }

    fn paste_command(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Wayland => (
                "wl-paste",
                &["--no-newline", "--type", "text/plain;charset=utf-8"],
            ),
            Self::Xclip => ("xclip", &["-selection", "clipboard", "-out"]),
            Self::Xsel => ("xsel", &["--clipboard", "--output"]),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Wayland => "wl-clipboard",
            Self::Xclip => "xclip",
            Self::Xsel => "xsel",
        }
    }
}

impl SystemClipboard {
    pub(crate) fn set_text(&mut self, value: &str) -> Result<(), String> {
        tracing::debug!(
            value_bytes = value.len(),
            session = %desktop_session(),
            "写入系统剪贴板"
        );
        match self.set_text_native(value) {
            Ok(()) => {
                tracing::debug!(backend = "arboard", "系统剪贴板写入完成");
                Ok(())
            }
            Err(native_error) => {
                tracing::warn!(error = %native_error, "原生剪贴板写入失败，尝试命令行回退");
                self.set_text_command(value).map_err(|command_error| {
                    let error = clipboard_error(&native_error, &command_error);
                    tracing::error!(error = %error, "系统剪贴板写入失败");
                    error
                })
            }
        }
    }

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

    fn set_text_native(&mut self, value: &str) -> Result<(), String> {
        self.get_or_init()?
            .set_text(value)
            .map_err(|error| format!("原生剪贴板写入失败: {error}"))
    }

    fn get_text_native(&mut self) -> Result<String, String> {
        self.get_or_init()?
            .get_text()
            .map_err(|error| format!("原生剪贴板读取失败: {error}"))
    }

    fn set_text_command(&self, value: &str) -> Result<(), String> {
        let mut errors = Vec::new();
        for backend in command_backends() {
            let (program, arguments) = backend.copy_command();
            match write_command(program, arguments, value) {
                Ok(()) => {
                    tracing::debug!(backend = backend.label(), "命令行剪贴板写入完成");
                    return Ok(());
                }
                Err(error) => {
                    tracing::debug!(backend = backend.label(), error = %error, "命令行剪贴板写入不可用");
                    errors.push(format!("{}: {error}", backend.label()));
                }
            }
        }
        Err(command_error_message(errors))
    }

    fn get_text_command(&self) -> Result<String, String> {
        let mut errors = Vec::new();
        for backend in command_backends() {
            let (program, arguments) = backend.paste_command();
            match read_command(program, arguments) {
                Ok(value) => {
                    tracing::debug!(
                        backend = backend.label(),
                        value_bytes = value.len(),
                        "命令行剪贴板读取完成"
                    );
                    return Ok(value);
                }
                Err(error) => {
                    tracing::debug!(backend = backend.label(), error = %error, "命令行剪贴板读取不可用");
                    errors.push(format!("{}: {error}", backend.label()));
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

fn desktop_session() -> &'static str {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        "wayland"
    } else if std::env::var_os("DISPLAY").is_some() {
        "x11"
    } else {
        "unknown"
    }
}

fn write_command(program: &str, arguments: &[&str], value: &str) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("无法启动 {program}: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("无法打开 {program} 的标准输入"))?;
    stdin
        .write_all(value.as_bytes())
        .map_err(|error| format!("写入 {program} 失败: {error}"))?;
    drop(stdin);

    let output = child
        .wait_with_output()
        .map_err(|error| format!("等待 {program} 结束失败: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_status_error(program, &output.stderr))
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
    format!(
        "命令行剪贴板不可用: {attempts}。麒麟/统信 Wayland 请安装 wl-clipboard；X11 请安装 xclip 或 xsel"
    )
}

fn clipboard_error(native_error: &str, command_error: &str) -> String {
    format!("系统剪贴板不可用: {native_error}；{command_error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_clipboard_installation_guidance() {
        let message = command_error_message(vec!["xclip: 未安装".to_string()]);
        assert!(message.contains("wl-clipboard"));
        assert!(message.contains("xclip"));
        assert!(message.contains("xsel"));
    }

    #[test]
    fn formats_command_failure_without_stderr() {
        assert_eq!(command_status_error("xclip", b"\n"), "xclip 退出状态异常");
    }
}
