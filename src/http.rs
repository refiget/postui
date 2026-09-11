use std::{
    error::Error,
    fmt, fs,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

use reqwest::{
    Method,
    blocking::{
        Client,
        multipart::{Form, Part},
    },
    header::HeaderName,
};

use crate::{config::DownloadTarget, template::ResolvedRequest};

const MAX_LOG_VALUE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HttpError {
    Timeout(String),
    Failed(String),
}

impl HttpError {
    fn failed(message: impl Into<String>) -> Self {
        Self::Failed(message.into())
    }

    fn from_reqwest(context: &str, error: reqwest::Error) -> Self {
        let message = format!("{context}: {error}");
        if error.is_timeout() {
            Self::Timeout(message)
        } else {
            Self::Failed(message)
        }
    }

    pub(crate) fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout(_))
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout(message) | Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl Error for HttpError {}

impl From<String> for HttpError {
    fn from(message: String) -> Self {
        Self::failed(message)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResponseData {
    pub(crate) status: u16,
    pub(crate) reason: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: String,
    pub(crate) download_path: Option<PathBuf>,
    pub(crate) elapsed_ms: u128,
}

pub(crate) fn send(
    request: &ResolvedRequest,
    timeout_seconds: u64,
    file_directory: &Path,
    download_directory: &Path,
    operation_id: &str,
) -> Result<ResponseData, HttpError> {
    let started = Instant::now();
    let loopback = is_loopback_url(&request.url);
    let _span = tracing::debug_span!(
        "http_request",
        operation_id = %operation_id,
        method = %request.method,
        url = %log_url(&request.url)
    )
    .entered();
    tracing::debug!(
        timeout_seconds = timeout_seconds.max(1),
        loopback,
        header_count = request.headers.len(),
        form_field_count = request.form.len(),
        file_count = request.files.len(),
        has_body = request.raw_body.is_some(),
        download = ?request.download,
        file_directory = %file_directory.display(),
        download_directory = %download_directory.display(),
        "开始准备 HTTP 请求"
    );

    let method = Method::from_bytes(request.method.as_bytes()).map_err(|error| {
        tracing::error!(error = %error, method = %request.method, "HTTP 方法无效");
        HttpError::failed(format!("HTTP 方法无效: {error}"))
    })?;
    let mut client_builder = Client::builder()
        .timeout(Duration::from_secs(timeout_seconds.max(1)))
        .user_agent("postui/0.1");
    if loopback {
        client_builder = client_builder.no_proxy();
        tracing::debug!("检测到本地地址，关闭代理");
    }
    let client = client_builder.build().map_err(|error| {
        tracing::error!(error = %error, error_debug = ?error, "创建 HTTP 客户端失败");
        HttpError::failed(format!("创建 HTTP 客户端失败: {error}"))
    })?;
    tracing::debug!("HTTP 客户端创建完成");

    let mut builder = client.request(method, &request.url);
    let request_headers = request
        .headers
        .iter()
        .map(|(name, value)| (name.clone(), log_field_value(name, value)))
        .collect::<Vec<_>>();
    tracing::debug!(headers = ?request_headers, "准备请求头");
    for (name, value) in &request.headers {
        let header_name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
            tracing::error!(header = %name, error = %error, "请求头名称无效");
            HttpError::failed(format!("请求头名称无效 {name}: {error}"))
        })?;
        builder = builder.header(header_name, value);
    }
    if !request.form.is_empty() || !request.files.is_empty() {
        let form_fields = request
            .form
            .iter()
            .map(|(name, value)| (name.clone(), log_field_value(name, value)))
            .collect::<Vec<_>>();
        tracing::debug!(fields = ?form_fields, "准备 multipart 表单字段");
        let mut form = Form::new();
        for (name, value) in &request.form {
            form = form.text(name.clone(), value.clone());
        }
        for file in &request.files {
            if file.path.trim().is_empty() {
                let error = format!("上传文件路径为空: 字段 {}", file.field);
                tracing::error!(field = %file.field, "上传文件路径为空");
                return Err(HttpError::failed(error));
            }
            let path = upload_path(file_directory, &file.path).map_err(HttpError::failed)?;
            tracing::debug!(
                field = %file.field,
                path = %path.display(),
                configured_path = %file.path,
                "读取上传文件"
            );
            let bytes = fs::read(&path).map_err(|error| {
                tracing::error!(path = %path.display(), error = %error, "读取上传文件失败");
                format!("读取上传文件失败 {}: {error}", path.display())
            })?;
            let filename = file
                .filename
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| {
                    Path::new(&file.path)
                        .file_name()
                        .and_then(|value| value.to_str())
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_else(|| "upload.bin".to_string());
            tracing::debug!(
                field = %file.field,
                filename = %filename,
                content_type = ?file.content_type,
                bytes = bytes.len(),
                "上传文件已读取"
            );
            let mut part = Part::bytes(bytes).file_name(filename);
            if let Some(content_type) = &file.content_type {
                part = part.mime_str(content_type).map_err(|error| {
                    tracing::error!(
                        content_type = %content_type,
                        error = %error,
                        "上传文件类型无效"
                    );
                    HttpError::failed(format!("上传文件类型无效 {content_type}: {error}"))
                })?;
            }
            form = form.part(file.field.clone(), part);
        }
        builder = builder.multipart(form);
    } else if let Some(body) = &request.raw_body {
        tracing::debug!(body = %log_body(body), "准备原始请求体");
        builder = builder.body(body.clone());
    } else {
        tracing::debug!("请求没有请求体");
    }

    tracing::debug!("发出 HTTP 请求");
    let response = builder.send().map_err(|error| {
        tracing::error!(
            elapsed_ms = started.elapsed().as_millis(),
            error = %error,
            error_debug = ?error,
            "HTTP 请求失败"
        );
        HttpError::from_reqwest("请求失败", error)
    })?;
    let status = response.status();
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.to_str().unwrap_or("<非 UTF-8 值>").to_string(),
            )
        })
        .collect();
    let response_headers = headers
        .iter()
        .map(|(name, value)| (name.clone(), log_field_value(name, value)))
        .collect::<Vec<_>>();
    tracing::debug!(
        status = status.as_u16(),
        reason = status.canonical_reason().unwrap_or_default(),
        headers = ?response_headers,
        "收到 HTTP 响应头"
    );
    let auto_download = status.is_success() && response_looks_like_download(&headers);
    let download_target = match request.download.clone() {
        Some(DownloadTarget::Auto) => auto_download.then_some(DownloadTarget::RemoteName {
            use_content_disposition: true,
        }),
        Some(target) => Some(target),
        None => auto_download.then_some(DownloadTarget::RemoteName {
            use_content_disposition: true,
        }),
    };
    tracing::debug!(
        configured_download = ?request.download,
        auto_download,
        detected_download = ?download_target,
        "判断响应处理方式"
    );
    let (body, download_path) = if let Some(target) = &download_target {
        let bytes = response.bytes().map_err(|error| {
            tracing::error!(
                status = status.as_u16(),
                elapsed_ms = started.elapsed().as_millis(),
                error = %error,
                error_debug = ?error,
                "读取下载响应失败"
            );
            HttpError::from_reqwest("读取下载响应失败", error)
        })?;
        let path = save_download(&bytes, target, download_directory, &headers, &request.url)?;
        tracing::debug!(
            status = status.as_u16(),
            bytes = bytes.len(),
            path = %path.display(),
            "下载响应已保存"
        );
        (String::new(), Some(path))
    } else {
        let body = response.text().map_err(|error| {
            tracing::error!(
                status = status.as_u16(),
                elapsed_ms = started.elapsed().as_millis(),
                error = %error,
                error_debug = ?error,
                "读取响应失败"
            );
            HttpError::from_reqwest("读取响应失败", error)
        })?;
        (body, None)
    };
    let elapsed_ms = started.elapsed().as_millis();
    tracing::debug!(
        status = status.as_u16(),
        elapsed_ms,
        body_bytes = body.len(),
        download_path = download_path
            .as_deref()
            .map(|path| path.display().to_string()),
        body = %log_body(&body),
        "HTTP 响应读取完成"
    );

    Ok(ResponseData {
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or_default().to_string(),
        headers,
        body,
        download_path,
        elapsed_ms,
    })
}

