use std::{error::Error, fmt, iter::Peekable, str::Chars, time::Duration};

use crate::config::{DataPart, FileUpload, NameValue, RequestParam};

const MAX_COMMAND_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<NameValue>,
    pub query_parts: Vec<DataPart>,
    pub body_parts: Vec<DataPart>,
    pub form: Vec<RequestParam>,
    pub files: Vec<FileUpload>,
    pub timeout: Option<Duration>,
    pub skip_ssl_verification: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    message: String,
}

impl ParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ParseError {}

#[derive(Debug, Default)]
struct ParsedArguments {
    method: Option<String>,
    urls: Vec<String>,
    headers: Vec<NameValue>,
    data: Vec<DataArgument>,
    forms: Vec<FormArgument>,
    timeout: Option<Duration>,
    skip_ssl_verification: bool,
    use_get: bool,
}

#[derive(Debug)]
enum DataArgument {
    Plain(String),
    UrlEncoded(String),
}

#[derive(Debug)]
enum FormArgument {
    Form(String),
    Literal(String),
}

pub fn parse(command: &str) -> Result<ImportedRequest, ParseError> {
    if command.len() > MAX_COMMAND_BYTES {
        return Err(ParseError::new("cURL 命令超过 4 MiB"));
    }
    let tokens = repair_option_markers(tokenize(command)?);
    let arguments = parse_arguments(&tokens[1..])?;
    build_request(arguments)
}

fn repair_option_markers(tokens: Vec<String>) -> Vec<String> {
    let mut repaired = Vec::with_capacity(tokens.len());
    let mut tokens = tokens.into_iter().peekable();
    while let Some(token) = tokens.next() {
        if token == "-"
            && let Some(option) = tokens.peek()
            && matches!(
                option.as_str(),
                "X" | "H" | "d" | "F" | "u" | "A" | "e" | "b" | "m"
            )
        {
            repaired.push(format!("-{option}"));
            tokens.next();
        } else {
            repaired.push(token);
        }
    }
    repaired
}

fn tokenize(command: &str) -> Result<Vec<String>, ParseError> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut chars = command.chars().peekable();
    let mut quote = None;
    let mut started = false;

    while let Some(character) = chars.next() {
        match quote {
            Some('\'') => {
                if character == '\'' {
                    quote = None;
                } else {
                    token.push(character);
                }
            }
            Some('"') => match character {
                '"' => quote = None,
                '\\' if consume_line_continuation(&mut chars) => {}
                '\\' => match chars.peek().copied() {
                    Some('"' | '\\' | '$' | '`') => token.push(chars.next().unwrap()),
                    _ => token.push('\\'),
                },
                _ => token.push(character),
            },
            None => match character {
                '\'' | '"' => {
                    quote = Some(character);
                    started = true;
                }
                '\\' if consume_line_continuation(&mut chars) => {
                    if started && (token.eq_ignore_ascii_case("curl") || token.starts_with('-')) {
                        tokens.push(std::mem::take(&mut token));
                        started = false;
                    }
                }
                '\\' => match chars.next() {
                    Some(escaped) => {
                        token.push(escaped);
                        started = true;
                    }
                    None => return Err(ParseError::new("cURL 命令末尾存在未完成的转义")),
                },
                '^' | '`' if consume_line_continuation(&mut chars) => {
                    if started && (token.eq_ignore_ascii_case("curl") || token.starts_with('-')) {
                        tokens.push(std::mem::take(&mut token));
                        started = false;
                    }
                }
                value if value.is_whitespace() => {
                    if started {
                        tokens.push(std::mem::take(&mut token));
                        started = false;
                    }
                }
                _ => {
                    token.push(character);
                    started = true;
                }
            },
            _ => unreachable!(),
        }
    }

    if quote.is_some() {
        return Err(ParseError::new("cURL 命令存在未闭合的引号"));
    }
    if started {
        tokens.push(token);
    }
    let curl_index = tokens
        .iter()
        .position(|value| value.eq_ignore_ascii_case("curl"));
    if curl_index.is_some_and(|index| index > 1) {
        return Err(ParseError::new("命令必须以 curl 开头"));
    }
    if curl_index == Some(1) && !is_shell_prompt(&tokens[0]) {
        return Err(ParseError::new("命令必须以 curl 开头"));
    }
    if curl_index == Some(1) {
        tokens.remove(0);
    }
    if curl_index.is_none() {
        return Err(ParseError::new("命令必须以 curl 开头"));
    }
    if tokens.len() == 1 {
        return Err(ParseError::new("cURL 命令中没有请求地址"));
    }
    Ok(tokens)
}

