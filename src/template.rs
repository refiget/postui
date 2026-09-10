use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::config::{ApiRequest, BodyPart, DownloadTarget};

#[derive(Debug, Clone)]
pub(crate) struct ResolvedRequest {
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) raw_body: Option<String>,
    pub(crate) form: BTreeMap<String, String>,
    pub(crate) files: Vec<ResolvedFile>,
    pub(crate) download: Option<DownloadTarget>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedFile {
    pub(crate) field: String,
    pub(crate) path: String,
    pub(crate) filename: Option<String>,
    pub(crate) content_type: Option<String>,
}

pub(crate) fn resolve_request(
    request: &ApiRequest,
    variables: &BTreeMap<String, String>,
) -> ResolvedRequest {
    let mut url = expand_text(&request.url, variables);
    let query = resolve_data_parts(&request.query_parts, variables);
    if !query.is_empty() {
        url = append_query(&url, &query);
    }

    ResolvedRequest {
        method: request.method.clone(),
        url,
        headers: expand_text_map(&request.headers, variables),
        raw_body: (!request.body_parts.is_empty())
            .then(|| resolve_data_parts(&request.body_parts, variables)),
        form: expand_text_map(&request.form, variables),
        files: request
            .files
            .iter()
            .map(|file| ResolvedFile {
                field: expand_text(&file.field, variables),
                path: expand_text(&file.path, variables),
                filename: file
                    .filename
                    .as_deref()
                    .map(|value| expand_text(value, variables)),
                content_type: file
                    .content_type
                    .as_deref()
                    .map(|value| expand_text(value, variables)),
            })
            .collect(),
        download: request.download.as_ref().map(|target| match target {
            DownloadTarget::Path(path) => DownloadTarget::Path(expand_text(path, variables)),
            DownloadTarget::RemoteName {
                use_content_disposition,
            } => DownloadTarget::RemoteName {
                use_content_disposition: *use_content_disposition,
            },
            DownloadTarget::Auto => DownloadTarget::Auto,
        }),
    }
}

/// 在接口列表中显示配置里的原始地址，不展开变量。
pub(crate) fn display_url(request: &ApiRequest) -> String {
    append_display_query(&request.url, &request.query_parts)
}

pub(crate) fn variable_names(request: &ApiRequest) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_text(&request.url, &mut names);
    collect_text_map(&request.headers, &mut names);
    collect_text_map(&request.form, &mut names);
    for file in &request.files {
        collect_text(&file.field, &mut names);
        collect_text(&file.path, &mut names);
        if let Some(filename) = &file.filename {
            collect_text(filename, &mut names);
        }
        if let Some(content_type) = &file.content_type {
            collect_text(content_type, &mut names);
        }
    }
    for part in &request.body_parts {
        collect_text(body_part_value(part), &mut names);
    }
    for part in &request.query_parts {
        collect_text(body_part_value(part), &mut names);
    }
    for extract in &request.extracts {
        let variable = strip_variable_delimiters(&extract.variable);
        if !variable.is_empty() {
            names.insert(variable.to_string());
        }
    }
    if let Some(DownloadTarget::Path(path)) = &request.download {
        collect_text(path, &mut names);
    }
    names.into_iter().collect()
}

pub(crate) fn unresolved_request_names(request: &ResolvedRequest) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_text(&request.url, &mut names);
    collect_text_map(&request.headers, &mut names);
    collect_text_map(&request.form, &mut names);
    if let Some(body) = &request.raw_body {
        collect_text(body, &mut names);
    }
    for file in &request.files {
        collect_text(&file.field, &mut names);
        collect_text(&file.path, &mut names);
        if let Some(filename) = &file.filename {
            collect_text(filename, &mut names);
        }
        if let Some(content_type) = &file.content_type {
            collect_text(content_type, &mut names);
        }
    }
    if let Some(DownloadTarget::Path(path)) = &request.download {
        collect_text(path, &mut names);
    }
    names.into_iter().collect()
}

pub(crate) fn extract_json_value(body: &str, path: &str) -> Result<String, String> {
    let root: Value = serde_json::from_str(body)
        .map_err(|error| format!("响应不是有效 JSON，无法提取: {error}"))?;
    let path = path.trim();
    if path.is_empty() {
        return Err("响应提取路径不能为空".to_string());
    }

    let value = if path.starts_with('/') {
        root.pointer(path)
            .ok_or_else(|| format!("响应中找不到字段: {path}"))?
    } else {
        let segments = path_segments(path);
        if segments.is_empty() {
            return Err(format!("响应提取路径无效: {path}"));
        }
        let mut value = &root;
        for segment in segments {
            value = match value {
                Value::Object(fields) => fields
                    .get(segment)
                    .ok_or_else(|| format!("响应中找不到字段: {path}"))?,
                Value::Array(items) => {
                    let index = segment
                        .parse::<usize>()
                        .map_err(|_| format!("数组下标无效: {segment}"))?;
                    items
                        .get(index)
                        .ok_or_else(|| format!("响应中找不到字段: {path}"))?
                }
                _ => return Err(format!("字段路径中无法继续读取: {segment}")),
            };
        }
        value
    };

    match value {
        Value::String(value) => Ok(value.clone()),
        _ => serde_json::to_string(value).map_err(|error| format!("响应字段无法复制: {error}")),
    }
}

