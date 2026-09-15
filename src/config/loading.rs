use super::{
    ConfigurationDocument, RequestConfig, WorkspaceConfiguration,
    documents::RawWorkspaceConfig,
    files::{
        ConfigurationFile, RequestFile, normalize_configuration_name, normalize_request_id,
        parse_request_file, read_configuration_files, read_optional_file, read_request_files,
        resolve_directory,
    },
    validation::{
        normalize_headers, normalize_override, normalize_request, normalize_variables,
        validate_timeout,
    },
};
use crate::diagnostics;
use anyhow::{Result, bail};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(crate) struct LoadOutcome {
    pub(crate) config: RequestConfig,
    pub(crate) warnings: Vec<crate::diagnostics::ConfigDiagnostic>,
}

pub fn load(workspace_path: &Path) -> Result<RequestConfig> {
    load_workspace(workspace_path, false).map(|outcome| outcome.config)
}

pub(crate) fn load_tolerant(workspace_path: &Path) -> Result<LoadOutcome> {
    load_workspace(workspace_path, true)
}

fn load_workspace(workspace_path: &Path, tolerant: bool) -> Result<LoadOutcome> {
    if !workspace_path.is_dir() {
        return Err(diagnostics::invalid(
            workspace_path,
            "workspace",
            "PostUI workspace path must be a directory",
        ));
    }

    let mut warnings = Vec::new();
    let workspace_config_path = workspace_path.join("postui.yaml");
    let workspace_config = match read_optional_file(&workspace_config_path) {
        Ok(text) => text,
        Err(error) if tolerant => {
            record_warning(&mut warnings, config_diagnostic(&error)?);
            None
        }
        Err(error) => return Err(error),
    };
    let configuration_files = diagnostics::standardize(
        read_configuration_files(&workspace_path.join("scenarios"), tolerant, &mut warnings),
        &workspace_path.join("scenarios"),
        "scenarios",
    )?;
    let request_files = diagnostics::standardize(
        read_request_files(&workspace_path.join("requests"), tolerant, &mut warnings),
        &workspace_path.join("requests"),
        "requests",
    )?;
    tracing::debug!(
        path = %workspace_path.display(),
        config_path = %workspace_config_path.display(),
        config_present = workspace_config.is_some(),
        configuration_count = configuration_files.len(),
        request_count = request_files.len(),
        "读取工作区"
    );

    let config = parse_workspace_config(
        &workspace_config_path,
        workspace_config.as_deref(),
        workspace_path,
        &configuration_files,
        &request_files,
        tolerant,
        &mut warnings,
    )?;

    tracing::debug!(
        name = %config.name,
        request_count = config.requests.len(),
        variable_count = config.editable_variables.len(),
        configuration_count = config.configurations.len(),
        workspace_header_count = config.headers.len(),
        timeout_seconds = config.timeout_seconds,
        file_directory = %config.file_directory.display(),
        download_directory = %config.download_directory.display(),
        "配置文件加载完成"
    );
    Ok(LoadOutcome { config, warnings })
}

fn parse_workspace_config(
    path: &Path,
    text: Option<&str>,
    workspace_path: &Path,
    configuration_files: &[ConfigurationFile],
    request_files: &[RequestFile],
    tolerant: bool,
    warnings: &mut Vec<crate::diagnostics::ConfigDiagnostic>,
) -> Result<RequestConfig> {
    let raw = match text {
        Some(text) => match diagnostics::parse_yaml(path, "postui.yaml", text) {
            Ok(raw) => raw,
            Err(error) if tolerant => {
                record_warning(warnings, config_diagnostic(&error)?);
                RawWorkspaceConfig::default()
            }
            Err(error) => return Err(error),
        },
        None => RawWorkspaceConfig::default(),
    };
    let normalized = diagnostics::standardize(
        normalize_config(
            path,
            raw,
            workspace_path,
            configuration_files,
            request_files,
            tolerant,
            warnings,
        ),
        path,
        "workspace",
    );
    match normalized {
        Ok(config) => Ok(config),
        Err(error) if tolerant => {
            record_warning(warnings, config_diagnostic(&error)?);
            normalize_config(
                path,
                RawWorkspaceConfig::default(),
                workspace_path,
                configuration_files,
                request_files,
                tolerant,
                warnings,
            )
        }
        Err(error) => Err(error),
    }
}

