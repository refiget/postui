use std::path::Path;

use anyhow::{Result, bail};

use super::{DataPart, FileUpload, NameValue, ParsedCommand, RequestParam};

pub(super) fn parse_curl(source: &str, request_id: &str) -> Result<ParsedCommand> {
    if source.trim().is_empty() {
        bail!("接口 {} 缺少 request 内容", request_id)
    }
    let command = clean_command_block(source);
    if command.contains("$(") || command.contains('`') {
        bail!("接口 {} 的 request 不支持 shell 命令替换", request_id)
    }
    let tokens = shlex::split(&command)
        .ok_or_else(|| anyhow::anyhow!("接口 {} 的 request 存在未闭合的引号", request_id))?;
    let Some(start) = tokens.iter().position(|token| is_curl_command(token)) else {
        bail!("接口 {} 的 request 中没有找到 curl 命令", request_id)
    };

    let mut parsed = ParsedCommand::default();
    parse_curl_tokens(&mut parsed, &tokens[start + 1..], request_id)?;

    if parsed
        .url
        .as_deref()
        .is_none_or(|url| url.trim().is_empty())
    {
        bail!("接口 {} 的 curl 命令缺少 URL", request_id)
    }
    if parsed.get_mode {
        parsed.query_data = std::mem::take(&mut parsed.data)
            .into_iter()
            .flat_map(split_query_data_part)
            .collect();
    }
    let has_data = !parsed.data.is_empty();
    let has_form = !parsed.form.is_empty() || !parsed.files.is_empty();
    if has_data {
        if parsed.method.is_none() {
            parsed.method = Some("POST".to_string());
        }
        insert_header_if_missing(
            &mut parsed.headers,
            "Content-Type",
            "application/x-www-form-urlencoded",
        );
    } else if has_form && parsed.method.is_none() {
        parsed.method = Some("POST".to_string());
    }
    if has_data && has_form {
        bail!("接口 {} 的 curl 命令不能同时使用 data 和 form", request_id)
    }
    Ok(parsed)
}

fn split_query_data_part(part: DataPart) -> Vec<DataPart> {
    match part {
        DataPart::Raw(value) => value
            .split('&')
            .filter(|part| !part.is_empty())
            .map(|part| DataPart::Raw(part.to_string()))
            .collect(),
        part => vec![part],
    }
}

fn parse_curl_tokens(
    parsed: &mut ParsedCommand,
    tokens: &[String],
    request_id: &str,
) -> Result<()> {
    let mut stop_options = false;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if stop_options {
            set_url(&mut parsed.url, token, request_id)?;
            index += 1;
            continue;
        }
        if token == "--" {
            stop_options = true;
            index += 1;
            continue;
        }
        parse_curl_option(parsed, tokens, &mut index, request_id)?;
        index += 1;
    }
    Ok(())
}

fn parse_curl_option(
    parsed: &mut ParsedCommand,
    tokens: &[String],
    index: &mut usize,
    request_id: &str,
) -> Result<()> {
    let token = &tokens[*index];
    if let Some((option, value)) = attached_option(token) {
        return parse_option_value(parsed, option, value, request_id);
    }

    if is_value_option(token) {
        let value = next_argument(tokens, index, token, request_id)?;
        return parse_option_value(parsed, token, &value, request_id);
    }

    match token.as_str() {
        "-G" | "--get" => {
            parsed.get_mode = true;
            parsed.method = Some("GET".to_string());
        }
        _ if is_ignored_curl_flag(token) => {}
        _ if token.starts_with('-') => {
            bail!("接口 {} 的 curl 参数不支持: {}", request_id, token)
        }
        _ => {
            set_url(&mut parsed.url, token, request_id)?;
        }
    }
    Ok(())
}

fn attached_option(token: &str) -> Option<(&str, &str)> {
    for option in ["-X", "-H", "-d", "-F", "-o"] {
        if let Some(value) = token.strip_prefix(option).filter(|value| !value.is_empty()) {
            return Some((option, value));
        }
    }

    let (option, value) = token.split_once('=')?;
    is_value_option(option).then_some((option, value))
}

