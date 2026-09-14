use super::{
    ApiRequest, DataPart, FileUpload, NameValue, RequestOverride, RequestParam, ResponseExtract,
    VariableDefinition, WorkspaceConfiguration, headers,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

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
    #[serde(default = "super::default_temporary_variable")]
    pub temporary: bool,
}

impl From<VariableDefinition> for RawVariableDefinition {
    fn from(definition: VariableDefinition) -> Self {
        Self::Definition(VariableDefinitionDocument {
            value: definition.default,
            secret: definition.secret,
            temporary: definition.temporary,
        })
    }
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawWorkspaceConfig {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) directories: RawDirectories,
    #[serde(default)]
    pub(super) variables: BTreeMap<String, Option<RawVariableDefinition>>,
    #[serde(default)]
    pub(super) default_scenario: Option<String>,
    #[serde(default, deserialize_with = "headers::deserialize")]
    pub(super) headers: Vec<NameValue>,
    #[serde(default = "default_timeout_seconds")]
    pub(super) timeout: u64,
    #[serde(default)]
    pub(super) skip_ssl_verification: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawDirectories {
    #[serde(default = "default_upload_directory")]
    pub(super) uploads: PathBuf,
    #[serde(default = "default_download_directory")]
    pub(super) downloads: PathBuf,
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

fn default_method() -> String {
    "GET".to_string()
}

fn is_default_method(method: &str) -> bool {
    method == "GET"
}

pub(super) fn default_timeout_seconds() -> u64 {
    30
}

pub(super) fn default_upload_directory() -> PathBuf {
    PathBuf::from("test_files")
}

pub(super) fn default_download_directory() -> PathBuf {
    PathBuf::from("temp")
}