fn save_download(
    bytes: &[u8],
    target: &DownloadTarget,
    download_directory: &Path,
    headers: &[(String, String)],
    url: &str,
) -> Result<PathBuf, String> {
    let configured_path = match target {
        DownloadTarget::Path(path) => path.trim().to_string(),
        DownloadTarget::RemoteName {
            use_content_disposition,
        } => {
            if *use_content_disposition {
                content_disposition_filename(headers)
                    .or_else(|| url_filename(url))
                    .unwrap_or_else(|| "download.bin".to_string())
            } else {
                url_filename(url).unwrap_or_else(|| "download.bin".to_string())
            }
        }
        DownloadTarget::Auto => unreachable!("automatic download target must be resolved first"),
    };
    if configured_path.is_empty() {
        return Err("下载文件路径不能为空".to_string());
    }

    let configured_path = Path::new(&configured_path);
    let destination = download_path(download_directory, configured_path)?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(download_directory);
    fs::create_dir_all(parent)
        .map_err(|error| format!("创建下载目录失败 {}: {error}", parent.display()))?;
    fs::write(&destination, bytes)
        .map_err(|error| format!("保存下载文件失败 {}: {error}", destination.display()))?;
    Ok(destination)
}

fn content_disposition_filename(headers: &[(String, String)]) -> Option<String> {
    let value = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-disposition"))
        .map(|(_, value)| value)?;
    value.split(';').skip(1).find_map(|part| {
        let (name, value) = part.split_once('=')?;
        if !name.trim().eq_ignore_ascii_case("filename")
            && !name.trim().eq_ignore_ascii_case("filename*")
        {
            return None;
        }
        safe_filename(value.trim().trim_matches('"'))
    })
}