fn is_shell_prompt(value: &str) -> bool {
    matches!(value, "$" | ">" | "#") || value.ends_with('>')
}

fn consume_line_continuation(chars: &mut Peekable<Chars<'_>>) -> bool {
    let mut lookahead = chars.clone();
    let mut consumed = 0;
    while matches!(lookahead.peek(), Some(' ' | '\t')) {
        lookahead.next();
        consumed += 1;
    }
    match lookahead.next() {
        Some('\n') => consumed += 1,
        Some('\r') => {
            consumed += 1;
            if lookahead.next() == Some('\n') {
                consumed += 1;
            }
        }
        _ => return false,
    }
    for _ in 0..consumed {
        chars.next();
    }
    true
}

fn parse_arguments(tokens: &[String]) -> Result<ParsedArguments, ParseError> {
    let mut parsed = ParsedArguments::default();
    let mut index = 0;
    let mut options = true;

    while index < tokens.len() {
        let token = &tokens[index];
        if options && token == "--" {
            options = false;
            index += 1;
            continue;
        }
        if options && token.starts_with('-') && token != "-" {
            let (option, inline_value) = split_option(token);
            match option {
                "-X" | "--request" => {
                    parsed.method = Some(option_value(tokens, &mut index, option, inline_value)?);
                }
                "-H" | "--header" => {
                    let value = header_value(tokens, &mut index, option, inline_value)?;
                    parse_header(&value, &mut parsed.headers)?;
                }
                "-d" | "--data" | "--data-raw" | "--data-binary" => {
                    let value = option_value(tokens, &mut index, option, inline_value)?;
                    parsed.data.push(DataArgument::Plain(value));
                }
                "--data-urlencode" => {
                    let value = option_value(tokens, &mut index, option, inline_value)?;
                    parsed.data.push(DataArgument::UrlEncoded(value));
                }
                "--json" => {
                    let value = option_value(tokens, &mut index, option, inline_value)?;
                    parsed.data.push(DataArgument::Plain(value));
                    add_header_if_missing(&mut parsed.headers, "Content-Type", "application/json");
                    add_header_if_missing(&mut parsed.headers, "Accept", "application/json");
                }
                "-F" | "--form" => {
                    let value = option_value(tokens, &mut index, option, inline_value)?;
                    parsed.forms.push(FormArgument::Form(value));
                }
                "--form-string" => {
                    let value = option_value(tokens, &mut index, option, inline_value)?;
                    parsed.forms.push(FormArgument::Literal(value));
                }
                "-u" | "--user" => {
                    let value = option_value(tokens, &mut index, option, inline_value)?;
                    let credentials = base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        value.as_bytes(),
                    );
                    replace_header(
                        &mut parsed.headers,
                        "Authorization",
                        format!("Basic {credentials}"),
                    );
                }
                "-A" | "--user-agent" => {
                    let value = option_value(tokens, &mut index, option, inline_value)?;
                    replace_header(&mut parsed.headers, "User-Agent", value);
                }
                "-e" | "--referer" => {
                    let value = option_value(tokens, &mut index, option, inline_value)?;
                    replace_header(&mut parsed.headers, "Referer", value);
                }
                "-b" | "--cookie" => {
                    let value = option_value(tokens, &mut index, option, inline_value)?;
                    append_cookie(&mut parsed.headers, value);
                }
                "-m" | "--max-time" => {
                    let value = option_value(tokens, &mut index, option, inline_value)?;
                    let seconds = value
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite() && *value >= 0.0)
                        .ok_or_else(|| ParseError::new("--max-time 的值无效"))?;
                    parsed.timeout = Some(
                        Duration::try_from_secs_f64(seconds)
                            .map_err(|_| ParseError::new("--max-time 的值无效"))?,
                    );
                }
                "--url" => {
                    parsed
                        .urls
                        .push(option_value(tokens, &mut index, option, inline_value)?);
                }
                "-k" | "--insecure" => parsed.skip_ssl_verification = true,
                "-G" | "--get" => parsed.use_get = true,
                "-I" | "--head" => parsed.method = Some("HEAD".to_string()),
                "-L" | "--location" | "--compressed" | "-s" | "--silent" | "-S"
                | "--show-error" => {}
                _ => return Err(ParseError::new(format!("不支持的 cURL 参数: {option}"))),
            }
        } else {
            parsed.urls.push(token.clone());
        }
        index += 1;
    }

    Ok(parsed)
}

