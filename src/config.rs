use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) struct RequestConfig {
    pub(crate) name: String,
    pub(crate) file_directory: PathBuf,
    pub(crate) variables: BTreeMap<String, VariableDefinition>,
    pub(crate) requests: Vec<ApiRequest>,
    pub(crate) timeout_seconds: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct VariableDefinition {
    pub(crate) default: Option<Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct ApiRequest {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) description: String,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body_parts: Vec<BodyPart>,
    pub(crate) query_parts: Vec<BodyPart>,
    pub(crate) form: BTreeMap<String, String>,
    pub(crate) files: Vec<FileUpload>,
    pub(crate) extracts: Vec<ResponseExtract>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BodyPart {
    Raw(String),
    UrlEncoded(String),
}

#[derive(Debug, Clone)]
pub(crate) struct FileUpload {
    pub(crate) field: String,
    pub(crate) path: String,
    pub(crate) filename: Option<String>,
    pub(crate) content_type: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResponseExtract {
    pub(crate) name: String,
    pub(crate) variable: String,
    pub(crate) path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRequestConfig {
    #[serde(default = "default_name")]
    name: String,
    #[serde(default = "default_file_directory")]
    file_directory: PathBuf,
    #[serde(default)]
    variables: Vec<RawVariable>,
    #[serde(default)]
    requests: Vec<RawApiRequest>,
    #[serde(default = "default_timeout_seconds")]
    timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVariable {
    name: String,
    #[serde(default)]
    default: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawApiRequest {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    description: String,
    request: String,
    #[serde(default)]
    extract: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
struct ParsedCommand {
    method: Option<String>,
    url: Option<String>,
    headers: BTreeMap<String, String>,
    data: Vec<BodyPart>,
    query_data: Vec<BodyPart>,
    form: BTreeMap<String, String>,
    files: Vec<FileUpload>,
    get_mode: bool,
}

fn default_name() -> String {
    "PostUI".to_string()
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_timeout_seconds() -> u64 {
    30
}

fn default_file_directory() -> PathBuf {
    PathBuf::from("files")
}

pub(crate) fn load(path: &Path) -> Result<RequestConfig> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("无法读取配置文件: {}", path.display()))?;

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    tracing::debug!(
        path = %path.display(),
        format = if extension.is_empty() { "yaml" } else { extension.as_str() },
        bytes = text.len(),
        "读取配置文件"
    );

    let raw: RawRequestConfig = if extension == "json" {
        serde_json::from_str(&text).with_context(|| "JSON 配置格式无效")?
    } else {
        serde_yaml::from_str(&text).with_context(|| "YAML 配置格式无效")?
    };

    let config = normalize(raw)?;
    tracing::debug!(
        name = %config.name,
        request_count = config.requests.len(),
        variable_count = config.variables.len(),
        timeout_seconds = config.timeout_seconds,
        "配置文件加载完成"
    );
    Ok(config)
}

fn normalize(raw: RawRequestConfig) -> Result<RequestConfig> {
    if raw.requests.is_empty() {
        bail!("配置文件中至少需要一个接口")
    }

    let variables = normalize_variables(raw.variables)?;
    let timeout_seconds = if raw.timeout_seconds == 0 {
        default_timeout_seconds()
    } else {
        raw.timeout_seconds
    };

    let mut request_ids = BTreeSet::new();
    let mut requests = Vec::with_capacity(raw.requests.len());
    for (index, raw_request) in raw.requests.into_iter().enumerate() {
        let request = normalize_request(raw_request, index)?;
        if !request_ids.insert(request.id.clone()) {
            bail!("接口 id 重复: {}", request.id)
        }
        tracing::debug!(
            request_id = %request.id,
            method = %request.method,
            url = %request.url,
            header_count = request.headers.len(),
            form_field_count = request.form.len(),
            file_count = request.files.len(),
            extract_count = request.extracts.len(),
            body_part_count = request.body_parts.len(),
            query_part_count = request.query_parts.len(),
            "规范化接口配置"
        );
        requests.push(request);
    }

    let config = RequestConfig {
        name: if raw.name.trim().is_empty() {
            default_name()
        } else {
            raw.name.trim().to_string()
        },
        file_directory: raw.file_directory,
        variables,
        requests,
        timeout_seconds,
    };
    validate_declared_variables(&config)?;
    Ok(config)
}

fn normalize_variables(
    raw_variables: Vec<RawVariable>,
) -> Result<BTreeMap<String, VariableDefinition>> {
    let mut variables = BTreeMap::new();
    for raw in raw_variables {
        let Some(name) = normalize_variable_name(&raw.name) else {
            bail!("变量名称无效: {}", raw.name)
        };
        if variables
            .insert(
                name.clone(),
                VariableDefinition {
                    default: raw.default,
                },
            )
            .is_some()
        {
            bail!("变量名称重复: {name}")
        }
    }
    Ok(variables)
}

fn normalize_request(raw: RawApiRequest, index: usize) -> Result<ApiRequest> {
    let id = raw
        .id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("request-{}", index + 1));
    let name = if raw.name.trim().is_empty() {
        id.clone()
    } else {
        raw.name.trim().to_string()
    };
    let parsed = parse_curl(&raw.request, &id)?;
    let method = parsed
        .method
        .unwrap_or_else(default_method)
        .trim()
        .to_ascii_uppercase();
    let url = parsed
        .url
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .ok_or_else(|| anyhow::anyhow!("接口 {} 的 curl 命令缺少 URL", id))?;
    let mut request = ApiRequest {
        id,
        name,
        method,
        url,
        description: raw.description.trim().to_string(),
        headers: parsed.headers,
        body_parts: parsed.data,
        query_parts: parsed.query_data,
        form: parsed.form,
        files: parsed.files,
        extracts: raw
            .extract
            .into_iter()
            .map(|(variable, path)| ResponseExtract {
                name: String::new(),
                variable,
                path,
            })
            .collect(),
    };

    for file in &request.files {
        validate_file(&request.id, file)?;
    }
    for extract in &mut request.extracts {
        normalize_extract(&request.id, extract)?;
    }
    Ok(request)
}

fn parse_curl(source: &str, request_id: &str) -> Result<ParsedCommand> {
    if source.trim().is_empty() {
        bail!("接口 {} 缺少 request 内容", request_id)
    }
    let command = clean_command_block(source);
    if command.contains("$(") || command.contains(char::from(96)) {
        bail!("接口 {} 的 request 不支持 shell 命令替换", request_id)
    }
    let tokens = shlex::split(&command)
        .ok_or_else(|| anyhow::anyhow!("接口 {} 的 request 存在未闭合的引号", request_id))?;
    let Some(start) = tokens.iter().position(|token| is_curl_command(token)) else {
        bail!("接口 {} 的 request 中没有找到 curl 命令", request_id)
    };

    let mut parsed = ParsedCommand::default();
    let mut stop_options = false;
    let mut index = start + 1;
    while index < tokens.len() {
        let token = &tokens[index];
        if !stop_options && token == "--" {
            stop_options = true;
            index += 1;
            continue;
        }

        if !stop_options {
            match token.as_str() {
                "-X" | "--request" => {
                    parsed.method = Some(next_argument(&tokens, &mut index, token, request_id)?);
                }
                "-H" | "--header" => {
                    let header = next_argument(&tokens, &mut index, token, request_id)?;
                    parse_header(&mut parsed.headers, &header, request_id)?;
                }
                "-d" | "--data" | "--data-ascii" | "--data-binary" => {
                    let data = next_argument(&tokens, &mut index, token, request_id)?;
                    parse_body_argument(&mut parsed, token, &data, request_id)?;
                }
                "--data-raw" | "--data-urlencode" | "--json" => {
                    let data = next_argument(&tokens, &mut index, token, request_id)?;
                    parse_body_argument(&mut parsed, token, &data, request_id)?;
                }
                "-F" | "--form" => {
                    let form = next_argument(&tokens, &mut index, token, request_id)?;
                    parse_form(&mut parsed, &form, request_id)?;
                }
                "--form-string" => {
                    let form = next_argument(&tokens, &mut index, token, request_id)?;
                    parse_form_string(&mut parsed.form, &form, request_id)?;
                }
                "-G" | "--get" => {
                    parsed.get_mode = true;
                    parsed.method = Some("GET".to_string());
                }
                "-b" | "--cookie" => {
                    let cookie = next_argument(&tokens, &mut index, token, request_id)?;
                    parsed.headers.insert("Cookie".to_string(), cookie);
                }
                "-A" | "--user-agent" => {
                    let user_agent = next_argument(&tokens, &mut index, token, request_id)?;
                    parsed.headers.insert("User-Agent".to_string(), user_agent);
                }
                "-e" | "--referer" => {
                    let referer = next_argument(&tokens, &mut index, token, request_id)?;
                    parsed.headers.insert("Referer".to_string(), referer);
                }
                "--url" => {
                    let url = next_argument(&tokens, &mut index, token, request_id)?;
                    set_url(&mut parsed.url, &url, request_id)?;
                }
                _ if is_ignored_curl_flag(token) => {}
                _ if token.starts_with("--request=") => {
                    parsed.method = Some(token[10..].to_string());
                }
                _ if token.starts_with("--header=") => {
                    parse_header(&mut parsed.headers, &token[9..], request_id)?;
                }
                _ if token.starts_with("--url=") => {
                    set_url(&mut parsed.url, &token[6..], request_id)?;
                }
                _ if token.starts_with("--data=")
                    || token.starts_with("--data-ascii=")
                    || token.starts_with("--data-binary=")
                    || token.starts_with("--data-raw=")
                    || token.starts_with("--data-urlencode=")
                    || token.starts_with("--json=") =>
                {
                    let (option, data) = token
                        .split_once('=')
                        .ok_or_else(|| anyhow::anyhow!("curl 参数格式无效: {token}"))?;
                    parse_body_argument(&mut parsed, option, data, request_id)?;
                }
                _ if token.starts_with("--form=") => {
                    parse_form(&mut parsed, &token[7..], request_id)?;
                }
                _ if token.starts_with("--form-string=") => {
                    parse_form_string(&mut parsed.form, &token[14..], request_id)?;
                }
                _ if token.starts_with("--cookie=") => {
                    parsed
                        .headers
                        .insert("Cookie".to_string(), token[9..].to_string());
                }
                _ if token.starts_with("--user-agent=") => {
                    parsed
                        .headers
                        .insert("User-Agent".to_string(), token[13..].to_string());
                }
                _ if token.starts_with("--referer=") => {
                    parsed
                        .headers
                        .insert("Referer".to_string(), token[10..].to_string());
                }
                _ if token.len() > 2 && token.starts_with("-X") => {
                    parsed.method = Some(token[2..].to_string());
                }
                _ if token.len() > 2 && token.starts_with("-H") => {
                    parse_header(&mut parsed.headers, &token[2..], request_id)?;
                }
                _ if token.len() > 2 && token.starts_with("-d") => {
                    reject_body_file(&token[2..], token, request_id)?;
                    parsed.data.push(BodyPart::Raw(token[2..].to_string()));
                }
                _ if token.len() > 2 && token.starts_with("-F") => {
                    parse_form(&mut parsed, &token[2..], request_id)?;
                }
                _ if token.starts_with('-') => {
                    bail!("接口 {} 的 curl 参数暂不支持: {}", request_id, token)
                }
                _ => {
                    set_url(&mut parsed.url, token, request_id)?;
                }
            }
        } else {
            set_url(&mut parsed.url, token, request_id)?;
        }
        index += 1;
    }

    let Some(url) = parsed.url.take() else {
        bail!("接口 {} 的 curl 命令缺少 URL", request_id)
    };
    if parsed.get_mode {
        parsed.query_data = std::mem::take(&mut parsed.data);
    } else {
        parsed.query_data.clear();
    }
    parsed.url = Some(url);
    if !parsed.data.is_empty() {
        if parsed.method.is_none() {
            parsed.method = Some("POST".to_string());
        }
        insert_header_if_missing(
            &mut parsed.headers,
            "Content-Type",
            "application/x-www-form-urlencoded",
        );
    } else if (!parsed.form.is_empty() || !parsed.files.is_empty()) && parsed.method.is_none() {
        parsed.method = Some("POST".to_string());
    }
    if !parsed.data.is_empty() && (!parsed.form.is_empty() || !parsed.files.is_empty()) {
        bail!("接口 {} 的 curl 命令不能同时使用 data 和 form", request_id)
    }
    Ok(parsed)
}

fn clean_command_block(source: &str) -> String {
    let mut command = String::new();
    let mut saw_fence = false;
    let mut in_fence = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if saw_fence && in_fence {
                in_fence = false;
            } else if !saw_fence {
                saw_fence = true;
                in_fence = true;
            }
            continue;
        }
        if saw_fence && !in_fence {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let line = line.trim_end();
        if let Some(line) = line.strip_suffix('\\') {
            command.push_str(line);
            command.push(' ');
        } else {
            command.push_str(line);
            command.push('\n');
        }
    }
    command
}

fn is_curl_command(token: &str) -> bool {
    Path::new(token)
        .file_name()
        .and_then(|value| value.to_str())
        == Some("curl")
}

fn next_argument(
    tokens: &[String],
    index: &mut usize,
    option: &str,
    request_id: &str,
) -> Result<String> {
    *index += 1;
    tokens
        .get(*index)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("接口 {} 的 curl 参数 {} 缺少值", request_id, option))
}

fn set_url(url: &mut Option<String>, value: &str, request_id: &str) -> Result<()> {
    if url.replace(value.to_string()).is_some() {
        bail!("接口 {} 的 curl 命令包含多个 URL", request_id)
    }
    Ok(())
}

fn parse_header(
    headers: &mut BTreeMap<String, String>,
    value: &str,
    request_id: &str,
) -> Result<()> {
    let Some((name, value)) = value.split_once(':') else {
        bail!("接口 {} 的 curl 请求头格式无效: {}", request_id, value)
    };
    let name = name.trim();
    if name.is_empty() {
        bail!("接口 {} 的 curl 请求头名称不能为空", request_id)
    }
    headers.insert(name.to_string(), value.trim().to_string());
    Ok(())
}

fn parse_form(parsed: &mut ParsedCommand, value: &str, request_id: &str) -> Result<()> {
    let (field, content) = split_form_field(value, request_id)?;
    if let Some(file) = content.strip_prefix('@') {
        let mut parts = file.split(';');
        let path = parts.next().unwrap_or_default().trim();
        if path.is_empty() {
            bail!("接口 {} 的 curl 上传文件路径不能为空", request_id)
        }
        let mut filename = None;
        let mut content_type = None;
        for part in parts {
            let Some((key, value)) = part.split_once('=') else {
                bail!("接口 {} 的 curl 文件参数格式无效: {}", request_id, part)
            };
            match key.trim() {
                "filename" => filename = Some(value.trim().to_string()),
                "type" => content_type = Some(value.trim().to_string()),
                key => bail!("接口 {} 的 curl 文件参数暂不支持: {}", request_id, key),
            }
        }
        parsed.files.push(FileUpload {
            field: field.to_string(),
            path: path.to_string(),
            filename,
            content_type,
        });
    } else {
        parsed.form.insert(field.to_string(), content.to_string());
    }
    Ok(())
}

fn parse_form_string(
    form: &mut BTreeMap<String, String>,
    value: &str,
    request_id: &str,
) -> Result<()> {
    let (field, content) = split_form_field(value, request_id)?;
    form.insert(field.to_string(), content.to_string());
    Ok(())
}

fn split_form_field<'a>(value: &'a str, request_id: &str) -> Result<(&'a str, &'a str)> {
    let Some((field, content)) = value.split_once('=') else {
        bail!("接口 {} 的 curl 表单字段格式无效: {}", request_id, value)
    };
    let field = field.trim();
    if field.is_empty() {
        bail!("接口 {} 的 curl 表单字段名称不能为空", request_id)
    }
    Ok((field, content))
}

fn parse_body_argument(
    parsed: &mut ParsedCommand,
    option: &str,
    value: &str,
    request_id: &str,
) -> Result<()> {
    match option {
        "--data-urlencode" => parsed.data.push(BodyPart::UrlEncoded(value.to_string())),
        "--json" => {
            reject_body_file(value, option, request_id)?;
            parsed.data.push(BodyPart::Raw(value.to_string()));
            insert_header_if_missing(&mut parsed.headers, "Content-Type", "application/json");
            insert_header_if_missing(&mut parsed.headers, "Accept", "application/json");
        }
        "--data-raw" => parsed.data.push(BodyPart::Raw(value.to_string())),
        _ => {
            reject_body_file(value, option, request_id)?;
            parsed.data.push(BodyPart::Raw(value.to_string()));
        }
    }
    Ok(())
}

fn reject_body_file(value: &str, option: &str, request_id: &str) -> Result<()> {
    if value.starts_with('@') {
        bail!(
            "接口 {} 的 curl 参数 {} 不支持通过 @ 读取请求体文件，请使用 --form 上传文件",
            request_id,
            option
        )
    }
    Ok(())
}

fn insert_header_if_missing(headers: &mut BTreeMap<String, String>, name: &str, value: &str) {
    if !headers.keys().any(|key| key.eq_ignore_ascii_case(name)) {
        headers.insert(name.to_string(), value.to_string());
    }
}

fn is_ignored_curl_flag(value: &str) -> bool {
    matches!(
        value,
        "--location"
            | "--compressed"
            | "--silent"
            | "--show-error"
            | "--fail"
            | "--fail-with-body"
            | "--globoff"
            | "--path-as-is"
            | "--http1.1"
            | "--http2"
            | "--http2-prior-knowledge"
            | "--no-buffer"
            | "--verbose"
            | "-s"
            | "-S"
            | "-f"
            | "-v"
            | "-i"
            | "-sS"
    )
}

fn validate_declared_variables(config: &RequestConfig) -> Result<()> {
    let declared = config.variables.keys().collect::<BTreeSet<_>>();
    for request in &config.requests {
        for name in crate::template::variable_names(request) {
            if !declared.contains(&name) {
                bail!("接口 {} 使用了未声明的变量: {name}", request.name,)
            }
        }
    }
    Ok(())
}

fn validate_file(request_id: &str, file: &FileUpload) -> Result<()> {
    if file.field.trim().is_empty() {
        bail!("接口 {} 的上传文件缺少 field", request_id)
    }
    if file.path.trim().is_empty() {
        bail!("接口 {} 的上传文件缺少 path", request_id)
    }
    Ok(())
}

fn normalize_extract(request_id: &str, extract: &mut ResponseExtract) -> Result<()> {
    let raw_variable = extract.variable.clone();
    let Some(variable) = normalize_variable_name(&raw_variable) else {
        bail!(
            "接口 {} 的响应提取 variable 无效: {}",
            request_id,
            raw_variable
        )
    };
    if extract.path.trim().is_empty() {
        bail!("接口 {} 的响应提取缺少 path", request_id)
    }
    extract.variable = variable.clone();
    extract.path = extract.path.trim().to_string();
    if extract.name.trim().is_empty() {
        extract.name = variable;
    }
    Ok(())
}

fn normalize_variable_name(value: &str) -> Option<String> {
    let value = value.trim();
    let value = if value.starts_with("{{") || value.ends_with("}}") {
        value.strip_prefix("{{")?.strip_suffix("}}")?.trim()
    } else {
        value
    };
    if value.is_empty() || value.contains('{') || value.contains('}') {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_simplified_config() {
        let config = load(Path::new(".postui/requests.yaml")).expect("示例请求配置应当可以加载");
        assert_eq!(config.requests.len(), 17);
        assert_eq!(config.requests[0].method, "GET");
        assert_eq!(
            config
                .file_directory
                .file_name()
                .and_then(|value| value.to_str()),
            Some("files")
        );
        assert_eq!(config.requests[2].files[0].path, "{{review_file}}");
        assert_eq!(config.requests[2].extracts[0].variable, "file_id");
        assert_eq!(config.requests[2].extracts[0].path, "data.fileId");
        assert!(
            config.requests[3]
                .body_parts
                .iter()
                .any(|part| matches!(part, BodyPart::Raw(body) if body.contains("{{file_id}}")))
        );
        assert!(!config.variables.contains_key("host"));
        assert!(config.variables["task_id"].default.is_none());
    }

    #[test]
    fn parses_fenced_curl_with_headers_and_upload() {
        let fence = char::from(96).to_string().repeat(3);
        let source = format!(
            "{fence}bash\ncurl --request POST 'http://{{{{host}}}}/upload' \\\n  --header 'X-Demo: {{{{token}}}}' \\\n  --form 'file=@{{{{path}}}};type=text/plain;filename={{{{name}}}}'\n{fence}"
        );
        let parsed = parse_curl(&source, "upload").expect("curl 代码块应当可以解析");

        assert_eq!(parsed.method.as_deref(), Some("POST"));
        assert_eq!(parsed.url.as_deref(), Some("http://{{host}}/upload"));
        assert_eq!(
            parsed.headers.get("X-Demo").map(String::as_str),
            Some("{{token}}")
        );
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.files[0].path, "{{path}}");
        assert_eq!(parsed.files[0].content_type.as_deref(), Some("text/plain"));
        assert_eq!(parsed.files[0].filename.as_deref(), Some("{{name}}"));
    }

    #[test]
    fn parses_json_data_without_rewriting_the_body() {
        let parsed = parse_curl(
            "curl --json '{\"taskId\":\"{{task_id}}\",\"ok\":true}' http://example.test/tasks",
            "create",
        )
        .expect("json curl 应当可以解析");

        assert_eq!(parsed.method.as_deref(), Some("POST"));
        assert_eq!(
            parsed.data,
            vec![BodyPart::Raw(
                "{\"taskId\":\"{{task_id}}\",\"ok\":true}".to_string()
            )]
        );
        assert_eq!(
            parsed.headers.get("Content-Type").map(String::as_str),
            Some("application/json")
        );
    }

    #[test]
    fn keeps_urlencoded_data_until_template_resolution() {
        let parsed = parse_curl(
            "curl --get 'http://example.test/search' --data-urlencode 'q={{query}}'",
            "search",
        )
        .expect("urlencoded 参数应当可以解析");

        assert_eq!(parsed.url.as_deref(), Some("http://example.test/search"));
        assert_eq!(
            parsed.query_data,
            vec![BodyPart::UrlEncoded("q={{query}}".to_string())]
        );
    }
}
