use std::{
    env,
    path::{Path, PathBuf},
};

pub(crate) fn discover_user_config_path() -> Option<PathBuf> {
    let home = user_home_directory();
    let found =
        config_directory(home.as_deref()).map(|directory| directory.join("postui/config.yaml"));
    if let Some(path) = &found {
        tracing::debug!(path = %path.display(), "自动发现用户界面配置");
    }
    found
}

fn find_project_path(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|directory| directory.join(".postui").is_dir())
        .map(Path::to_path_buf)
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

pub(crate) fn discover_project_path() -> Option<PathBuf> {
    let current_directory = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let path = find_project_path(&current_directory);
    match path.as_deref() {
        Some(path) => tracing::debug!(
            start = %current_directory.display(),
            path = %path.display(),
            "自动发现 PostUI 项目"
        ),
        None => tracing::debug!(
            start = %current_directory.display(),
            "当前目录及父目录没有 PostUI 项目"
        ),
    }
    path
}

pub(crate) fn resolve_cli_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}
