use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod curl;

use curl::parse_curl;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RequestConfig {
    pub(crate) name: String,
    pub(crate) file_directory: PathBuf,
    pub(crate) download_directory: PathBuf,
    pub(crate) headers: BTreeMap<String, String>,
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
struct RawCollectionConfig {
    #[serde(default = "default_name")]
    name: String,
    #[serde(default = "default_file_directory")]
    file_directory: PathBuf,
    #[serde(default = "default_download_directory")]
    download_directory: PathBuf,
    #[serde(default)]
    variables: BTreeMap<String, Option<Value>>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default = "default_timeout_seconds")]
    timeout_seconds: u64,
}

impl Default for RawCollectionConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            file_directory: default_file_directory(),
            download_directory: default_download_directory(),
            variables: BTreeMap::new(),
            headers: BTreeMap::new(),
            timeout_seconds: default_timeout_seconds(),
        }
    }
}

#[derive(Debug, Clone)]
struct RequestFile {
    path: PathBuf,
    text: String,
}

#[derive(Debug, Clone)]
struct ParsedRequest {
    name: String,
    id: String,
    timeout_seconds: Option<u64>,
    description: String,
    request: String,
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

pub(crate) fn load(collection_path: &Path) -> Result<RequestConfig> {
    if !collection_path.is_dir() {
        bail!("请求集合必须是目录: {}", collection_path.display())
    }

    let collection_config_path = collection_path.join("config.yaml");
    let collection_config = read_optional_file(&collection_config_path)?;
    let request_files = read_request_files(&collection_path.join("requests"))?;
    let fingerprint = collection_fingerprint(
        collection_path,
        collection_config.as_deref(),
        &request_files,
    );
    tracing::debug!(
        path = %collection_path.display(),
        config_path = %collection_config_path.display(),
        config_present = collection_config.is_some(),
        request_count = request_files.len(),
        "读取请求集合"
    );

    let (config, cache_hit) = match crate::cache::load(collection_path, &fingerprint) {
        Some(config) => (config, true),
        None => {
            let config = parse_collection_config(
                &collection_config_path,
                collection_config.as_deref(),
                collection_path,
                &request_files,
            )?;
            (config, false)
        }
    };

    if !cache_hit && let Err(error) = crate::cache::store(collection_path, &fingerprint, &config) {
        tracing::debug!(
            path = %collection_path.display(),
            error = ?error,
            "请求集合缓存写入失败，继续使用解析结果"
        );
    }

    tracing::debug!(
        name = %config.name,
        request_count = config.requests.len(),
        variable_count = config.variables.len(),
        collection_header_count = config.headers.len(),
        timeout_seconds = config.timeout_seconds,
        file_directory = %config.file_directory.display(),
        download_directory = %config.download_directory.display(),
        cache_hit,
        "配置文件加载完成"
    );
    Ok(config)
}

fn parse_collection_config(
    path: &Path,
    text: Option<&str>,
    collection_path: &Path,
    request_files: &[RequestFile],
) -> Result<RequestConfig> {
    let raw = match text {
        Some(text) => serde_yaml::from_str(text)
            .with_context(|| format!("YAML 配置格式无效: {}", path.display()))?,
        None => RawCollectionConfig::default(),
    };
    normalize_config(raw, collection_path, request_files)
}

fn normalize_config(
    raw: RawCollectionConfig,
    collection_path: &Path,
    request_files: &[RequestFile],
) -> Result<RequestConfig> {
    if request_files.is_empty() {
        bail!("请求集合中至少需要一个 .http、.rest 或 .curl 文件")
    }

    let mut variables = normalize_variables(raw.variables)?;
    let headers = normalize_headers(raw.headers)?;
    let timeout_seconds = if raw.timeout_seconds == 0 {
        default_timeout_seconds()
    } else {
        raw.timeout_seconds
    };

    let file_directory = resolve_directory(
        collection_path,
        &raw.file_directory,
        default_file_directory(),
        "file_directory",
    )?;
    let download_directory = resolve_directory(
        collection_path,
        &raw.download_directory,
        default_download_directory(),
        "download_directory",
    )?;

    let mut request_ids = BTreeSet::new();
    let mut requests = Vec::with_capacity(request_files.len());
    for file in request_files {
        let raw_request = parse_request_file(file, collection_path)?;
        let request = normalize_request(raw_request, timeout_seconds)?;
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

    for request in &requests {
        for name in crate::template::variable_names(request) {
            variables
                .entry(name)
                .or_insert_with(|| VariableDefinition { default: None });
        }
    }
    for (name, value) in &headers {
        for variable in crate::template::variable_names_in_text(name)
            .into_iter()
            .chain(crate::template::variable_names_in_text(value))
        {
            variables
                .entry(variable)
                .or_insert_with(|| VariableDefinition { default: None });
        }
    }

    let config = RequestConfig {
        name: if raw.name.trim().is_empty() {
            default_name()
        } else {
            raw.name.trim().to_string()
        },
        file_directory,
        download_directory,
        headers,
        variables,
        requests,
        timeout_seconds,
    };
    Ok(config)
}

fn read_optional_file(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("无法读取集合配置: {}", path.display())),
    }
}