fn is_value_option(option: &str) -> bool {
    matches!(
        option,
        "-X" | "--request"
            | "-H"
            | "--header"
            | "-d"
            | "--url"
            | "--data"
            | "--data-ascii"
            | "--data-binary"
            | "--data-raw"
            | "--data-urlencode"
            | "--json"
            | "-F"
            | "--form"
            | "--form-string"
            | "-o"
            | "--output"
            | "-b"
            | "--cookie"
            | "-A"
            | "--user-agent"
            | "-e"
            | "--referer"
    )
}

fn parse_option_value(
    parsed: &mut ParsedCommand,
    option: &str,
    value: &str,
    request_id: &str,
) -> Result<()> {
    match option {
        "-X" | "--request" => parsed.method = Some(value.to_string()),
        "-H" | "--header" => parse_header(&mut parsed.headers, value, request_id)?,
        "-d" | "--data" | "--data-ascii" | "--data-binary" | "--data-raw" | "--data-urlencode"
        | "--json" => parse_body_argument(parsed, option, value, request_id)?,
        "-F" | "--form" => parse_form(parsed, value, request_id)?,
        "--form-string" => parse_form_string(&mut parsed.form, value, request_id)?,
        "-o" | "--output" => {}
        "-b" | "--cookie" => {
            parsed.headers.push(NameValue {
                name: "Cookie".to_string(),
                value: value.to_string(),
            });
        }
        "-A" | "--user-agent" => {
            parsed.headers.push(NameValue {
                name: "User-Agent".to_string(),
                value: value.to_string(),
            });
        }
        "-e" | "--referer" => {
            parsed.headers.push(NameValue {
                name: "Referer".to_string(),
                value: value.to_string(),
            });
        }
        "--url" => set_url(&mut parsed.url, value, request_id)?,
        _ => unreachable!("unsupported curl value option: {option}"),
    }
    Ok(())
}

fn clean_command_block(source: &str) -> String {
    let mut command = String::new();
    let mut saw_fence = false;
    let mut in_fence = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if saw_fence && in_fence {
                in_fence = false;
            } else if !saw_fence {
                saw_fence = true;
                in_fence = true;
            }
            continue;
        }
        if saw_fence && !in_fence {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        let line = line.trim_end();
        if let Some(line) = line.strip_suffix('\\') {
            command.push_str(line);
            command.push(' ');
        } else if let Some(line) = line.strip_suffix('`') {
            command.push_str(line);
            command.push(' ');
        } else {
            command.push_str(line);
            command.push('\n');
        }
    }
    command
}

fn is_curl_command(token: &str) -> bool {
    Path::new(token)
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("curl") || value.eq_ignore_ascii_case("curl.exe")
        })
}

fn next_argument(
    tokens: &[String],
    index: &mut usize,
    option: &str,
    request_id: &str,
) -> Result<String> {
    *index += 1;
    tokens
        .get(*index)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("接口 {} 的 curl 参数 {} 缺少值", request_id, option))
}

fn set_url(url: &mut Option<String>, value: &str, request_id: &str) -> Result<()> {
    if url.replace(value.to_string()).is_some() {
        bail!("接口 {} 的 curl 命令包含多个 URL", request_id)
    }
    Ok(())
}

fn parse_header(headers: &mut Vec<NameValue>, value: &str, request_id: &str) -> Result<()> {
    let Some((name, value)) = value.split_once(':') else {
        bail!("接口 {} 的 curl 请求头格式无效: {}", request_id, value)
    };
    let name = name.trim();
    if name.is_empty() {
        bail!("接口 {} 的 curl 请求头名称不能为空", request_id)
    }
    headers.push(NameValue {
        name: name.to_string(),
        value: value.trim().to_string(),
    });
    Ok(())
}

