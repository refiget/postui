use std::collections::{BTreeMap, BTreeSet};

use form_urlencoded::{Serializer, parse};
use serde_json::Value;
use url::Url;

use crate::config::{
    ApiRequest, DataPart, NameValue, RequestOverride, RequestParam, ResponseExtract,
};

#[derive(Debug, Clone)]
pub struct ResolvedRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<NameValue>,
    pub raw_body: Option<String>,
    pub form: Vec<RequestParam>,
    pub files: Vec<ResolvedFile>,
    pub extracts: Vec<ResponseExtract>,
}

#[derive(Debug, Clone)]
pub struct ResolvedFile {
    pub field: String,
    pub path: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlParts {
    pub base: String,
    pub query: String,
    pub fragment: String,
}

pub fn resolve_request(
    request: &ApiRequest,
    variables: &BTreeMap<String, String>,
) -> ResolvedRequest {
    let url = resolve_url(request, variables);

    ResolvedRequest {
        method: request.method.clone(),
        url,
        headers: expand_headers(&request.headers, variables),
        raw_body: (!request.body_parts.is_empty())
            .then(|| resolve_data_parts(&request.body_parts, variables)),
        form: expand_params(&request.form, variables),
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
        extracts: request.extracts.clone(),
    }
}

/// 在接口列表中显示配置里的原始地址，不展开变量。
pub fn display_url(request: &ApiRequest) -> String {
    append_display_query(&request.url, &request.query_parts)
}

pub fn variable_names(request: &ApiRequest) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_text_request(request, &mut names);
    names.into_iter().collect()
}

pub fn variable_names_in_override(request_override: &RequestOverride) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_text_override(request_override, &mut names);
    names.into_iter().collect()
}

pub fn variable_names_in_text(input: &str) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_text(input, &mut names);
    names.into_iter().collect()
}

pub fn extract_json_value(root: &Value, path: &str) -> Result<String, String> {
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
        let mut value = root;
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

pub fn resolve_text(input: &str, variables: &BTreeMap<String, String>) -> String {
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

fn resolve_url(request: &ApiRequest, variables: &BTreeMap<String, String>) -> String {
    let url_parts = split_url_query(&request.url);
    let base = resolve_text(&url_parts.base, variables);
    let mut query = parse_query_params(&url_parts.query)
        .iter()
        .map(|parameter| encode_parameter(&resolve_parameter(parameter, variables)))
        .collect::<Vec<_>>();
    query.extend(
        request
            .query_parts
            .iter()
            .map(|part| resolve_data_part(part, variables))
            .filter(|part| !part.is_empty()),
    );
    let mut url = append_query(&base, &query.join("&"));
    let fragment = resolve_text(&url_parts.fragment, variables);
    if !fragment.is_empty() {
        url.push('#');
        url.push_str(&fragment);
    }
    url
}

fn resolve_data_parts(parts: &[DataPart], variables: &BTreeMap<String, String>) -> String {
    parts
        .iter()
        .map(|part| resolve_data_part(part, variables))
        .collect::<Vec<_>>()
        .join("&")
}

fn resolve_data_part(part: &DataPart, variables: &BTreeMap<String, String>) -> String {
    match part {
        DataPart::Raw(value) => resolve_text(value, variables),
        DataPart::UrlEncoded(parameter) => {
            encode_parameter(&resolve_parameter(parameter, variables))
        }
    }
}

pub fn data_part_text(part: &DataPart) -> String {
    match part {
        DataPart::Raw(value) => value.clone(),
        DataPart::UrlEncoded(parameter) => parameter.to_text(),
    }
}

fn resolve_parameter(
    parameter: &RequestParam,
    variables: &BTreeMap<String, String>,
) -> RequestParam {
    RequestParam::new(
        resolve_text(&parameter.name, variables),
        resolve_text(&parameter.value, variables),
        parameter.has_equals,
    )
}

fn encode_parameter(parameter: &RequestParam) -> String {
    let mut serializer = Serializer::new(String::new());
    if parameter.has_equals || !parameter.value.is_empty() {
        serializer.append_pair(&parameter.name, &parameter.value);
    } else {
        serializer.append_key_only(&parameter.name);
    }
    serializer.finish()
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

pub fn parse_query_params(query: &str) -> Vec<RequestParam> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let parameter = RequestParam::from_text(part);
            RequestParam::new(
                decode_component(&parameter.name),
                decode_component(&parameter.value),
                parameter.has_equals,
            )
        })
        .collect()
}

