use std::path::Path;

#[cfg(debug_assertions)]
use std::path::PathBuf;

use anyhow::Result;

#[cfg(debug_assertions)]
const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;

// Release 构建不编译详细日志写入器，避免正式二进制产生 debug 日志。
pub(crate) fn init(enabled: bool, perf_only: bool, log_path: &Path) -> Result<()> {
    if !enabled {
        return Ok(());
    }

    #[cfg(debug_assertions)]
    {
        init_debug(log_path, perf_only)
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = (log_path, perf_only);
        anyhow::bail!("--debug 仅在 debug 构建中可用");
    }
}

#[cfg(debug_assertions)]
fn init_debug(log_path: &Path, perf_only: bool) -> Result<()> {
    use anyhow::{Context, anyhow};
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let writer = RotatingMakeWriter::new(log_path, MAX_LOG_BYTES)
        .with_context(|| format!("创建 debug 日志文件失败: {}", log_path.display()))?;

    tracing_subscriber::registry()
        .with(tracing_subscriber::filter::filter_fn(move |metadata| {
            *metadata.level() <= tracing::Level::DEBUG
                && (!perf_only || metadata.target() == "postui::perf")
        }))
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_file(true)
                .with_line_number(true)
                .with_ansi(false)
                .with_writer(QueuedMakeWriter::new(writer)?),
        )
        .try_init()
        .map_err(|error| anyhow!("注册 debug 日志订阅器失败: {error}"))?;

    tracing::debug!(
        target: "postui::perf",
        perf_only,
        max_bytes = MAX_LOG_BYTES,
        "debug 日志已启用"
    );
    Ok(())
}

#[cfg(debug_assertions)]
#[derive(Clone)]
struct QueuedMakeWriter {
    sender: std::sync::mpsc::SyncSender<Vec<u8>>,
    dropped: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

#[cfg(debug_assertions)]
impl QueuedMakeWriter {
    fn new(writer: RotatingMakeWriter) -> std::io::Result<Self> {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<Vec<u8>>(256);
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let lost = std::sync::Arc::clone(&dropped);
        std::thread::Builder::new()
            .name("postui-logs".to_string())
            .spawn(move || {
                use std::io::Write;
                use tracing_subscriber::fmt::MakeWriter;
                let mut output = writer.make_writer();
                while let Ok(buffer) = receiver.recv() {
                    let count = lost.swap(0, std::sync::atomic::Ordering::Relaxed);
                    if count > 0 {
                        // Write directly: tracing here would enqueue another log message.
                        let _ = writeln!(
                            output,
                            "postui::perf log_queue_overflow dropped_chunks={count}"
                        );
                    }
                    if output.write_all(&buffer).is_err() {
                        break;
                    }
                }
                let _ = output.flush();
            })?;
        Ok(Self { sender, dropped })
    }
}

#[cfg(debug_assertions)]
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for QueuedMakeWriter {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

#[cfg(debug_assertions)]
impl std::io::Write for QueuedMakeWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        // 调试日志允许丢弃；磁盘慢或队列满时不能阻塞交互线程。
        if self.sender.try_send(buffer.to_vec()).is_err() {
            self.dropped
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(buffer.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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
