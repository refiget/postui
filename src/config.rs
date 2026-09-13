use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
};

use crate::template;
use anyhow::{Context, Result, bail};
use reqwest::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod headers;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestConfig {
    pub name: String,
    pub file_directory: PathBuf,
    pub download_directory: PathBuf,
    pub headers: Vec<NameValue>,
    pub variables: BTreeMap<String, VariableDefinition>,
    pub configurations: BTreeMap<String, WorkspaceConfiguration>,
    pub default_configuration: String,
    #[serde(default)]
    pub editable_variables: BTreeSet<String>,
    pub requests: Vec<ApiRequest>,
    pub timeout_seconds: u64,
    #[serde(default)]
    pub skip_ssl_verification: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceConfig {
    pub name: String,
    pub file_directory: PathBuf,
    pub download_directory: PathBuf,
    pub headers: Vec<NameValue>,
    pub variables: BTreeMap<String, VariableDefinition>,
    pub configurations: BTreeMap<String, WorkspaceConfiguration>,
    pub default_configuration: String,
    pub editable_variables: BTreeSet<String>,
    pub skip_ssl_verification: bool,
}

impl RequestConfig {
    pub fn into_workspace(self) -> (WorkspaceConfig, Vec<ApiRequest>) {
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
            skip_ssl_verification,
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
                skip_ssl_verification,
            },
            requests,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariableDefinition {
    pub default: Option<Value>,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RawVariableDefinition {
    Definition(VariableDefinitionDocument),
    Value(Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableDefinitionDocument {
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub secret: bool,
}

impl From<VariableDefinition> for RawVariableDefinition {
    fn from(definition: VariableDefinition) -> Self {
        Self::Definition(VariableDefinitionDocument {
            value: definition.default,
            secret: definition.secret,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameValue {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// 一个有序的请求参数。`has_equals` 用于区分 `flag` 和 `flag=`。
#[serde(deny_unknown_fields)]
pub struct RequestParam {
    pub name: String,
    pub value: String,
    #[serde(default = "default_has_equals", skip_serializing_if = "has_equals")]
    pub has_equals: bool,
}

impl RequestParam {
    pub fn new(name: String, value: String, has_equals: bool) -> Self {
        Self {
            name,
            value,
            has_equals,
        }
    }

    pub fn from_text(value: &str) -> Self {
        if let Some((name, value)) = value.split_once('=') {
            Self::new(name.to_string(), value.to_string(), true)
        } else {
            Self::new(value.to_string(), String::new(), false)
        }
    }

    pub fn to_text(&self) -> String {
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

fn has_equals(value: &bool) -> bool {
    *value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiRequest {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub timeout_seconds: u64,
    pub skip_ssl_verification: bool,
    pub description: String,
    pub headers: Vec<NameValue>,
    pub body_parts: Vec<DataPart>,
    pub query_parts: Vec<DataPart>,
    pub form: Vec<RequestParam>,
    pub files: Vec<FileUpload>,
    pub extracts: Vec<ResponseExtract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceConfiguration {
    #[serde(default)]
    pub path: Option<PathBuf>,
    pub variables: BTreeMap<String, VariableDefinition>,
    pub headers: Vec<NameValue>,
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub skip_ssl_verification: Option<bool>,
    pub request_overrides: BTreeMap<String, RequestOverride>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestOverride {
    pub method: Option<String>,
    pub url: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub skip_ssl_verification: Option<bool>,
    pub headers: Option<Vec<NameValue>>,
    pub query_parts: Option<Vec<DataPart>>,
    pub body_parts: Option<Vec<DataPart>>,
    pub form: Option<Vec<RequestParam>>,
    pub files: Option<Vec<FileUpload>>,
    pub extracts: Option<Vec<ResponseExtract>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestDocument {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default = "default_method", skip_serializing_if = "is_default_method")]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_ssl_verification: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "headers")]
    pub headers: Vec<NameValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<RequestParam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub form: Vec<RequestParam>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileUpload>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extracts: Vec<ResponseExtract>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationDocument {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, Option<RawVariableDefinition>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "headers")]
    pub headers: Vec<NameValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_ssl_verification: Option<bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, RequestOverrideDocument>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestOverrideDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_ssl_verification: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "headers::optional"
    )]
    pub headers: Option<Vec<NameValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<RequestParam>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<Vec<RequestParam>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileUpload>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extracts: Option<Vec<ResponseExtract>>,
}

impl ApiRequest {
    pub fn for_configuration(&self, configuration: &WorkspaceConfiguration) -> Self {
        let mut request = self.clone();
        if let Some(timeout_seconds) = configuration.timeout_seconds {
            request.timeout_seconds = timeout_seconds;
        }
        if let Some(skip_ssl_verification) = configuration.skip_ssl_verification {
            request.skip_ssl_verification = skip_ssl_verification;
        }
        if let Some(request_override) = configuration.request_overrides.get(&self.id) {
            request_override.apply_to(&mut request);
        }
        request
    }
}

impl RequestOverride {
    pub fn is_empty(&self) -> bool {
        self.method.is_none()
            && self.url.is_none()
            && self.timeout_seconds.is_none()
            && self.skip_ssl_verification.is_none()
            && self.headers.is_none()
            && self.query_parts.is_none()
            && self.body_parts.is_none()
            && self.form.is_none()
            && self.files.is_none()
            && self.extracts.is_none()
    }

    pub fn apply_to(&self, request: &mut ApiRequest) {
        if let Some(method) = &self.method {
            request.method = method.clone();
        }
        if let Some(url) = &self.url {
            request.url = url.clone();
        }
        if let Some(timeout_seconds) = self.timeout_seconds {
            request.timeout_seconds = timeout_seconds;
        }
        if let Some(skip_ssl_verification) = self.skip_ssl_verification {
            request.skip_ssl_verification = skip_ssl_verification;
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
            timeout: Some(request.timeout_seconds),
            skip_ssl_verification: Some(request.skip_ssl_verification),
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
                .map(|(name, definition)| (name.clone(), Some(definition.clone().into())))
                .collect(),
            headers: configuration.headers.clone(),
            timeout: configuration.timeout_seconds,
            skip_ssl_verification: configuration.skip_ssl_verification,
            overrides: configuration
                .request_overrides
                .iter()
                .map(|(request_id, request_override)| {
                    (
                        request_id
                            .strip_prefix("requests/")
                            .expect("request IDs are rooted in requests/")
                            .to_string(),
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
            skip_ssl_verification: request_override.skip_ssl_verification,
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
pub enum DataPart {
    Raw(String),
    UrlEncoded(RequestParam),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileUpload {
    pub field: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseExtract {
    pub variable: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceConfig {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    directories: RawDirectories,
    #[serde(default)]
    variables: BTreeMap<String, Option<RawVariableDefinition>>,
    #[serde(default)]
    default_scenario: Option<String>,
    #[serde(default, deserialize_with = "headers::deserialize")]
    headers: Vec<NameValue>,
    #[serde(default = "default_timeout_seconds")]
    timeout: u64,
    #[serde(default)]
    skip_ssl_verification: bool,
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
            default_scenario: None,
            headers: Vec::new(),
            timeout: default_timeout_seconds(),
            skip_ssl_verification: false,
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
    path: String,
    document: RequestDocument,
}

fn default_method() -> String {
    "GET".to_string()
}

fn is_default_method(method: &str) -> bool {
    method == "GET"
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

pub fn load(workspace_path: &Path) -> Result<RequestConfig> {
    load_internal(workspace_path, true)
}

pub fn reload(workspace_path: &Path) -> Result<RequestConfig> {
    load_internal(workspace_path, false)
}

fn load_internal(workspace_path: &Path, use_cache: bool) -> Result<RequestConfig> {
    if !workspace_path.is_dir() {
        bail!("PostUI 工作区必须是目录: {}", workspace_path.display())
    }

    let workspace_config_path = workspace_path.join("postui.yaml");
    let workspace_config = read_optional_file(&workspace_config_path)?;
    let configuration_files = read_configuration_files(&workspace_path.join("scenarios"))?;
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

    let cached = use_cache
        .then(|| crate::cache::load(workspace_path, &fingerprint))
        .flatten();
    let cache_hit = cached.is_some();
    let config = match cached {
        Some(config) => config,
        None => parse_workspace_config(
            &workspace_config_path,
            workspace_config.as_deref(),
            workspace_path,
            &configuration_files,
            &request_files,
        )?,
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
        Some(text) => parse_yaml(path, "postui.yaml", text)?,
        None => RawWorkspaceConfig::default(),
    };
    normalize_config(
        path,
        raw,
        workspace_path,
        configuration_files,
        request_files,
    )
    .with_context(|| format!("工作区配置无效: {}", path.display()))
}

fn parse_yaml<'a, T>(path: &Path, field: &str, text: &'a str) -> Result<T>
where
    T: serde::Deserialize<'a>,
{
    serde_saphyr::from_str(text).map_err(|error| yaml_parse_error(path, field, error))
}

fn yaml_parse_error(path: &Path, field: &str, error: serde_saphyr::Error) -> anyhow::Error {
    let message = error.to_string();
    let location = parse_yaml_position(&message);
    config_error_with_position(path, field, &message, location)
}

fn config_error_with_position(
    path: &Path,
    field: &str,
    problem: &str,
    location: Option<(usize, usize)>,
) -> anyhow::Error {
    match location {
        Some((line, column)) => {
            anyhow::anyhow!(
                "文件: {}\n行: {}\n列: {}\n字段: {}\n问题: {}",
                path.display(),
                line,
                column,
                field,
                problem,
            )
        }
        None => {
            anyhow::anyhow!(
                "文件: {}\n字段: {}\n问题: {}",
                path.display(),
                field,
                problem,
            )
        }
    }
}

fn config_error(path: &Path, field: &str, problem: impl AsRef<str>) -> anyhow::Error {
    anyhow::anyhow!(
        "文件: {}\n字段: {}\n问题: {}",
        path.display(),
        field,
        problem.as_ref(),
    )
}

fn parse_yaml_position(text: &str) -> Option<(usize, usize)> {
    let marker = "line ";
    let start = text.find(marker)?;
    let rest = &text[start + marker.len()..];
    let mut parts = rest.split_whitespace();
    let line = parts.next()?.parse::<usize>().ok()?;
    if parts.next()? != "column" {
        return None;
    }
    let column = parts
        .next()?
        .trim_end_matches([',', ':', ';', '\n'])
        .parse::<usize>()
        .ok()?;
    Some((line, column))
}

fn normalize_config(
    path: &Path,
    raw: RawWorkspaceConfig,
    workspace_path: &Path,
    configuration_files: &[ConfigurationFile],
    request_files: &[RequestFile],
) -> Result<RequestConfig> {
    let RawWorkspaceConfig {
        name,
        directories,
        variables: raw_variables,
        default_scenario: raw_default_configuration,
        headers: raw_headers,
        timeout,
        skip_ssl_verification,
    } = raw;
    let variables = normalize_variables(raw_variables)?;
    let headers = normalize_headers(&path.display().to_string(), raw_headers)?;
    let timeout_seconds = validate_timeout(timeout)?;

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
        let request = normalize_request(
            raw_request,
            timeout_seconds,
            skip_ssl_verification,
            &file_directory,
        )?;
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

    let configurations =
        normalize_configurations(configuration_files, &request_ids, &file_directory)?;
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
        skip_ssl_verification,
    };
    Ok(config)
}

fn normalize_configurations(
    configuration_files: &[ConfigurationFile],
    request_ids: &BTreeSet<String>,
    file_directory: &Path,
) -> Result<BTreeMap<String, WorkspaceConfiguration>> {
    if configuration_files.is_empty() {
        return Ok(BTreeMap::from([(
            "default".to_string(),
            WorkspaceConfiguration {
                path: None,
                variables: BTreeMap::new(),
                headers: Vec::new(),
                timeout_seconds: None,
                skip_ssl_verification: None,
                request_overrides: BTreeMap::new(),
            },
        )]));
    }

    let mut configurations = BTreeMap::new();
    for file in configuration_files {
        let raw = parse_yaml::<Option<ConfigurationDocument>>(&file.path, &file.name, &file.text)?
            .unwrap_or_default();
        let variables = normalize_variables(raw.variables)
            .with_context(|| format!("配置 {} 的变量配置无效", file.name))?;
        let headers = normalize_headers(&file.path.to_string_lossy(), raw.headers)
            .with_context(|| format!("配置 {} 的 headers 配置无效", file.name))?;
        let timeout_seconds = raw
            .timeout
            .map(validate_timeout)
            .transpose()
            .with_context(|| {
                config_error_with_position(&file.path, "timeouts", "场景配置超时配置无效", None)
            })?;
        let mut request_overrides = BTreeMap::new();
        for (raw_request_id, raw_override) in raw.overrides {
            let request_id = normalize_request_id(&raw_request_id)?;
            if !request_ids.contains(&request_id) {
                return Err(config_error(
                    &file.path,
                    "overrides",
                    format!("未找到配置引用的接口: {raw_request_id}"),
                ));
            }
            let request_override = normalize_override(
                raw_override,
                &request_id,
                &file.path.to_string_lossy(),
                file_directory,
            )?;
            if request_overrides
                .insert(request_id.clone(), request_override)
                .is_some()
            {
                return Err(config_error(
                    &file.path,
                    "overrides",
                    format!("重复声明接口覆盖: {request_id}"),
                ));
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
                    skip_ssl_verification: raw.skip_ssl_verification,
                    request_overrides,
                },
            )
            .is_some()
        {
            return Err(config_error(
                &file.path,
                "name",
                format!("配置名称重复: {}", file.name),
            ));
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
    if !optional_directory_exists(requests_directory)? {
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
    if !optional_directory_exists(configurations_directory)? {
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

fn optional_directory_exists(path: &Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => bail!("配置路径必须是目录: {}", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("无法读取配置目录: {}", path.display())),
    }
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
    let path = file.path.to_string_lossy().to_string();
    let id = file
        .path
        .strip_prefix(workspace_path)
        .unwrap_or(&file.path)
        .to_string_lossy()
        .replace('\\', "/");
    let document: RequestDocument = parse_yaml(&file.path, &id, &file.text)?;
    Ok(ParsedRequest { id, path, document })
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
    raw_variables: BTreeMap<String, Option<RawVariableDefinition>>,
) -> Result<BTreeMap<String, VariableDefinition>> {
    let mut variables = BTreeMap::new();
    for (raw_name, raw_definition) in raw_variables {
        let Some(name) = normalize_variable_name(&raw_name) else {
            bail!("变量名称无效: {raw_name}")
        };
        let definition = match raw_definition {
            None => VariableDefinition {
                default: None,
                secret: false,
            },
            Some(RawVariableDefinition::Value(default)) => VariableDefinition {
                default: Some(default),
                secret: false,
            },
            Some(RawVariableDefinition::Definition(definition)) => VariableDefinition {
                default: definition.value,
                secret: definition.secret,
            },
        };
        if variables.insert(name.clone(), definition).is_some() {
            bail!("变量名称重复: {name}")
        }
    }
    Ok(variables)
}

fn normalize_headers(context: &str, raw_headers: Vec<NameValue>) -> Result<Vec<NameValue>> {
    let mut headers = Vec::with_capacity(raw_headers.len());
    for mut header in raw_headers {
        let name = header.name.trim().to_string();
        if name.is_empty() {
            return Err(config_error(
                Path::new(context),
                "headers",
                "Header 名称不能为空",
            ));
        }
        HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
            config_error(
                Path::new(context),
                "headers",
                format!("Header 名称无效 ({name}): {error}"),
            )
        })?;
        HeaderValue::from_str(&header.value).map_err(|error| {
            config_error(
                Path::new(context),
                "headers",
                format!("Header 值无效 ({name}): {error}"),
            )
        })?;
        if header.value.contains('\n') || header.value.contains('\r') {
            return Err(config_error(
                Path::new(context),
                "headers",
                format!("Header 值不应包含换行符 ({name})"),
            ));
        }
        header.name = name;
        header.value = header.value.trim().to_string();
        headers.push(header);
    }
    Ok(headers)
}

fn normalize_request(
    raw: ParsedRequest,
    default_timeout_seconds: u64,
    default_skip_ssl_verification: bool,
    file_directory: &Path,
) -> Result<ApiRequest> {
    let id = raw.id;
    let path = raw.path;
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
    let headers = normalize_headers(&id, document.headers)
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
    let files = normalize_files(document.files, &id, &path, file_directory)?;
    let extracts = normalize_extracts(document.extracts, &id)?;
    let timeout_seconds = document
        .timeout
        .map(validate_timeout)
        .transpose()
        .with_context(|| format!("请求 {id} 的 timeout 配置无效"))?
        .unwrap_or(default_timeout_seconds);
    let skip_ssl_verification = document
        .skip_ssl_verification
        .unwrap_or(default_skip_ssl_verification);

    Ok(ApiRequest {
        id,
        name,
        method,
        url,
        timeout_seconds,
        skip_ssl_verification,
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
    file_directory: &Path,
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
        .map(|headers| normalize_headers(configuration, headers))
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
        .map(|files| normalize_files(files, request_id, configuration, file_directory))
        .transpose()?;
    let extracts = raw
        .extracts
        .map(|extracts| normalize_extracts(extracts, request_id))
        .transpose()?;
    let timeout_seconds = raw
        .timeout
        .map(validate_timeout)
        .transpose()
        .with_context(|| format!("场景 {configuration} 的请求 {request_id} timeout 配置无效"))?;
    if method.is_none()
        && url.is_none()
        && timeout_seconds.is_none()
        && raw.skip_ssl_verification.is_none()
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
        skip_ssl_verification: raw.skip_ssl_verification,
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

fn validate_timeout(seconds: u64) -> Result<u64> {
    if seconds == 0 {
        bail!("timeout 必须是大于 0 的整数（秒）；使用默认值请省略此字段")
    }
    Ok(seconds)
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

fn normalize_files(
    files: Vec<FileUpload>,
    request_id: &str,
    request_path: &str,
    file_directory: &Path,
) -> Result<Vec<FileUpload>> {
    for file in &files {
        validate_file(request_id, request_path, file, file_directory)?;
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

fn validate_file(
    request_id: &str,
    request_path: &str,
    file: &FileUpload,
    file_directory: &Path,
) -> Result<()> {
    if file.field.trim().is_empty() {
        return Err(config_error(
            Path::new(request_path),
            "files.field",
            format!("请求 {} 的上传文件缺少 field", request_id),
        ));
    }
    if file.path.trim().is_empty() {
        return Err(config_error(
            Path::new(request_path),
            "files.path",
            format!("请求 {} 的上传文件缺少 path", request_id),
        ));
    }
    let trimmed = file.path.trim();
    let normalized = Path::new(trimmed);
    if normalized.is_relative()
        && normalized
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(config_error(
            Path::new(request_path),
            "files.path",
            "上传文件相对路径中不能包含 ..",
        ));
    }
    if normalized.is_relative()
        && !contains_template(trimmed)
        && !file_directory.join(normalized).is_file()
    {
        return Err(config_error(
            Path::new(request_path),
            "files.path",
            format!(
                "上传文件不存在: {}",
                file_directory.join(normalized).display()
            ),
        ));
    }
    Ok(())
}

fn contains_template(value: &str) -> bool {
    template::find_placeholder(value).is_some()
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
    let path = Path::new(&value);
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

pub fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}
