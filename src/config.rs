use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod documents;
mod files;
mod headers;
mod loading;
mod validation;

pub use documents::{
    ConfigurationDocument, RawVariableDefinition, RequestDocument, RequestOverrideDocument,
    VariableDefinitionDocument,
};
use documents::{default_download_directory, default_timeout_seconds, default_upload_directory};
use files::normalize_path;
pub use loading::load;

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
    pub fn default_for_workspace(workspace_path: &Path) -> Self {
        let project_path = workspace_path.parent().unwrap_or_else(|| Path::new("."));
        let name = project_path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "PostUI".to_string());
        let default_configuration = WorkspaceConfiguration {
            path: None,
            variables: BTreeMap::new(),
            headers: Vec::new(),
            timeout_seconds: None,
            skip_ssl_verification: None,
            request_overrides: BTreeMap::new(),
        };

        Self {
            name,
            file_directory: normalize_path(&project_path.join(default_upload_directory())),
            download_directory: normalize_path(&project_path.join(default_download_directory())),
            headers: Vec::new(),
            variables: BTreeMap::new(),
            configurations: BTreeMap::from([("default".to_string(), default_configuration)]),
            default_configuration: "default".to_string(),
            editable_variables: BTreeSet::new(),
            requests: Vec::new(),
            timeout_seconds: default_timeout_seconds(),
            skip_ssl_verification: false,
        }
    }

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
    #[serde(default = "default_temporary_variable")]
    pub temporary: bool,
}

pub(super) fn default_temporary_variable() -> bool {
    true
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

pub fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}
