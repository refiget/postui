use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn save_response(
    body: &[u8],
    headers: &[(String, String)],
    request_id: &str,
    directory: &Path,
) -> Result<PathBuf, String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("创建下载目录失败 {}: {error}", directory.display()))?;

    let filename = response_filename(body, headers, request_id);
    let path = directory.join(filename);
    fs::write(&path, body).map_err(|error| format!("无法写入文件 {}: {error}", path.display()))?;
    Ok(path)
}

fn response_filename(body: &[u8], headers: &[(String, String)], request_id: &str) -> String {
    content_disposition_filename(headers).unwrap_or_else(|| {
        format!(
            "{}{}",
            safe_stem(request_id),
            content_extension(body, headers)
        )
    })
}

fn safe_stem(request_id: &str) -> String {
    let mut stem = String::new();
    for character in request_id.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            stem.push(character);
        } else if !stem.ends_with('-') {
            stem.push('-');
        }
    }
    let stem = stem.trim_matches(['-', '.']);
    if stem.is_empty() {
        "response".to_string()
    } else if is_windows_reserved_filename(stem) {
        format!("_{stem}")
    } else {
        stem.to_string()
    }
}

fn content_extension(body: &[u8], headers: &[(String, String)]) -> &'static str {
    let content_type = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value.split(';').next().unwrap_or_default().trim())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match content_type.as_str() {
        "application/json" | "application/problem+json" => ".json",
        "application/xml" | "text/xml" => ".xml",
        "text/html" => ".html",
        value if value.starts_with("text/") => ".txt",
        "application/pdf" => ".pdf",
        "application/zip" => ".zip",
        value if value.starts_with("image/") => ".bin",
        _ if body.is_empty() => ".txt",
        _ => ".bin",
    }
}

fn content_disposition_filename(headers: &[(String, String)]) -> Option<String> {
    let value = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-disposition"))
        .map(|(_, value)| value)?;
    value.split(';').skip(1).find_map(|part| {
        let (name, value) = part.split_once('=')?;
        if !name.trim().eq_ignore_ascii_case("filename")
            && !name.trim().eq_ignore_ascii_case("filename*")
        {
            return None;
        }
        safe_filename(value.trim().trim_matches('"'))
    })
}

fn safe_filename(value: &str) -> Option<String> {
    let value = value.trim().rsplit(['/', '\\']).next()?.trim();
    if value.is_empty() || matches!(value, "." | "..") {
        return None;
    }
    let mut filename: String = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
            {
                '_'
            } else {
                character
            }
        })
        .collect();
    while filename.ends_with([' ', '.']) {
        filename.pop();
    }
    if filename.is_empty() || matches!(filename.as_str(), "." | "..") {
        return None;
    }
    if is_windows_reserved_filename(&filename) {
        filename.insert(0, '_');
    }
    Some(filename)
}

fn is_windows_reserved_filename(filename: &str) -> bool {
    let stem = filename
        .split_once('.')
        .map_or(filename, |(stem, _)| stem)
        .trim_end_matches([' ', '.']);
    let stem = stem.to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}
