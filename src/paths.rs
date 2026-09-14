use std::{
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use directories::ProjectDirs;

#[derive(Debug)]
pub(crate) enum WorkspaceSource {
    Explicit,
    Discovered,
    Selected,
}

pub(crate) struct WorkspaceLocation {
    pub(crate) path: PathBuf,
    pub(crate) source: WorkspaceSource,
}

pub(crate) fn discover_user_config_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "postui")
        .map(|directories| directories.config_dir().join("config.yaml"))
}

pub(crate) fn discover_workspace() -> Result<Option<WorkspaceLocation>> {
    let start = env::current_dir().context("Cannot determine current directory")?;
    for directory in start.ancestors() {
        if let Some(path) = existing_workspace(&directory.join(".postui"))? {
            return Ok(Some(WorkspaceLocation {
                path,
                source: WorkspaceSource::Discovered,
            }));
        }
    }
    Ok(None)
}

pub(crate) fn resolve_workspace(input: &Path) -> Result<WorkspaceLocation> {
    ensure!(
        !input.as_os_str().is_empty(),
        "Workspace path must not be empty"
    );
    let directory = fs::canonicalize(input)
        .with_context(|| format!("Cannot resolve workspace path: {}", input.display()))?;
    ensure!(
        fs::metadata(&directory)
            .with_context(|| format!("Cannot access directory: {}", directory.display()))?
            .is_dir(),
        "Workspace path must be a directory: {}",
        input.display()
    );
    let path = if input.file_name().is_some_and(|name| name == ".postui")
        || directory.file_name().is_some_and(|name| name == ".postui")
    {
        directory
    } else {
        directory.join(".postui")
    };
    let path = existing_workspace(&path)?
        .with_context(|| format!("PostUI workspace not found: {}", path.display()))?;
    Ok(WorkspaceLocation {
        path,
        source: WorkspaceSource::Explicit,
    })
}

fn existing_workspace(path: &Path) -> Result<Option<PathBuf>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Cannot access workspace: {}", path.display()));
        }
    };
    let metadata = if metadata.is_symlink() {
        fs::metadata(path).with_context(|| {
            format!("Cannot access workspace symlink target: {}", path.display())
        })?
    } else {
        metadata
    };
    ensure!(
        metadata.is_dir(),
        "Workspace path must be a directory: {}",
        path.display()
    );
    fs::canonicalize(path)
        .map(Some)
        .with_context(|| format!("Cannot resolve workspace path: {}", path.display()))
}

pub(crate) fn resolve_cli_path(path: &Path) -> Result<PathBuf> {
    std::path::absolute(path).with_context(|| format!("Cannot resolve path: {}", path.display()))
}
