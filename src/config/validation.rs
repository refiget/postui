use super::{
    ApiRequest, DataPart, FileUpload, NameValue, RawVariableDefinition, RequestOverride,
    RequestOverrideDocument, RequestParam, ResponseExtract, VariableDefinition,
    files::ParsedRequest,
};
use anyhow::{Context, Result, bail};
use reqwest::header::{HeaderName, HeaderValue};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
};

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

pub(super) fn normalize_variables(
    raw_variables: BTreeMap<String, Option<RawVariableDefinition>>,
) -> Result<BTreeMap<String, VariableDefinition>> {
    let mut variables = BTreeMap::new();
    for (raw_name, raw_definition) in raw_variables {
        let Some(name) = normalize_variable_name(&raw_name) else {
            bail!("Variable name is invalid: {raw_name}")
        };
        let definition = match raw_definition {
            None => VariableDefinition {
                default: None,
                secret: false,
                temporary: true,
            },
            Some(RawVariableDefinition::Value(default)) => VariableDefinition {
                default: Some(default),
                secret: false,
                temporary: true,
            },
            Some(RawVariableDefinition::Definition(definition)) => VariableDefinition {
                default: definition.value,
                secret: definition.secret,
                temporary: definition.temporary,
            },
        };
        if variables.insert(name.clone(), definition).is_some() {
            bail!("Variable name is declared more than once: {name}")
        }
    }
    Ok(variables)
}

pub(super) fn normalize_headers(raw_headers: Vec<NameValue>) -> Result<Vec<NameValue>> {
    let mut headers = Vec::with_capacity(raw_headers.len());
    for mut header in raw_headers {
        let name = header.name.trim().to_string();
        if name.is_empty() {
            bail!("Header name cannot be empty")
        }
        HeaderName::from_bytes(name.as_bytes())
            .map_err(|error| anyhow::anyhow!("Invalid Header name ({name}): {error}"))?;
        HeaderValue::from_str(&header.value)
            .map_err(|error| anyhow::anyhow!("Invalid Header value ({name}): {error}"))?;
        if header.value.contains('\n') || header.value.contains('\r') {
            bail!("Header value must not contain line breaks ({name})")
        }
        header.name = name;
        header.value = header.value.trim().to_string();
        headers.push(header);
    }
    Ok(headers)
}

pub(super) fn normalize_request(
    raw: ParsedRequest,
    default_timeout_seconds: u64,
    default_skip_ssl_verification: bool,
) -> Result<ApiRequest> {
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
        bail!("Request {id} is missing url")
    }
    let headers = normalize_headers(document.headers)?;
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
    let timeout_seconds = document
        .timeout
        .map(validate_timeout)
        .transpose()?
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

pub(super) fn normalize_override(
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
        bail!("Scenario {configuration} request {request_id} override url cannot be empty")
    }
    let headers = raw.headers.map(normalize_headers).transpose()?;
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
    let timeout_seconds = raw.timeout.map(validate_timeout).transpose()?;
    let request_override = RequestOverride {
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
    };
    if request_override.is_empty() {
        bail!("Scenario {configuration} request {request_id} override cannot be empty")
    }
    Ok(request_override)
}

fn normalize_method(value: &str, request_id: &str) -> Result<String> {
    crate::http_method::parse(value)
        .map(|method| method.to_string())
        .with_context(|| format!("Request {request_id} has an invalid method: {value}"))
}

pub(super) fn validate_timeout(seconds: u64) -> Result<u64> {
    if seconds == 0 {
        bail!("timeout must be a positive integer in seconds; omit it to use the default")
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
            bail!("Request {request_id} {field} parameter name cannot be empty")
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
                "Request {} declares response extraction variable more than once: {}",
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
        bail!("Request {request_id} upload file is missing field")
    }
    if file.path.trim().is_empty() {
        bail!("Request {request_id} upload file is missing path")
    }
    let trimmed = file.path.trim();
    let normalized = Path::new(trimmed);
    if normalized.is_relative()
        && normalized
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        bail!("Request {request_id} upload file path cannot contain ..")
    }
    Ok(())
}

fn normalize_extract(request_id: &str, extract: &mut ResponseExtract) -> Result<()> {
    let raw_variable = extract.variable.clone();
    let Some(variable) = normalize_variable_name(&raw_variable) else {
        bail!(
            "Request {} has an invalid response extraction variable: {}",
            request_id,
            raw_variable
        )
    };
    if extract.path.trim().is_empty() {
        bail!("Request {request_id} response extraction is missing path")
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
