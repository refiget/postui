use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use ratatui::style::Color;
use serde::Deserialize;

const DEFAULT_THEME: &str = "gruvbox-dark";
pub(crate) const BUILT_IN_THEME_NAMES: [&str; 10] = [
    "gruvbox-dark",
    "dracula",
    "catppuccin-mocha",
    "tokyo-night",
    "nord",
    "one-dark",
    "solarized-dark",
    "kanagawa",
    "rose-pine",
    "monokai",
];
pub(crate) const DEFAULT_SYNTAX_THEME: &str = "base16-mocha.dark";
pub(crate) const DEFAULT_MAX_RESPONSE_DISPLAY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub(crate) enum Language {
    #[serde(rename = "en")]
    #[default]
    English,
    #[serde(rename = "zh")]
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
    pub(crate) language: Language,
    pub(crate) theme: UiTheme,
    pub(crate) max_response_display_bytes: usize,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            path: None,
            language: Language::default(),
            theme: UiTheme::default(),
            max_response_display_bytes: DEFAULT_MAX_RESPONSE_DISPLAY_BYTES,
        }
    }
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
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGlobalConfig {
    #[serde(default)]
    language: Language,
    #[serde(default = "default_theme")]
    theme: String,
    #[serde(default = "default_max_response_display_bytes")]
    max_response_display_bytes: usize,
}

impl Default for RawGlobalConfig {
    fn default() -> Self {
        Self {
            language: Language::default(),
            theme: default_theme(),
            max_response_display_bytes: default_max_response_display_bytes(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ThemeDefinition {
    name: &'static str,
    primary: &'static str,
    secondary: &'static str,
    accent: &'static str,
    background: &'static str,
    surface: &'static str,
    text: &'static str,
    muted: &'static str,
    error: &'static str,
    success: &'static str,
    warning: &'static str,
    selection: &'static str,
    variable: &'static str,
    syntax_theme: &'static str,
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
        }
    }
}

pub(crate) fn load(path: &Path) -> Result<GlobalConfig> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("无法读取用户界面配置: {}", path.display()))?;
    let raw = serde_saphyr::from_str::<Option<RawGlobalConfig>>(&text)
        .with_context(|| format!("个人配置 YAML 格式无效: {}", path.display()))?
        .unwrap_or_default();
    let global =
        normalize(path, raw).with_context(|| format!("用户界面配置无效: {}", path.display()))?;
    tracing::debug!(
        path = %path.display(),
        language = global.language.as_str(),
        theme = %global.theme.name,
        syntax_theme = %global.theme.syntax_theme,
        max_response_display_bytes = global.max_response_display_bytes,
        "用户界面配置加载完成"
    );
    Ok(global)
}

pub(crate) fn default_config() -> GlobalConfig {
    tracing::debug!(
        language = Language::default().as_str(),
        theme = DEFAULT_THEME,
        max_response_display_bytes = DEFAULT_MAX_RESPONSE_DISPLAY_BYTES,
        "未找到用户界面配置，使用内置默认值"
    );
    GlobalConfig::default()
}

fn normalize(path: &Path, raw: RawGlobalConfig) -> Result<GlobalConfig> {
    if raw.max_response_display_bytes == 0 {
        bail!("max_response_display_bytes 必须大于 0")
    }

    Ok(GlobalConfig {
        path: Some(path.to_path_buf()),
        language: raw.language,
        max_response_display_bytes: raw.max_response_display_bytes,
        theme: theme(&raw.theme)?,
    })
}

pub(crate) fn next_theme(current: &str) -> Result<UiTheme> {
    let current_index = BUILT_IN_THEME_NAMES
        .iter()
        .position(|name| name.eq_ignore_ascii_case(current))
        .unwrap_or_default();
    let next_name = BUILT_IN_THEME_NAMES[(current_index + 1) % BUILT_IN_THEME_NAMES.len()];
    theme(next_name)
}

fn default_max_response_display_bytes() -> usize {
    DEFAULT_MAX_RESPONSE_DISPLAY_BYTES
}

