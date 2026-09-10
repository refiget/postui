use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RequestConfig {
    pub(crate) name: String,
    pub(crate) file_directory: PathBuf,
    pub(crate) download_directory: PathBuf,
    pub(crate) variables: BTreeMap<String, VariableDefinition>,
    pub(crate) requests: Vec<ApiRequest>,
    pub(crate) timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VariableDefinition {
    pub(crate) default: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ApiRequest {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) timeout_seconds: u64,
    pub(crate) description: String,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body_parts: Vec<BodyPart>,
    pub(crate) query_parts: Vec<BodyPart>,
    pub(crate) form: BTreeMap<String, String>,
    pub(crate) files: Vec<FileUpload>,
    pub(crate) download: Option<DownloadTarget>,
    pub(crate) extracts: Vec<ResponseExtract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DownloadTarget {
    Path(String),
    RemoteName { use_content_disposition: bool },
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum BodyPart {
    Raw(String),
    UrlEncoded(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FileUpload {
    pub(crate) field: String,
    pub(crate) path: String,
    pub(crate) filename: Option<String>,
    pub(crate) content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResponseExtract {
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
    #[serde(default = "default_download_directory")]
    download_directory: PathBuf,
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
    timeout_seconds: Option<u64>,
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
    download: Option<DownloadTarget>,
    remote_header_name: bool,
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

fn default_download_directory() -> PathBuf {
    PathBuf::from("tmp")
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

    let (config, cache_hit) = match crate::cache::load(path, text.as_bytes()) {
        Some(config) => match validate_declared_variables(&config) {
            Ok(()) => (config, true),
            Err(error) => {
                tracing::debug!(
                    path = %path.display(),
                    error = ?error,
                    "请求配置缓存内容无效，重新解析配置"
                );
                let config = parse_source(path, &text, &extension)?;
                (config, false)
            }
        },
        None => {
            let config = parse_source(path, &text, &extension)?;
            (config, false)
        }
    };

    if !cache_hit && let Err(error) = crate::cache::store(path, text.as_bytes(), &config) {
        tracing::debug!(
            path = %path.display(),
            error = ?error,
            "请求配置缓存写入失败，继续使用解析结果"
        );
    }

    tracing::debug!(
        name = %config.name,
        request_count = config.requests.len(),
        variable_count = config.variables.len(),
        timeout_seconds = config.timeout_seconds,
        file_directory = %config.file_directory.display(),
        download_directory = %config.download_directory.display(),
        cache_hit,
        "配置文件加载完成"
    );
    Ok(config)
}

fn parse_source(path: &Path, text: &str, extension: &str) -> Result<RequestConfig> {
    let raw: RawRequestConfig = if extension == "json" {
        serde_json::from_str(text).with_context(|| "JSON 配置格式无效")?
    } else {
        serde_yaml::from_str(text).with_context(|| "YAML 配置格式无效")?
    };

    let base_directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    normalize_at(raw, base_directory)
}

fn normalize_at(raw: RawRequestConfig, base_directory: &Path) -> Result<RequestConfig> {
    if raw.requests.is_empty() {
        bail!("配置文件中至少需要一个接口")
    }

    let variables = normalize_variables(raw.variables)?;
    let timeout_seconds = if raw.timeout_seconds == 0 {
        default_timeout_seconds()
    } else {
        raw.timeout_seconds
    };

    let file_directory = resolve_directory(
        base_directory,
        &raw.file_directory,
        default_file_directory(),
        "file_directory",
    )?;
    let download_directory = resolve_directory(
        base_directory,
        &raw.download_directory,
        default_download_directory(),
        "download_directory",
    )?;

    let mut request_ids = BTreeSet::new();
    let mut requests = Vec::with_capacity(raw.requests.len());
    for (index, raw_request) in raw.requests.into_iter().enumerate() {
        let request = normalize_request(raw_request, index, timeout_seconds)?;
        if !request_ids.insert(request.id.clone()) {
            bail!("接口 id 重复: {}", request.id)
        }
        tracing::debug!(
            request_id = %request.id,
            method = %request.method,
            url = %request.url,
            timeout_seconds = request.timeout_seconds,
            header_count = request.headers.len(),
            form_field_count = request.form.len(),
            file_count = request.files.len(),
            extract_count = request.extracts.len(),
            body_part_count = request.body_parts.len(),
            query_part_count = request.query_parts.len(),
            download = ?request.download,
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
        file_directory,
        download_directory,
        variables,
        requests,
        timeout_seconds,
    };
    validate_declared_variables(&config)?;
    Ok(config)
}

fn resolve_directory(
    base_directory: &Path,
    configured_path: &Path,
    default_path: PathBuf,
    field: &str,
) -> Result<PathBuf> {
    let configured_path = if configured_path.as_os_str().is_empty() {
        default_path
    } else {
        configured_path.to_path_buf()
    };
    let resolved_path = if configured_path.is_absolute() {
        configured_path.clone()
    } else {
        base_directory.join(&configured_path)
    };
    let resolved_path = normalize_path(&resolved_path);
    if resolved_path.exists() && !resolved_path.is_dir() {
        bail!("配置项 {field} 不是目录: {}", resolved_path.display())
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

fn normalize_path(path: &Path) -> PathBuf {
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

fn normalize_request(
    raw: RawApiRequest,
    index: usize,
    default_timeout_seconds: u64,
) -> Result<ApiRequest> {
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
        timeout_seconds: raw
            .timeout_seconds
            .filter(|timeout| *timeout > 0)
            .unwrap_or(default_timeout_seconds),
        description: raw.description.trim().to_string(),
        headers: parsed.headers,
        body_parts: parsed.data,
        query_parts: parsed.query_data,
        form: parsed.form,
        files: parsed.files,
        download: parsed.download,
        extracts: raw
            .extract
            .into_iter()
            .map(|(variable, path)| ResponseExtract { variable, path })
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
    parse_curl_tokens(&mut parsed, &tokens[start + 1..], request_id)?;

    let Some(url) = parsed.url.take() else {
        bail!("接口 {} 的 curl 命令缺少 URL", request_id)
    };
    if parsed.get_mode {
        parsed.query_data = std::mem::take(&mut parsed.data);
    }
    parsed.url = Some(url);
    let has_data = !parsed.data.is_empty();
    let has_form = !parsed.form.is_empty() || !parsed.files.is_empty();
    if has_data {
        if parsed.method.is_none() {
            parsed.method = Some("POST".to_string());
        }
        insert_header_if_missing(
            &mut parsed.headers,
            "Content-Type",
            "application/x-www-form-urlencoded",
        );
    } else if has_form && parsed.method.is_none() {
        parsed.method = Some("POST".to_string());
    }
    if has_data && has_form {
        bail!("接口 {} 的 curl 命令不能同时使用 data 和 form", request_id)
    }
    if parsed.download.is_none() && accepts_binary_response(&parsed.headers) {
        parsed.download = Some(DownloadTarget::Auto);
    }
    Ok(parsed)
}

fn parse_curl_tokens(
    parsed: &mut ParsedCommand,
    tokens: &[String],
    request_id: &str,
) -> Result<()> {
    let mut stop_options = false;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if stop_options {
            set_url(&mut parsed.url, token, request_id)?;
            index += 1;
            continue;
        }
        if token == "--" {
            stop_options = true;
            index += 1;
            continue;
        }
        parse_curl_option(parsed, tokens, &mut index, request_id)?;
        index += 1;
    }
    Ok(())
}

fn parse_curl_option(
    parsed: &mut ParsedCommand,
    tokens: &[String],
    index: &mut usize,
    request_id: &str,
) -> Result<()> {
    let token = &tokens[*index];
    if let Some((option, value)) = attached_option(token) {
        return parse_option_value(parsed, option, value, request_id);
    }

    if is_value_option(token) {
        let value = next_argument(tokens, index, token, request_id)?;
        return parse_option_value(parsed, token, &value, request_id);
    }

    match token.as_str() {
        "-G" | "--get" => {
            parsed.get_mode = true;
            parsed.method = Some("GET".to_string());
        }
        "-O" | "--remote-name" | "--remote-name-all" => {
            set_remote_name(parsed, request_id)?;
        }
        "-J" | "--remote-header-name" => {
            parsed.remote_header_name = true;
            enable_content_disposition(&mut parsed.download);
        }
        _ if is_ignored_curl_flag(token) => {}
        _ if is_download_flag_cluster(token) => {
            parse_download_flag_cluster(parsed, token, request_id)?;
        }
        _ if token.starts_with('-') => {
            bail!("接口 {} 的 curl 参数暂不支持: {}", request_id, token)
        }
        _ => {
            set_url(&mut parsed.url, token, request_id)?;
        }
    }
    Ok(())
}

fn attached_option(token: &str) -> Option<(&str, &str)> {
    for option in ["-X", "-H", "-d", "-F", "-o"] {
        if let Some(value) = token.strip_prefix(option).filter(|value| !value.is_empty()) {
            return Some((option, value));
        }
    }

    let (option, value) = token.split_once('=')?;
    is_value_option(option).then_some((option, value))
}

fn is_value_option(option: &str) -> bool {
    matches!(
        option,
        "-X" | "--request"
            | "-H"
            | "--header"
            | "-d"
            | "--url"
            | "--data"
            | "--data-ascii"
            | "--data-binary"
            | "--data-raw"
            | "--data-urlencode"
            | "--json"
            | "-F"
            | "--form"
            | "--form-string"
            | "-o"
            | "--output"
            | "-b"
            | "--cookie"
            | "-A"
            | "--user-agent"
            | "-e"
            | "--referer"
    )
}

fn parse_option_value(
    parsed: &mut ParsedCommand,
    option: &str,
    value: &str,
    request_id: &str,
) -> Result<()> {
    match option {
        "-X" | "--request" => parsed.method = Some(value.to_string()),
        "-H" | "--header" => parse_header(&mut parsed.headers, value, request_id)?,
        "-d" | "--data" | "--data-ascii" | "--data-binary" | "--data-raw" | "--data-urlencode"
        | "--json" => parse_body_argument(parsed, option, value, request_id)?,
        "-F" | "--form" => parse_form(parsed, value, request_id)?,
        "--form-string" => parse_form_string(&mut parsed.form, value, request_id)?,
        "-o" | "--output" => set_download_path(parsed, value, request_id)?,
        "-b" | "--cookie" => {
            parsed
                .headers
                .insert("Cookie".to_string(), value.to_string());
        }
        "-A" | "--user-agent" => {
            parsed
                .headers
                .insert("User-Agent".to_string(), value.to_string());
        }
        "-e" | "--referer" => {
            parsed
                .headers
                .insert("Referer".to_string(), value.to_string());
        }
        "--url" => set_url(&mut parsed.url, value, request_id)?,
        _ => unreachable!("unsupported curl value option: {option}"),
    }
    Ok(())
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
        } else if let Some(line) = line.strip_suffix(char::from(96)) {
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
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("curl") || value.eq_ignore_ascii_case("curl.exe")
        })
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

fn set_download_path(parsed: &mut ParsedCommand, value: &str, request_id: &str) -> Result<()> {
    let value = value.trim();
    if value.is_empty() {
        bail!("接口 {} 的 curl 输出文件路径不能为空", request_id)
    }
    if value == "-" {
        return Ok(());
    }
    if parsed.download.is_some() {
        bail!("接口 {} 的 curl 命令包含多个下载目标", request_id)
    }
    parsed.download = Some(DownloadTarget::Path(value.to_string()));
    Ok(())
}

fn set_remote_name(parsed: &mut ParsedCommand, request_id: &str) -> Result<()> {
    if parsed.download.is_some() {
        bail!("接口 {} 的 curl 命令包含多个下载目标", request_id)
    }
    parsed.download = Some(DownloadTarget::RemoteName {
        use_content_disposition: parsed.remote_header_name,
    });
    Ok(())
}

fn enable_content_disposition(download: &mut Option<DownloadTarget>) {
    if let Some(DownloadTarget::RemoteName {
        use_content_disposition,
    }) = download
    {
        *use_content_disposition = true;
    }
}

fn is_download_flag_cluster(value: &str) -> bool {
    value.len() > 2
        && value.starts_with('-')
        && value[1..].chars().all(|flag| matches!(flag, 'O' | 'J'))
}

fn parse_download_flag_cluster(
    parsed: &mut ParsedCommand,
    value: &str,
    request_id: &str,
) -> Result<()> {
    for flag in value[1..].chars() {
        match flag {
            'O' => set_remote_name(parsed, request_id)?,
            'J' => {
                parsed.remote_header_name = true;
                enable_content_disposition(&mut parsed.download);
            }
            _ => unreachable!("validated download flag cluster"),
        }
    }
    Ok(())
}

fn accepts_binary_response(headers: &BTreeMap<String, String>) -> bool {
    headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("accept")
            && value.split(',').any(|media_type| {
                let media_type = media_type.trim().to_ascii_lowercase();
                is_binary_media_type(&media_type)
            })
    })
}

fn is_binary_media_type(value: &str) -> bool {
    !value.ends_with("+json")
        && !value.ends_with("+xml")
        && (value == "application/octet-stream"
            || value == "application/pdf"
            || value == "application/zip"
            || value.starts_with("application/vnd.")
            || value.starts_with("image/")
            || value.starts_with("audio/")
            || value.starts_with("video/"))
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
    extract.variable = variable;
    extract.path = extract.path.trim().to_string();
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
        assert_eq!(config.requests.len(), 12);
        assert_eq!(config.requests[0].method, "GET");
        assert_eq!(
            config
                .file_directory
                .file_name()
                .and_then(|value| value.to_str()),
            Some("files")
        );
        assert_eq!(config.download_directory, PathBuf::from(".postui/tmp"));
        assert_eq!(config.requests[4].files[0].path, "{{demo_file}}");
        assert!(config.requests[0].extracts.iter().any(|extract| {
            extract.variable == "echoed_source" && extract.path == "args.source"
        }));
        assert!(
            config.requests[1].body_parts.iter().any(
                |part| matches!(part, BodyPart::Raw(body) if body.contains("{{demo_message}}"))
            )
        );
        assert!(config.variables["request_uuid"].default.is_none());
    }

    #[test]
    fn resolves_directories_relative_to_the_request_config() {
        let raw: RawRequestConfig = serde_yaml::from_str(
            r#"
name: path-test
file_directory: ../files
download_directory: ../temp
requests:
  - id: download
    name: Download
    request: curl --output result.bin https://example.test/file
"#,
        )
        .expect("目录配置应当可以解析");

        let config = normalize_at(raw, Path::new("/workspace/project/.postui"))
            .expect("目录配置应当可以规范化");

        assert_eq!(
            config.file_directory,
            PathBuf::from("/workspace/project/files")
        );
        assert_eq!(
            config.download_directory,
            PathBuf::from("/workspace/project/temp")
        );
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
    fn parses_powershell_curl_with_backtick_continuations() {
        let parsed = parse_curl(
            "curl.exe --request POST `\n  --url \"https://example.test/tasks/{{task_id}}\" `\n  --header \"Content-Type: application/json\" `\n  --data-raw '{\"ok\":true}'",
            "powershell",
        )
        .expect("PowerShell curl.exe 应当可以解析");

        assert_eq!(parsed.method.as_deref(), Some("POST"));
        assert_eq!(
            parsed.url.as_deref(),
            Some("https://example.test/tasks/{{task_id}}")
        );
        assert_eq!(parsed.data.len(), 1);
        assert_eq!(
            parsed.headers.get("Content-Type").map(String::as_str),
            Some("application/json")
        );
    }

    #[test]
    fn applies_request_timeout_over_the_collection_default() {
        let raw: RawRequestConfig = serde_yaml::from_str(
            r#"
name: timeout-test
timeout_seconds: 12
requests:
  - id: inherited
    name: Inherited
    request: curl https://example.test/inherited
  - id: custom
    name: Custom
    timeout_seconds: 4
    request: curl https://example.test/custom
  - id: fallback
    name: Fallback
    timeout_seconds: 0
    request: curl https://example.test/fallback
"#,
        )
        .expect("超时配置应当可以解析");

        let config = normalize_at(raw, Path::new(".")).expect("超时配置应当可以规范化");
        assert_eq!(config.timeout_seconds, 12);
        assert_eq!(config.requests[0].timeout_seconds, 12);
        assert_eq!(config.requests[1].timeout_seconds, 4);
        assert_eq!(config.requests[2].timeout_seconds, 12);
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

    #[test]
    fn parses_curl_download_targets() {
        let output = parse_curl(
            "curl --location --output '{{download_name}}' https://example.test/report",
            "download",
        )
        .expect("output 参数应当可以解析");
        assert_eq!(
            output.download,
            Some(DownloadTarget::Path("{{download_name}}".to_string()))
        );

        let remote_name = parse_curl(
            "curl -OJ https://example.test/reports/latest",
            "remote-download",
        )
        .expect("remote name 参数应当可以解析");
        assert_eq!(
            remote_name.download,
            Some(DownloadTarget::RemoteName {
                use_content_disposition: true
            })
        );

        let automatic = parse_curl(
            "curl --header 'accept: application/octet-stream' https://example.test/file",
            "automatic-download",
        )
        .expect("二进制 Accept 应当可以解析");
        assert_eq!(automatic.download, Some(DownloadTarget::Auto));

        let json_response = parse_curl(
            "curl --header 'Accept: application/vnd.api+json' https://example.test/data",
            "json-response",
        )
        .expect("JSON media type 应当可以解析");
        assert_eq!(json_response.download, None);
    }
}
