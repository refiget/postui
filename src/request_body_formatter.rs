use std::{fs, path::PathBuf, thread};

use anyhow::{Context, Result};

use crate::{config::RequestDocument, request_file::write_text_if_unchanged};

pub(crate) fn format_after_load(workspace_path: PathBuf) {
    thread::spawn(move || {
        if let Err(error) = format_request_bodies(&workspace_path) {
            tracing::error!(
                path = %workspace_path.display(),
                error = ?error,
                "请求体格式化失败"
            );
        }
    });
}

fn format_request_bodies(workspace_path: &std::path::Path) -> Result<()> {
    let requests_path = workspace_path.join("requests");
    let mut paths = Vec::new();
    collect_request_paths(&requests_path, &mut paths)?;
    paths.sort();

    for path in paths {
        if let Err(error) = format_request_body(&path) {
            tracing::warn!(path = %path.display(), error = ?error, "请求体格式化失败");
        }
    }
    Ok(())
}

fn collect_request_paths(directory: &std::path::Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("Could not read request directory: {}", directory.display())
            });
        }
    };

    for entry in entries {
        let entry = entry.with_context(|| {
            format!("Could not read request directory: {}", directory.display())
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("Could not inspect request path: {}", path.display()))?;
        if file_type.is_dir() {
            collect_request_paths(&path, paths)?;
        } else if file_type.is_file() && is_yaml_file(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

fn format_request_body(path: &std::path::Path) -> Result<()> {
    let original = fs::read_to_string(path)
        .with_context(|| format!("Could not read request file: {}", path.display()))?;
    let mut document: RequestDocument = serde_saphyr::from_str(&original)
        .with_context(|| format!("Could not parse request file: {}", path.display()))?;
    let Some(body) = document.body.as_mut() else {
        return Ok(());
    };
    let json: serde_json::Value = match serde_json::from_str(body) {
        Ok(json) => json,
        Err(_) => return Ok(()),
    };
    let formatted = serde_json::to_string_pretty(&json).context("Could not format JSON body")?;
    if *body == formatted {
        return Ok(());
    }
    let Some(replacement) = replace_body(&original, &formatted) else {
        return Ok(());
    };
    if write_text_if_unchanged(path, &original, &replacement)? {
        tracing::debug!(path = %path.display(), "请求体已格式化");
    }
    Ok(())
}

fn replace_body(document: &str, formatted: &str) -> Option<String> {
    let newline = if document.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut offset = 0;
    let mut body_start = None;
    let mut body_end = document.len();

    for line in document.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        if body_start.is_some() && !content.is_empty() && !content.starts_with([' ', '\t']) {
            body_end = offset;
            break;
        }
        if content
            .strip_prefix("body:")
            .is_some_and(|value| value.is_empty() || value.starts_with(char::is_whitespace))
        {
            body_start = Some(offset);
        }
        offset += line.len();
    }

    let body_start = body_start?;
    let mut replacement = String::with_capacity(document.len() + formatted.len());
    replacement.push_str(&document[..body_start]);
    replacement.push_str("body: |-");
    replacement.push_str(newline);
    for line in formatted.lines() {
        replacement.push_str("  ");
        replacement.push_str(line);
        replacement.push_str(newline);
    }
    replacement.push_str(&document[body_end..]);
    Some(replacement)
}

fn is_yaml_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "yaml" | "yml"))
}
