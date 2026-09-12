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
    pub(crate) headers: Vec<NameValue>,
    pub(crate) variables: BTreeMap<String, VariableDefinition>,
    pub(crate) configurations: BTreeMap<String, WorkspaceConfiguration>,
    pub(crate) default_configuration: String,
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
    pub(crate) configurations: BTreeMap<String, WorkspaceConfiguration>,
    pub(crate) default_configuration: String,
    pub(crate) editable_variables: BTreeSet<String>,
}

impl RequestConfig {
    pub(crate) fn into_workspace(self) -> (WorkspaceConfig, Vec<ApiRequest>) {
        let Self {
            name,
            file_directory,
            download_directory,
            headers,
            variables,
            configurations,
            default_configuration,
            editable_variables,
            requests,
            timeout_seconds: _,
        } = self;
        (
            WorkspaceConfig {
                name,
                file_directory,
                download_directory,
                headers,
                variables,
                configurations,
                default_configuration,
                editable_variables,
            },
            requests,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
pub(crate) struct WorkspaceConfiguration {
    #[serde(default)]
    pub(crate) path: Option<PathBuf>,
    pub(crate) variables: BTreeMap<String, VariableDefinition>,
    pub(crate) headers: Vec<NameValue>,
    pub(crate) timeout_seconds: Option<u64>,
    pub(crate) request_overrides: BTreeMap<String, RequestOverride>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RequestOverride {
    pub(crate) method: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) timeout_seconds: Option<u64>,
    pub(crate) headers: Option<Vec<NameValue>>,
    pub(crate) query_parts: Option<Vec<DataPart>>,
    pub(crate) body_parts: Option<Vec<DataPart>>,
    pub(crate) form: Option<Vec<RequestParam>>,
    pub(crate) files: Option<Vec<FileUpload>>,
    pub(crate) extracts: Option<Vec<ResponseExtract>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestDocument {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) description: String,
    #[serde(default = "default_method")]
    pub(crate) method: String,
    #[serde(default)]
    pub(crate) url: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) timeout: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) headers: Vec<NameValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) params: Vec<RequestParam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) body: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) form: Vec<RequestParam>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) files: Vec<FileUpload>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) extracts: Vec<ResponseExtract>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigurationDocument {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) variables: BTreeMap<String, Option<Value>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) headers: Vec<NameValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) overrides: BTreeMap<String, RequestOverrideDocument>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestOverrideDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) headers: Option<Vec<NameValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) params: Option<Vec<RequestParam>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) form: Option<Vec<RequestParam>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) files: Option<Vec<FileUpload>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) extracts: Option<Vec<ResponseExtract>>,
}

impl ApiRequest {
    pub(crate) fn for_configuration(&self, configuration: &WorkspaceConfiguration) -> Self {
        let mut request = self.clone();
        if let Some(timeout_seconds) = configuration.timeout_seconds {
            request.timeout_seconds = timeout_seconds;
        }
        if let Some(request_override) = configuration.request_overrides.get(&self.id) {
            request_override.apply_to(&mut request);
        }
        request
    }
}

impl RequestOverride {
    pub(crate) fn is_empty(&self) -> bool {
        self.method.is_none()
            && self.url.is_none()
            && self.timeout_seconds.is_none()
            && self.headers.is_none()
            && self.query_parts.is_none()
            && self.body_parts.is_none()
            && self.form.is_none()
            && self.files.is_none()
            && self.extracts.is_none()
    }

    pub(crate) fn apply_to(&self, request: &mut ApiRequest) {
        if let Some(method) = &self.method {
            request.method = method.clone();
        }
        if let Some(url) = &self.url {
            request.url = url.clone();
        }
        if let Some(timeout_seconds) = self.timeout_seconds {
            request.timeout_seconds = timeout_seconds;
        }
        if let Some(headers) = &self.headers {
            request.headers = headers.clone();
        }
        if let Some(query_parts) = &self.query_parts {
            request.query_parts = query_parts.clone();
        }
        if let Some(body_parts) = &self.body_parts {
            request.body_parts = body_parts.clone();
        }
        if let Some(form) = &self.form {
            request.form = form.clone();
        }
        if let Some(files) = &self.files {
            request.files = files.clone();
        }
        if let Some(extracts) = &self.extracts {
            request.extracts = extracts.clone();
        }
    }
}

