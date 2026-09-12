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
    pub(crate) headers: Vec<NameValue>,
    pub(crate) variables: BTreeMap<String, VariableDefinition>,
    #[serde(default)]
    pub(crate) editable_variables: BTreeSet<String>,
    pub(crate) requests: Vec<ApiRequest>,
    pub(crate) timeout_seconds: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceConfig {
    pub(crate) name: String,
    pub(crate) file_directory: PathBuf,
    pub(crate) download_directory: PathBuf,
    pub(crate) headers: Vec<NameValue>,
    pub(crate) variables: BTreeMap<String, VariableDefinition>,
    pub(crate) editable_variables: BTreeSet<String>,
    pub(crate) timeout_seconds: u64,
}

impl RequestConfig {
    pub(crate) fn into_workspace(self) -> (WorkspaceConfig, Vec<ApiRequest>) {
        let Self {
            name,
            file_directory,
            download_directory,
            headers,
            variables,
            editable_variables,
            requests,
            timeout_seconds,
        } = self;
        (
            WorkspaceConfig {
                name,
                file_directory,
                download_directory,
                headers,
                variables,
                editable_variables,
                timeout_seconds,
            },
            requests,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VariableDefinition {
    pub(crate) default: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NameValue {
    pub(crate) name: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// 一个有序的请求参数。`has_equals` 用于区分 `flag` 和 `flag=`。
pub(crate) struct RequestParam {
    pub(crate) name: String,
    pub(crate) value: String,
    #[serde(default = "default_has_equals")]
    pub(crate) has_equals: bool,
}

impl RequestParam {
    pub(crate) fn new(name: String, value: String, has_equals: bool) -> Self {
        Self {
            name,
            value,
            has_equals,
        }
    }

    pub(crate) fn from_text(value: &str) -> Self {
        if let Some((name, value)) = value.split_once('=') {
            Self::new(name.to_string(), value.to_string(), true)
        } else {
            Self::new(value.to_string(), String::new(), false)
        }
    }

    pub(crate) fn to_text(&self) -> String {
        if self.has_equals || !self.value.is_empty() {
            format!("{}={}", self.name, self.value)
        } else {
            self.name.clone()
        }
    }
}

fn default_has_equals() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ApiRequest {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) timeout_seconds: u64,
    pub(crate) description: String,
    pub(crate) headers: Vec<NameValue>,
    pub(crate) body_parts: Vec<DataPart>,
    pub(crate) query_parts: Vec<DataPart>,
    pub(crate) form: Vec<RequestParam>,
    pub(crate) files: Vec<FileUpload>,
    pub(crate) extracts: Vec<ResponseExtract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// curl data 的一个片段；URL 编码片段保存逻辑参数，发送时再编码。
pub(crate) enum DataPart {
    Raw(String),
    UrlEncoded(RequestParam),
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
struct RawWorkspaceConfig {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    directories: RawDirectories,
    #[serde(default)]
    variables: BTreeMap<String, Option<Value>>,
    #[serde(default)]
    headers: Vec<NameValue>,
    #[serde(default = "default_timeout_seconds")]
    timeout: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDirectories {
    #[serde(default = "default_upload_directory")]
    uploads: PathBuf,
    #[serde(default = "default_download_directory")]
    downloads: PathBuf,
}

impl Default for RawDirectories {
    fn default() -> Self {
        Self {
            uploads: default_upload_directory(),
            downloads: default_download_directory(),
        }
    }
}

impl Default for RawWorkspaceConfig {
    fn default() -> Self {
        Self {
            name: None,
            directories: RawDirectories::default(),
            variables: BTreeMap::new(),
            headers: Vec::new(),
            timeout: default_timeout_seconds(),
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
    headers: Vec<NameValue>,
    data: Vec<DataPart>,
    query_data: Vec<DataPart>,
    form: Vec<RequestParam>,
    files: Vec<FileUpload>,
    get_mode: bool,
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_timeout_seconds() -> u64 {
    30
}

fn default_upload_directory() -> PathBuf {
    PathBuf::from("test_files")
}

fn default_download_directory() -> PathBuf {
    PathBuf::from("temp")
}

pub(crate) fn load(workspace_path: &Path) -> Result<RequestConfig> {
    if !workspace_path.is_dir() {
        bail!("PostUI 工作区必须是目录: {}", workspace_path.display())
    }

    let workspace_config_path = workspace_path.join("postui.yaml");
    let workspace_config = read_optional_file(&workspace_config_path)?;
    let request_files = read_request_files(&workspace_path.join("requests"))?;
    let fingerprint =
        workspace_fingerprint(workspace_path, workspace_config.as_deref(), &request_files);
    tracing::debug!(
        path = %workspace_path.display(),
        config_path = %workspace_config_path.display(),
        config_present = workspace_config.is_some(),
        request_count = request_files.len(),
        "读取工作区"
    );

    let (config, cache_hit) = match crate::cache::load(workspace_path, &fingerprint) {
        Some(config) => (config, true),
        None => {
            let config = parse_workspace_config(
                &workspace_config_path,
                workspace_config.as_deref(),
                workspace_path,
                &request_files,
            )?;
            (config, false)
        }
    };

    if !cache_hit && let Err(error) = crate::cache::store(workspace_path, &fingerprint, &config) {
        tracing::debug!(
            path = %workspace_path.display(),
            error = ?error,
            "工作区缓存写入失败，继续使用解析结果"
        );
    }

    tracing::debug!(
        name = %config.name,
        request_count = config.requests.len(),
        variable_count = config.variables.len(),
        workspace_header_count = config.headers.len(),
        timeout_seconds = config.timeout_seconds,
        file_directory = %config.file_directory.display(),
        download_directory = %config.download_directory.display(),
        cache_hit,
        "配置文件加载完成"
    );
    Ok(config)
}

fn parse_workspace_config(
    path: &Path,
    text: Option<&str>,
    workspace_path: &Path,
    request_files: &[RequestFile],
) -> Result<RequestConfig> {
    let raw = match text {
        Some(text) => serde_saphyr::from_str(text)
            .with_context(|| format!("YAML 配置格式无效: {}", path.display()))?,
        None => RawWorkspaceConfig::default(),
    };
    normalize_config(raw, workspace_path, request_files)
}

fn normalize_config(
    raw: RawWorkspaceConfig,
    workspace_path: &Path,
    request_files: &[RequestFile],
) -> Result<RequestConfig> {
    let mut variables = normalize_variables(raw.variables)?;
    let editable_variables = variables.keys().cloned().collect();
    let headers = normalize_headers(raw.headers)?;
    let timeout_seconds = if raw.timeout == 0 {
        default_timeout_seconds()
    } else {
        raw.timeout
    };

    let file_directory = resolve_directory(
        workspace_path,
        &raw.directories.uploads,
        "directories.uploads",
    )?;
    let download_directory = resolve_directory(
        workspace_path,
        &raw.directories.downloads,
        "directories.downloads",
    )?;

    let mut request_ids = BTreeSet::new();
    let mut requests = Vec::with_capacity(request_files.len());
    for file in request_files {
        let raw_request = parse_request_file(file, workspace_path)?;
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
    for header in &headers {
        for variable in crate::template::variable_names_in_text(&header.name)
            .into_iter()
            .chain(crate::template::variable_names_in_text(&header.value))
        {
            variables
                .entry(variable)
                .or_insert_with(|| VariableDefinition { default: None });
        }
    }

    let config = RequestConfig {
        name: raw
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .or_else(|| {
                workspace_path
                    .parent()
                    .and_then(Path::file_name)
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "PostUI".to_string()),
        file_directory,
        download_directory,
        headers,
        variables,
        editable_variables,
        requests,
        timeout_seconds,
    };
    Ok(config)
}

fn read_optional_file(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("无法读取工作区配置: {}", path.display())),
    }
}

fn read_request_files(requests_directory: &Path) -> Result<Vec<RequestFile>> {
    if !requests_directory.is_dir() {
        return Ok(Vec::new());
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

fn workspace_fingerprint(
    workspace_path: &Path,
    workspace_config: Option<&str>,
    request_files: &[RequestFile],
) -> blake3::Hash {
    let mut fingerprint = blake3::Hasher::new();
    append_fingerprint_part(&mut fingerprint, b"postui.yaml");
    append_fingerprint_part(
        &mut fingerprint,
        workspace_config.unwrap_or("<missing-config>").as_bytes(),
    );
    for file in request_files {
        let relative = file
            .path
            .strip_prefix(workspace_path)
            .unwrap_or(&file.path)
            .to_string_lossy()
            .replace('\\', "/");
        append_fingerprint_part(&mut fingerprint, relative.as_bytes());
        append_fingerprint_part(&mut fingerprint, file.text.as_bytes());
    }
    fingerprint.finalize()
}

fn append_fingerprint_part(fingerprint: &mut blake3::Hasher, part: &[u8]) {
    fingerprint.update(&(part.len() as u64).to_le_bytes());
    fingerprint.update(part);
}

fn parse_request_file(file: &RequestFile, workspace_path: &Path) -> Result<ParsedRequest> {
    let id = file
        .path
        .strip_prefix(workspace_path)
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

fn normalize_headers(raw_headers: Vec<NameValue>) -> Result<Vec<NameValue>> {
    let mut headers = Vec::with_capacity(raw_headers.len());
    for mut header in raw_headers {
        let name = header.name.trim().to_string();
        if name.is_empty() {
            bail!("工作区 Header 名称不能为空")
        }
        header.name = name;
        headers.push(header);
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
        extracts: raw
            .extract
            .into_iter()
            .map(|(variable, path)| ResponseExtract { variable, path })
            .collect(),
    };

    for file in &request.files {
        validate_file(&request.id, file)?;
    }
    let mut extract_variables = BTreeSet::new();
    for extract in &mut request.extracts {
        normalize_extract(&request.id, extract)?;
        if !extract_variables.insert(extract.variable.clone()) {
            bail!(
                "接口 {} 重复声明响应提取变量: {}",
                request.id,
                extract.variable
            )
        }
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