fn normalize_config(
    path: &Path,
    raw: RawWorkspaceConfig,
    workspace_path: &Path,
    configuration_files: &[ConfigurationFile],
    request_files: &[RequestFile],
    tolerant: bool,
    warnings: &mut Vec<crate::diagnostics::ConfigDiagnostic>,
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
    let variables =
        diagnostics::standardize(normalize_variables(raw_variables), path, "variables")?;
    let headers = diagnostics::standardize(normalize_headers(raw_headers), path, "headers")?;
    let timeout_seconds = diagnostics::standardize(validate_timeout(timeout), path, "timeout")?;

    let file_directory = diagnostics::standardize(
        resolve_directory(workspace_path, &directories.uploads, "directories.uploads"),
        path,
        "directories.uploads",
    )?;
    let download_directory = diagnostics::standardize(
        resolve_directory(
            workspace_path,
            &directories.downloads,
            "directories.downloads",
        ),
        path,
        "directories.downloads",
    )?;

    let mut request_ids = BTreeSet::new();
    let mut requests = Vec::with_capacity(request_files.len());
    for file in request_files {
        let request = (|| {
            let raw_request = parse_request_file(file, workspace_path)?;
            diagnostics::standardize(
                normalize_request(raw_request, timeout_seconds, skip_ssl_verification),
                &file.path,
                "request",
            )
        })();
        let request = match request {
            Ok(request) => request,
            Err(error) if tolerant => {
                record_warning(warnings, config_diagnostic(&error)?);
                continue;
            }
            Err(error) => return Err(error),
        };
        if !request_ids.insert(request.id.clone()) {
            let error = diagnostics::invalid(
                &file.path,
                "id",
                format!("Request id is duplicated: {}", request.id),
            );
            if tolerant {
                record_warning(warnings, config_diagnostic(&error)?);
                continue;
            }
            return Err(error);
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

    let configurations = diagnostics::standardize(
        normalize_configurations(configuration_files, &request_ids, tolerant, warnings),
        path,
        "scenarios",
    )?;
    let default_configuration = diagnostics::standardize(
        normalize_default_configuration(raw_default_configuration, &configurations),
        path,
        "default_scenario",
    )?;

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
    tolerant: bool,
    warnings: &mut Vec<crate::diagnostics::ConfigDiagnostic>,
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
        let configuration = normalize_configuration(file, request_ids);
        let configuration = match configuration {
            Ok(configuration) => configuration,
            Err(error) if tolerant => {
                record_warning(warnings, config_diagnostic(&error)?);
                continue;
            }
            Err(error) => return Err(error),
        };
        let (name, configuration) = configuration;
        if configurations.insert(name, configuration).is_some() {
            unreachable!("scenario filenames are unique within one directory");
        }
    }
    if configurations.is_empty() {
        configurations.insert(
            "default".to_string(),
            WorkspaceConfiguration {
                path: None,
                variables: BTreeMap::new(),
                headers: Vec::new(),
                timeout_seconds: None,
                skip_ssl_verification: None,
                request_overrides: BTreeMap::new(),
            },
        );
    }
    Ok(configurations)
}

fn normalize_configuration(
    file: &ConfigurationFile,
    request_ids: &BTreeSet<String>,
) -> Result<(String, WorkspaceConfiguration)> {
    let raw = diagnostics::parse_yaml::<Option<ConfigurationDocument>>(
        &file.path, &file.name, &file.text,
    )?
    .unwrap_or_default();
    let variables =
        diagnostics::standardize(normalize_variables(raw.variables), &file.path, "variables")?;
    let headers = diagnostics::standardize(normalize_headers(raw.headers), &file.path, "headers")?;
    let timeout_seconds = diagnostics::standardize(
        raw.timeout.map(validate_timeout).transpose(),
        &file.path,
        "timeout",
    )?;
    let mut request_overrides = BTreeMap::new();
    for (raw_request_id, raw_override) in raw.overrides {
        let request_id = diagnostics::standardize(
            normalize_request_id(&raw_request_id),
            &file.path,
            "overrides",
        )?;
        if !request_ids.contains(&request_id) {
            return Err(diagnostics::invalid(
                &file.path,
                "overrides",
                format!("Referenced request does not exist: {raw_request_id}"),
            ));
        }
        let request_override = diagnostics::standardize(
            normalize_override(raw_override, &request_id, &file.name),
            &file.path,
            format!("overrides.{request_id}"),
        )?;
        if request_overrides
            .insert(request_id.clone(), request_override)
            .is_some()
        {
            return Err(diagnostics::invalid(
                &file.path,
                "overrides",
                format!("Request override is declared more than once: {request_id}"),
            ));
        }
    }
    Ok((
        file.name.clone(),
        WorkspaceConfiguration {
            path: Some(file.path.clone()),
            variables,
            headers,
            timeout_seconds,
            skip_ssl_verification: raw.skip_ssl_verification,
            request_overrides,
        },
    ))
}

fn config_diagnostic(error: &anyhow::Error) -> Result<crate::diagnostics::ConfigDiagnostic> {
    diagnostics::from_error(error).ok_or_else(|| anyhow::anyhow!("{error:#}"))
}

fn record_warning(
    warnings: &mut Vec<crate::diagnostics::ConfigDiagnostic>,
    warning: crate::diagnostics::ConfigDiagnostic,
) {
    let message = warning.to_string();
    if !warnings
        .iter()
        .any(|existing| existing.to_string() == message)
    {
        warnings.push(warning);
    }
}

fn normalize_default_configuration(
    raw_name: Option<String>,
    configurations: &BTreeMap<String, WorkspaceConfiguration>,
) -> Result<String> {
    if let Some(raw_name) = raw_name {
        let Some(name) = normalize_configuration_name(&raw_name) else {
            bail!("Default scenario name is invalid: {raw_name}")
        };
        if !configurations.contains_key(&name) {
            bail!("Default scenario does not exist: {name}")
        }
        return Ok(name);
    }

    configurations
        .keys()
        .next()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Workspace has no usable scenarios"))
}