fn split_option(token: &str) -> (&str, Option<&str>) {
    if token.starts_with("--") {
        token
            .split_once('=')
            .map_or((token, None), |(option, value)| (option, Some(value)))
    } else if token.len() > 2
        && let Some(option @ ("-X" | "-H" | "-d" | "-F" | "-u" | "-A" | "-e" | "-b" | "-m")) =
            token.get(..2)
    {
        (option, Some(&token[2..]))
    } else {
        (token, None)
    }
}

fn header_value(
    tokens: &[String],
    index: &mut usize,
    option: &str,
    inline_value: Option<&str>,
) -> Result<String, ParseError> {
    let mut value = option_value(tokens, index, option, inline_value)?;
    while !value.contains(':') {
        let Some(next) = tokens.get(*index + 1) else {
            break;
        };
        if next.starts_with('-') || looks_like_url_start(next) {
            break;
        }
        *index += 1;
        value.push_str(next);
    }
    if value.ends_with(':')
        && let Some(next) = tokens.get(*index + 1)
        && !next.starts_with('-')
        && !looks_like_url_start(next)
    {
        *index += 1;
        value.push_str(next);
    }
    Ok(value)
}

fn option_value(
    tokens: &[String],
    index: &mut usize,
    option: &str,
    inline_value: Option<&str>,
) -> Result<String, ParseError> {
    if let Some(value) = inline_value {
        return Ok(value.to_string());
    }
    *index += 1;
    tokens
        .get(*index)
        .cloned()
        .ok_or_else(|| ParseError::new(format!("{option} 缺少参数值")))
}

fn parse_header(value: &str, headers: &mut Vec<NameValue>) -> Result<(), ParseError> {
    let Some((name, value)) = value.split_once(':') else {
        return Err(ParseError::new(format!("请求头格式无效: {value}")));
    };
    let name = name.trim();
    if name.is_empty() {
        return Err(ParseError::new("请求头名称为空"));
    }
    headers.push(NameValue {
        name: name.to_string(),
        value: value.trim_start().to_string(),
    });
    Ok(())
}

fn add_header_if_missing(headers: &mut Vec<NameValue>, name: &str, value: &str) {
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

fn replace_header(headers: &mut Vec<NameValue>, name: &str, value: String) {
    headers.retain(|header| !header.name.eq_ignore_ascii_case(name));
    headers.push(NameValue {
        name: name.to_string(),
        value,
    });
}

fn append_cookie(headers: &mut Vec<NameValue>, value: String) {
    if let Some(cookie) = headers
        .iter_mut()
        .find(|header| header.name.eq_ignore_ascii_case("Cookie"))
    {
        if !cookie.value.is_empty() && !value.is_empty() {
            cookie.value.push_str("; ");
        }
        cookie.value.push_str(&value);
    } else {
        headers.push(NameValue {
            name: "Cookie".to_string(),
            value,
        });
    }
}

fn merge_url_fragments(fragments: Vec<String>) -> Result<String, ParseError> {
    if fragments.is_empty() {
        return Err(ParseError::new("cURL 命令中没有请求地址"));
    }
    if fragments
        .iter()
        .filter(|fragment| looks_like_url_start(fragment))
        .count()
        > 1
    {
        return Err(ParseError::new("一条 cURL 命令只能包含一个请求地址"));
    }
    Ok(fragments.concat())
}

fn looks_like_url_start(value: &str) -> bool {
    value.contains("://")
        || value.starts_with("{{")
        || value
            .split_once(':')
            .map_or(value, |(host, _)| host)
            .parse::<std::net::IpAddr>()
            .is_ok()
}

fn build_request(arguments: ParsedArguments) -> Result<ImportedRequest, ParseError> {
    let has_body = !arguments.data.is_empty() || !arguments.forms.is_empty();
    let source_url = merge_url_fragments(arguments.urls)?;
    let (url, mut query_parts) = split_url(&source_url)?;
    let content_type = arguments
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("Content-Type"))
        .map(|header| header.value.to_ascii_lowercase());
    let mut body_parts = Vec::new();
    let mut form = Vec::new();
    let mut files = Vec::new();

    if arguments.use_get {
        query_parts.extend(arguments.data.into_iter().map(data_argument_to_part));
    } else if content_type
        .as_deref()
        .is_some_and(|value| value.starts_with("application/x-www-form-urlencoded"))
        || content_type.is_none()
            && arguments
                .data
                .iter()
                .all(|value| matches!(value, DataArgument::Plain(text) if looks_like_form(text)))
    {
        for argument in arguments.data {
            match argument {
                DataArgument::Plain(value) => {
                    body_parts.extend(value.split('&').map(raw_part));
                }
                DataArgument::UrlEncoded(value) => {
                    body_parts.push(DataPart::UrlEncoded(RequestParam::from_text(&value)));
                }
            }
        }
    } else {
        body_parts.extend(arguments.data.into_iter().map(data_argument_to_part));
    }

    for argument in arguments.forms {
        match argument {
            FormArgument::Form(value) => parse_form_part(&value, &mut form, &mut files, true)?,
            FormArgument::Literal(value) => parse_form_part(&value, &mut form, &mut files, false)?,
        }
    }

    let method = arguments.method.unwrap_or_else(|| {
        if arguments.use_get {
            "GET".to_string()
        } else if has_body {
            "POST".to_string()
        } else {
            "GET".to_string()
        }
    });
    crate::http_method::parse(&method)
        .map_err(|_| ParseError::new(format!("HTTP 方法无效: {method}")))?;

    Ok(ImportedRequest {
        method: method.to_ascii_uppercase(),
        url,
        headers: arguments.headers,
        query_parts,
        body_parts,
        form,
        files,
        timeout: arguments.timeout,
        skip_ssl_verification: arguments.skip_ssl_verification,
    })
}

