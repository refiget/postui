use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};

const SIDEBAR_WIDE: u16 = 30;
const SIDEBAR_MEDIUM: u16 = 22;
const SIDEBAR_NARROW: u16 = 22;
pub(super) const PREVIEW_ACTION_WIDTH: u16 = 14;
pub(super) const SEND_BUTTON_HEIGHT: u16 = 1;
// Keep the primary action visually detached from the preview panel frame.
const SEND_BUTTON_BORDER_GAP: u16 = 1;
const SEND_BUTTON_RESERVED_HEIGHT: u16 = SEND_BUTTON_HEIGHT + SEND_BUTTON_BORDER_GAP;

#[derive(Debug, Clone, Copy)]
pub(super) struct ScrollAreas {
    pub(super) content: Rect,
    pub(super) scrollbar: Rect,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct UiLayout {
    pub(super) header: Rect,
    pub(super) footer: Rect,
    pub(super) header_content: Rect,
    pub(super) header_action: Rect,
    pub(super) requests: Rect,
    pub(super) workspace_selector: Rect,
    pub(super) variables_button: Rect,
    pub(super) request_search: Rect,
    pub(super) request_list: Rect,
    pub(super) request_scrollbar: Rect,
    pub(super) preview: Rect,
    pub(super) preview_details: Rect,
    pub(super) preview_summary: Rect,
    pub(super) preview_tabs: Rect,
    pub(super) preview_content: Rect,
    pub(super) send_button: Rect,
    pub(super) response: Rect,
    pub(super) response_format_button: Rect,
    pub(super) response_menu_button: Rect,
    pub(super) response_zoom_button: Rect,
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

pub(super) fn response_zoom(area: Rect) -> UiLayout {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height(area.height)),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);
    let header_content = header_content(sections[0]);
    let response = sections[1];
    let (response_format_button, response_zoom_button, response_menu_button) =
        response_action_buttons(response);
    UiLayout {
        header: sections[0],
        footer: sections[2],
        header_content,
        header_action: Rect::default(),
        requests: Rect::default(),
        workspace_selector: Rect::default(),
        variables_button: Rect::default(),
        request_search: Rect::default(),
        request_list: Rect::default(),
        request_scrollbar: Rect::default(),
        preview: Rect::default(),
        preview_details: Rect::default(),
        preview_summary: Rect::default(),
        preview_tabs: Rect::default(),
        preview_content: Rect::default(),
        send_button: Rect::default(),
        response,
        response_format_button,
        response_menu_button,
        response_zoom_button,
    }
}

pub(super) fn variables_page(area: Rect) -> UiLayout {
    let mut layout = response_zoom(area);
    layout.send_button = Rect::default();
    layout.response_format_button = Rect::default();
    layout.response_menu_button = Rect::default();
    layout.response_zoom_button = Rect::default();
    layout
}

pub(super) fn screen_with_summary(area: Rect, summary_height: u16) -> UiLayout {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height(area.height)),
            Constraint::Min(0),
            Constraint::Length(2),
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
        .direction(if area.width < 110 {
            Direction::Vertical
        } else {
            Direction::Horizontal
        })
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(columns[1]);
    let sidebar = sidebar_parts(columns[0]);
    let header_content = header_content(sections[0]);
    let preview_details = main[0].inner(Margin::new(1, 1));
    let preview_content_area = Rect::new(
        preview_details.x,
        preview_details.y,
        preview_details.width,
        preview_details
            .height
            .saturating_sub(SEND_BUTTON_RESERVED_HEIGHT),
    );
    let preview = preview_sections(preview_content_area, summary_height);
    let (response_format_button, response_zoom_button, response_menu_button) =
        response_action_buttons(main[1]);
    let send_button = request_send_button(main[0]);
    let header_action = Rect::new(
        response_menu_button.x,
        header_content.y,
        response_menu_button.width,
        u16::from(!response_menu_button.is_empty()),
    );
    UiLayout {
        header: sections[0],
        footer: sections[2],
        header_content,
        header_action,
        requests: columns[0],
        workspace_selector: sidebar.workspace_selector,
        variables_button: sidebar.variables_button,
        request_search: sidebar.request_search,
        request_list: sidebar.request_list.content,
        request_scrollbar: sidebar.request_list.scrollbar,
        preview: main[0],
        preview_details,
        preview_summary: preview.summary,
        preview_tabs: preview.tabs,
        preview_content: preview.content,
        send_button,
        response: main[1],
        response_format_button,
        response_menu_button,
        response_zoom_button,
    }
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
        4.. => 3,
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
    request_search: Rect,
    request_list: ScrollAreas,
}