fn url_filename(url: &str) -> Option<String> {
    let parsed = url.parse::<reqwest::Url>().ok()?;
    let name = parsed
        .path_segments()?
        .rev()
        .find(|segment| !segment.is_empty())?;
    safe_filename(name)
}

fn safe_filename(value: &str) -> Option<String> {
    let value = value.trim().rsplit(['/', '\\']).next()?.trim();
    (!value.is_empty() && !matches!(value, "." | "..")).then(|| value.to_string())
}

fn response_looks_like_download(headers: &[(String, String)]) -> bool {
    if headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("content-disposition")
            && value
                .split(';')
                .any(|part| part.trim().eq_ignore_ascii_case("attachment"))
    }) {
        return true;
    }

    headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .and_then(|(_, value)| value.split(';').next())
        .map(str::trim)
        .is_some_and(is_binary_media_type)
}

fn is_binary_media_type(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
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

fn upload_path(file_directory: &Path, configured_path: &str) -> Result<PathBuf, String> {
    resolve_child_path(
        file_directory,
        configured_path,
        "上传文件相对路径不能包含 ..",
    )
}

fn download_path(download_directory: &Path, configured_path: &Path) -> Result<PathBuf, String> {
    if configured_path.is_absolute() {
        return Ok(configured_path.to_path_buf());
    }
    if has_parent_component(configured_path) {
        return Err("下载文件相对路径不能包含 ..".to_string());
    }
    Ok(download_directory.join(configured_path))
}

fn resolve_child_path(
    root_directory: &Path,
    configured_path: &str,
    parent_error: &str,
) -> Result<PathBuf, String> {
    let configured_path = configured_path.trim();
    if configured_path.is_empty() {
        return Err("文件路径不能为空".to_string());
    }
    let path = Path::new(configured_path);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    if has_parent_component(path) {
        return Err(parent_error.to_string());
    }
    Ok(root_directory.join(path))
}

fn has_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn is_loopback_url(value: &str) -> bool {
    let Ok(url) = value.parse::<reqwest::Url>() else {
        return false;
    };
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
}

fn log_field_value(name: &str, value: &str) -> String {
    if is_sensitive_name(name) {
        "<已隐藏>".to_string()
    } else {
        log_text(value)
    }
}

fn is_sensitive_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("authorization")
        || name.contains("cookie")
        || name.contains("token")
        || name.contains("secret")
        || name.contains("password")
        || name == "api-key"
        || name == "x-api-key"
}

