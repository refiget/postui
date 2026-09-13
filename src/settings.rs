use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use ratatui::style::Color;
use serde::Deserialize;

use crate::diagnostics;

const DEFAULT_THEME: &str = "postui";
pub const BUILT_IN_THEME_NAMES: [&str; 11] = [
    "postui",
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
pub const DEFAULT_SYNTAX_THEME: &str = "base16-ocean.dark";
pub const DEFAULT_MAX_RESPONSE_DISPLAY_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Language {
    #[serde(rename = "en")]
    #[default]
    English,
    #[serde(rename = "zh")]
    Chinese,
}

impl Language {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "zh",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub path: Option<PathBuf>,
    pub language: Language,
    pub theme: UiTheme,
    pub max_response_display_bytes: usize,
    pub max_response_bytes: usize,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            path: None,
            language: Language::default(),
            theme: UiTheme::default(),
            max_response_display_bytes: DEFAULT_MAX_RESPONSE_DISPLAY_BYTES,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UiTheme {
    pub name: String,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub background: Color,
    pub surface: Color,
    pub text: Color,
    pub muted: Color,
    pub error: Color,
    pub success: Color,
    pub warning: Color,
    pub selection: Color,
    pub variable: Color,
    pub syntax_theme: String,
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
    #[serde(default = "default_max_response_bytes")]
    max_response_bytes: usize,
}

fn default_max_response_bytes() -> usize {
    DEFAULT_MAX_RESPONSE_BYTES
}

impl Default for RawGlobalConfig {
    fn default() -> Self {
        Self {
            language: Language::default(),
            theme: default_theme(),
            max_response_display_bytes: default_max_response_display_bytes(),
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
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
        theme(DEFAULT_THEME).expect("built-in default theme must be valid")
    }
}

pub fn load(path: &Path) -> Result<GlobalConfig> {
    let text = fs::read_to_string(path).map_err(|error| diagnostics::read(path, &error))?;
    let raw: Option<RawGlobalConfig> = diagnostics::parse_yaml(path, "user configuration", &text)?;
    let global = diagnostics::standardize(
        normalize(path, raw.unwrap_or_default()),
        path,
        "user configuration",
    )?;
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

pub fn default_config() -> GlobalConfig {
    tracing::debug!(
        language = Language::default().as_str(),
        theme = DEFAULT_THEME,
        max_response_display_bytes = DEFAULT_MAX_RESPONSE_DISPLAY_BYTES,
        "未找到用户界面配置，使用内置默认值"
    );
    GlobalConfig::default()
}

pub fn is_not_found(error: &anyhow::Error) -> bool {
    diagnostics::is_not_found(error)
}

fn normalize(path: &Path, raw: RawGlobalConfig) -> Result<GlobalConfig> {
    if raw.max_response_bytes == 0 {
        return Err(diagnostics::invalid(
            path,
            "max_response_bytes",
            "must be greater than 0",
        ));
    }
    if raw.max_response_display_bytes == 0 {
        return Err(diagnostics::invalid(
            path,
            "max_response_display_bytes",
            "must be greater than 0",
        ));
    }

    let theme = theme(&raw.theme)
        .map_err(|error| diagnostics::invalid(path, "theme", format!("{error:#}")))?;

    Ok(GlobalConfig {
        path: Some(path.to_path_buf()),
        language: raw.language,
        max_response_display_bytes: raw.max_response_display_bytes,
        max_response_bytes: raw.max_response_bytes,
        theme,
    })
}

pub fn next_theme(current: &str) -> Result<UiTheme> {
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
    value
        .trim()
        .parse::<Color>()
        .with_context(|| format!("Invalid theme color: {field}={value}"))
}

fn theme(name: &str) -> Result<UiTheme> {
    let normalized = name.trim().to_ascii_lowercase();
    let definition = theme_definition(&normalized).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown built-in theme: {}; choose one of {}",
            name,
            BUILT_IN_THEME_NAMES.join(", ")
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
        "postui" => [
            "#69d6d0",
            "#c2a9ff",
            "#7da9ff",
            "#091019",
            "#131d29",
            "#e8f0f7",
            "#8291a5",
            "#ff6f88",
            "#70d6a0",
            "#f2bd68",
            "#223650",
            "#f3a978",
            "base16-ocean.dark",
        ],
        "gruvbox-dark" => [
            "#83a598",
            "#fabd2f",
            "#8ec07c",
            "#1d2021",
            "#282828",
            "#ebdbb2",
            "#928374",
            "#fb4934",
            "#b8bb26",
            "#fe8019",
            "#3c3836",
            "#d3869b",
            "base16-mocha.dark",
        ],
        "dracula" => [
            "#8be9fd",
            "#bd93f9",
            "#ff79c6",
            "#242631",
            "#2d303e",
            "#f8f8f2",
            "#9698a8",
            "#ff5555",
            "#50fa7b",
            "#f1fa8c",
            "#41445a",
            "#ffb86c",
            "base16-ocean.dark",
        ],
        "catppuccin-mocha" => [
            "#89b4fa",
            "#cba6f7",
            "#f5c2e7",
            "#181825",
            "#242435",
            "#cdd6f4",
            "#9ba3bd",
            "#f38ba8",
            "#a6e3a1",
            "#f9e2af",
            "#3b3d55",
            "#fab387",
            "base16-ocean.dark",
        ],
        "tokyo-night" => [
            "#7aa2f7",
            "#bb9af7",
            "#2ac3de",
            "#13141c",
            "#1c2030",
            "#c0caf5",
            "#828daf",
            "#f7768e",
            "#9ece6a",
            "#e0af68",
            "#293a68",
            "#ff9e64",
            "base16-ocean.dark",
        ],
        "nord" => [
            "#88c0d0",
            "#ebcb8b",
            "#5e81ac",
            "#242933",
            "#303744",
            "#eceff4",
            "#9aa6b6",
            "#bf616a",
            "#a3be8c",
            "#d08770",
            "#3d4859",
            "#b48ead",
            "base16-ocean.dark",
        ],
        "one-dark" => [
            "#61afef",
            "#c678dd",
            "#56b6c2",
            "#1d2026",
            "#282d36",
            "#abb2bf",
            "#7d8594",
            "#e06c75",
            "#98c379",
            "#e5c07b",
            "#383f4d",
            "#d19a66",
            "base16-ocean.dark",
        ],
        "solarized-dark" => [
            "#268bd2",
            "#6c71c4",
            "#2aa198",
            "#002b36",
            "#063844",
            "#eee8d5",
            "#93a1a1",
            "#dc322f",
            "#859900",
            "#b58900",
            "#164b59",
            "#d33682",
            "Solarized (dark)",
        ],
        "kanagawa" => [
            "#7e9cd8",
            "#e6c384",
            "#7fb4ca",
            "#181820",
            "#252530",
            "#dcd7ba",
            "#938aa9",
            "#e82424",
            "#98bb6c",
            "#ffa066",
            "#343444",
            "#957fb8",
            "base16-ocean.dark",
        ],
        "rose-pine" => [
            "#9ccfd8",
            "#f6c177",
            "#ebbcba",
            "#15131f",
            "#232034",
            "#e0def4",
            "#908caa",
            "#eb6f92",
            "#9ccfd8",
            "#ea9a97",
            "#39364a",
            "#c4a7e7",
            "base16-ocean.dark",
        ],
        "monokai" => [
            "#66d9ef",
            "#ae81ff",
            "#f92672",
            "#20211c",
            "#303128",
            "#f8f8f2",
            "#a2a398",
            "#f92672",
            "#a6e22e",
            "#e6db74",
            "#414238",
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
