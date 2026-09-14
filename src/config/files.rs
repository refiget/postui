use super::RequestDocument;
use crate::diagnostics;
use anyhow::{Result, bail};
use std::{
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug)]
pub(super) struct RequestFile {
    pub(super) path: PathBuf,
    pub(super) text: String,
}

#[derive(Debug)]
pub(super) struct ConfigurationFile {
    pub(super) path: PathBuf,
    pub(super) name: String,
    pub(super) text: String,
}

#[derive(Debug)]
pub(super) struct ParsedRequest {
    pub(super) id: String,
    pub(super) document: RequestDocument,
}

pub(super) fn read_optional_file(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(diagnostics::read(path, &error)),
    }
}

pub(super) fn read_request_files(requests_directory: &Path) -> Result<Vec<RequestFile>> {
    if !optional_directory_exists(requests_directory)? {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    collect_request_paths(requests_directory, &mut paths)?;
    paths.sort();

    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let text = fs::read_to_string(&path).map_err(|error| diagnostics::read(&path, &error))?;
        tracing::debug!(path = %path.display(), bytes = text.len(), "读取请求文件");
        files.push(RequestFile { path, text });
    }
    Ok(files)
}

pub(super) fn read_configuration_files(
    configurations_directory: &Path,
) -> Result<Vec<ConfigurationFile>> {
    if !optional_directory_exists(configurations_directory)? {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(configurations_directory)
        .map_err(|error| diagnostics::read(configurations_directory, &error))?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| diagnostics::read(configurations_directory, &error))?;
    entries.sort_by_key(|entry| entry.path());

    let mut files = Vec::new();
    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| diagnostics::read(&path, &error))?;
        if !file_type.is_file() || !is_yaml_file(&path) {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let Some(name) = normalize_configuration_name(stem) else {
            bail!("Configuration file name is invalid: {}", path.display())
        };
        let text = fs::read_to_string(&path).map_err(|error| diagnostics::read(&path, &error))?;
        tracing::debug!(path = %path.display(), name = %name, bytes = text.len(), "读取 workspace 配置");
        files.push(ConfigurationFile { path, name, text });
    }
    Ok(files)
}

fn collect_request_paths(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| diagnostics::read(directory, &error))?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| diagnostics::read(directory, &error))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| diagnostics::read(&path, &error))?;
        if file_type.is_dir() {
            collect_request_paths(&path, paths)?;
        } else if file_type.is_file() && is_yaml_file(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

fn optional_directory_exists(path: &Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err(diagnostics::invalid(
            path,
            "path",
            "Configured path must be a directory",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(diagnostics::read(path, &error)),
    }
}

fn is_yaml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "yaml" | "yml"))
}

pub(super) fn parse_request_file(
    file: &RequestFile,
    workspace_path: &Path,
) -> Result<ParsedRequest> {
    let id = file
        .path
        .strip_prefix(workspace_path)
        .unwrap_or(&file.path)
        .to_string_lossy()
        .replace('\\', "/");
    let document: RequestDocument = diagnostics::parse_yaml(&file.path, &id, &file.text)?;
    Ok(ParsedRequest { id, document })
}

pub(super) fn resolve_directory(
    workspace_path: &Path,
    configured_path: &Path,
    field: &str,
) -> Result<PathBuf> {
    let project_path = workspace_path.parent().unwrap_or_else(|| Path::new("."));
    let resolved_path = if configured_path.is_absolute() {
        normalize_path(configured_path)
    } else {
        normalize_path(&project_path.join(configured_path))
    };
    if resolved_path.exists() && !resolved_path.is_dir() {
        bail!(
            "Configured path for {field} is not a directory: {}",
            resolved_path.display()
        )
    }
    tracing::debug!(
        field,
        configured_path = %configured_path.display(),
        resolved_path = %resolved_path.display(),
        exists = resolved_path.exists(),
        "解析文件目录配置"
    );
    Ok(resolved_path)
}

pub(super) fn normalize_path(path: &Path) -> PathBuf {
    let mut prefix: Option<OsString> = None;
    let mut rooted = false;
    let mut parts: Vec<OsString> = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(value) => prefix = Some(value.as_os_str().to_os_string()),
            Component::RootDir => rooted = true,
            Component::CurDir => {}
            Component::Normal(value) => parts.push(value.to_os_string()),
            Component::ParentDir => {
                if parts
                    .last()
                    .is_some_and(|part| part != std::ffi::OsStr::new(".."))
                {
                    parts.pop();
                } else if !rooted {
                    parts.push(OsString::from(".."));
                }
            }
        }
    }

    let mut normalized = PathBuf::new();
    if let Some(prefix) = prefix {
        normalized.push(prefix);
    }
    if rooted {
        normalized.push(std::path::MAIN_SEPARATOR_STR);
    }
    for part in parts {
        normalized.push(part);
    }
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

pub(super) fn normalize_request_id(value: &str) -> Result<String> {
    let value = value.trim().replace('\\', "/");
    let path = Path::new(&value);
    if value.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("Request override path is invalid: {value}")
    }
    let normalized = normalize_path(path).to_string_lossy().replace('\\', "/");
    Ok(format!("requests/{normalized}"))
}

pub(super) fn normalize_configuration_name(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        }))
    .then(|| value.to_string())
}
