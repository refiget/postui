use std::io::{self, Write};

use bytes::Bytes;
use quick_xml::{Reader, Writer, events::Event};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyFormat {
    Json,
    Xml,
    Form,
    Text,
}

#[derive(Debug, Clone, Copy)]
pub enum FormatNote {
    Original,
    Limited,
    Invalid,
}

impl BodyFormat {
    pub fn syntax(self, headers: &[(String, String)]) -> Option<&'static str> {
        match self {
            Self::Json => return Some("json"),
            Self::Xml => return Some("xml"),
            _ => {}
        }
        let value = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))?
            .1
            .split(';')
            .next()?
            .trim()
            .to_ascii_lowercase();
        match value.as_str() {
            "text/html" | "application/xhtml+xml" => Some("html"),
            "text/css" => Some("css"),
            "text/javascript" | "application/javascript" => Some("js"),
            "application/yaml" | "application/x-yaml" | "text/yaml" | "text/x-yaml" => Some("yaml"),
            "text/markdown" => Some("md"),
            _ => None,
        }
    }

    pub fn detect(headers: &[(String, String)], body: &[u8]) -> Self {
        let media_type = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| {
                value
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase()
            });
        match media_type.as_deref() {
            Some("application/json" | "text/json") => Self::Json,
            Some(value) if value.ends_with("+json") => Self::Json,
            Some("application/xml" | "text/xml") => Self::Xml,
            Some(value) if value.ends_with("+xml") && value != "application/xhtml+xml" => Self::Xml,
            Some("application/x-www-form-urlencoded") => Self::Form,
            None if body
                .iter()
                .find(|byte| !byte.is_ascii_whitespace())
                .is_some_and(|byte| {
                    matches!(
                        byte,
                        b'{' | b'[' | b'"' | b'-' | b'0'..=b'9' | b't' | b'f' | b'n'
                    )
                }) =>
            {
                Self::Json
            }
            _ => Self::Text,
        }
    }

    pub fn format(
        self,
        body: &[u8],
        max_display_bytes: usize,
    ) -> Result<(Bytes, usize), FormatNote> {
        if matches!(self, Self::Text | Self::Json) {
            return Err(FormatNote::Original);
        }
        std::str::from_utf8(body).map_err(|_| FormatNote::Invalid)?;
        let mut writer = DisplayPrefix {
            bytes: Vec::new(),
            limit: max_display_bytes.max(1),
            total: 0,
        };
        match self {
            Self::Xml => format_xml(body, &mut writer)?,
            Self::Form => {
                for (name, value) in form_urlencoded::parse(body) {
                    // JSON quoting keeps decoded newlines and control characters unambiguous.
                    serde_json::to_writer(&mut writer, &name).map_err(|_| FormatNote::Limited)?;
                    writer.write_all(b": ").map_err(|_| FormatNote::Limited)?;
                    serde_json::to_writer(&mut writer, &value).map_err(|_| FormatNote::Limited)?;
                    writer.write_all(b"\n").map_err(|_| FormatNote::Limited)?;
                }
            }
            Self::Text | Self::Json => unreachable!(),
        }
        Ok((Bytes::from(writer.bytes), writer.total))
    }
}

// Retain only the configured display prefix, while accepting/counting the full output.
struct DisplayPrefix {
    bytes: Vec<u8>,
    limit: usize,
    total: usize,
}

impl Write for DisplayPrefix {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let retained = bytes.len().min(self.limit.saturating_sub(self.bytes.len()));
        self.bytes.extend_from_slice(&bytes[..retained]);
        self.total = self.total.saturating_add(bytes.len());
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn format_xml(body: &[u8], output: &mut impl Write) -> Result<(), FormatNote> {
    let mut reader = Reader::from_reader(body);
    let mut writer = Writer::new_with_indent(output, b' ', 2);
    let mut elements: Vec<(bool, bool)> = Vec::new();
    let mut roots = 0;
    loop {
        let event = reader.read_event().map_err(|_| FormatNote::Invalid)?;
        match &event {
            Event::Start(element) | Event::Empty(element) => {
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|_| FormatNote::Invalid)?;
                    if attribute.key.as_ref() == b"xml:space" {
                        return Err(FormatNote::Original);
                    }
                }
                if let Some((children, text)) = elements.last_mut() {
                    *children = true;
                    if *text {
                        return Err(FormatNote::Original);
                    }
                } else {
                    roots += 1;
                    if roots > 1 {
                        return Err(FormatNote::Invalid);
                    }
                }
                if matches!(event, Event::Start(_)) {
                    if elements.len() >= 128 {
                        return Err(FormatNote::Limited);
                    }
                    elements.push((false, false));
                }
            }
            Event::End(_) => {
                elements.pop().ok_or(FormatNote::Invalid)?;
            }
            Event::Text(text) if !text.as_ref().iter().all(u8::is_ascii_whitespace) => {
                let (children, has_text) = elements.last_mut().ok_or(FormatNote::Invalid)?;
                *has_text = true;
                if *children {
                    return Err(FormatNote::Original);
                }
            }
            Event::CData(_) | Event::GeneralRef(_) | Event::DocType(_) => {
                return Err(FormatNote::Original);
            }
            Event::Eof => {
                return if elements.is_empty() && roots == 1 {
                    Ok(())
                } else {
                    Err(FormatNote::Invalid)
                };
            }
            _ => {}
        }
        writer.write_event(event).map_err(|_| FormatNote::Limited)?;
    }
}