fn read_request_files(requests_directory: &Path) -> Result<Vec<RequestFile>> {
    if !requests_directory.is_dir() {
        bail!(
            "请求集合缺少 requests 目录: {}",
            requests_directory.display()
        )
    }

    let mut paths = Vec::new();
    collect_request_paths(requests_directory, &mut paths)?;
    paths.sort();

    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("无法读取请求文件: {}", path.display()))?;
        tracing::debug!(path = %path.display(), bytes = text.len(), "读取请求文件");
        files.push(RequestFile { path, text });
    }
    if files.is_empty() {
        bail!(
            "请求集合中没有 .http、.rest 或 .curl 文件: {}",
            requests_directory.display()
        )
    }
    Ok(files)
}

fn collect_request_paths(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = fs::read_dir(directory)
        .with_context(|| format!("无法读取请求目录: {}", directory.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("无法枚举请求目录: {}", directory.display()))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("无法读取文件类型: {}", path.display()))?;
        if file_type.is_dir() {
            collect_request_paths(&path, paths)?;
        } else if file_type.is_file() && is_request_file(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

fn is_request_file(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "http" | "rest" | "curl"
            )
        })
}

fn collection_fingerprint(
    collection_path: &Path,
    collection_config: Option<&str>,
    request_files: &[RequestFile],
) -> Vec<u8> {
    let mut fingerprint = Vec::new();
    append_fingerprint_part(&mut fingerprint, b"config.yaml");
    append_fingerprint_part(
        &mut fingerprint,
        collection_config.unwrap_or("<missing-config>").as_bytes(),
    );
    for file in request_files {
        let relative = file
            .path
            .strip_prefix(collection_path)
            .unwrap_or(&file.path)
            .to_string_lossy()
            .replace('\\', "/");
        append_fingerprint_part(&mut fingerprint, relative.as_bytes());
        append_fingerprint_part(&mut fingerprint, file.text.as_bytes());
    }
    fingerprint
}

fn append_fingerprint_part(fingerprint: &mut Vec<u8>, part: &[u8]) {
    fingerprint.extend_from_slice(&(part.len() as u64).to_le_bytes());
    fingerprint.extend_from_slice(part);
}

fn parse_request_file(file: &RequestFile, collection_path: &Path) -> Result<ParsedRequest> {
    let id = file
        .path
        .strip_prefix(collection_path)
        .unwrap_or(&file.path)
        .to_string_lossy()
        .replace('\\', "/");
    let mut name = None;
    let mut description = None;
    let mut timeout_seconds = None;
    let mut extract = BTreeMap::new();

    for (line_number, line) in file.text.lines().enumerate() {
        let Some((directive, value)) = request_directive(line) else {
            continue;
        };
        match directive {
            "name" => {
                if name.replace(value.to_string()).is_some() {
                    bail!("请求 {} 重复声明 @name (第 {} 行)", id, line_number + 1)
                }
            }
            "description" => {
                if description.replace(value.to_string()).is_some() {
                    bail!(
                        "请求 {} 重复声明 @description (第 {} 行)",
                        id,
                        line_number + 1
                    )
                }
            }
            "timeout" => {
                let timeout = value
                    .parse::<u64>()
                    .with_context(|| format!("请求 {} 的 @timeout 无效: {}", id, value))?;
                if timeout_seconds.replace(timeout).is_some() {
                    bail!("请求 {} 重复声明 @timeout (第 {} 行)", id, line_number + 1)
                }
            }
            "extract" => {
                let Some((variable, path)) =
                    value.split_once('=').or_else(|| value.split_once(':'))
                else {
                    bail!(
                        "请求 {} 的 @extract 格式应为 variable = response.path (第 {} 行)",
                        id,
                        line_number + 1
                    )
                };
                let variable = variable.trim().to_string();
                let path = path.trim().to_string();
                if variable.is_empty() || path.is_empty() {
                    bail!(
                        "请求 {} 的 @extract 不能为空 (第 {} 行)",
                        id,
                        line_number + 1
                    )
                }
                if extract.insert(variable, path).is_some() {
                    bail!("请求 {} 重复声明 @extract (第 {} 行)", id, line_number + 1)
                }
            }
            other => bail!("请求 {} 使用了未知指令: @{}", id, other),
        }
    }

    let name = name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| request_display_name(&file.path));
    let description = description.unwrap_or_default();
    Ok(ParsedRequest {
        name,
        id,
        timeout_seconds,
        description,
        request: file.text.clone(),
        extract,
    })
}

fn request_directive(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    let directive = trimmed
        .strip_prefix("# @")
        .or_else(|| trimmed.strip_prefix("// @"))?;
    let (name, value) = directive.split_once(char::is_whitespace)?;
    Some((name, value.trim()))
}

