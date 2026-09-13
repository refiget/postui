use crate::settings::{DEFAULT_SYNTAX_THEME, UiTheme};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::sync::OnceLock;
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, Theme, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

mod response;
pub use response::{
    ResponseHighlight, ResponseHighlightCache, clear_response_highlight_focus,
    take_response_highlight_change,
};

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEMES: OnceLock<ThemeSet> = OnceLock::new();

pub fn plain_style(theme: &UiTheme) -> Style {
    Style::default().fg(theme.text)
}

pub fn variable_style(base: Style, theme: &UiTheme) -> Style {
    base.patch(
        Style::default()
            .fg(theme.variable)
            .add_modifier(Modifier::BOLD),
    )
}

pub fn template_line(value: &str, base_style: Style, theme: &UiTheme) -> Line<'static> {
    Line::from(template_spans(value, base_style, theme))
}

pub fn template_spans(value: &str, base_style: Style, theme: &UiTheme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = value;

    while let Some((start, end, _)) = crate::template::find_placeholder(rest) {
        if start > 0 {
            spans.push(Span::styled(rest[..start].to_string(), base_style));
        }
        spans.push(Span::styled(
            rest[start..end].to_string(),
            variable_style(base_style, theme),
        ));
        rest = &rest[end..];
    }

    if !rest.is_empty() {
        spans.push(Span::styled(rest.to_string(), base_style));
    }
    spans
}

pub fn json_text_lines_window(
    value: &str,
    offset: usize,
    count: usize,
    theme: &UiTheme,
) -> Vec<Line<'static>> {
    if count == 0 {
        return Vec::new();
    }
    if value.len() > 64 * 1024 {
        return LinesWithEndings::from(value)
            .skip(offset)
            .take(count)
            .map(|line| {
                let mut end = line.len().min(8192);
                while !line.is_char_boundary(end) {
                    end -= 1;
                }
                template_line(trim_line_ending(&line[..end]), plain_style(theme), theme)
            })
            .collect();
    }
    let syntax_set = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let Some(syntax) = syntax_set.find_syntax_by_extension("json") else {
        return plain_lines(value, theme)
            .into_iter()
            .skip(offset)
            .take(count)
            .collect();
    };
    let mut highlighter = HighlightLines::new(syntax, syntax_theme(&theme.syntax_theme));

    LinesWithEndings::from(value)
        .enumerate()
        .filter_map(|(index, line)| {
            let line = trim_line_ending(line);
            let highlighted = highlighter.highlight_line(line, syntax_set);
            if index < offset {
                return None;
            }
            match highlighted {
                Ok(regions) => {
                    let mut spans = Vec::new();
                    for (style, text) in regions {
                        spans.extend(template_spans(text, syntect_style(style), theme));
                    }
                    Some(Line::from(spans))
                }
                Err(error) => {
                    tracing::debug!(error = %error, "JSON 语法高亮失败，使用普通文本");
                    Some(template_line(line, plain_style(theme), theme))
                }
            }
        })
        .take(count)
        .collect()
}

pub fn plain_lines(value: &str, theme: &UiTheme) -> Vec<Line<'static>> {
    LinesWithEndings::from(value)
        .map(|line| template_line(trim_line_ending(line), plain_style(theme), theme))
        .collect()
}

fn syntax_theme(name: &str) -> &'static Theme {
    let themes = THEMES.get_or_init(ThemeSet::load_defaults);
    if let Some(theme) = themes.themes.get(name) {
        return theme;
    }
    tracing::debug!(
        syntax_theme = %name,
        fallback = DEFAULT_SYNTAX_THEME,
        "找不到配置的语法主题，使用默认语法主题"
    );
    themes
        .themes
        .get(DEFAULT_SYNTAX_THEME)
        .or_else(|| themes.themes.values().next())
        .expect("syntect 默认主题不应为空")
}

fn syntect_style(style: syntect::highlighting::Style) -> Style {
    let mut tui_style = Style::default().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ));
    if style.font_style.intersects(FontStyle::BOLD) {
        tui_style = tui_style.add_modifier(Modifier::BOLD);
    }
    if style.font_style.intersects(FontStyle::ITALIC) {
        tui_style = tui_style.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.intersects(FontStyle::UNDERLINE) {
        tui_style = tui_style.add_modifier(Modifier::UNDERLINED);
    }
    tui_style
}

fn trim_line_ending(value: &str) -> &str {
    value
        .strip_suffix("\r\n")
        .or_else(|| value.strip_suffix('\n'))
        .or_else(|| value.strip_suffix('\r'))
        .unwrap_or(value)
}