impl From<&ApiRequest> for RequestDocument {
    fn from(request: &ApiRequest) -> Self {
        Self {
            name: request.name.clone(),
            description: request.description.clone(),
            method: request.method.clone(),
            url: request.url.clone(),
            timeout: request.timeout_seconds,
            headers: request.headers.clone(),
            params: request
                .query_parts
                .iter()
                .map(request_param_from_part)
                .collect(),
            body: body_text(&request.body_parts),
            form: request.form.clone(),
            files: request.files.clone(),
            extracts: request.extracts.clone(),
        }
    }
}

impl From<&WorkspaceConfiguration> for ConfigurationDocument {
    fn from(configuration: &WorkspaceConfiguration) -> Self {
        Self {
            variables: configuration
                .variables
                .iter()
                .map(|(name, definition)| (name.clone(), definition.default.clone()))
                .collect(),
            headers: configuration.headers.clone(),
            timeout: configuration.timeout_seconds,
            overrides: configuration
                .request_overrides
                .iter()
                .map(|(request_id, request_override)| {
                    (
                        request_id.clone(),
                        RequestOverrideDocument::from(request_override),
                    )
                })
                .collect(),
        }
    }
}

impl From<&RequestOverride> for RequestOverrideDocument {
    fn from(request_override: &RequestOverride) -> Self {
        Self {
            method: request_override.method.clone(),
            url: request_override.url.clone(),
            timeout: request_override.timeout_seconds,
            headers: request_override.headers.clone(),
            params: request_override
                .query_parts
                .as_ref()
                .map(|parts| parts.iter().map(request_param_from_part).collect()),
            body: request_override
                .body_parts
                .as_ref()
                .map(|parts| body_text(parts).unwrap_or_default()),
            form: request_override.form.clone(),
            files: request_override.files.clone(),
            extracts: request_override.extracts.clone(),
        }
    }
}

fn request_param_from_part(part: &DataPart) -> RequestParam {
    match part {
        DataPart::Raw(value) => RequestParam::from_text(value),
        DataPart::UrlEncoded(parameter) => parameter.clone(),
    }
}

