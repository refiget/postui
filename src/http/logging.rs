const MAX_LOG_VALUE_BYTES: usize = 64 * 1024;

pub(super) fn log_field_value(name: &str, value: &str) -> String {
    if is_sensitive_name(name) {
        "<已隐藏>".to_string()
    } else {
        log_text(value)
    }
}

pub(super) fn redact_secret_values(value: &str, secret_values: &[String]) -> String {
    secret_values
        .iter()
        .fold(value.to_string(), |redacted, secret| {
            redacted.replace(secret, "<已隐藏>")
        })
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

pub(super) fn log_body(body: &str) -> String {
    if body.len() > MAX_LOG_VALUE_BYTES {
        return format!("<日志字段已省略，原始长度 {} 字节>", body.len());
    }
    serde_json::from_str::<serde_json::Value>(body)
        .map(|value| log_json_value(&value))
        .unwrap_or_else(|_| log_form_body(body).unwrap_or_else(|| log_text(body)))
}

pub(super) fn log_body_bytes(body: &[u8]) -> String {
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

pub(super) fn log_url(url: &str) -> String {
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