fn log_json_value(value: &serde_json::Value) -> String {
    serde_json::to_string(&redact_json_value(value))
        .map(|value| log_text(&value))
        .unwrap_or_else(|error| format!("<JSON 序列化失败: {error}>"))
}

fn redact_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(fields) => {
            let mut redacted = serde_json::Map::new();
            for (name, value) in fields {
                let value = if is_sensitive_name(name) {
                    serde_json::Value::String("<已隐藏>".to_string())
                } else {
                    redact_json_value(value)
                };
                redacted.insert(name.clone(), value);
            }
            serde_json::Value::Object(redacted)
        }
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.iter().map(redact_json_value).collect())
        }
        value => value.clone(),
    }
}

fn log_text(value: &str) -> String {
    if value.len() <= MAX_LOG_VALUE_BYTES {
        return value.to_string();
    }

    let end = value
        .char_indices()
        .find(|(index, _)| *index >= MAX_LOG_VALUE_BYTES)
        .map(|(index, _)| index)
        .unwrap_or(value.len());
    format!(
        "{}…<日志字段已截断，原始长度 {} 字节>",
        &value[..end],
        value.len()
    )
}

fn log_body(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .map(|value| log_json_value(&value))
        .unwrap_or_else(|_| log_text(body))
}

fn log_url(url: &str) -> String {
    let Some((path, query)) = url.split_once('?') else {
        return log_text(url);
    };
    let query = query
        .split('&')
        .map(|part| {
            let Some((name, value)) = part.split_once('=') else {
                return part.to_string();
            };
            if is_sensitive_name(name) {
                format!("{name}=<已隐藏>")
            } else {
                format!("{name}={}", log_text(value))
            }
        })
        .collect::<Vec<_>>()
        .join("&");
    log_text(&format!("{path}?{query}"))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        env,
        path::{Path, PathBuf},
    };

    use serde_json::Value;

    use super::*;
    use crate::{config, logging, template};

    #[test]
    #[ignore = "需要先启动 mock/run_e2e.py 提供 FastAPI 服务"]
    fn fastapi_mock_covers_configured_requests() {
        if !cfg!(debug_assertions) {
            eprintln!("FastAPI mock 测试需要 debug 构建，以便写入 debug 日志");
            return;
        }

        let config_path = Path::new("mock/.postui");
        let log_path = env::var_os("POSTUI_E2E_LOG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("target/postui-fastapi-e2e.log"));
        logging::init(true, &log_path).expect("应初始化 debug 日志");

        let app_config = config::load(config_path).expect("mock 测试配置应当可以加载");
        assert_eq!(app_config.requests.len(), 17);
        let mut variables = app_config
            .variables
            .iter()
            .map(|(name, definition)| {
                (
                    name.clone(),
                    definition
                        .default
                        .as_ref()
                        .map(config::value_to_string)
                        .unwrap_or_default(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        if let Some(host) = env::var_os("POSTUI_MOCK_HOST") {
            variables.insert("host".to_string(), host.to_string_lossy().into_owned());
        }
        let file_directory = app_config.file_directory.clone();
        let download_directory = app_config.download_directory.clone();

        for request in &app_config.requests {
            let resolved = template::resolve_request(request, &variables);
            tracing::debug!(
                request_id = %request.id,
                method = %resolved.method,
                url = %resolved.url,
                "FastAPI mock 测试发送接口"
            );
            let result = send(
                &resolved,
                request.timeout_seconds,
                &file_directory,
                &download_directory,
                &format!("e2e-{}", request.id),
            );

            match request.id.as_str() {
                "requests/01-health.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "service"), "postui-fastapi-mock");
                }
                "requests/02-search.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.query.q"), "文档审查 & edge");
                    assert_eq!(json_field(&response, "data.query.page"), "2");
                }
                "requests/03-create-task.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 201);
                    assert_eq!(json_field(&response, "data.taskId"), "task-from-config");
                    assert_eq!(json_field(&response, "data.payload.name"), "文档接口测试");
                }
                "requests/04-task-status.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.taskId"), "task-from-config");
                    assert_eq!(json_field(&response, "data.status"), "processing");
                    assert_eq!(json_field(&response, "data.items[0].id"), "file-001");
                }
                "requests/05-put-item.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.method"), "PUT");
                }
                "requests/06-patch-item.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.method"), "PATCH");
                }
                "requests/07-delete-item.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.deleted"), "true");
                }
                "requests/08-form.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.form.name"), "文档接口测试");
                    assert_eq!(json_field(&response, "data.form.note"), "multipart note");
                }
                "requests/09-upload.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(
                        json_field(&response, "data.files[0].filename"),
                        "sample-upload.txt"
                    );
                    assert_eq!(
                        json_field(&response, "data.files[1].filename"),
                        "second.txt"
                    );
                    assert_eq!(json_field(&response, "data.note"), "multipart note");
                }
                "requests/10-headers.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.token"), "mock-secret-token");
                    assert_eq!(json_field(&response, "data.cookie"), "session=session-001");
                    assert_eq!(json_field(&response, "data.user_agent"), "postui-test/1.0");
                    assert_eq!(
                        json_field(&response, "data.referer"),
                        "http://example.test/source"
                    );
                }
                "requests/11-redirect.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "service"), "postui-fastapi-mock");
                }
                "requests/12-error.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 422);
                    assert_eq!(json_field(&response, "error.code"), "MOCK_VALIDATION");
                }
                "requests/13-empty.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 204);
                    assert!(response.body.is_empty());
                }
                "requests/14-plain.http" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(response.body, "postui mock plain text\n");
                }
                "requests/15-timeout.http" | "requests/16-missing-file.http" => {
                    assert!(result.is_err(), "{} 应当返回错误", request.id);
                }
                "requests/17-empty-file-variable.http" => {
                    assert!(result.is_err(), "{} 应当返回错误", request.id);
                }
                other => panic!("未覆盖的 mock 请求: {other}"),
            }

            if let Ok(response) = &result {
                for extract in &request.extracts {
                    let value = template::extract_json_value(&response.body, &extract.path)
                        .unwrap_or_else(|error| {
                            panic!(
                                "接口 {} 提取 {} 失败: {}",
                                request.id, extract.variable, error
                            )
                        });
                    variables.insert(extract.variable.clone(), value);
                }
            }
        }

        tracing::debug!("FastAPI mock 全场景测试完成");
    }

    fn successful_ref(result: &Result<ResponseData, HttpError>, request_id: &str) -> ResponseData {
        result
            .as_ref()
            .unwrap_or_else(|error| panic!("接口 {request_id} 请求失败: {error}"))
            .clone()
    }

    fn json_field(response: &ResponseData, path: &str) -> String {
        let value: Value = serde_json::from_str(&response.body)
            .unwrap_or_else(|error| panic!("响应不是 JSON: {error}\n{}", response.body));
        let mut current = &value;
        for segment in path.split(['.', '[', ']']).filter(|part| !part.is_empty()) {
            current = match current {
                Value::Object(fields) => fields
                    .get(segment)
                    .unwrap_or_else(|| panic!("响应缺少字段 {path}: {response:?}")),
                Value::Array(items) => {
                    let index = segment.parse::<usize>().expect("数组路径应使用数字下标");
                    items
                        .get(index)
                        .unwrap_or_else(|| panic!("响应缺少数组字段 {path}: {response:?}"))
                }
                other => panic!("字段路径 {path} 无法继续读取: {other:?}"),
            };
        }
        match current {
            Value::String(value) => value.clone(),
            value => value.to_string(),
        }
    }

    #[test]
    fn saves_download_bytes_and_prefers_content_disposition_when_requested() {
        let directory =
            env::temp_dir().join(format!("postui-download-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let headers = vec![(
            "Content-Disposition".to_string(),
            "attachment; filename=report.pdf".to_string(),
        )];
        let path = save_download(
            b"not text",
            &DownloadTarget::RemoteName {
                use_content_disposition: true,
            },
            &directory,
            &headers,
            "https://example.test/reports/fallback.bin",
        )
        .expect("下载文件应当可以保存");

        assert_eq!(path, directory.join("report.pdf"));
        assert_eq!(fs::read(&path).expect("应当可以读取下载文件"), b"not text");
        fs::remove_dir_all(&directory).expect("应清理测试下载目录");
    }

    #[test]
    fn resolves_relative_upload_paths_against_the_configured_directory() {
        let directory = Path::new("/workspace/files");
        assert_eq!(
            upload_path(directory, "nested/report.pdf").expect("相对上传路径应当可以解析"),
            directory.join("nested/report.pdf")
        );
        assert_eq!(
            upload_path(directory, "/tmp/report.pdf").expect("绝对上传路径应当可以解析"),
            PathBuf::from("/tmp/report.pdf")
        );
        assert!(upload_path(directory, "../report.pdf").is_err());
    }

    #[test]
    fn sends_file_from_the_configured_upload_directory() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };

        let directory = env::temp_dir().join(format!(
            "postui-upload-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("系统时间应有效")
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("应创建上传目录");
        fs::write(directory.join("configured.txt"), b"configured-upload").expect("应写入上传文件");

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("应创建本地 HTTP 测试服务");
        let address = listener.local_addr().expect("应读取本地 HTTP 测试地址");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("应接收 HTTP 请求");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 4096];
            while !request
                .windows(b"configured-upload".len())
                .any(|window| window == b"configured-upload")
            {
                let count = stream.read(&mut chunk).expect("应读取 HTTP 请求");
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..count]);
            }
            assert!(
                request
                    .windows(b"configured-upload".len())
                    .any(|window| window == b"configured-upload"),
                "请求应包含配置目录中的文件内容"
            );
            let body = b"{\"uploaded\":true}";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .expect("应写入测试响应头");
            stream.write_all(body).expect("应写入测试响应体");
        });

        let request = ResolvedRequest {
            method: "POST".to_string(),
            url: format!("http://{address}/upload"),
            query_parts: Vec::new(),
            headers: BTreeMap::new(),
            raw_body: None,
            form: BTreeMap::new(),
            files: vec![crate::template::ResolvedFile {
                field: "file".to_string(),
                path: "configured.txt".to_string(),
                filename: Some("configured.txt".to_string()),
                content_type: Some("text/plain".to_string()),
            }],
            download: None,
        };
        let response = send(
            &request,
            2,
            &directory,
            &env::temp_dir().join("postui-upload-downloads"),
            "upload-directory-test",
        )
        .expect("上传请求应当成功");

        server.join().expect("测试 HTTP 服务线程应正常结束");
        assert_eq!(response.status, 200);
        fs::remove_dir_all(&directory).expect("应清理上传目录");
    }

    #[test]
    fn detects_attachment_and_binary_response_types() {
        assert!(response_looks_like_download(&[
            (
                "content-disposition".to_string(),
                "attachment; filename=report.pdf".to_string(),
            ),
            ("content-type".to_string(), "application/json".to_string()),
        ]));
        assert!(response_looks_like_download(&[(
            "content-type".to_string(),
            "application/octet-stream".to_string(),
        )]));
        assert!(!response_looks_like_download(&[(
            "content-type".to_string(),
            "application/json; charset=utf-8".to_string(),
        )]));
    }

    #[test]
    fn sends_binary_response_to_the_download_directory() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("应创建本地 HTTP 测试服务");
        let address = listener.local_addr().expect("应读取本地 HTTP 测试地址");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("应接收 HTTP 请求");
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            let body = b"binary-data";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=report.bin\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .expect("应写入测试响应头");
            stream.write_all(body).expect("应写入测试响应体");
        });

        let directory =
            env::temp_dir().join(format!("postui-auto-download-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let request = ResolvedRequest {
            method: "GET".to_string(),
            url: format!("http://{address}/reports/latest"),
            query_parts: Vec::new(),
            headers: BTreeMap::new(),
            raw_body: None,
            form: BTreeMap::new(),
            files: Vec::new(),
            download: None,
        };
        let response = send(
            &request,
            2,
            Path::new("."),
            &directory,
            "auto-download-test",
        )
        .expect("二进制响应应当可以保存");

        server.join().expect("测试 HTTP 服务线程应正常结束");
        assert_eq!(response.body, "");
        assert_eq!(response.download_path, Some(directory.join("report.bin")));
        assert_eq!(
            fs::read(response.download_path.expect("应记录下载路径"))
                .expect("应读取保存的下载文件"),
            b"binary-data"
        );
        fs::remove_dir_all(&directory).expect("应清理自动下载目录");
    }
}
