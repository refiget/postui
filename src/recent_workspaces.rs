use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub(crate) struct RecentWorkspace {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
}

#[derive(Default, Deserialize, Serialize)]
pub(crate) struct RecentWorkspaces {
    pub(crate) workspaces: Vec<RecentWorkspace>,
}

impl RecentWorkspaces {
    fn path() -> Result<PathBuf> {
        let directories = ProjectDirs::from("", "", "postui")
            .context("Cannot determine PostUI data directory")?;
        Ok(directories.data_local_dir().join("recent-workspaces.json"))
    }

    pub(crate) fn load() -> Result<Self> {
        let path = Self::path()?;
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("Cannot read recent workspaces: {}", path.display())),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error)
                .with_context(|| format!("Cannot read recent workspaces: {}", path.display())),
        }
    }

    pub(crate) fn remember(path: PathBuf, name: String) -> Result<()> {
        let mut recent = Self::load()?;
        recent.workspaces.retain(|workspace| workspace.path != path);
        recent.workspaces.insert(0, RecentWorkspace { path, name });
        recent.workspaces.truncate(30);
        recent.save()
    }

    pub(crate) fn remove(&mut self, index: usize) -> Result<()> {
        self.workspaces.remove(index);
        self.save()
    }

    fn save(&self) -> Result<()> {
        let path = Self::path()?;
        let directory = path.parent().context("Invalid recent workspaces path")?;
        fs::create_dir_all(directory)
            .with_context(|| format!("Cannot create directory: {}", directory.display()))?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let bytes = serde_json::to_vec_pretty(self)?;
        if let Err(error) =
            fs::write(&temporary, bytes).and_then(|()| fs::rename(&temporary, &path))
        {
            remove_temporary_file(&temporary)?;
            return Err(error)
                .with_context(|| format!("Cannot save recent workspaces: {}", path.display()));
        }
        Ok(())
    }
}

fn remove_temporary_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("Cannot remove recent workspaces temporary file"),
    }
}