pub fn append_display_query(url: &str, parts: &[DataPart]) -> String {
    let query = parts
        .iter()
        .map(data_part_text)
        .collect::<Vec<_>>()
        .join("&");
    append_query(url, &query)
}

pub fn split_url_query(input: &str) -> UrlParts {
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

pub fn rebuild_url(base: &str, query_parts: &[RequestParam], fragment: &str) -> String {
    let query = query_parts
        .iter()
        .map(encode_parameter)
        .collect::<Vec<_>>()
        .join("&");
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

fn expand_headers(values: &[NameValue], variables: &BTreeMap<String, String>) -> Vec<NameValue> {
    values
        .iter()
        .map(|value| NameValue {
            name: resolve_text(&value.name, variables),
            value: resolve_text(&value.value, variables),
        })
        .collect()
}

fn expand_params(
    values: &[RequestParam],
    variables: &BTreeMap<String, String>,
) -> Vec<RequestParam> {
    values
        .iter()
        .map(|value| resolve_parameter(value, variables))
        .collect()
}

fn collect_text_values(values: &[NameValue], names: &mut BTreeSet<String>) {
    for value in values {
        collect_text(&value.name, names);
        collect_text(&value.value, names);
    }
}

fn collect_text_request(request: &ApiRequest, names: &mut BTreeSet<String>) {
    collect_text(&request.url, names);
    collect_text_values(&request.headers, names);
    collect_text_params(&request.form, names);
    for file in &request.files {
        collect_text(&file.field, names);
        collect_text(&file.path, names);
        if let Some(filename) = &file.filename {
            collect_text(filename, names);
        }
        if let Some(content_type) = &file.content_type {
            collect_text(content_type, names);
        }
    }
    for part in request.body_parts.iter().chain(&request.query_parts) {
        collect_text_data_part(part, names);
    }
    for extract in &request.extracts {
        let variable = strip_variable_delimiters(&extract.variable);
        if !variable.is_empty() {
            names.insert(variable.to_string());
        }
    }
}

fn collect_text_override(request_override: &RequestOverride, names: &mut BTreeSet<String>) {
    if let Some(url) = &request_override.url {
        collect_text(url, names);
    }
    if let Some(headers) = &request_override.headers {
        collect_text_values(headers, names);
    }
    if let Some(body_parts) = &request_override.body_parts {
        for part in body_parts {
            collect_text_data_part(part, names);
        }
    }
    if let Some(query_parts) = &request_override.query_parts {
        for part in query_parts {
            collect_text_data_part(part, names);
        }
    }
    if let Some(form) = &request_override.form {
        collect_text_params(form, names);
    }
    if let Some(files) = &request_override.files {
        for file in files {
            collect_text(&file.field, names);
            collect_text(&file.path, names);
            if let Some(filename) = &file.filename {
                collect_text(filename, names);
            }
            if let Some(content_type) = &file.content_type {
                collect_text(content_type, names);
            }
        }
    }
    if let Some(extracts) = &request_override.extracts {
        for extract in extracts {
            let variable = strip_variable_delimiters(&extract.variable);
            if !variable.is_empty() {
                names.insert(variable.to_string());
            }
        }
    }
}

fn collect_text_params(values: &[RequestParam], names: &mut BTreeSet<String>) {
    for value in values {
        collect_text(&value.name, names);
        collect_text(&value.value, names);
    }
}

fn collect_text_data_part(part: &DataPart, names: &mut BTreeSet<String>) {
    match part {
        DataPart::Raw(value) => collect_text(value, names),
        DataPart::UrlEncoded(parameter) => {
            collect_text(&parameter.name, names);
            collect_text(&parameter.value, names);
        }
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

pub fn find_placeholder(input: &str) -> Option<(usize, usize, &str)> {
    let start = input.find("{{")?;
    let after_open = &input[start + 2..];
    let close = after_open.find("}}")?;
    let end = start + 2 + close + 2;
    Some((start, end, after_open[..close].trim()))
}
