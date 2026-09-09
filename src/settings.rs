use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use ratatui::style::Color;
use serde::Deserialize;

const DEFAULT_REQUEST_CONFIG: &str = ".postui/requests.yaml";
const DEFAULT_THEME: &str = "ocean";
const DEFAULT_SYNTAX_THEME: &str = "base16-ocean.dark";

#[derive(Debug, Clone)]
pub(crate) struct GlobalConfig {
    pub(crate) path: Option<PathBuf>,
    pub(crate) request_config: PathBuf,
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
            theme: UiTheme::default(),
        }
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            name: DEFAULT_THEME.to_string(),
            primary: Color::Rgb(125, 211, 252),
            secondary: Color::Rgb(251, 191, 36),
            accent: Color::Rgb(125, 211, 252),
            background: Color::Rgb(11, 17, 32),
            surface: Color::Rgb(17, 24, 39),
            text: Color::Rgb(248, 250, 252),
            muted: Color::Rgb(148, 163, 184),
            error: Color::Rgb(248, 113, 113),
            success: Color::Rgb(74, 222, 128),
            warning: Color::Rgb(251, 191, 36),
            selection: Color::Rgb(30, 41, 59),
            variable: Color::Rgb(192, 132, 252),
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
                "未知内置主题: {}；可选 ocean、nord、mono，或设置 theme_file",
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
        .unwrap_or_else(|| "#c084fc".to_string());

    Ok(GlobalConfig {
        path: Some(path.to_path_buf()),
        request_config,
        theme: UiTheme {
            name: theme_name,
            primary: color("primary", raw_theme.primary, "#7dd3fc")?,
            secondary: color("secondary", raw_theme.secondary, "#fbbf24")?,
            accent: color("accent", raw_theme.accent, "#7dd3fc")?,
            background: color("background", raw_theme.background, "#0b1120")?,
            surface: color("surface", raw_theme.surface, "#111827")?,
            text: color("text", raw_theme.text, "#f8fafc")?,
            muted: color("muted", raw_theme.muted, "#94a3b8")?,
            error: color("error", raw_theme.error, "#f87171")?,
            success: color("success", raw_theme.success, "#4ade80")?,
            warning: color("warning", raw_theme.warning, "#fbbf24")?,
            selection: color("selection", raw_theme.selection, "#1e293b")?,
            variable: color("variable", Some(variable), "#c084fc")?,
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
            syntax: Some(DEFAULT_SYNTAX_THEME.to_string()),
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
        let theme = built_in_theme("ocean").expect("内置主题应存在");
        assert_eq!(theme.syntax.as_deref(), Some(DEFAULT_SYNTAX_THEME));
        assert_eq!(theme.variable.as_deref(), Some("#c084fc"));
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
    fn loads_repository_global_config() {
        let config = load(Path::new("config.yaml")).expect("全局配置应当可以加载");
        assert!(config.path.is_some());
        assert!(config.request_config.ends_with(".postui/requests.yaml"));
        assert_eq!(config.theme.name, "ocean");
        assert!(config.theme.highlight_enabled);
    }
}
