use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};

const SIDEBAR_WIDE: u16 = 30;
const SIDEBAR_NARROW: u16 = 22;
const MAIN_HORIZONTAL_MIN_WIDTH: u16 = 64;
/// 底栏高度：状态行和按键提示行。
const FOOTER_HEIGHT: u16 = 2;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ScrollAreas {
    pub(super) content: Rect,
    pub(super) scrollbar: Rect,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct UiLayout {
    pub(super) header: Rect,
    pub(super) footer: Rect,
    pub(super) header_content: Rect,
    pub(super) requests: Rect,
    pub(super) request_search: Rect,
    pub(super) request_list: Rect,
    pub(super) request_scrollbar: Rect,
    pub(super) configuration_menu: Rect,
    pub(super) preview: Rect,
    pub(super) preview_summary: Rect,
    pub(super) preview_tabs: Rect,
    pub(super) preview_content: Rect,
    pub(super) response: Rect,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PreviewSections {
    pub(super) summary: Rect,
    pub(super) tabs: Rect,
    pub(super) content: Rect,
}

/// 顶栏、主区域和底栏的三段纵向布局。
fn screen_sections(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height(area.height)),
            Constraint::Min(0),
            Constraint::Length(FOOTER_HEIGHT),
        ])
        .split(area)
}

pub(super) fn response_zoom(area: Rect) -> UiLayout {
    let sections = screen_sections(area);
    UiLayout {
        header: sections[0],
        footer: sections[2],
        header_content: header_content(sections[0]),
        configuration_menu: configuration_anchor(sections[0]),
        response: sections[1],
        ..UiLayout::default()
    }
}

pub(super) fn screen_with_summary(area: Rect, summary_height: u16) -> UiLayout {
    let sections = screen_sections(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sidebar_width(sections[1].width)),
            Constraint::Min(0),
        ])
        .split(sections[1]);
    let main_direction = if columns[1].width < MAIN_HORIZONTAL_MIN_WIDTH {
        Direction::Vertical
    } else {
        Direction::Horizontal
    };
    let main_constraints = if main_direction == Direction::Vertical {
        [
            Constraint::Length(summary_height.saturating_add(3)),
            Constraint::Min(0),
        ]
    } else {
        [Constraint::Percentage(45), Constraint::Percentage(55)]
    };
    let main = Layout::default()
        .direction(main_direction)
        .constraints(main_constraints)
        .split(columns[1]);
    let sidebar = sidebar_parts(columns[0]);
    let preview = preview_sections(main[0].inner(Margin::new(1, 1)), summary_height);
    UiLayout {
        header: sections[0],
        footer: sections[2],
        header_content: header_content(sections[0]),
        requests: columns[0],
        request_search: sidebar.request_search,
        request_list: sidebar.request_list.content,
        request_scrollbar: sidebar.request_list.scrollbar,
        configuration_menu: configuration_anchor(sections[0]),
        preview: main[0],
        preview_summary: preview.summary,
        preview_tabs: preview.tabs,
        preview_content: preview.content,
        response: main[1],
    }
}

pub(super) fn preview_sections(area: Rect, requested_summary_height: u16) -> PreviewSections {
    if area.is_empty() {
        return PreviewSections::default();
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
        4.. => 3,
        _ => 0,
    }
}

fn sidebar_width(width: u16) -> u16 {
    let preferred = if width >= 100 {
        SIDEBAR_WIDE
    } else if width >= 48 {
        SIDEBAR_NARROW
    } else {
        width.saturating_sub(14).max(10)
    };
    preferred.min(width)
}

#[derive(Debug, Clone, Copy, Default)]
struct SidebarLayout {
    request_search: Rect,
    request_list: ScrollAreas,
}

fn sidebar_parts(area: Rect) -> SidebarLayout {
    let inner = area.inner(Margin::new(1, 1));
    if inner.is_empty() {
        return SidebarLayout {
            request_search: Rect::default(),
            request_list: inner_scroll_areas(area),
        };
    }
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);
    SidebarLayout {
        request_search: parts[0],
        request_list: inner_scroll_areas(parts[1]),
    }
}

fn header_content(area: Rect) -> Rect {
    let inner = area.inner(Margin::new(1, 1));
    if inner.is_empty() {
        return Rect::default();
    }
    inner
}

/// 配置下拉菜单的定位锚点：紧贴顶栏下沿的左上角。
fn configuration_anchor(header: Rect) -> Rect {
    Rect::new(
        header.x.saturating_add(1),
        header.bottom().saturating_sub(1),
        1,
        1,
    )
}

pub(super) fn inner_scroll_areas(area: Rect) -> ScrollAreas {
    if area.is_empty() {
        return ScrollAreas {
            content: Rect::default(),
            scrollbar: Rect::default(),
        };
    }
    let scrollbar_width = u16::from(area.width > 1);
    let content_width = area.width.saturating_sub(scrollbar_width);
    ScrollAreas {
        content: Rect::new(area.x, area.y, content_width, area.height),
        scrollbar: Rect::new(
            area.x.saturating_add(content_width),
            area.y,
            scrollbar_width,
            area.height,
        ),
    }
}