fn split_url(value: &str) -> Result<(String, Vec<DataPart>), ParseError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ParseError::new("请求地址为空"));
    }
    let normalized = if value.contains("{{") || value.contains("://") {
        value.to_string()
    } else {
        format!("http://{value}")
    };
    if !normalized.contains("{{") {
        let scheme = normalized.split_once("://").map(|(scheme, _)| scheme);
        if !matches!(scheme, Some("http" | "https")) {
            return Err(ParseError::new("请求地址必须使用 http 或 https"));
        }
    }
    let parts = crate::template::split_url_query(&normalized);
    let query_parts = if parts.query.is_empty() {
        Vec::new()
    } else {
        parts.query.split('&').map(raw_part).collect()
    };
    let mut url = parts.base;
    if !parts.fragment.is_empty() {
        url.push('#');
        url.push_str(&parts.fragment);
    }
    Ok((url, query_parts))
}

fn raw_part(value: &str) -> DataPart {
    DataPart::Raw(value.to_string())
}

fn data_argument_to_part(argument: DataArgument) -> DataPart {
    match argument {
        DataArgument::Plain(value) => DataPart::Raw(value),
        DataArgument::UrlEncoded(value) => DataPart::UrlEncoded(RequestParam::from_text(&value)),
    }
}

fn looks_like_form(value: &str) -> bool {
    !value.trim_start().starts_with(['{', '['])
}

fn parse_form_part(
    value: &str,
    form: &mut Vec<RequestParam>,
    files: &mut Vec<FileUpload>,
    allow_file: bool,
) -> Result<(), ParseError> {
    let Some((field, value)) = value.split_once('=') else {
        return Err(ParseError::new(format!("表单字段格式无效: {value}")));
    };
    if field.is_empty() {
        return Err(ParseError::new("表单字段名称为空"));
    }
    if let Some(file) = value.strip_prefix('@').filter(|_| allow_file) {
        let mut attributes = file.split(';');
        let path = attributes.next().unwrap_or_default();
        if path.is_empty() {
            return Err(ParseError::new(format!("上传字段 {field} 没有文件路径")));
        }
        let mut upload = FileUpload {
            field: field.to_string(),
            path: path.to_string(),
            filename: None,
            content_type: None,
        };
        for attribute in attributes {
            if let Some(value) = attribute.strip_prefix("filename=") {
                upload.filename = Some(value.to_string());
            } else if let Some(value) = attribute.strip_prefix("type=") {
                upload.content_type = Some(value.to_string());
            }
        }
        files.push(upload);
    } else {
        form.push(RequestParam::new(
            field.to_string(),
            value.to_string(),
            true,
        ));
    }
    Ok(())
}
