use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use ratatui::style::Color;
use serde::Deserialize;

const DEFAULT_REQUEST_CONFIG: &str = ".postui/requests.yaml";
const DEFAULT_THEME: &str = "gruvbox-dark";
pub(crate) const DEFAULT_SYNTAX_THEME: &str = "base16-mocha.dark";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub(crate) enum Language {
    #[serde(rename = "en", alias = "english")]
    #[default]
    English,
    #[serde(rename = "zh", alias = "chinese")]
    Chinese,
}

impl Language {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "zh",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GlobalConfig {
    pub(crate) path: Option<PathBuf>,
    pub(crate) request_config: PathBuf,
    pub(crate) language: Language,
    pub(crate) theme: UiTheme,
}

#[derive(Debug, Clone)]
pub(crate) struct UiTheme {
    pub(crate) name: String,
    pub(crate) primary: Color,
    pub(crate) secondary: Color,
    pub(crate) accent: Color,
    pub(crate) background: Color,
    pub(crate) surface: Color,
    pub(crate) text: Color,
    pub(crate) muted: Color,
    pub(crate) error: Color,
    pub(crate) success: Color,
    pub(crate) warning: Color,
    pub(crate) selection: Color,
    pub(crate) variable: Color,
    pub(crate) syntax_theme: String,
    pub(crate) highlight_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGlobalConfig {
    #[serde(default = "default_request_config")]
    request_config: PathBuf,
    #[serde(default)]
    language: Language,
    #[serde(default = "default_theme")]
    theme: String,
    #[serde(default)]
    theme_file: Option<PathBuf>,
    #[serde(default)]
    highlight: RawHighlightConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHighlightConfig {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    syntax: Option<String>,
    #[serde(default)]
    variable: Option<String>,
}

impl Default for RawHighlightConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            syntax: None,
            variable: None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTheme {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    primary: Option<String>,
    #[serde(default)]
    secondary: Option<String>,
    #[serde(default)]
    accent: Option<String>,
    #[serde(default)]
    background: Option<String>,
    #[serde(default)]
    surface: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    muted: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    success: Option<String>,
    #[serde(default)]
    warning: Option<String>,
    #[serde(default)]
    selection: Option<String>,
    #[serde(default)]
    variable: Option<String>,
    #[serde(default)]
    syntax: Option<String>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            path: None,
            request_config: PathBuf::from(DEFAULT_REQUEST_CONFIG),
            language: Language::default(),
            theme: UiTheme::default(),
        }
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            name: DEFAULT_THEME.to_string(),
            primary: Color::Rgb(131, 165, 152),
            secondary: Color::Rgb(250, 189, 47),
            accent: Color::Rgb(142, 192, 124),
            background: Color::Rgb(40, 40, 40),
            surface: Color::Rgb(60, 56, 54),
            text: Color::Rgb(235, 219, 178),
            muted: Color::Rgb(168, 153, 132),
            error: Color::Rgb(251, 73, 52),
            success: Color::Rgb(184, 187, 38),
            warning: Color::Rgb(254, 128, 25),
            selection: Color::Rgb(80, 73, 69),
            variable: Color::Rgb(211, 134, 155),
            syntax_theme: DEFAULT_SYNTAX_THEME.to_string(),
            highlight_enabled: true,
        }
    }
}

pub(crate) fn load(path: &Path) -> Result<GlobalConfig> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("无法读取全局配置文件: {}", path.display()))?;
    let raw: RawGlobalConfig = parse_document(path, &text)?;
    let global = normalize(path, raw)?;
    tracing::debug!(
        path = %path.display(),
        request_config = %global.request_config.display(),
        language = global.language.as_str(),
        theme = %global.theme.name,
        syntax_theme = %global.theme.syntax_theme,
        highlight_enabled = global.theme.highlight_enabled,
        "全局配置加载完成"
    );
    Ok(global)
}

pub(crate) fn default_config() -> GlobalConfig {
    tracing::debug!(
        request_config = %DEFAULT_REQUEST_CONFIG,
        language = Language::default().as_str(),
        theme = DEFAULT_THEME,
        "未找到全局配置，使用内置默认配置"
    );
    GlobalConfig::default()
}

