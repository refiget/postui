use std::{
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

#[cfg(test)]
use std::fs;

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
    pub(crate) body_bytes: Vec<u8>,
    pub(crate) elapsed_ms: u128,
}

pub(crate) fn send(
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
            let mut part = Part::file(&path).map_err(|error| {
                tracing::error!(path = %path.display(), error = %error, "读取上传文件失败");
                HttpError::failed(error.to_string())
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
    let body_bytes = response.bytes().map_err(|error| {
        tracing::error!(
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis(),
            error = %error,
            error_debug = ?error,
            "读取响应失败"
        );
        HttpError::from_reqwest("读取响应失败", error)
    })?;
    let body = String::from_utf8_lossy(&body_bytes).into_owned();
    let elapsed_ms = started.elapsed().as_millis();
    tracing::debug!(
        status = status.as_u16(),
        elapsed_ms,
        body_bytes = body_bytes.len(),
        body = %log_body(&body),
        "HTTP 响应读取完成"
    );

    Ok(ResponseData {
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or_default().to_string(),
        headers,
        body,
        body_bytes: body_bytes.to_vec(),
        elapsed_ms,
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
    serde_json::from_str::<serde_json::Value>(body)
        .map(|value| log_json_value(&value))
        .unwrap_or_else(|_| log_form_body(body).unwrap_or_else(|| log_text(body)))
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
    fn debug_logging_redacts_form_and_api_key_values() {
        let body = log_body("password=secret-value&flag&note=visible&api_key=key-value");
        assert_eq!(body, "password=<已隐藏>&flag&note=visible&api_key=<已隐藏>");
        assert_eq!(
            log_url("https://example.test/path?api_key=key-value&note=visible"),
            "https://example.test/path?api_key=<已隐藏>&note=visible"
        );
    }

    #[test]
    #[ignore = "需要先启动 mock/main.py 提供 FastAPI 服务"]
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
    fn missing_upload_file_preserves_the_standard_io_error() {
        let directory =
            env::temp_dir().join(format!("postui-missing-upload-test-{}", std::process::id()));
        let path = directory.join("does-not-exist.txt");
        let expected = fs::File::open(&path).unwrap_err().to_string();
        let request = ResolvedRequest {
            method: "POST".to_string(),
            url: "http://127.0.0.1:1/upload".to_string(),
            headers: BTreeMap::new(),
            raw_body: None,
            form: BTreeMap::new(),
            files: vec![crate::template::ResolvedFile {
                field: "file".to_string(),
                path: "does-not-exist.txt".to_string(),
                filename: None,
                content_type: None,
            }],
        };

        let error = send(&request, 1, &directory, "missing-upload-test").unwrap_err();

        assert_eq!(error, HttpError::Failed(expected));
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
            headers: BTreeMap::new(),
            raw_body: None,
            form: BTreeMap::new(),
            files: vec![crate::template::ResolvedFile {
                field: "file".to_string(),
                path: "configured.txt".to_string(),
                filename: Some("configured.txt".to_string()),
                content_type: Some("text/plain".to_string()),
            }],
        };
        let response =
            send(&request, 2, &directory, "upload-directory-test").expect("上传请求应当成功");

        server.join().expect("测试 HTTP 服务线程应正常结束");
        assert_eq!(response.status, 200);
        fs::remove_dir_all(&directory).expect("应清理上传目录");
    }

    #[test]
    fn keeps_binary_response_bytes_without_saving_automatically() {
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

        let directory = env::temp_dir().join(format!(
            "postui-no-auto-download-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        let request = ResolvedRequest {
            method: "GET".to_string(),
            url: format!("http://{address}/reports/latest"),
            headers: BTreeMap::new(),
            raw_body: None,
            form: BTreeMap::new(),
            files: Vec::new(),
        };
        let response = send(&request, 2, Path::new("."), "auto-download-test")
            .expect("二进制响应应当可以读取");

        server.join().expect("测试 HTTP 服务线程应正常结束");
        assert_eq!(response.body, "binary-data");
        assert_eq!(response.body_bytes, b"binary-data");
        assert!(!directory.exists());
    }
}
