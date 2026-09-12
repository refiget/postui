use std::{
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

use bytes::Bytes;
use reqwest::{
    Method,
    blocking::{
        Client,
        multipart::{Form, Part},
    },
    header::HeaderName,
};

use crate::template::ResolvedRequest;

const MAX_LOG_VALUE_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub(crate) enum HttpError {
    InvalidRequest(String),
    Upload(String),
    Timeout(reqwest::Error),
    Connection(reqwest::Error),
    Transport(reqwest::Error),
    ResponseRead(reqwest::Error),
    ClientInitialization(reqwest::Error),
}

impl HttpError {
    fn from_reqwest(error: reqwest::Error) -> Self {
        let error = error.without_url();
        if error.is_timeout() {
            Self::Timeout(error)
        } else if error.is_connect() {
            Self::Connection(error)
        } else if error.is_builder() {
            Self::InvalidRequest(error.to_string())
        } else {
            Self::Transport(error)
        }
    }

    pub(crate) fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout(_))
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) | Self::Upload(message) => formatter.write_str(message),
            Self::Timeout(_) => formatter.write_str("HTTP request timed out"),
            Self::Connection(_) => formatter.write_str("HTTP connection failed"),
            Self::Transport(_) => formatter.write_str("HTTP transport failed"),
            Self::ResponseRead(_) => formatter.write_str("Response body read failed"),
            Self::ClientInitialization(_) => {
                formatter.write_str("HTTP client initialization failed")
            }
        }
    }
}

