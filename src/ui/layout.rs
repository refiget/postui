use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};

const SIDEBAR_WIDE: u16 = 30;
const SIDEBAR_MEDIUM: u16 = 26;
const SIDEBAR_NARROW: u16 = 22;
pub(super) const PREVIEW_ACTION_WIDTH: u16 = 14;
pub(super) const SEND_BUTTON_HEIGHT: u16 = 3;
const REQUEST_DETAILS_MIN_WIDTH: u16 = 34;

use super::{ScrollAreas, inner_scroll_areas, panel_scroll_areas};

#[derive(Debug, Clone, Copy)]
pub(super) struct UiLayout {
    pub(super) header: Rect,
    pub(super) requests: Rect,
    pub(super) collection_label: Rect,
    pub(super) variables_button: Rect,
    pub(super) request_list: Rect,
    pub(super) request_scrollbar: Rect,
    pub(super) preview: Rect,
    pub(super) preview_details: Rect,
    pub(super) preview_tabs: Rect,
    pub(super) preview_content: Rect,
    pub(super) edit_button: Rect,
    pub(super) send_button: Rect,
    pub(super) response: Rect,
    pub(super) footer: Rect,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PreviewSections {
    pub(super) summary: Rect,
    pub(super) tabs: Rect,
    pub(super) content: Rect,
}

pub(super) fn screen(area: Rect) -> UiLayout {
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
    let (preview_details, [edit_button, send_button]) = preview_parts(main[0]);
    let preview = preview_sections(preview_details);

    UiLayout {
        header: sections[0],
        requests: columns[0],
        collection_label: sidebar.collection_label,
        variables_button: sidebar.variables_button,
        request_list: sidebar.request_list.content,
        request_scrollbar: sidebar.request_list.scrollbar,
        preview: main[0],
        preview_details,
        preview_tabs: preview.tabs,
        preview_content: preview.content,
        edit_button,
        send_button,
        response: main[1],
        footer: sections[2],
    }
}

pub(super) fn preview_sections(area: Rect) -> PreviewSections {
    if area.is_empty() {
        return PreviewSections {
            summary: Rect::default(),
            tabs: Rect::default(),
            content: Rect::default(),
        };
    }
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(area.height.min(2)),
            Constraint::Length(u16::from(area.height >= 3)),
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
    collection_label: Rect,
    variables_button: Rect,
    request_list: ScrollAreas,
}

fn sidebar_parts(area: Rect) -> SidebarLayout {
    let inner = area.inner(Margin::new(1, 1));
    if inner.height < 5 {
        return SidebarLayout {
            collection_label: Rect::default(),
            variables_button: Rect::default(),
            request_list: panel_scroll_areas(area),
        };
    }
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(if inner.height >= 6 { 3 } else { 1 }),
            Constraint::Min(0),
        ])
        .split(inner);
    SidebarLayout {
        collection_label: parts[0],
        variables_button: parts[1],
        request_list: inner_scroll_areas(parts[2]),
    }
}

fn preview_parts(area: Rect) -> (Rect, [Rect; 2]) {
    let inner = area.inner(Margin::new(1, 1));
    let button_height = inner.height.min(SEND_BUTTON_HEIGHT);
    if inner.width < PREVIEW_ACTION_WIDTH.saturating_add(REQUEST_DETAILS_MIN_WIDTH) {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(button_height)])
            .split(inner);
        return (parts[0], [Rect::default(), parts[1]]);
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(PREVIEW_ACTION_WIDTH)])
        .split(inner);
    let button = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(inner.height.saturating_sub(button_height) / 2),
            Constraint::Length(button_height),
            Constraint::Min(0),
        ])
        .split(columns[1])[1];
    (columns[0], [Rect::default(), button])
}