fn parse_form(parsed: &mut ParsedCommand, value: &str, request_id: &str) -> Result<()> {
    let (field, content) = split_form_field(value, request_id)?;
    if let Some(file) = content.strip_prefix('@') {
        let mut parts = file.split(';');
        let path = parts.next().unwrap_or_default().trim();
        if path.is_empty() {
            bail!("接口 {} 的 curl 上传文件路径不能为空", request_id)
        }
        let mut filename = None;
        let mut content_type = None;
        for part in parts {
            let Some((key, value)) = part.split_once('=') else {
                bail!("接口 {} 的 curl 文件参数格式无效: {}", request_id, part)
            };
            match key.trim() {
                "filename" => filename = Some(value.trim().to_string()),
                "type" => content_type = Some(value.trim().to_string()),
                key => bail!("接口 {} 的 curl 文件参数不支持: {}", request_id, key),
            }
        }
        parsed.files.push(FileUpload {
            field: field.to_string(),
            path: path.to_string(),
            filename,
            content_type,
        });
    } else {
        parsed.form.push(RequestParam::new(
            field.to_string(),
            content.to_string(),
            true,
        ));
    }
    Ok(())
}

fn parse_form_string(form: &mut Vec<RequestParam>, value: &str, request_id: &str) -> Result<()> {
    let (field, content) = split_form_field(value, request_id)?;
    form.push(RequestParam::new(
        field.to_string(),
        content.to_string(),
        true,
    ));
    Ok(())
}

fn split_form_field<'a>(value: &'a str, request_id: &str) -> Result<(&'a str, &'a str)> {
    let Some((field, content)) = value.split_once('=') else {
        bail!("接口 {} 的 curl 表单字段格式无效: {}", request_id, value)
    };
    let field = field.trim();
    if field.is_empty() {
        bail!("接口 {} 的 curl 表单字段名称不能为空", request_id)
    }
    Ok((field, content))
}

fn parse_body_argument(
    parsed: &mut ParsedCommand,
    option: &str,
    value: &str,
    request_id: &str,
) -> Result<()> {
    match option {
        "--data-urlencode" => parsed
            .data
            .push(DataPart::UrlEncoded(RequestParam::from_text(value))),
        "--json" => {
            reject_body_file(value, option, request_id)?;
            parsed.data.push(DataPart::Raw(value.to_string()));
            insert_header_if_missing(&mut parsed.headers, "Content-Type", "application/json");
            insert_header_if_missing(&mut parsed.headers, "Accept", "application/json");
        }
        "--data-raw" => parsed.data.push(DataPart::Raw(value.to_string())),
        _ => {
            reject_body_file(value, option, request_id)?;
            parsed.data.push(DataPart::Raw(value.to_string()));
        }
    }
    Ok(())
}

fn reject_body_file(value: &str, option: &str, request_id: &str) -> Result<()> {
    if value.starts_with('@') {
        bail!(
            "接口 {} 的 curl 参数 {} 不支持通过 @ 读取请求体文件，请使用 --form 上传文件",
            request_id,
            option
        )
    }
    Ok(())
}

fn insert_header_if_missing(headers: &mut Vec<NameValue>, name: &str, value: &str) {
    if !headers
        .iter()
        .any(|header| header.name.eq_ignore_ascii_case(name))
    {
        headers.push(NameValue {
            name: name.to_string(),
            value: value.to_string(),
        });
    }
}

fn is_ignored_curl_flag(value: &str) -> bool {
    matches!(
        value,
        "--location"
            | "--compressed"
            | "--silent"
            | "--show-error"
            | "--fail"
            | "--fail-with-body"
            | "--globoff"
            | "--path-as-is"
            | "--http1.1"
            | "--http2"
            | "--http2-prior-knowledge"
            | "--no-buffer"
            | "--verbose"
            | "-s"
            | "-S"
            | "-f"
            | "-v"
            | "-i"
            | "-sS"
            | "-O"
            | "--remote-name"
            | "--remote-name-all"
            | "-J"
            | "--remote-header-name"
            | "-OJ"
    )
}