fn color(field: &str, value: &str) -> Result<Color> {
    let value = value.trim();
    let Some(hex) = value.strip_prefix('#') else {
        return named_color(value).ok_or_else(|| {
            anyhow::anyhow!("主题颜色无效: {field}={value}，请使用 #RRGGBB 或标准颜色名")
        });
    };
    if hex.len() != 6 || !hex.is_ascii() {
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

fn theme(name: &str) -> Result<UiTheme> {
    let normalized = name.trim().to_ascii_lowercase();
    let definition = theme_definition(&normalized).ok_or_else(|| {
        anyhow::anyhow!(
            "未知内置主题: {}；可选 {}",
            name,
            BUILT_IN_THEME_NAMES.join("、")
        )
    })?;
    Ok(UiTheme {
        name: definition.name.to_string(),
        primary: color("primary", definition.primary)?,
        secondary: color("secondary", definition.secondary)?,
        accent: color("accent", definition.accent)?,
        background: color("background", definition.background)?,
        surface: color("surface", definition.surface)?,
        text: color("text", definition.text)?,
        muted: color("muted", definition.muted)?,
        error: color("error", definition.error)?,
        success: color("success", definition.success)?,
        warning: color("warning", definition.warning)?,
        selection: color("selection", definition.selection)?,
        variable: color("variable", definition.variable)?,
        syntax_theme: definition.syntax_theme.to_string(),
    })
}

fn theme_definition(name: &str) -> Option<ThemeDefinition> {
    let values = match name {
        "gruvbox-dark" => [
            "#83a598",
            "#fabd2f",
            "#8ec07c",
            "#282828",
            "#3c3836",
            "#ebdbb2",
            "#a89984",
            "#fb4934",
            "#b8bb26",
            "#fe8019",
            "#504945",
            "#d3869b",
            "base16-mocha.dark",
        ],
        "dracula" => [
            "#8be9fd",
            "#bd93f9",
            "#ff79c6",
            "#282a36",
            "#343746",
            "#f8f8f2",
            "#a9a9b3",
            "#ff5555",
            "#50fa7b",
            "#f1fa8c",
            "#44475a",
            "#ff79c6",
            "base16-ocean.dark",
        ],
        "catppuccin-mocha" => [
            "#89b4fa",
            "#cba6f7",
            "#f5c2e7",
            "#1e1e2e",
            "#313244",
            "#cdd6f4",
            "#a6adc8",
            "#f38ba8",
            "#a6e3a1",
            "#f9e2af",
            "#45475a",
            "#cba6f7",
            "base16-ocean.dark",
        ],
        "tokyo-night" => [
            "#7aa2f7",
            "#bb9af7",
            "#2ac3de",
            "#1a1b26",
            "#24283b",
            "#c0caf5",
            "#9aa5ce",
            "#f7768e",
            "#9ece6a",
            "#e0af68",
            "#33467c",
            "#ff9e64",
            "base16-ocean.dark",
        ],
        "nord" => [
            "#88c0d0",
            "#81a1c1",
            "#5e81ac",
            "#2e3440",
            "#3b4252",
            "#eceff4",
            "#aeb8c6",
            "#bf616a",
            "#a3be8c",
            "#ebcb8b",
            "#434c5e",
            "#b48ead",
            "base16-ocean.dark",
        ],
        "one-dark" => [
            "#61afef",
            "#c678dd",
            "#56b6c2",
            "#282c34",
            "#353b45",
            "#abb2bf",
            "#7f8795",
            "#e06c75",
            "#98c379",
            "#e5c07b",
            "#3e4451",
            "#d19a66",
            "base16-ocean.dark",
        ],
        "solarized-dark" => [
            "#268bd2",
            "#6c71c4",
            "#2aa198",
            "#002b36",
            "#073642",
            "#eee8d5",
            "#93a1a1",
            "#dc322f",
            "#859900",
            "#b58900",
            "#0b4a5a",
            "#d33682",
            "Solarized (dark)",
        ],
        "kanagawa" => [
            "#7e9cd8",
            "#957fb8",
            "#7fb4ca",
            "#1f1f28",
            "#2a2a37",
            "#dcd7ba",
            "#938aa9",
            "#e82424",
            "#98bb6c",
            "#e6c384",
            "#363646",
            "#ffa066",
            "base16-ocean.dark",
        ],
        "rose-pine" => [
            "#9ccfd8",
            "#c4a7e7",
            "#ebbcba",
            "#191724",
            "#26233a",
            "#e0def4",
            "#908caa",
            "#eb6f92",
            "#9ccfd8",
            "#f6c177",
            "#403d52",
            "#c4a7e7",
            "base16-ocean.dark",
        ],
        "monokai" => [
            "#66d9ef",
            "#ae81ff",
            "#f92672",
            "#272822",
            "#3e3d32",
            "#f8f8f2",
            "#a6a69c",
            "#f92672",
            "#a6e22e",
            "#e6db74",
            "#49483e",
            "#fd971f",
            "base16-eighties.dark",
        ],
        _ => return None,
    };
    Some(ThemeDefinition {
        name: BUILT_IN_THEME_NAMES
            .iter()
            .copied()
            .find(|candidate| *candidate == name)?,
        primary: values[0],
        secondary: values[1],
        accent: values[2],
        background: values[3],
        surface: values[4],
        text: values[5],
        muted: values[6],
        error: values[7],
        success: values[8],
        warning: values[9],
        selection: values[10],
        variable: values[11],
        syntax_theme: values[12],
    })
}

fn default_theme() -> String {
    DEFAULT_THEME.to_string()
}
