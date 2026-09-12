use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    text::Line,
};

const SIDEBAR_WIDE: u16 = 30;
const SIDEBAR_MEDIUM: u16 = 26;
const SIDEBAR_NARROW: u16 = 22;
pub(super) const PREVIEW_ACTION_WIDTH: u16 = 14;
pub(super) const SEND_BUTTON_HEIGHT: u16 = 1;

use super::{ScrollAreas, inner_scroll_areas, panel_scroll_areas};

#[derive(Debug, Clone, Copy)]
pub(super) struct UiLayout {
    pub(super) header: Rect,
    pub(super) header_content: Rect,
    pub(super) requests: Rect,
    pub(super) workspace_selector: Rect,
    pub(super) variables_button: Rect,
    pub(super) request_list: Rect,
    pub(super) request_scrollbar: Rect,
    pub(super) preview: Rect,
    pub(super) preview_details: Rect,
    pub(super) preview_summary: Rect,
    pub(super) preview_tabs: Rect,
    pub(super) preview_content: Rect,
    pub(super) send_button: Rect,
    pub(super) response: Rect,
    pub(super) response_menu_button: Rect,
    pub(super) footer: Rect,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PreviewSections {
    pub(super) summary: Rect,
    pub(super) tabs: Rect,
    pub(super) content: Rect,
}

pub(super) fn screen(area: Rect) -> UiLayout {
    screen_with_summary(area, 2)
}

pub(super) fn screen_with_summary(area: Rect, summary_height: u16) -> UiLayout {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height(area.height)),
            Constraint::Min(0),
            Constraint::Length(u16::from(area.height >= 8)),
        ])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sidebar_width(sections[1].width)),
            Constraint::Min(0),
        ])
        .split(sections[1]);
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(columns[1]);
    let sidebar = sidebar_parts(columns[0]);
    let (header_content, send_button) = header_parts(sections[0]);
    let preview_details = main[0].inner(Margin::new(1, 1));
    let preview = preview_sections(preview_details, summary_height);
    let response_menu_button = response_menu_button(main[1]);

    UiLayout {
        header: sections[0],
        header_content,
        requests: columns[0],
        workspace_selector: sidebar.workspace_selector,
        variables_button: sidebar.variables_button,
        request_list: sidebar.request_list.content,
        request_scrollbar: sidebar.request_list.scrollbar,
        preview: main[0],
        preview_details,
        preview_summary: preview.summary,
        preview_tabs: preview.tabs,
        preview_content: preview.content,
        send_button,
        response: main[1],
        response_menu_button,
        footer: sections[2],
    }
}

pub(super) fn preview_summary_height(width: u16, method: &str, address: &str, url: &str) -> u16 {
    if width == 0 {
        return 0;
    }
    let prefix_width = Line::from(format!("{method}  {address}  ")).width();
    let url_width = Line::from(url).width();
    let total_width = prefix_width.saturating_add(url_width);
    let url_lines = total_width
        .saturating_add(usize::from(width).saturating_sub(1))
        .checked_div(usize::from(width))
        .unwrap_or(1)
        .max(1);
    u16::try_from(url_lines.saturating_add(1)).unwrap_or(u16::MAX)
}

pub(super) fn preview_sections(area: Rect, requested_summary_height: u16) -> PreviewSections {
    if area.is_empty() {
        return PreviewSections {
            summary: Rect::default(),
            tabs: Rect::default(),
            content: Rect::default(),
        };
    }
    let summary_height = requested_summary_height.min(area.height.saturating_sub(1));
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(summary_height),
            Constraint::Length(u16::from(area.height > summary_height)),
            Constraint::Min(0),
        ])
        .split(area);
    PreviewSections {
        summary: sections[0],
        tabs: sections[1],
        content: sections[2],
    }
}

fn header_height(height: u16) -> u16 {
    match height {
        18.. => 4,
        4..=17 => 3,
        _ => 0,
    }
}

fn sidebar_width(width: u16) -> u16 {
    let preferred = if width >= 100 {
        SIDEBAR_WIDE
    } else if width >= 72 {
        SIDEBAR_MEDIUM
    } else if width >= 48 {
        SIDEBAR_NARROW
    } else {
        width.saturating_sub(14).max(10)
    };
    preferred.min(width)
}

#[derive(Debug, Clone, Copy)]
struct SidebarLayout {
    workspace_selector: Rect,
    variables_button: Rect,
    request_list: ScrollAreas,
}

fn sidebar_parts(area: Rect) -> SidebarLayout {
    let inner = area.inner(Margin::new(1, 1));
    if inner.height < 4 {
        return SidebarLayout {
            workspace_selector: Rect::default(),
            variables_button: Rect::default(),
            request_list: panel_scroll_areas(area),
        };
    }
    let parts = if inner.height >= 7 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(inner)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(inner)
    };
    let (variables_button, request_list) = if inner.height >= 7 {
        (parts[2], parts[4])
    } else {
        (parts[1], parts[2])
    };
    SidebarLayout {
        workspace_selector: parts[0],
        variables_button,
        request_list: inner_scroll_areas(request_list),
    }
}

fn header_parts(area: Rect) -> (Rect, Rect) {
    let inner = area.inner(Margin::new(1, 1));
    if inner.is_empty() {
        return (Rect::default(), Rect::default());
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(PREVIEW_ACTION_WIDTH)])
        .split(inner);
    let button = Rect::new(
        columns[1].x,
        columns[1].y,
        columns[1].width,
        columns[1].height.min(SEND_BUTTON_HEIGHT),
    );
    (columns[0], button)
}

fn response_menu_button(area: Rect) -> Rect {
    if area.width <= 2 || area.height <= 1 {
        return Rect::default();
    }
    let inner = area.inner(Margin::new(1, 1));
    if inner.is_empty() {
        return Rect::default();
    }
    let width = 10.min(area.width.saturating_sub(2));
    if width == 0 {
        return Rect::default();
    }
    Rect::new(inner.right().saturating_sub(width), inner.y, width, 1)
}