fn body_text(parts: &[DataPart]) -> Option<String> {
    (!parts.is_empty()).then(|| {
        parts
            .iter()
            .map(|part| match part {
                DataPart::Raw(value) => value.clone(),
                DataPart::UrlEncoded(parameter) => parameter.to_text(),
            })
            .collect::<Vec<_>>()
            .join("&")
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// 请求体或 URL 编码参数的一个片段；URL 编码片段保存逻辑参数，发送时再编码。
pub(crate) enum DataPart {
    Raw(String),
    UrlEncoded(RequestParam),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FileUpload {
    pub(crate) field: String,
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) filename: Option<String>,
    #[serde(default)]
    pub(crate) content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ResponseExtract {
    pub(crate) variable: String,
    pub(crate) path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceConfig {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    directories: RawDirectories,
    #[serde(default)]
    variables: BTreeMap<String, Option<Value>>,
    #[serde(default)]
    default_configuration: Option<String>,
    #[serde(default)]
    headers: Vec<NameValue>,
    #[serde(default = "default_timeout_seconds")]
    timeout: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfiguration {
    #[serde(default)]
    variables: BTreeMap<String, Option<Value>>,
    #[serde(default)]
    headers: Vec<NameValue>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    overrides: BTreeMap<String, RequestOverrideDocument>,
}

#[derive(Debug, Deserialize)]
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
            default_configuration: None,
            headers: Vec::new(),
            timeout: default_timeout_seconds(),
        }
    }
}

#[derive(Debug)]
struct RequestFile {
    path: PathBuf,
    text: String,
}

#[derive(Debug)]
struct ConfigurationFile {
    path: PathBuf,
    name: String,
    text: String,
}

#[derive(Debug)]
struct ParsedRequest {
    id: String,
    document: RequestDocument,
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

fn is_zero(value: &u64) -> bool {
    *value == 0
}

pub(crate) fn load(workspace_path: &Path) -> Result<RequestConfig> {
    if !workspace_path.is_dir() {
        bail!("PostUI 工作区必须是目录: {}", workspace_path.display())
    }

    let workspace_config_path = workspace_path.join("postui.yaml");
    let workspace_config = read_optional_file(&workspace_config_path)?;
    let configuration_files = read_configuration_files(&workspace_path.join("configs"))?;
    let request_files = read_request_files(&workspace_path.join("requests"))?;
    let fingerprint = workspace_fingerprint(
        workspace_path,
        workspace_config.as_deref(),
        &configuration_files,
        &request_files,
    );
    tracing::debug!(
        path = %workspace_path.display(),
        config_path = %workspace_config_path.display(),
        config_present = workspace_config.is_some(),
        configuration_count = configuration_files.len(),
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
                &configuration_files,
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
        variable_count = config.editable_variables.len(),
        configuration_count = config.configurations.len(),
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
    configuration_files: &[ConfigurationFile],
    request_files: &[RequestFile],
) -> Result<RequestConfig> {
    let raw = match text {
        Some(text) => serde_saphyr::from_str(text)
            .with_context(|| format!("YAML 配置格式无效: {}", path.display()))?,
        None => RawWorkspaceConfig::default(),
    };
    normalize_config(raw, workspace_path, configuration_files, request_files)
}

fn normalize_config(
    raw: RawWorkspaceConfig,
    workspace_path: &Path,
    configuration_files: &[ConfigurationFile],
    request_files: &[RequestFile],
) -> Result<RequestConfig> {
    let RawWorkspaceConfig {
        name,
        directories,
        variables: raw_variables,
        default_configuration: raw_default_configuration,
        headers: raw_headers,
        timeout,
    } = raw;
    let variables = normalize_variables(raw_variables)?;
    let headers = normalize_headers(raw_headers)?;
    let timeout_seconds = if timeout == 0 {
        default_timeout_seconds()
    } else {
        timeout
    };

    let file_directory =
        resolve_directory(workspace_path, &directories.uploads, "directories.uploads")?;
    let download_directory = resolve_directory(
        workspace_path,
        &directories.downloads,
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

    let configurations = normalize_configurations(configuration_files, &request_ids)?;
    let default_configuration =
        normalize_default_configuration(raw_default_configuration, &configurations)?;

    let mut editable_variables = variables.keys().cloned().collect::<BTreeSet<_>>();
    for configuration in configurations.values() {
        editable_variables.extend(configuration.variables.keys().cloned());
        for header in &configuration.headers {
            editable_variables.extend(
                crate::template::variable_names_in_text(&header.name)
                    .into_iter()
                    .chain(crate::template::variable_names_in_text(&header.value)),
            );
        }
        for request_override in configuration.request_overrides.values() {
            editable_variables.extend(crate::template::variable_names_in_override(
                request_override,
            ));
        }
    }
    for request in &requests {
        editable_variables.extend(crate::template::variable_names(request));
    }
    for header in &headers {
        editable_variables.extend(
            crate::template::variable_names_in_text(&header.name)
                .into_iter()
                .chain(crate::template::variable_names_in_text(&header.value)),
        );
    }

    let config = RequestConfig {
        name: name
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
        configurations,
        default_configuration,
        editable_variables,
        requests,
        timeout_seconds,
    };
    Ok(config)
}

fn normalize_configurations(
    configuration_files: &[ConfigurationFile],
    request_ids: &BTreeSet<String>,
) -> Result<BTreeMap<String, WorkspaceConfiguration>> {
    if configuration_files.is_empty() {
        return Ok(BTreeMap::from([(
            "default".to_string(),
            WorkspaceConfiguration {
                path: None,
                variables: BTreeMap::new(),
                headers: Vec::new(),
                timeout_seconds: None,
                request_overrides: BTreeMap::new(),
            },
        )]));
    }

    let mut configurations = BTreeMap::new();
    for file in configuration_files {
        let raw = if file.text.trim().is_empty() {
            RawConfiguration::default()
        } else {
            serde_saphyr::from_str(&file.text)
                .with_context(|| format!("配置 {} 的 YAML 格式无效", file.name))?
        };
        let variables = normalize_variables(raw.variables)
            .with_context(|| format!("配置 {} 的变量配置无效", file.name))?;
        let headers = normalize_headers(raw.headers)
            .with_context(|| format!("配置 {} 的 headers 配置无效", file.name))?;
        let timeout_seconds = raw.timeout.filter(|timeout| *timeout > 0);
        let mut request_overrides = BTreeMap::new();
        for (raw_request_id, raw_override) in raw.overrides {
            let request_id = normalize_request_id(&raw_request_id)?;
            if !request_ids.contains(&request_id) {
                bail!("配置 {} 使用了不存在的接口: {}", file.name, raw_request_id)
            }
            let request_override = normalize_override(raw_override, &request_id, &file.name)?;
            if request_overrides
                .insert(request_id.clone(), request_override)
                .is_some()
            {
                bail!("配置 {} 重复声明接口覆盖: {request_id}", file.name)
            }
        }
        if configurations
            .insert(
                file.name.clone(),
                WorkspaceConfiguration {
                    path: Some(file.path.clone()),
                    variables,
                    headers,
                    timeout_seconds,
                    request_overrides,
                },
            )
            .is_some()
        {
            bail!("配置名称重复: {}", file.name)
        }
    }
    Ok(configurations)
}

fn normalize_default_configuration(
    raw_name: Option<String>,
    configurations: &BTreeMap<String, WorkspaceConfiguration>,
) -> Result<String> {
    if let Some(raw_name) = raw_name {
        let Some(name) = normalize_configuration_name(&raw_name) else {
            bail!("默认配置名称无效: {raw_name}")
        };
        if !configurations.contains_key(&name) {
            bail!("默认配置不存在: {name}")
        }
        return Ok(name);
    }

    configurations
        .keys()
        .next()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("工作区没有可用配置"))
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

fn read_configuration_files(configurations_directory: &Path) -> Result<Vec<ConfigurationFile>> {
    if !configurations_directory.is_dir() {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(configurations_directory)
        .with_context(|| format!("无法读取配置目录: {}", configurations_directory.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("无法枚举配置目录: {}", configurations_directory.display()))?;
    entries.sort_by_key(|entry| entry.path());

    let mut files = Vec::new();
    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("无法读取文件类型: {}", path.display()))?;
        if !file_type.is_file() || !is_configuration_file(&path) {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let Some(name) = normalize_configuration_name(stem) else {
            bail!("配置文件名称无效: {}", path.display())
        };
        let text = fs::read_to_string(&path)
            .with_context(|| format!("无法读取配置文件: {}", path.display()))?;
        tracing::debug!(path = %path.display(), name = %name, bytes = text.len(), "读取 workspace 配置");
        files.push(ConfigurationFile { path, name, text });
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
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "yaml" | "yml"))
}

fn is_configuration_file(path: &Path) -> bool {
    is_request_file(path)
}

fn workspace_fingerprint(
    workspace_path: &Path,
    workspace_config: Option<&str>,
    configuration_files: &[ConfigurationFile],
    request_files: &[RequestFile],
) -> blake3::Hash {
    let mut fingerprint = blake3::Hasher::new();
    append_fingerprint_part(&mut fingerprint, b"postui.yaml");
    append_fingerprint_part(
        &mut fingerprint,
        workspace_config.unwrap_or("<missing-config>").as_bytes(),
    );
    for file in configuration_files {
        let relative = file
            .path
            .strip_prefix(workspace_path)
            .unwrap_or(&file.path)
            .to_string_lossy()
            .replace('\\', "/");
        append_fingerprint_part(&mut fingerprint, relative.as_bytes());
        append_fingerprint_part(&mut fingerprint, file.text.as_bytes());
    }
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
    let document: RequestDocument = serde_saphyr::from_str(&file.text)
        .with_context(|| format!("请求 {} 的 YAML 配置无效", id))?;
    Ok(ParsedRequest { id, document })
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
    let document = raw.document;
    let name = if document.name.trim().is_empty() {
        request_display_name(Path::new(&id))
    } else {
        document.name.trim().to_string()
    };
    let method = normalize_method(&document.method, &id)?;
    let url = document.url.trim().to_string();
    if url.is_empty() {
        bail!("接口 {} 缺少 url", id)
    }
    let headers = normalize_headers(document.headers)
        .with_context(|| format!("接口 {id} 的 headers 配置无效"))?;
    let query_parts = normalize_params(document.params, &id, "params")?
        .into_iter()
        .map(DataPart::UrlEncoded)
        .collect();
    let body_parts = document
        .body
        .filter(|body| !body.is_empty())
        .map(DataPart::Raw)
        .into_iter()
        .collect();
    let form = normalize_params(document.form, &id, "form")?;
    let files = normalize_files(document.files, &id)?;
    let extracts = normalize_extracts(document.extracts, &id)?;

    Ok(ApiRequest {
        id,
        name,
        method,
        url,
        timeout_seconds: if document.timeout == 0 {
            default_timeout_seconds
        } else {
            document.timeout
        },
        description: document.description.trim().to_string(),
        headers,
        body_parts,
        query_parts,
        form,
        files,
        extracts,
    })
}

fn normalize_override(
    raw: RequestOverrideDocument,
    request_id: &str,
    configuration: &str,
) -> Result<RequestOverride> {
    let method = raw
        .method
        .map(|method| normalize_method(&method, request_id))
        .transpose()?;
    let url = raw.url.map(|url| url.trim().to_string());
    if url.as_deref().is_some_and(str::is_empty) {
        bail!("配置 {configuration} 的接口 {request_id} 覆盖 url 不能为空")
    }
    let headers = raw
        .headers
        .map(normalize_headers)
        .transpose()
        .with_context(|| format!("配置 {configuration} 的接口 {request_id} headers 配置无效"))?;
    let query_parts = raw
        .params
        .map(|params| normalize_params(params, request_id, "params"))
        .transpose()?
        .map(|params| params.into_iter().map(DataPart::UrlEncoded).collect());
    let body_parts = raw.body.map(|body| {
        if body.is_empty() {
            Vec::new()
        } else {
            vec![DataPart::Raw(body)]
        }
    });
    let form = raw
        .form
        .map(|form| normalize_params(form, request_id, "form"))
        .transpose()?;
    let files = raw
        .files
        .map(|files| normalize_files(files, request_id))
        .transpose()?;
    let extracts = raw
        .extracts
        .map(|extracts| normalize_extracts(extracts, request_id))
        .transpose()?;
    let timeout_seconds = raw.timeout.filter(|timeout| *timeout > 0);
    if method.is_none()
        && url.is_none()
        && timeout_seconds.is_none()
        && headers.is_none()
        && query_parts.is_none()
        && body_parts.is_none()
        && form.is_none()
        && files.is_none()
        && extracts.is_none()
    {
        bail!("配置 {configuration} 的接口 {request_id} 覆盖不能为空")
    }
    Ok(RequestOverride {
        method,
        url,
        timeout_seconds,
        headers,
        query_parts,
        body_parts,
        form,
        files,
        extracts,
    })
}

fn normalize_method(value: &str, request_id: &str) -> Result<String> {
    let method = value.trim().to_ascii_uppercase();
    if method.is_empty() {
        bail!("接口 {request_id} 的 method 不能为空")
    }
    if !method.chars().all(|character| {
        character.is_ascii_uppercase() || character.is_ascii_digit() || character == '-'
    }) {
        bail!("接口 {request_id} 的 method 无效: {value}")
    }
    Ok(method)
}

fn normalize_params(
    params: Vec<RequestParam>,
    request_id: &str,
    field: &str,
) -> Result<Vec<RequestParam>> {
    let mut normalized = Vec::with_capacity(params.len());
    for mut parameter in params {
        parameter.name = parameter.name.trim().to_string();
        if parameter.name.is_empty() {
            bail!("接口 {request_id} 的 {field} 参数名称不能为空")
        }
        normalized.push(parameter);
    }
    Ok(normalized)
}

fn normalize_files(files: Vec<FileUpload>, request_id: &str) -> Result<Vec<FileUpload>> {
    for file in &files {
        validate_file(request_id, file)?;
    }
    Ok(files)
}

fn normalize_extracts(
    extracts: Vec<ResponseExtract>,
    request_id: &str,
) -> Result<Vec<ResponseExtract>> {
    let mut normalized = Vec::with_capacity(extracts.len());
    let mut names = BTreeSet::new();
    for mut extract in extracts {
        normalize_extract(request_id, &mut extract)?;
        if !names.insert(extract.variable.clone()) {
            bail!(
                "接口 {} 重复声明响应提取变量: {}",
                request_id,
                extract.variable
            )
        }
        normalized.push(extract);
    }
    Ok(normalized)
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

fn normalize_request_id(value: &str) -> Result<String> {
    let value = value.trim().replace('\\', "/");
    let value = value.strip_prefix("requests/").unwrap_or(&value);
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("配置覆盖中的接口路径无效: {value}")
    }
    let normalized = normalize_path(path).to_string_lossy().replace('\\', "/");
    Ok(format!("requests/{normalized}"))
}

fn normalize_configuration_name(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        }))
    .then(|| value.to_string())
}

pub(crate) fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}