fn request_display_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "request".to_string());
    let Some(separator) = stem.find(['-', '_']) else {
        return stem;
    };
    if separator > 0
        && stem[..separator]
            .chars()
            .all(|value| value.is_ascii_digit())
    {
        let display = stem[separator + 1..].trim();
        if !display.is_empty() {
            return display.to_string();
        }
    }
    stem
}

fn resolve_directory(
    collection_path: &Path,
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
        normalize_path(&configured_path)
    } else {
        normalize_path(&collection_path.join(&configured_path))
    };
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
    raw_variables: BTreeMap<String, Option<Value>>,
) -> Result<BTreeMap<String, VariableDefinition>> {
    let mut variables = BTreeMap::new();
    for (raw_name, default) in raw_variables {
        let Some(name) = normalize_variable_name(&raw_name) else {
            bail!("变量名称无效: {raw_name}")
        };
        if variables
            .insert(name.clone(), VariableDefinition { default })
            .is_some()
        {
            bail!("变量名称重复: {name}")
        }
    }
    Ok(variables)
}

fn normalize_headers(raw_headers: BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
    let mut headers = BTreeMap::<String, String>::new();
    for (raw_name, value) in raw_headers {
        let name = raw_name.trim().to_string();
        if name.is_empty() {
            bail!("集合 Header 名称不能为空")
        }
        if headers
            .keys()
            .any(|existing| existing.eq_ignore_ascii_case(&name))
        {
            bail!("集合 Header 名称重复: {name}")
        }
        headers.insert(name, value);
    }
    Ok(headers)
}

fn normalize_request(raw: ParsedRequest, default_timeout_seconds: u64) -> Result<ApiRequest> {
    let id = raw.id;
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
        let config = load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载");
        assert_eq!(config.requests.len(), 17);
        assert_eq!(config.requests[0].method, "GET");
        assert_eq!(
            config
                .file_directory
                .file_name()
                .and_then(|value| value.to_str()),
            Some("files")
        );
        assert_eq!(config.download_directory, PathBuf::from("mock/.postui/tmp"));
        assert_eq!(
            config
                .headers
                .get("X-PostUI-Collection")
                .map(String::as_str),
            Some("{{collection_name}}")
        );
        assert!(config.variables.contains_key("collection_name"));
        assert_eq!(config.requests[8].files[0].path, "{{upload_file}}");
        assert!(
            config.requests[0].extracts.iter().any(|extract| {
                extract.variable == "health_service" && extract.path == "service"
            })
        );
        assert!(
            config.requests[2]
                .body_parts
                .iter()
                .any(|part| matches!(part, BodyPart::Raw(body) if body.contains("{{task_id}}")))
        );
        assert!(config.variables["empty_file"].default.is_none());
    }

    #[test]
    fn resolves_directories_relative_to_the_request_config() {
        let raw: RawCollectionConfig = serde_yaml::from_str(
            r#"
name: path-test
file_directory: ../files
download_directory: ../temp
"#,
        )
        .expect("目录配置应当可以解析");

        let files = vec![RequestFile {
            path: PathBuf::from("/workspace/project/.postui/requests/download.http"),
            text: "curl --output result.bin https://example.test/file".to_string(),
        }];
        let config = normalize_config(raw, Path::new("/workspace/project/.postui"), &files)
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
        let raw: RawCollectionConfig = serde_yaml::from_str(
            r#"
name: timeout-test
timeout_seconds: 12
"#,
        )
        .expect("超时配置应当可以解析");

        let files = vec![
            RequestFile {
                path: PathBuf::from("inherited.http"),
                text: "curl https://example.test/inherited".to_string(),
            },
            RequestFile {
                path: PathBuf::from("custom.http"),
                text: "# @timeout 4\ncurl https://example.test/custom".to_string(),
            },
            RequestFile {
                path: PathBuf::from("fallback.http"),
                text: "# @timeout 0\ncurl https://example.test/fallback".to_string(),
            },
        ];
        let config = normalize_config(raw, Path::new("."), &files).expect("超时配置应当可以规范化");
        assert_eq!(config.timeout_seconds, 12);
        assert_eq!(config.requests[0].timeout_seconds, 12);
        assert_eq!(config.requests[1].timeout_seconds, 4);
        assert_eq!(config.requests[2].timeout_seconds, 12);
    }

    #[test]
    fn parses_request_metadata_and_discovers_variables() {
        let raw = RawCollectionConfig::default();
        let files = vec![RequestFile {
            path: PathBuf::from("requests/01-health.http"),
            text: "# @name Health check\n# @description A simple check\n# @extract service = data.service\ncurl http://{{host}}/health".to_string(),
        }];

        let config =
            normalize_config(raw, Path::new("."), &files).expect("请求文件元数据应当可以解析");
        assert_eq!(config.requests[0].id, "requests/01-health.http");
        assert_eq!(config.requests[0].name, "Health check");
        assert_eq!(config.requests[0].description, "A simple check");
        assert_eq!(config.requests[0].extracts[0].variable, "service");
        assert_eq!(config.requests[0].extracts[0].path, "data.service");
        assert!(config.variables.contains_key("host"));
        assert!(config.variables.contains_key("service"));
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