impl Error for HttpError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Timeout(error)
            | Self::Connection(error)
            | Self::Transport(error)
            | Self::ResponseRead(error)
            | Self::ClientInitialization(error) => Some(error),
            Self::InvalidRequest(_) | Self::Upload(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HttpClient {
    regular: Client,
    no_proxy: Client,
}

impl HttpClient {
    pub(crate) fn new() -> Result<Self, HttpError> {
        let regular = build_client(false)?;
        let no_proxy = build_client(true)?;
        tracing::debug!("HTTP 客户端池初始化完成");
        Ok(Self { regular, no_proxy })
    }

    fn client(&self, loopback: bool) -> &Client {
        if loopback {
            &self.no_proxy
        } else {
            &self.regular
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResponseData {
    pub(crate) status: u16,
    pub(crate) reason: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body_bytes: Bytes,
    pub(crate) elapsed_ms: u128,
}

pub(crate) fn send(
    client: &HttpClient,
    request: &ResolvedRequest,
    timeout_seconds: u64,
    file_directory: &Path,
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
        file_directory = %file_directory.display(),
        "开始准备 HTTP 请求"
    );

    let method = Method::from_bytes(request.method.as_bytes()).map_err(|error| {
        tracing::debug!(error = %error, method = %request.method, "HTTP 方法无效");
        HttpError::InvalidRequest(format!("Invalid HTTP method: {error}"))
    })?;
    let mut builder = client
        .client(loopback)
        .request(method, &request.url)
        .timeout(Duration::from_secs(timeout_seconds.max(1)));
    if loopback {
        tracing::debug!("检测到本地地址，使用无代理 HTTP 客户端");
    } else {
        tracing::debug!("使用复用的 HTTP 客户端");
    }
    let request_headers = request
        .headers
        .iter()
        .map(|header| {
            (
                header.name.clone(),
                log_field_value(&header.name, &header.value),
            )
        })
        .collect::<Vec<_>>();
    tracing::debug!(headers = ?request_headers, "准备请求头");
    for header in &request.headers {
        let header_name = HeaderName::from_bytes(header.name.as_bytes()).map_err(|error| {
            tracing::debug!(header = %header.name, error = %error, "请求头名称无效");
            HttpError::InvalidRequest(format!("Invalid header name '{}': {error}", header.name))
        })?;
        builder = builder.header(header_name, &header.value);
    }
    if !request.form.is_empty() || !request.files.is_empty() {
        let form_fields = request
            .form
            .iter()
            .map(|field| {
                (
                    field.name.clone(),
                    log_field_value(&field.name, &field.value),
                )
            })
            .collect::<Vec<_>>();
        tracing::debug!(fields = ?form_fields, "准备 multipart 表单字段");
        let mut form = Form::new();
        for field in &request.form {
            form = form.text(field.name.clone(), field.value.clone());
        }
        for file in &request.files {
            if file.path.trim().is_empty() {
                let error = format!("Upload field '{}': file path is empty", file.field);
                tracing::debug!(field = %file.field, "上传文件路径为空");
                return Err(HttpError::Upload(error));
            }
            let path = upload_path(file_directory, &file.path).map_err(HttpError::Upload)?;
            tracing::debug!(
                field = %file.field,
                path = %path.display(),
                configured_path = %file.path,
                "读取上传文件"
            );
            let mut part = Part::file(&path).map_err(|error| {
                tracing::debug!(path = %path.display(), error = %error, "读取上传文件失败");
                HttpError::Upload(format!(
                    "Upload field '{}', file '{}': {error}",
                    file.field,
                    path.display()
                ))
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
                "上传文件已读取"
            );
            part = part.file_name(filename);
            if let Some(content_type) = &file.content_type {
                part = part.mime_str(content_type).map_err(|error| {
                    tracing::debug!(
                        content_type = %content_type,
                        error = %error,
                        "上传文件类型无效"
                    );
                    HttpError::InvalidRequest(format!(
                        "Invalid upload content type '{content_type}': {error}"
                    ))
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
        tracing::debug!(elapsed_ms = started.elapsed().as_millis(), "HTTP 请求失败");
        HttpError::from_reqwest(error)
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
    let body_bytes = response.bytes().map_err(|error| {
        tracing::debug!(
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis(),
            "读取响应失败"
        );
        if error.is_timeout() {
            HttpError::Timeout(error.without_url())
        } else {
            HttpError::ResponseRead(error.without_url())
        }
    })?;
    let elapsed_ms = started.elapsed().as_millis();
    tracing::debug!(
        status = status.as_u16(),
        elapsed_ms,
        body_bytes = body_bytes.len(),
        body = %log_body_bytes(&body_bytes),
        "HTTP 响应读取完成"
    );

    Ok(ResponseData {
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or_default().to_string(),
        headers,
        body_bytes,
        elapsed_ms,
    })
}

fn build_client(no_proxy: bool) -> Result<Client, HttpError> {
    let client_kind = if no_proxy { "no_proxy" } else { "regular" };
    let mut builder = Client::builder().user_agent("postui/0.1");
    if no_proxy {
        builder = builder.no_proxy();
    }
    builder.build().map_err(|error| {
        tracing::debug!(
            client = client_kind,
            error = %error,
            error_debug = ?error,
            "创建 HTTP 客户端失败"
        );
        HttpError::ClientInitialization(error.without_url())
    })
}

fn upload_path(file_directory: &Path, configured_path: &str) -> Result<PathBuf, String> {
    resolve_child_path(
        file_directory,
        configured_path,
        "上传文件相对路径不能包含 ..",
    )
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
    let compact = name.replace(['-', '_', ' '], "");
    name.contains("authorization")
        || name.contains("cookie")
        || name.contains("token")
        || name.contains("secret")
        || name.contains("password")
        || compact.contains("apikey")
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
    if body.len() > MAX_LOG_VALUE_BYTES {
        return format!("<日志字段已省略，原始长度 {} 字节>", body.len());
    }
    serde_json::from_str::<serde_json::Value>(body)
        .map(|value| log_json_value(&value))
        .unwrap_or_else(|_| log_form_body(body).unwrap_or_else(|| log_text(body)))
}

fn log_body_bytes(body: &[u8]) -> String {
    if body.len() > MAX_LOG_VALUE_BYTES {
        return format!("<日志字段已省略，原始长度 {} 字节>", body.len());
    }
    log_body(&String::from_utf8_lossy(body))
}

fn log_form_body(body: &str) -> Option<String> {
    let mut has_field = false;
    let mut fields = Vec::new();
    for part in body.split('&') {
        let Some((name, value)) = part.split_once('=') else {
            fields.push(log_text(part));
            continue;
        };
        if name.is_empty() {
            return None;
        }
        has_field = true;
        if is_sensitive_name(name) {
            fields.push(format!("{name}=<已隐藏>"));
        } else {
            fields.push(format!("{name}={}", log_text(value)));
        }
    }
    if !has_field {
        return None;
    }
    Some(log_text(&fields.join("&")))
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
