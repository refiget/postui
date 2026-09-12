use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy)]
struct ClipboardCommand {
    program: &'static str,
    arguments: &'static [&'static str],
}

#[cfg(target_os = "linux")]
const CLIPBOARD_COMMANDS: &[ClipboardCommand] = &[
    ClipboardCommand {
        program: "wl-copy",
        arguments: &[],
    },
    ClipboardCommand {
        program: "xclip",
        arguments: &["-selection", "clipboard"],
    },
    ClipboardCommand {
        program: "xsel",
        arguments: &["--clipboard", "--input"],
    },
];

#[cfg(windows)]
const CLIPBOARD_COMMANDS: &[ClipboardCommand] = &[ClipboardCommand {
    program: "clip",
    arguments: &[],
}];

pub(crate) fn copy_text(text: &str) -> Result<(), String> {
    let mut errors = Vec::new();
    for command in CLIPBOARD_COMMANDS {
        match run_clipboard_command(command, text) {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(format!("{}: {error}", command.program)),
        }
    }

    Err(if errors.is_empty() {
        "当前平台没有可用的剪贴板命令".to_string()
    } else {
        format!("系统剪贴板不可用（{}）", errors.join("；"))
    })
}

fn run_clipboard_command(command: &ClipboardCommand, text: &str) -> Result<(), String> {
    let mut process = Command::new(command.program)
        .args(command.arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;

    let Some(mut stdin) = process.stdin.take() else {
        return Err("无法打开剪贴板输入流".to_string());
    };
    if let Err(error) = stdin.write_all(text.as_bytes()) {
        let _ = process.kill();
        return Err(error.to_string());
    }
    drop(stdin);

    let output = process
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }

    let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if error.is_empty() {
        Err(format!("命令退出码 {}", output.status))
    } else {
        Err(error)
    }
}
