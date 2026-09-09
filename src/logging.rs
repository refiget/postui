use std::path::Path;

#[cfg(debug_assertions)]
use std::path::PathBuf;

use anyhow::Result;

#[cfg(debug_assertions)]
const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;

// Release 构建不编译详细日志写入器，避免正式二进制产生 debug 日志。
pub(crate) fn init(enabled: bool, log_path: &Path) -> Result<()> {
    if !enabled {
        return Ok(());
    }

    #[cfg(debug_assertions)]
    {
        init_debug(log_path)
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = log_path;
        anyhow::bail!("--debug 仅在 debug 构建中可用");
    }
}

#[cfg(debug_assertions)]
fn init_debug(log_path: &Path) -> Result<()> {
    use anyhow::{Context, anyhow};

    let writer = RotatingMakeWriter::new(log_path, MAX_LOG_BYTES)
        .with_context(|| format!("创建 debug 日志文件失败: {}", log_path.display()))?;

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .with_ansi(false)
        .with_writer(writer)
        .try_init()
        .map_err(|error| anyhow!("注册 debug 日志订阅器失败: {error}"))?;

    tracing::debug!(
        log_file = %log_path.display(),
        max_bytes = MAX_LOG_BYTES,
        "debug 日志已启用"
    );
    Ok(())
}

#[cfg(debug_assertions)]
#[derive(Clone)]
struct RotatingMakeWriter {
    state: std::sync::Arc<std::sync::Mutex<LogFileState>>,
}

#[cfg(debug_assertions)]
struct LogFileState {
    path: PathBuf,
    backup_path: PathBuf,
    file: std::fs::File,
    bytes_written: u64,
    max_bytes: u64,
}

#[cfg(debug_assertions)]
impl RotatingMakeWriter {
    fn new(path: &Path, max_bytes: u64) -> std::io::Result<Self> {
        if path.as_os_str().is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "日志路径不能为空",
            ));
        }
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }

        let backup_path = backup_path(path);
        let mut file = open_log_file(path)?;
        let mut bytes_written = file.metadata()?.len();
        if bytes_written >= max_bytes {
            rotate_file(path, &backup_path)?;
            file = open_log_file(path)?;
            bytes_written = 0;
        }

        Ok(Self {
            state: std::sync::Arc::new(std::sync::Mutex::new(LogFileState {
                path: path.to_path_buf(),
                backup_path,
                file,
                bytes_written,
                max_bytes,
            })),
        })
    }
}

#[cfg(debug_assertions)]
struct LogWriter {
    state: std::sync::Arc<std::sync::Mutex<LogFileState>>,
}

#[cfg(debug_assertions)]
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for RotatingMakeWriter {
    type Writer = LogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogWriter {
            state: std::sync::Arc::clone(&self.state),
        }
    }
}

#[cfg(debug_assertions)]
impl std::io::Write for LogWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| std::io::Error::other("debug 日志锁已损坏"))?;
        let incoming = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
        if state.bytes_written > 0 && state.bytes_written.saturating_add(incoming) > state.max_bytes
        {
            rotate_file(&state.path, &state.backup_path)?;
            state.file = open_log_file(&state.path)?;
            state.bytes_written = 0;
        }

        let written = state.file.write(buffer)?;
        state.bytes_written = state
            .bytes_written
            .saturating_add(u64::try_from(written).unwrap_or(u64::MAX));
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| std::io::Error::other("debug 日志锁已损坏"))?;
        state.file.flush()
    }
}

#[cfg(debug_assertions)]
fn open_log_file(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
}

#[cfg(debug_assertions)]
fn backup_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("postui-debug.log");
    path.with_file_name(format!("{name}.1"))
}

#[cfg(debug_assertions)]
fn rotate_file(path: &Path, backup_path: &Path) -> std::io::Result<()> {
    if backup_path.exists() {
        std::fs::remove_file(backup_path)?;
    }
    std::fs::rename(path, backup_path)
}
