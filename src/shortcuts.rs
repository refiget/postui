use crate::settings::Language;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Context {
    Global,
    Requests,
    Preview,
    Response,
    Headers,
    Params,
    Menu,
    Variables,
    CurlImport,
    Confirm,
    Editor,
    Help,
}

impl Context {
    fn inherits_global(self) -> bool {
        matches!(
            self,
            Self::Requests | Self::Preview | Self::Response | Self::Headers | Self::Params
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Back,
    Quit,
    FocusNext,
    FocusPrevious,
    Send,
    Reload,
    Workspace,
    Variables,
    ImportCurl,
    ResponseMenu,
    Search,
    NextMatch,
    PreviousMatch,
    Help,
    Theme,
    ResetRequest,
    ResetScenario,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Activate,
    Toggle,
    Add,
    Confirm,
    SelectAll,
    End,
    Home,
    Clear,
    WordLeft,
    WordRight,
    DeleteWordRight,
    DeleteWordLeft,
    DeleteToEnd,
    Backspace,
}

impl Command {
    fn repeatable(self) -> bool {
        matches!(
            self,
            Self::Up
                | Self::Down
                | Self::Left
                | Self::Right
                | Self::WordLeft
                | Self::WordRight
                | Self::DeleteWordLeft
                | Self::DeleteWordRight
                | Self::Backspace
        )
    }
}

struct Binding {
    command: Command,
    keys: &'static [(KeyCode, KeyModifiers)],
    label: &'static str,
    english: &'static str,
    chinese: &'static str,
}

const fn plain(code: KeyCode) -> (KeyCode, KeyModifiers) {
    (code, KeyModifiers::NONE)
}

const fn ctrl(code: KeyCode) -> (KeyCode, KeyModifiers) {
    (code, KeyModifiers::CONTROL)
}

macro_rules! binding {
    ($command:ident, $label:literal, $en:literal, $zh:literal, $($key:expr),+ $(,)?) => {
        Binding { command: Command::$command, keys: &[$($key),+], label: $label, english: $en, chinese: $zh }
    };
}

use KeyCode::*;

const GLOBAL: &[Binding] = &[
    binding!(FocusNext, "Tab", "Focus", "焦点", plain(Tab)),
    binding!(
        FocusPrevious,
        "Shift+Tab",
        "Previous focus",
        "上一焦点",
        plain(BackTab)
    ),
    binding!(Send, "s", "Send/Stop", "发送/停止", plain(Char('s'))),
    binding!(Reload, "r", "Reload", "重载", plain(Char('r'))),
    binding!(Workspace, "c", "Scenario", "场景", plain(Char('c'))),
    binding!(Variables, "v", "Variables", "变量", plain(Char('v'))),
    binding!(ImportCurl, "n", "New", "新建", plain(Char('n'))),
    binding!(
        ResponseMenu,
        "m",
        "Response actions",
        "响应操作",
        plain(Char('m'))
    ),
    binding!(Search, "/", "Filter", "筛选", plain(Char('/'))),
    binding!(Help, "?", "Keys", "按键", plain(Char('?'))),
    binding!(
        Back,
        "Esc/q",
        "Back / clear filter / exit",
        "返回 / 清除筛选 / 退出",
        plain(Esc),
        plain(Char('q'))
    ),
    binding!(
        Activate,
        "Enter/Space",
        "Activate",
        "执行",
        plain(Enter),
        plain(Char(' '))
    ),
];
const MOVEMENT: &[Binding] = &[
    binding!(Up, "k/↑", "Up", "上移", plain(Char('k')), plain(Up)),
    binding!(Down, "j/↓", "Down", "下移", plain(Char('j')), plain(Down)),
];
const REQUESTS: &[Binding] = &[
    binding!(
        ResetRequest,
        "u",
        "Reset request",
        "重置接口",
        plain(Char('u'))
    ),
    binding!(
        ResetScenario,
        "U",
        "Reset scenario",
        "重置场景",
        plain(Char('U'))
    ),
    binding!(
        Delete,
        "d/Delete",
        "Delete source file (confirm)",
        "删除源文件（确认）",
        plain(Char('d')),
        plain(Delete)
    ),
];
const RESPONSE: &[Binding] = &[
    binding!(Search, "/", "Search response", "搜索响应", plain(Char('/'))),
    binding!(NextMatch, "n", "Next match", "下一匹配", plain(Char('n'))),
    binding!(
        PreviousMatch,
        "N",
        "Previous match",
        "上一匹配",
        plain(Char('N'))
    ),
];
const TABLE: &[Binding] = &[
    binding!(
        Left,
        "h/←",
        "Name column",
        "名称列",
        plain(Char('h')),
        plain(Left)
    ),
    binding!(
        Right,
        "l/→",
        "Value column",
        "值列",
        plain(Char('l')),
        plain(Right)
    ),
    binding!(
        Activate,
        "Enter/Space",
        "Edit",
        "编辑",
        plain(Enter),
        plain(Char(' '))
    ),
    binding!(Add, "a", "Add row", "添加行", plain(Char('a'))),
    binding!(
        Delete,
        "d/Delete",
        "Remove row (session)",
        "删除行（会话）",
        plain(Delete),
        plain(Char('d'))
    ),
    binding!(
        Back,
        "Esc/q",
        "Close table",
        "关闭表格",
        plain(Esc),
        plain(Char('q'))
    ),
];
const HEADERS: &[Binding] = &[
    binding!(Activate, "Enter", "Edit", "编辑", plain(Enter)),
    binding!(
        Toggle,
        "Space",
        "Toggle header",
        "启停请求头",
        plain(Char(' '))
    ),
];
const MENU: &[Binding] = &[
    binding!(
        Activate,
        "Enter/Space",
        "Apply",
        "应用",
        plain(Enter),
        plain(Char(' '))
    ),
    binding!(Back, "Esc/q", "Close", "关闭", plain(Esc), plain(Char('q'))),
];
const VARIABLES: &[Binding] = &[
    binding!(
        Activate,
        "Enter/Space",
        "Edit",
        "编辑",
        plain(Enter),
        plain(Char(' '))
    ),
    binding!(Back, "Esc/q", "Back", "返回", plain(Esc), plain(Char('q'))),
];
const CURL_IMPORT: &[Binding] = &[
    binding!(FocusNext, "Tab", "Next field", "下一字段", plain(Tab)),
    binding!(
        FocusPrevious,
        "Shift+Tab",
        "Previous field",
        "上一字段",
        plain(BackTab)
    ),
    binding!(Back, "Esc/q", "Back", "返回", plain(Esc), plain(Char('q'))),
    binding!(Clear, "Ctrl+U", "Clear", "清空", ctrl(Char('u'))),
];
const CONFIRM: &[Binding] = &[
    binding!(
        Confirm,
        "Y",
        "Confirm",
        "确认",
        plain(Char('y')),
        plain(Char('Y'))
    ),
    binding!(
        Back,
        "N/Esc",
        "Cancel",
        "取消",
        plain(Char('n')),
        plain(Char('N')),
        plain(Esc),
        ctrl(Char('c'))
    ),
];
const EDITOR: &[Binding] = &[
    binding!(
        Confirm,
        "Enter/Tab",
        "Apply",
        "应用",
        plain(Enter),
        plain(Tab)
    ),
    binding!(
        Back,
        "Esc/Ctrl+C",
        "Cancel",
        "取消",
        plain(Esc),
        ctrl(Char('c'))
    ),
    binding!(SelectAll, "Ctrl+A", "Select all", "全选", ctrl(Char('a'))),
    binding!(
        End,
        "End/Ctrl+E",
        "End",
        "行尾",
        plain(End),
        ctrl(Char('e'))
    ),
    binding!(Home, "Home", "Start", "行首", plain(Home)),
    binding!(Clear, "Ctrl+U", "Clear line", "清空行", ctrl(Char('u'))),
    binding!(WordLeft, "Ctrl+←", "Previous word", "上一词", ctrl(Left)),
    binding!(WordRight, "Ctrl+→", "Next word", "下一词", ctrl(Right)),
    binding!(
        DeleteWordRight,
        "Ctrl+Delete",
        "Delete next word",
        "删除后一词",
        ctrl(Delete)
    ),
    binding!(
        DeleteWordLeft,
        "Alt+Backspace",
        "Delete previous word",
        "删除前一词",
        (Backspace, KeyModifiers::ALT),
        (Backspace, KeyModifiers::META)
    ),
    binding!(
        DeleteToEnd,
        "Ctrl+K",
        "Delete to end",
        "删除至行尾",
        ctrl(Char('k'))
    ),
    binding!(
        Backspace,
        "Backspace",
        "Backspace",
        "退格",
        plain(Backspace)
    ),
    binding!(
        Delete,
        "Delete",
        "Delete character",
        "删除字符",
        plain(Delete)
    ),
    binding!(Left, "←", "Cursor left", "光标左移", plain(Left)),
    binding!(Right, "→", "Cursor right", "光标右移", plain(Right)),
];
const HELP: &[Binding] = &[
    binding!(
        Up,
        "↑/k",
        "Scroll up",
        "向上滚动",
        plain(Up),
        plain(Char('k'))
    ),
    binding!(
        Down,
        "↓/j",
        "Scroll down",
        "向下滚动",
        plain(Down),
        plain(Char('j'))
    ),
    binding!(
        Back,
        "Esc/q/?",
        "Close",
        "关闭",
        plain(Esc),
        plain(Char('q')),
        plain(Char('?')),
        ctrl(Char('c'))
    ),
];
const COMMON: &[Binding] = &[
    binding!(Help, "F1", "Keys", "按键", plain(F(1))),
    binding!(Quit, "Ctrl+C", "Exit", "退出", ctrl(Char('c'))),
];
const TABS: &[Binding] = &[
    binding!(
        Left,
        "h/←",
        "Previous tab",
        "上一页签",
        plain(Char('h')),
        plain(Left),
        (Left, KeyModifiers::ALT)
    ),
    binding!(
        Right,
        "l/→",
        "Next tab",
        "下一页签",
        plain(Char('l')),
        plain(Right),
        (Right, KeyModifiers::ALT)
    ),
];
const DEBUG: &[Binding] = &[binding!(Theme, "F5", "Theme", "主题", plain(F(5)))];

fn bindings(context: Context, debug: bool) -> impl Iterator<Item = &'static Binding> {
    let groups: &[&[Binding]] = match context {
        Context::Global => &[GLOBAL],
        Context::Requests => &[REQUESTS, MOVEMENT],
        Context::Preview => &[MOVEMENT],
        Context::Response => &[RESPONSE, MOVEMENT],
        Context::Headers => &[HEADERS, TABLE, MOVEMENT],
        Context::Params => &[TABLE, MOVEMENT],
        Context::Menu => &[MENU, MOVEMENT],
        Context::Variables => &[VARIABLES, MOVEMENT],
        Context::CurlImport => &[CURL_IMPORT],
        Context::Confirm => &[CONFIRM],
        Context::Editor => &[EDITOR],
        Context::Help => &[HELP],
    };
    groups
        .iter()
        .flat_map(|group| group.iter())
        .chain(
            matches!(
                context,
                Context::Preview | Context::Response | Context::Headers | Context::Params
            )
            .then_some(TABS)
            .into_iter()
            .flatten(),
        )
        .chain(
            context
                .inherits_global()
                .then_some(GLOBAL)
                .into_iter()
                .flatten(),
        )
        .chain(COMMON)
        .chain(debug.then_some(DEBUG).into_iter().flatten())
}

pub(crate) fn normalize(mut key: KeyEvent) -> KeyEvent {
    // Terminals encode Shift+Tab and shifted characters in two different forms.
    if key.code == Tab && key.modifiers.contains(KeyModifiers::SHIFT) {
        key.code = BackTab;
    }
    if let Char(character) = key.code {
        if key.modifiers.contains(KeyModifiers::SHIFT) && character.is_ascii_lowercase() {
            key.code = Char(character.to_ascii_uppercase());
        }
        key.modifiers.remove(KeyModifiers::SHIFT);
    } else if key.code == BackTab {
        key.modifiers.remove(KeyModifiers::SHIFT);
    }
    key
}

pub(crate) fn resolve(context: Context, key: KeyEvent, debug: bool) -> Option<Command> {
    let key = normalize(key);
    if key.kind == KeyEventKind::Release {
        return None;
    }
    bindings(context, debug)
        .find(|binding| binding.keys.contains(&(key.code, key.modifiers)))
        .map(|binding| binding.command)
        .filter(|command| {
            key.kind != KeyEventKind::Repeat
                || command.repeatable()
                || (context == Context::Editor && *command == Command::Delete)
        })
}

pub(crate) fn hint(
    context: Context,
    language: Language,
    commands: &[Command],
    debug: bool,
) -> String {
    commands
        .iter()
        .filter_map(|command| {
            let binding = bindings(context, debug).find(|binding| binding.command == *command)?;
            Some(format!(
                "{} {}",
                binding.label,
                description(binding, language)
            ))
        })
        .collect::<Vec<_>>()
        .join("  ")
}

pub(crate) fn help(context: Context, language: Language, debug: bool) -> String {
    let mut seen = Vec::new();
    let mut keys = Vec::new();
    bindings(context, debug)
        .filter(|binding| {
            let reachable = binding.keys.iter().any(|key| !keys.contains(key));
            keys.extend_from_slice(binding.keys);
            if !reachable || seen.contains(&binding.command) {
                return false;
            }
            seen.push(binding.command);
            true
        })
        .map(|binding| format!("{:<18} {}", binding.label, description(binding, language)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn description(binding: &Binding, language: Language) -> &'static str {
    match language {
        Language::English => binding.english,
        Language::Chinese => binding.chinese,
    }
}