fn sidebar_parts(area: Rect) -> SidebarLayout {
    let inner = area.inner(Margin::new(1, 1));
    if inner.height < 4 {
        return SidebarLayout {
            workspace_selector: Rect::default(),
            variables_button: Rect::default(),
            request_search: Rect::default(),
            request_list: panel_scroll_areas(area),
        };
    }
    let parts = if inner.height >= 7 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
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
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(inner)
    };
    let (variables_button, request_search, request_list) = if inner.height >= 7 {
        (parts[2], parts[3], parts[4])
    } else {
        (parts[1], parts[2], parts[3])
    };
    SidebarLayout {
        workspace_selector: parts[0],
        variables_button,
        request_search,
        request_list: inner_scroll_areas(request_list),
    }
}

fn header_content(area: Rect) -> Rect {
    let inner = area.inner(Margin::new(1, 1));
    if inner.is_empty() {
        return Rect::default();
    }
    inner
}

fn request_send_button(area: Rect) -> Rect {
    if area.width <= 2 || area.height <= 1 {
        return Rect::default();
    }
    let inner = area.inner(Margin::new(1, 1));
    if inner.width <= SEND_BUTTON_BORDER_GAP || inner.height < SEND_BUTTON_RESERVED_HEIGHT {
        return Rect::default();
    }
    let width = PREVIEW_ACTION_WIDTH.min(inner.width - SEND_BUTTON_BORDER_GAP);
    Rect::new(
        inner
            .right()
            .saturating_sub(SEND_BUTTON_BORDER_GAP)
            .saturating_sub(width),
        inner
            .bottom()
            .saturating_sub(SEND_BUTTON_BORDER_GAP)
            .saturating_sub(SEND_BUTTON_HEIGHT),
        width,
        SEND_BUTTON_HEIGHT,
    )
}

fn response_action_buttons(area: Rect) -> (Rect, Rect, Rect) {
    if area.width <= 2 || area.height <= 1 {
        return (Rect::default(), Rect::default(), Rect::default());
    }
    let inner = area.inner(Margin::new(1, 1));
    if inner.is_empty() {
        return (Rect::default(), Rect::default(), Rect::default());
    }
    let button_count = 3u16;
    let width = 14.min(inner.width.saturating_sub(button_count) / button_count);
    if width == 0 {
        return (Rect::default(), Rect::default(), Rect::default());
    }
    let gap = 1u16;
    let total_width = width
        .saturating_mul(button_count)
        .saturating_add(gap.saturating_mul(button_count.saturating_sub(1)));
    if inner.width < total_width {
        return (Rect::default(), Rect::default(), Rect::default());
    }
    let format_x = inner.right().saturating_sub(total_width);
    let zoom_x = format_x.saturating_add(width.saturating_add(gap));
    let menu_x = zoom_x.saturating_add(width.saturating_add(gap));
    (
        Rect::new(format_x, inner.y, width, 1),
        Rect::new(zoom_x, inner.y, width, 1),
        Rect::new(menu_x, inner.y, width, 1),
    )
}

pub(super) fn panel_scroll_areas(area: Rect) -> ScrollAreas {
    inner_scroll_areas(area.inner(Margin::new(1, 1)))
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
