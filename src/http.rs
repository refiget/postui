use std::{
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

use bytes::Bytes;
use reqwest::{
    Client,
    header::HeaderName,
    multipart::{Form, Part},
};

use crate::template::ResolvedRequest;

mod logging;
use logging::{log_body, log_body_bytes, log_field_value, log_url, redact_secret_values};

#[derive(Debug)]
pub enum HttpError {
    InvalidRequest(String),
    Upload(String),
    Timeout(reqwest::Error),
    Connection(reqwest::Error),
    Transport(reqwest::Error),
    ResponseTooLarge(usize),
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

    pub fn is_timeout(&self) -> bool {
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
            Self::ResponseTooLarge(limit) => write!(
                formatter,
                "Response exceeds max_response_bytes ({limit} bytes)"
            ),
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
            | Self::ClientInitialization(error) => Some(error),
            Self::ResponseTooLarge(_) | Self::InvalidRequest(_) | Self::Upload(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpClient {
    regular: Client,
    no_proxy: Client,
    insecure: Client,
    insecure_no_proxy: Client,
}

impl HttpClient {
    pub fn new() -> Result<Self, HttpError> {
        let regular = build_client(false, false)?;
        let no_proxy = build_client(true, false)?;
        let insecure = build_client(false, true)?;
        let insecure_no_proxy = build_client(true, true)?;
        tracing::debug!("HTTP 客户端池初始化完成");
        Ok(Self {
            regular,
            no_proxy,
            insecure,
            insecure_no_proxy,
        })
    }

    fn client(&self, loopback: bool, skip_ssl_verification: bool) -> &Client {
        match (loopback, skip_ssl_verification) {
            (false, false) => &self.regular,
            (true, false) => &self.no_proxy,
            (false, true) => &self.insecure,
            (true, true) => &self.insecure_no_proxy,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResponseData {
    pub final_url: String,
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body_bytes: Bytes,
    pub elapsed_ms: u128,
    binary: bool,
}

pub struct RequestOptions<'a> {
    pub timeout_seconds: u64,
    pub file_directory: &'a Path,
    pub skip_ssl_verification: bool,
    pub secret_values: &'a [String],
    pub max_response_bytes: usize,
}

impl ResponseData {
    pub fn content_type(&self) -> Option<&str> {
        self.headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.as_str())
    }

    pub fn headers_text(&self) -> String {
        self.headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn is_binary(&self) -> bool {
        self.binary
    }
}

pub async fn send(
    client: &HttpClient,
    request: &ResolvedRequest,
    options: RequestOptions<'_>,
    operation_id: &str,
) -> Result<ResponseData, HttpError> {
    let RequestOptions {
        timeout_seconds,
        file_directory,
        skip_ssl_verification,
        secret_values,
        max_response_bytes,
    } = options;
    let started = Instant::now();
    let loopback = is_loopback_url(&request.url);
    tracing::debug!(operation_id, method = %request.method,
        url = %redact_secret_values(&log_url(&request.url), secret_values), "准备 HTTP 请求");
    tracing::debug!(
        timeout_seconds = timeout_seconds.max(1),
        loopback,
        skip_ssl_verification,
        header_count = request.headers.len(),
        form_field_count = request.form.len(),
        file_count = request.files.len(),
        has_body = request.raw_body.is_some(),
        file_directory = %file_directory.display(),
        "开始准备 HTTP 请求"
    );

    let method = crate::http_method::parse(&request.method).map_err(|error| {
        tracing::debug!(error = %error, method = %request.method, "HTTP 方法无效");
        HttpError::InvalidRequest(format!("Invalid HTTP method: {error}"))
    })?;
    let mut builder = client
        .client(loopback, skip_ssl_verification)
        .request(method, &request.url)
        .timeout(Duration::from_secs(timeout_seconds.max(1)));
    if loopback {
        tracing::debug!("检测到本地地址，使用无代理 HTTP 客户端");
    } else {
        tracing::debug!("使用复用的 HTTP 客户端");
    }
    if tracing::enabled!(tracing::Level::DEBUG) {
        let request_headers = request
            .headers
            .iter()
            .map(|header| {
                (
                    header.name.clone(),
                    redact_secret_values(
                        &log_field_value(&header.name, &header.value),
                        secret_values,
                    ),
                )
            })
            .collect::<Vec<_>>();
        tracing::debug!(headers = ?request_headers, "准备请求头");
    }
    for header in &request.headers {
        let header_name = HeaderName::from_bytes(header.name.as_bytes()).map_err(|error| {
            tracing::debug!(header = %header.name, error = %error, "请求头名称无效");
            HttpError::InvalidRequest(format!("Invalid header name '{}': {error}", header.name))
        })?;
        builder = builder.header(header_name, &header.value);
    }
    if !request.form.is_empty() || !request.files.is_empty() {
        if tracing::enabled!(tracing::Level::DEBUG) {
            let form_fields = request
                .form
                .iter()
                .map(|field| {
                    (
                        field.name.clone(),
                        redact_secret_values(
                            &log_field_value(&field.name, &field.value),
                            secret_values,
                        ),
                    )
                })
                .collect::<Vec<_>>();
            tracing::debug!(fields = ?form_fields, "准备 multipart 表单字段");
        }
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
            let path = upload_path(file_directory, &file.path)
                .await
                .map_err(HttpError::Upload)?;
            tracing::debug!(
                field = %file.field,
                path = %path.display(),
                configured_path = %file.path,
                "读取上传文件"
            );
            let mut part = Part::file(&path).await.map_err(|error| {
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
        tracing::debug!(body = %redact_secret_values(&log_body(body), secret_values), "准备原始请求体");
        builder = builder.body(body.clone());
    } else {
        tracing::debug!("请求没有请求体");
    }

    tracing::debug!("发出 HTTP 请求");
    let response = builder.send().await.map_err(|error| {
        tracing::debug!(elapsed_ms = started.elapsed().as_millis(), "HTTP 请求失败");
        HttpError::from_reqwest(error)
    })?;

    let final_url = response.url().to_string();
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
        .map(|(name, value)| {
            (
                name.clone(),
                redact_secret_values(&log_field_value(name, value), secret_values),
            )
        })
        .collect::<Vec<_>>();
    tracing::debug!(
        status = status.as_u16(),
        reason = status.canonical_reason().unwrap_or_default(),
        headers = ?response_headers,
        "收到 HTTP 响应头"
    );
    let mut response = response;
    if response
        .content_length()
        .is_some_and(|size| size > max_response_bytes as u64)
    {
        return Err(HttpError::ResponseTooLarge(max_response_bytes));
    }
    let mut body_bytes = Vec::new();
    loop {
        let chunk = response.chunk().await.map_err(|error| {
            tracing::debug!(
                status = status.as_u16(),
                elapsed_ms = started.elapsed().as_millis(),
                error = %error,
                "读取响应失败"
            );
            HttpError::from_reqwest(error)
        })?;
        let Some(chunk) = chunk else { break };
        if chunk.len() > max_response_bytes.saturating_sub(body_bytes.len()) {
            return Err(HttpError::ResponseTooLarge(max_response_bytes));
        }
        body_bytes.extend_from_slice(&chunk);
    }
    let body_bytes = bytes::Bytes::from(body_bytes);
    let elapsed_ms = started.elapsed().as_millis();
    tracing::debug!(
        status = status.as_u16(),
        elapsed_ms,
        body_bytes = body_bytes.len(),
        body = %redact_secret_values(&log_body_bytes(&body_bytes), secret_values),
        "HTTP 响应读取完成"
    );

    Ok(ResponseData {
        binary: std::str::from_utf8(&body_bytes).is_err() || body_bytes.contains(&0),
        final_url,
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or_default().to_string(),
        headers,
        body_bytes,
        elapsed_ms,
    })
}

fn build_client(no_proxy: bool, skip_ssl_verification: bool) -> Result<Client, HttpError> {
    let client_kind = match (no_proxy, skip_ssl_verification) {
        (false, false) => "regular",
        (true, false) => "no_proxy",
        (false, true) => "insecure",
        (true, true) => "insecure_no_proxy",
    };
    let mut builder = Client::builder()
        .user_agent("postui/0.1")
        .danger_accept_invalid_certs(skip_ssl_verification);
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

async fn upload_path(file_directory: &Path, configured_path: &str) -> Result<PathBuf, String> {
    let configured_path = configured_path.trim();
    if configured_path.is_empty() {
        return Err("文件路径不能为空".to_string());
    }
    let path = Path::new(configured_path);
    if !path.is_absolute()
        && path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("上传文件相对路径不能包含 ..".to_string());
    }
    let resolved = tokio::fs::canonicalize(file_directory.join(path))
        .await
        .map_err(|error| format!("上传文件 '{configured_path}': {error}"))?;
    if !path.is_absolute() {
        let root = tokio::fs::canonicalize(file_directory)
            .await
            .map_err(|error| format!("上传目录: {error}"))?;
        if !resolved.starts_with(root) {
            return Err("上传文件相对路径通过符号链接越界".to_string());
        }
    }
    if !tokio::fs::metadata(&resolved)
        .await
        .map_err(|error| format!("上传文件 '{configured_path}': {error}"))?
        .is_file()
    {
        return Err("上传路径必须指向普通文件".to_string());
    }
    Ok(resolved)
}

fn is_loopback_url(value: &str) -> bool {
    let Ok(url) = value.parse::<reqwest::Url>() else {
        return false;
    };
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
}