fn normalize(path: &Path, raw: RawGlobalConfig) -> Result<GlobalConfig> {
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let request_config = resolve_path(base, &raw.request_config);
    let mut raw_theme = if let Some(theme_file) = raw.theme_file.as_deref() {
        let theme_path = resolve_path(base, theme_file);
        let text = fs::read_to_string(&theme_path)
            .with_context(|| format!("无法读取主题文件: {}", theme_path.display()))?;
        let theme: RawTheme = parse_document(&theme_path, &text)?;
        tracing::debug!(theme_file = %theme_path.display(), "读取自定义主题文件");
        theme
    } else {
        built_in_theme(&raw.theme).ok_or_else(|| {
            anyhow::anyhow!(
                "未知内置主题: {}；可选 gruvbox-dark、ocean、nord、mono，或设置 theme_file",
                raw.theme
            )
        })?
    };

    let theme_name = raw_theme
        .name
        .take()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| raw.theme.trim().to_string());
    let syntax_theme = raw
        .highlight
        .syntax
        .or(raw_theme.syntax.take())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SYNTAX_THEME.to_string());
    let variable = raw
        .highlight
        .variable
        .or(raw_theme.variable.take())
        .unwrap_or_else(|| "#d3869b".to_string());

    Ok(GlobalConfig {
        path: Some(path.to_path_buf()),
        request_config,
        language: raw.language,
        theme: UiTheme {
            name: theme_name,
            primary: color("primary", raw_theme.primary, "#83a598")?,
            secondary: color("secondary", raw_theme.secondary, "#fabd2f")?,
            accent: color("accent", raw_theme.accent, "#8ec07c")?,
            background: color("background", raw_theme.background, "#282828")?,
            surface: color("surface", raw_theme.surface, "#3c3836")?,
            text: color("text", raw_theme.text, "#ebdbb2")?,
            muted: color("muted", raw_theme.muted, "#a89984")?,
            error: color("error", raw_theme.error, "#fb4934")?,
            success: color("success", raw_theme.success, "#b8bb26")?,
            warning: color("warning", raw_theme.warning, "#fe8019")?,
            selection: color("selection", raw_theme.selection, "#504945")?,
            variable: color("variable", Some(variable), "#d3869b")?,
            syntax_theme,
            highlight_enabled: raw.highlight.enabled,
        },
    })
}

fn parse_document<T>(path: &Path, text: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension == "json" {
        serde_json::from_str(text).with_context(|| format!("JSON 配置格式无效: {}", path.display()))
    } else {
        serde_yaml::from_str(text).with_context(|| format!("YAML 配置格式无效: {}", path.display()))
    }
}

fn resolve_path(base: &Path, value: &Path) -> PathBuf {
    if value.is_absolute() {
        value.to_path_buf()
    } else {
        base.join(value)
    }
}

fn color(field: &str, value: Option<String>, default: &str) -> Result<Color> {
    let value = value.as_deref().unwrap_or(default).trim();
    let Some(hex) = value.strip_prefix('#') else {
        return named_color(value).ok_or_else(|| {
            anyhow::anyhow!("主题颜色无效: {field}={value}，请使用 #RRGGBB 或标准颜色名")
        });
    };
    if hex.len() != 6 {
        bail!("主题颜色无效: {field}={value}，# 格式必须是六位十六进制")
    }
    let red = u8::from_str_radix(&hex[0..2], 16)
        .with_context(|| format!("主题颜色无效: {field}={value}"))?;
    let green = u8::from_str_radix(&hex[2..4], 16)
        .with_context(|| format!("主题颜色无效: {field}={value}"))?;
    let blue = u8::from_str_radix(&hex[4..6], 16)
        .with_context(|| format!("主题颜色无效: {field}={value}"))?;
    Ok(Color::Rgb(red, green, blue))
}

fn named_color(value: &str) -> Option<Color> {
    match value.to_ascii_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "dark-grey" => Some(Color::DarkGray),
        "light-red" => Some(Color::LightRed),
        "light-green" => Some(Color::LightGreen),
        "light-yellow" => Some(Color::LightYellow),
        "light-blue" => Some(Color::LightBlue),
        "light-magenta" => Some(Color::LightMagenta),
        "light-cyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        _ => None,
    }
}

