use super::widgets::asset_theme;
use crate::app::{App, ResponseMenuAction};
use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::{Modifier, Style},
};
use tui_assets_rust::{Dropdown as AssetDropdown, DropdownItem as AssetDropdownItem};

/// 响应操作菜单的区域：贴住响应面板内区右上角。
pub(super) fn response_menu_area(panel: Rect) -> Rect {
    if panel.is_empty() {
        return Rect::default();
    }
    let inner = panel.inner(Margin::new(1, 1));
    if inner.is_empty() {
        return Rect::default();
    }
    let width = 20.min(inner.width);
    let height = u16::try_from(ResponseMenuAction::all().len())
        .unwrap_or(u16::MAX)
        .saturating_add(2)
        .min(inner.height);
    if width < 3 || height < 3 {
        return Rect::default();
    }
    Rect::new(inner.right().saturating_sub(width), inner.y, width, height)
}

pub(super) fn draw_response_menu(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    if area.is_empty() || area.width < 3 || area.height < 3 {
        return;
    }
    let theme = &app.global_config.theme;
    let text = app.text();
    let items = ResponseMenuAction::all()
        .into_iter()
        .map(|action| {
            AssetDropdownItem::new(response_action_label(action, text))
                .symbol(response_action_symbol(action))
        })
        .collect::<Vec<_>>();
    let dropdown = AssetDropdown::new("", &items, asset_theme(theme))
        .border_color(theme.accent)
        .highlight_style(
            Style::default()
                .fg(theme.background)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(dropdown, area, &mut app.view.response.menu);
}

fn response_action_symbol(action: ResponseMenuAction) -> &'static str {
    match action {
        ResponseMenuAction::Download => "↓",
        ResponseMenuAction::CopyBody | ResponseMenuAction::CopyHeaders => "⧉",
    }
}

fn response_action_label(action: ResponseMenuAction, text: crate::i18n::UiText) -> &'static str {
    match action {
        ResponseMenuAction::Download => text.response_download(),
        ResponseMenuAction::CopyBody => text.response_copy_body(),
        ResponseMenuAction::CopyHeaders => text.response_copy_headers(),
    }
}
