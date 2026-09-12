use std::collections::{BTreeMap, BTreeSet};

use form_urlencoded::parse;
use serde_json::Value;
use url::Url;

use crate::config::{ApiRequest, BodyPart, NameValue};

#[derive(Debug, Clone)]
pub(crate) struct ResolvedRequest {
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) headers: Vec<NameValue>,
    pub(crate) raw_body: Option<String>,
    pub(crate) form: Vec<NameValue>,
    pub(crate) files: Vec<ResolvedFile>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedFile {
    pub(crate) field: String,
    pub(crate) path: String,
    pub(crate) filename: Option<String>,
    pub(crate) content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DisplayTextPart {
    pub(crate) text: String,
    pub(crate) variable: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UrlParts {
    pub(crate) base: String,
    pub(crate) query: String,
    pub(crate) fragment: String,
}

pub(crate) fn resolve_request(
    request: &ApiRequest,
    variables: &BTreeMap<String, String>,
) -> ResolvedRequest {
    let mut url = resolve_text(&request.url, variables);
    let query_parts = request
        .query_parts
        .iter()
        .map(|part| resolve_data_part(part, variables))
        .collect::<Vec<_>>();
    let query = query_parts.join("&");
    if !query.is_empty() {
        url = append_query(&url, &query);
    }

    ResolvedRequest {
        method: request.method.clone(),
        url,
        headers: expand_text_values(&request.headers, variables),
        raw_body: (!request.body_parts.is_empty())
            .then(|| resolve_data_parts(&request.body_parts, variables)),
        form: expand_text_values(&request.form, variables),
        files: request
            .files
            .iter()
            .map(|file| ResolvedFile {
                field: resolve_text(&file.field, variables),
                path: resolve_text(&file.path, variables),
                filename: file
                    .filename
                    .as_deref()
                    .map(|value| resolve_text(value, variables)),
                content_type: file
                    .content_type
                    .as_deref()
                    .map(|value| resolve_text(value, variables)),
            })
            .collect(),
    }
}

/// 在接口列表中显示配置里的原始地址，不展开变量。
pub(crate) fn display_url(request: &ApiRequest) -> String {
    append_display_query(&request.url, &request.query_parts)
}

pub(crate) fn variable_names(request: &ApiRequest) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_text(&request.url, &mut names);
    collect_text_values(&request.headers, &mut names);
    collect_text_values(&request.form, &mut names);
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
    names.into_iter().collect()
}

pub(crate) fn variable_names_in_text(input: &str) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_text(input, &mut names);
    names.into_iter().collect()
}

pub(crate) fn url_variable_names(input: &str) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_text(&input[url_path_start(input)..], &mut names);
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

pub(crate) fn resolve_text(input: &str, variables: &BTreeMap<String, String>) -> String {
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

pub(crate) fn display_text_parts(
    input: &str,
    variables: &BTreeMap<String, String>,
) -> Vec<DisplayTextPart> {
    let mut parts = Vec::new();
    let mut rest = input;

    while let Some((start, end, name)) = find_placeholder(rest) {
        if start > 0 {
            parts.push(DisplayTextPart {
                text: rest[..start].to_string(),
                variable: None,
            });
        }
        let token = &rest[start..end];
        let text = variables
            .get(name)
            .filter(|value| !value.is_empty())
            .cloned()
            .unwrap_or_else(|| token.to_string());
        parts.push(DisplayTextPart {
            text,
            variable: (!name.is_empty()).then(|| name.to_string()),
        });
        rest = &rest[end..];
    }

    if !rest.is_empty() {
        parts.push(DisplayTextPart {
            text: rest.to_string(),
            variable: None,
        });
    }
    parts
}

fn resolve_data_parts(parts: &[BodyPart], variables: &BTreeMap<String, String>) -> String {
    parts
        .iter()
        .map(|part| resolve_data_part(part, variables))
        .collect::<Vec<_>>()
        .join("&")
}

fn resolve_data_part(part: &BodyPart, variables: &BTreeMap<String, String>) -> String {
    match part {
        BodyPart::Raw(value) => resolve_text(value, variables),
        BodyPart::UrlEncoded(value) => urlencode_data(&resolve_text(value, variables)),
    }
}

pub(crate) fn body_part_value(part: &BodyPart) -> &str {
    match part {
        BodyPart::Raw(value) | BodyPart::UrlEncoded(value) => value,
    }
}

fn urlencode_data(value: &str) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    if let Some((name, content)) = value.split_once('=') {
        serializer.append_pair(name, content);
    } else {
        serializer.append_key_only(value);
    }
    serializer.finish()
}