fn expand_text(input: &str, variables: &BTreeMap<String, String>) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while let Some((start, end, name)) = find_placeholder(rest) {
        output.push_str(&rest[..start]);
        let token = &rest[start..end];
        if name.is_empty() {
            output.push_str(token);
        } else if let Some(value) = variables.get(name) {
            output.push_str(value);
        } else {
            output.push_str(token);
        }
        rest = &rest[end..];
    }

    output.push_str(rest);
    output
}

fn resolve_data_parts(parts: &[BodyPart], variables: &BTreeMap<String, String>) -> String {
    parts
        .iter()
        .map(|part| match part {
            BodyPart::Raw(value) => expand_text(value, variables),
            BodyPart::UrlEncoded(value) => urlencode_data(&expand_text(value, variables)),
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn body_part_value(part: &BodyPart) -> &str {
    match part {
        BodyPart::Raw(value) | BodyPart::UrlEncoded(value) => value,
    }
}

fn urlencode_data(value: &str) -> String {
    let (name, content) = value.split_once('=').unwrap_or(("", value));
    if name.is_empty() {
        encode_component(content)
    } else {
        format!("{}={}", encode_component(name), encode_component(content))
    }
}

fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn append_query(url: &str, query: &str) -> String {
    if query.is_empty() {
        return url.to_string();
    }
    let separator = if url.contains('?') {
        if url.ends_with('?') || url.ends_with('&') {
            ""
        } else {
            "&"
        }
    } else {
        "?"
    };
    format!("{url}{separator}{query}")
}

fn append_display_query(url: &str, parts: &[BodyPart]) -> String {
    let query = parts
        .iter()
        .map(body_part_value)
        .collect::<Vec<_>>()
        .join("&");
    append_query(url, &query)
}

fn expand_text_map(
    values: &BTreeMap<String, String>,
    variables: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| (expand_text(key, variables), expand_text(value, variables)))
        .collect()
}

fn collect_text_map(values: &BTreeMap<String, String>, names: &mut BTreeSet<String>) {
    for (key, value) in values {
        collect_text(key, names);
        collect_text(value, names);
    }
}

fn collect_text(input: &str, names: &mut BTreeSet<String>) {
    let mut rest = input;
    while let Some((_, end, name)) = find_placeholder(rest) {
        if !name.is_empty() {
            names.insert(name.to_string());
        }
        rest = &rest[end..];
    }
}

fn strip_variable_delimiters(value: &str) -> &str {
    let value = value.trim();
    value
        .strip_prefix("{{")
        .and_then(|value| value.strip_suffix("}}"))
        .map(str::trim)
        .unwrap_or(value)
}

fn path_segments(path: &str) -> Vec<&str> {
    path.split(['.', '[', ']'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect()
}

pub(crate) fn find_placeholder(input: &str) -> Option<(usize, usize, &str)> {
    let start = input.find("{{")?;
    let after_open = &input[start + 2..];
    let close = after_open.find("}}")?;
    let end = start + 2 + close + 2;
    Some((start, end, after_open[..close].trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ApiRequest, BodyPart, FileUpload};

    #[test]
    fn expands_known_variables_and_keeps_unknown_tokens() {
        let variables = BTreeMap::from([("host".to_string(), "localhost".to_string())]);
        assert_eq!(
            expand_text("http://{{ host }}/{{missing}}", &variables),
            "http://localhost/{{missing}}"
        );
    }

    #[test]
    fn resolves_url_without_rewriting_the_configured_host() {
        let request = ApiRequest {
            id: "health".to_string(),
            name: "Health".to_string(),
            method: "GET".to_string(),
            url: "https://example.com/health/{{version}}".to_string(),
            timeout_seconds: 30,
            description: String::new(),
            headers: BTreeMap::new(),
            body_parts: Vec::new(),
            query_parts: Vec::new(),
            form: BTreeMap::new(),
            files: Vec::new(),
            download: None,
            extracts: Vec::new(),
        };
        let variables = BTreeMap::from([("version".to_string(), "v1".to_string())]);
        let resolved = resolve_request(&request, &variables);
        assert_eq!(resolved.url, "https://example.com/health/v1");
    }

    #[test]
    fn displays_url_without_expanding_variables() {
        let request = ApiRequest {
            id: "timeline".to_string(),
            name: "任务时间线".to_string(),
            method: "GET".to_string(),
            url: "http://172.16.68.42/gmp/tasks/{{task_id}}".to_string(),
            timeout_seconds: 30,
            description: String::new(),
            headers: BTreeMap::new(),
            body_parts: Vec::new(),
            query_parts: Vec::new(),
            form: BTreeMap::new(),
            files: Vec::new(),
            download: None,
            extracts: Vec::new(),
        };
        assert_eq!(
            display_url(&request),
            "http://172.16.68.42/gmp/tasks/{{task_id}}"
        );
    }

    #[test]
    fn finds_raw_body_variables() {
        let request = ApiRequest {
            id: "create".to_string(),
            name: "Create".to_string(),
            method: "POST".to_string(),
            url: "/users".to_string(),
            timeout_seconds: 30,
            description: String::new(),
            headers: BTreeMap::new(),
            body_parts: vec![BodyPart::Raw(
                "{\"user\":{\"name\":\"{{name}}\"}}".to_string(),
            )],
            query_parts: Vec::new(),
            form: BTreeMap::new(),
            files: Vec::new(),
            download: None,
            extracts: Vec::new(),
        };
        assert_eq!(variable_names(&request), vec!["name"]);
    }

    #[test]
    fn expands_form_and_file_variables() {
        let request = ApiRequest {
            id: "upload".to_string(),
            name: "Upload".to_string(),
            method: "POST".to_string(),
            url: "/upload".to_string(),
            timeout_seconds: 30,
            description: String::new(),
            headers: BTreeMap::new(),
            body_parts: Vec::new(),
            query_parts: Vec::new(),
            form: BTreeMap::from([(String::from("note"), String::from("{{note}}"))]),
            files: vec![FileUpload {
                field: "file".to_string(),
                path: "{{file_name}}".to_string(),
                filename: Some("{{file_name}}".to_string()),
                content_type: Some("text/plain".to_string()),
            }],
            download: None,
            extracts: Vec::new(),
        };
        let variables = BTreeMap::from([
            ("file_name".to_string(), "sample.txt".to_string()),
            ("note".to_string(), "hello".to_string()),
        ]);
        let resolved = resolve_request(&request, &variables);

        assert_eq!(resolved.form.get("note").map(String::as_str), Some("hello"));
        assert_eq!(resolved.files[0].path, "sample.txt");
        assert_eq!(variable_names(&request), vec!["file_name", "note"]);
    }

    #[test]
    fn encodes_urlencoded_template_values_after_expansion() {
        let request = ApiRequest {
            id: "search".to_string(),
            name: "Search".to_string(),
            method: "GET".to_string(),
            url: "http://localhost/search".to_string(),
            timeout_seconds: 30,
            description: String::new(),
            headers: BTreeMap::new(),
            body_parts: Vec::new(),
            query_parts: vec![BodyPart::UrlEncoded("q={{query}}".to_string())],
            form: BTreeMap::new(),
            files: Vec::new(),
            download: None,
            extracts: Vec::new(),
        };
        let variables = BTreeMap::from([(String::from("query"), String::from("a b&c"))]);
        let resolved = resolve_request(&request, &variables);

        assert_eq!(resolved.url, "http://localhost/search?q=a%20b%26c");
    }

    #[test]
    fn expands_download_path_variables() {
        let request = ApiRequest {
            id: "download".to_string(),
            name: "Download".to_string(),
            method: "GET".to_string(),
            url: "https://example.test/report".to_string(),
            timeout_seconds: 30,
            description: String::new(),
            headers: BTreeMap::new(),
            body_parts: Vec::new(),
            query_parts: Vec::new(),
            form: BTreeMap::new(),
            files: Vec::new(),
            download: Some(DownloadTarget::Path("{{file_name}}".to_string())),
            extracts: Vec::new(),
        };
        let variables = BTreeMap::from([(String::from("file_name"), String::from("report.pdf"))]);
        let resolved = resolve_request(&request, &variables);

        assert_eq!(
            resolved.download,
            Some(DownloadTarget::Path("report.pdf".to_string()))
        );
        assert_eq!(variable_names(&request), vec!["file_name"]);
    }

    #[test]
    fn extracts_nested_json_values() {
        let body = r#"{
            "data": {
                "taskId": "task-001",
                "items": [{"fileId": "file-001"}],
                "count": 2
            }
        }"#;

        assert_eq!(
            extract_json_value(body, "data.taskId").expect("应提取字符串"),
            "task-001"
        );
        assert_eq!(
            extract_json_value(body, "data.items[0].fileId").expect("应提取数组字段"),
            "file-001"
        );
        assert_eq!(
            extract_json_value(body, "/data/count").expect("应支持 JSON Pointer"),
            "2"
        );
    }
}