fn built_in_theme(name: &str) -> Option<RawTheme> {
    let mut theme = match name.trim().to_ascii_lowercase().as_str() {
        "gruvbox-dark" | "gruvbox dark" | "gruvbox" => RawTheme {
            name: Some("gruvbox-dark".to_string()),
            primary: Some("#83a598".to_string()),
            secondary: Some("#fabd2f".to_string()),
            accent: Some("#8ec07c".to_string()),
            background: Some("#282828".to_string()),
            surface: Some("#3c3836".to_string()),
            text: Some("#ebdbb2".to_string()),
            muted: Some("#a89984".to_string()),
            error: Some("#fb4934".to_string()),
            success: Some("#b8bb26".to_string()),
            warning: Some("#fe8019".to_string()),
            selection: Some("#504945".to_string()),
            variable: Some("#d3869b".to_string()),
            syntax: Some(DEFAULT_SYNTAX_THEME.to_string()),
        },
        "ocean" | "default" => RawTheme {
            name: Some("ocean".to_string()),
            primary: Some("#7dd3fc".to_string()),
            secondary: Some("#fbbf24".to_string()),
            accent: Some("#7dd3fc".to_string()),
            background: Some("#0b1120".to_string()),
            surface: Some("#111827".to_string()),
            text: Some("#f8fafc".to_string()),
            muted: Some("#94a3b8".to_string()),
            error: Some("#f87171".to_string()),
            success: Some("#4ade80".to_string()),
            warning: Some("#fbbf24".to_string()),
            selection: Some("#1e293b".to_string()),
            variable: Some("#c084fc".to_string()),
            syntax: Some("base16-ocean.dark".to_string()),
        },
        "nord" => RawTheme {
            name: Some("nord".to_string()),
            primary: Some("#88c0d0".to_string()),
            secondary: Some("#ebcb8b".to_string()),
            accent: Some("#81a1c1".to_string()),
            background: Some("#2e3440".to_string()),
            surface: Some("#3b4252".to_string()),
            text: Some("#eceff4".to_string()),
            muted: Some("#d8dee9".to_string()),
            error: Some("#bf616a".to_string()),
            success: Some("#a3be8c".to_string()),
            warning: Some("#ebcb8b".to_string()),
            selection: Some("#434c5e".to_string()),
            variable: Some("#b48ead".to_string()),
            syntax: Some("base16-ocean.dark".to_string()),
        },
        "mono" => RawTheme {
            name: Some("mono".to_string()),
            primary: Some("white".to_string()),
            secondary: Some("gray".to_string()),
            accent: Some("white".to_string()),
            background: Some("black".to_string()),
            surface: Some("black".to_string()),
            text: Some("white".to_string()),
            muted: Some("gray".to_string()),
            error: Some("light-red".to_string()),
            success: Some("light-green".to_string()),
            warning: Some("light-yellow".to_string()),
            selection: Some("darkgray".to_string()),
            variable: Some("light-magenta".to_string()),
            syntax: Some("InspiredGitHub".to_string()),
        },
        _ => return None,
    };
    theme.name = Some(name.trim().to_string());
    Some(theme)
}

fn default_request_config() -> PathBuf {
    PathBuf::from(DEFAULT_REQUEST_CONFIG)
}

fn default_theme() -> String {
    DEFAULT_THEME.to_string()
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_theme_has_configurable_colors() {
        let theme = built_in_theme("gruvbox-dark").expect("Gruvbox Dark 主题应存在");
        assert_eq!(theme.syntax.as_deref(), Some(DEFAULT_SYNTAX_THEME));
        assert_eq!(theme.variable.as_deref(), Some("#d3869b"));
    }

    #[test]
    fn language_defaults_to_english_and_accepts_chinese() {
        let default: RawGlobalConfig = serde_yaml::from_str("theme: gruvbox-dark").unwrap();
        assert_eq!(default.language, Language::English);

        let chinese: RawGlobalConfig =
            serde_yaml::from_str("language: zh\ntheme: gruvbox-dark").unwrap();
        assert_eq!(chinese.language, Language::Chinese);
    }

    #[test]
    fn parses_hex_and_named_colors() {
        assert_eq!(
            color("test", Some("#123456".to_string()), "white").unwrap(),
            Color::Rgb(18, 52, 86)
        );
        assert_eq!(
            color("test", Some("cyan".to_string()), "white").unwrap(),
            Color::Cyan
        );
    }

    #[test]
    fn normalizes_default_global_config() {
        let raw: RawGlobalConfig = serde_yaml::from_str(
            "request_config: .postui/requests.yaml\nlanguage: en\ntheme: gruvbox-dark",
        )
        .unwrap();
        let config =
            normalize(Path::new("/tmp/postui/config.yaml"), raw).expect("全局配置应当可以规范化");
        assert!(config.path.is_some());
        assert!(config.request_config.ends_with(".postui/requests.yaml"));
        assert_eq!(config.language, Language::English);
        assert_eq!(config.theme.name, "gruvbox-dark");
        assert!(config.theme.highlight_enabled);
    }
}