pub(crate) fn decode_urlencoded_data(value: &str) -> String {
    let Some(_) = value.split_once('=') else {
        return decode_component(value);
    };
    parse(value.as_bytes())
        .next()
        .map(|(name, content)| format!("{name}={content}"))
        .unwrap_or_default()
}

fn decode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() + 1);
    encoded.push('=');
    encoded.push_str(value);
    parse(encoded.as_bytes())
        .next()
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}

fn append_query(url: &str, query: &str) -> String {
    if query.is_empty() {
        return url.to_string();
    }
    if find_placeholder(url).is_none()
        && let Ok(mut parsed) = Url::parse(url)
    {
        let existing_query = parsed.query().unwrap_or_default();
        let combined_query = if existing_query.is_empty() {
            query.to_string()
        } else {
            format!("{existing_query}&{query}")
        };
        parsed.set_query(Some(&combined_query));
        return parsed.into();
    }

    append_query_fallback(url, query)
}

fn append_display_query(url: &str, parts: &[BodyPart]) -> String {
    let query = parts
        .iter()
        .map(body_part_value)
        .collect::<Vec<_>>()
        .join("&");
    append_query(url, &query)
}

pub(crate) fn split_url_query(input: &str) -> UrlParts {
    let (without_fragment, raw_fragment) = input
        .split_once('#')
        .map_or((input, ""), |(base, fragment)| (base, fragment));
    let (raw_base, raw_query) = without_fragment
        .split_once('?')
        .map_or((without_fragment, ""), |(base, query)| (base, query));

    if find_placeholder(input).is_none()
        && let Ok(parsed) = Url::parse(input)
    {
        return UrlParts {
            base: raw_base.to_string(),
            query: parsed.query().unwrap_or_default().to_string(),
            fragment: parsed.fragment().unwrap_or_default().to_string(),
        };
    }

    UrlParts {
        base: raw_base.to_string(),
        query: raw_query.to_string(),
        fragment: raw_fragment.to_string(),
    }
}

pub(crate) fn rebuild_url(base: &str, query_parts: &[String], fragment: &str) -> String {
    let query = query_parts.join("&");
    if find_placeholder(base).is_none()
        && let Ok(mut parsed) = Url::parse(base)
    {
        parsed.set_query((!query.is_empty()).then_some(query.as_str()));
        parsed.set_fragment((!fragment.is_empty()).then_some(fragment));
        return parsed.into();
    }

    let mut rebuilt = append_query_fallback(base, &query);
    if !fragment.is_empty() {
        rebuilt.push('#');
        rebuilt.push_str(fragment);
    }
    rebuilt
}

fn append_query_fallback(url: &str, query: &str) -> String {
    if query.is_empty() {
        return url.to_string();
    }
    let (without_fragment, fragment) = url
        .split_once('#')
        .map_or((url, None), |(base, fragment)| (base, Some(fragment)));
    let separator = if without_fragment.contains('?') {
        if without_fragment.ends_with('?') || without_fragment.ends_with('&') {
            ""
        } else {
            "&"
        }
    } else {
        "?"
    };
    let mut rebuilt = format!("{without_fragment}{separator}{query}");
    if let Some(fragment) = fragment {
        rebuilt.push('#');
        rebuilt.push_str(fragment);
    }
    rebuilt
}

fn expand_text_values(
    values: &[NameValue],
    variables: &BTreeMap<String, String>,
) -> Vec<NameValue> {
    values
        .iter()
        .map(|value| NameValue {
            name: resolve_text(&value.name, variables),
            value: resolve_text(&value.value, variables),
        })
        .collect()
}

fn collect_text_values(values: &[NameValue], names: &mut BTreeSet<String>) {
    for value in values {
        collect_text(&value.name, names);
        collect_text(&value.value, names);
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

fn url_path_start(input: &str) -> usize {
    let authority_start = input.find("://").map_or(0, |index| index.saturating_add(3));
    input[authority_start..]
        .find(['/', '?', '#'])
        .map_or(input.len(), |index| authority_start + index)
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
