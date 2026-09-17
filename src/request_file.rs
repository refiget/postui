use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, anyhow, bail};

use crate::config::{ConfigurationDocument, RequestDocument};

static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct RequestFileStore {
    workspace_path: PathBuf,
}

impl RequestFileStore {
    pub fn new(workspace_path: PathBuf) -> Self {
        Self { workspace_path }
    }

    pub fn workspace_path(&self) -> &Path {
        &self.workspace_path
    }

    pub fn delete(&self, request_id: &str) -> Result<()> {
        let path = self.path_for_id(request_id)?;
        fs::remove_file(&path)
            .with_context(|| format!("Could not delete request file: {}", path.display()))?;
        tracing::debug!(path = %path.display(), "删除请求文件");
        Ok(())
    }

    pub fn create(&self, name: &str, request: &RequestDocument) -> Result<PathBuf> {
        let directory = self.workspace_path.join("requests");
        fs::create_dir_all(&directory).with_context(|| {
            format!(
                "Could not create request directory: {}",
                directory.display()
            )
        })?;

        let stem = request_file_stem(name);
        create_yaml_file(&directory, &stem, request)
    }

    pub fn save_configuration(
        &self,
        path: &Path,
        configuration: &ConfigurationDocument,
    ) -> Result<()> {
        let directory = path
            .parent()
            .ok_or_else(|| anyhow!("Invalid scenario configuration path"))?;
        fs::create_dir_all(directory).with_context(|| {
            format!(
                "Could not create scenario directory: {}",
                directory.display()
            )
        })?;
        write_yaml_file(path, configuration)
    }

    fn path_for_id(&self, request_id: &str) -> Result<PathBuf> {
        let relative = request_id
            .strip_prefix("requests/")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Request has no writable source file"))?;
        let relative_path = Path::new(relative);
        if relative_path.is_absolute()
            || relative_path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            bail!("Invalid request source path")
        }
        Ok(self.workspace_path.join("requests").join(relative_path))
    }
}

fn request_file_stem(name: &str) -> String {
    let mut stem = String::with_capacity(name.len());
    let mut separator = false;
    for character in name.trim().chars() {
        if character.is_alphanumeric() || matches!(character, '-' | '_') {
            stem.push(character);
            separator = false;
        } else if !stem.is_empty() && !separator {
            stem.push('-');
            separator = true;
        }
    }
    while stem.ends_with('-') {
        stem.pop();
    }
    if stem.is_empty() {
        "request".to_string()
    } else {
        stem
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(
        ".{file_name}.{}.{sequence}.tmp",
        std::process::id()
    ))
}

fn create_yaml_file<T: serde::Serialize>(
    directory: &Path,
    stem: &str,
    value: &T,
) -> Result<PathBuf> {
    let contents = serde_saphyr::to_string(value).context("Could not serialize configuration")?;
    for suffix in 1_u32..=u32::MAX {
        let file_name = if suffix == 1 {
            format!("{stem}.yaml")
        } else {
            format!("{stem}-{suffix}.yaml")
        };
        let path = directory.join(file_name);
        let temporary = temporary_path(&path);
        write_temporary_file(&temporary, &path, contents.as_bytes())?;
        match fs::hard_link(&temporary, &path) {
            Ok(()) => {
                remove_temporary_file(&temporary)?;
                sync_parent_directory(&path)?;
                return Ok(path);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                remove_temporary_file(&temporary)?;
            }
            Err(error) => {
                let save_error = Err(anyhow!(error))
                    .with_context(|| format!("Could not save request file: {}", path.display()));
                return match remove_temporary_file(&temporary) {
                    Ok(()) => save_error,
                    Err(cleanup_error) => save_error.map_err(|error| error.context(cleanup_error)),
                };
            }
        }
    }
    anyhow::bail!("Could not choose a unique request file name")
}

fn write_yaml_file<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_saphyr::to_string(value).context("Could not serialize configuration")?;
    let temporary = temporary_path(path);
    let result = write_temporary_file(&temporary, path, text.as_bytes())
        .and_then(|()| replace_file(&temporary, path));
    if let Err(error) = result {
        return match remove_temporary_file(&temporary) {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(error.context(cleanup_error)),
        };
    }
    sync_parent_directory(path)
}

fn write_temporary_file(temporary: &Path, path: &Path, contents: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
        .with_context(|| format!("Could not create temporary file: {}", temporary.display()))?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .with_context(|| format!("Could not write request file: {}", path.display()))?;
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, path: &Path) -> Result<()> {
    fs::rename(temporary, path)
        .with_context(|| format!("Could not save request file: {}", path.display()))?;
    Ok(())
}

#[cfg(windows)]
fn replace_file(temporary: &Path, path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) } == 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("Could not save request file: {}", path.display()));
    }
    Ok(())
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<()> {
    let Some(directory) = path.parent() else {
        return Ok(());
    };
    OpenOptions::new()
        .read(true)
        .open(directory)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("Could not sync request directory: {}", directory.display()))
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> Result<()> {
    Ok(())
}

fn remove_temporary_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(anyhow!(error))
            .with_context(|| format!("Could not remove temporary file: {}", path.display())),
    }
}
