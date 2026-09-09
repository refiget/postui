use std::{
    fs,
    path::{Path, PathBuf},
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

use crate::template::ResolvedRequest;

const MAX_LOG_VALUE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct ResponseData {
    pub(crate) status: u16,
    pub(crate) reason: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: String,
    pub(crate) elapsed_ms: u128,
}

pub(crate) fn send(
    request: &ResolvedRequest,
    timeout_seconds: u64,
    file_directory: &Path,
    operation_id: &str,
) -> Result<ResponseData, String> {
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
        format!("HTTP 方法无效: {error}")
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
        format!("创建 HTTP 客户端失败: {error}")
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
            format!("请求头名称无效 {name}: {error}")
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
                return Err(error);
            }
            let path = upload_path(file_directory, &file.path);
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
                    format!("上传文件类型无效 {content_type}: {error}")
                })?;
            }
            form = form.part(file.field.clone(), part);
        }
        builder = builder.multipart(form);
    } else if let Some(body) = &request.raw_body {
        tracing::debug!(body = %log_request_body(body), "准备原始请求体");
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
        format!("请求失败: {error}")
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
    let body = response.text().map_err(|error| {
        tracing::error!(
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis(),
            error = %error,
            error_debug = ?error,
            "读取响应失败"
        );
        format!("读取响应失败: {error}")
    })?;
    let elapsed_ms = started.elapsed().as_millis();
    tracing::debug!(
        status = status.as_u16(),
        elapsed_ms,
        body_bytes = body.len(),
        body = %log_response_body(&body),
        "HTTP 响应读取完成"
    );

    Ok(ResponseData {
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or_default().to_string(),
        headers,
        body,
        elapsed_ms,
    })
}

fn upload_path(file_directory: &Path, configured_path: &str) -> PathBuf {
    let path = Path::new(configured_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        file_directory.join(path)
    }
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

fn log_response_body(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .map(|value| log_json_value(&value))
        .unwrap_or_else(|_| log_text(body))
}

fn log_request_body(body: &str) -> String {
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

        let config_path = Path::new("mock/.postui/requests.yaml");
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
        let file_directory = config_path
            .parent()
            .expect("mock 配置应当有父目录")
            .join(&app_config.file_directory);

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
                app_config.timeout_seconds,
                &file_directory,
                &format!("e2e-{}", request.id),
            );

            match request.id.as_str() {
                "health" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "service"), "postui-fastapi-mock");
                }
                "search" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.query.q"), "文档审查 & edge");
                    assert_eq!(json_field(&response, "data.query.page"), "2");
                }
                "create-task" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 201);
                    assert_eq!(json_field(&response, "data.taskId"), "task-from-config");
                    assert_eq!(json_field(&response, "data.payload.name"), "文档接口测试");
                }
                "task-status" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.taskId"), "task-from-config");
                    assert_eq!(json_field(&response, "data.status"), "processing");
                    assert_eq!(json_field(&response, "data.items[0].id"), "file-001");
                }
                "put-item" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.method"), "PUT");
                }
                "patch-item" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.method"), "PATCH");
                }
                "delete-item" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.deleted"), "true");
                }
                "form" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "data.form.name"), "文档接口测试");
                    assert_eq!(json_field(&response, "data.form.note"), "multipart note");
                }
                "upload" => {
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
                "headers" => {
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
                "redirect" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(json_field(&response, "service"), "postui-fastapi-mock");
                }
                "error" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 422);
                    assert_eq!(json_field(&response, "error.code"), "MOCK_VALIDATION");
                }
                "empty" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 204);
                    assert!(response.body.is_empty());
                }
                "plain" => {
                    let response = successful_ref(&result, request.id.as_str());
                    assert_eq!(response.status, 200);
                    assert_eq!(response.body, "postui mock plain text\n");
                }
                "timeout" | "missing-file" => {
                    assert!(result.is_err(), "{} 应当返回错误", request.id);
                }
                "empty-file-variable" => {
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

    fn successful_ref(result: &Result<ResponseData, String>, request_id: &str) -> ResponseData {
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
}
