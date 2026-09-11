use std::path::{Path, PathBuf};

use ratatui::{Terminal, backend::TestBackend};

use super::*;
use crate::{
    app::{App, RequestRuntimeState},
    config::load,
    http::ResponseData,
};

fn test_app() -> App {
    App::new(
        load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
        PathBuf::from("mock/.postui"),
        crate::settings::GlobalConfig::default(),
    )
}

fn rendered(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("应创建测试终端");
    terminal
        .draw(|frame| draw(frame, app))
        .expect("TUI 应当可以渲染");
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn renders_without_overflow_at_compact_sizes() {
    let mut app = test_app();
    app.select_request(1);

    for (width, height) in [
        (120, 40),
        (80, 24),
        (64, 20),
        (48, 24),
        (48, 16),
        (36, 20),
        (36, 12),
        (24, 8),
    ] {
        let area = Rect::new(0, 0, width, height);
        let layout = screen_layout(area);
        for rect in [
            layout.header,
            layout.requests,
            layout.collection_label,
            layout.variables_button,
            layout.request_list,
            layout.request_scrollbar,
            layout.preview,
            layout.preview_details,
            layout.preview_tabs,
            layout.preview_content,
            layout.edit_button,
            layout.send_button,
            layout.response,
            layout.footer,
        ] {
            assert!(
                u32::from(rect.x) + u32::from(rect.width) <= u32::from(width)
                    && u32::from(rect.y) + u32::from(rect.height) <= u32::from(height),
                "布局越界: {rect:?}，终端: {width}x{height}"
            );
        }

        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("应创建测试终端");
        terminal
            .draw(|frame| draw(frame, &app))
            .expect("普通布局应当可以渲染");
    }
}

#[test]
fn preview_and_response_share_the_workspace_horizontally() {
    let layout = screen_layout(Rect::new(0, 0, 120, 40));

    assert_eq!(layout.preview.y, layout.response.y);
    assert_eq!(layout.preview.height, layout.response.height);
    assert_eq!(layout.preview.right(), layout.response.x);
    assert!(layout.preview.x < layout.response.x);
    assert!(layout.preview.width > layout.response.width);
}

#[test]
fn editable_tables_reserve_most_space_for_values() {
    for width in [30, 48, 60, 80] {
        let params = param_table_widths(width).map(constraint_length);
        let headers = header_table_widths(width).map(constraint_length);

        assert!(
            params[3] >= params[2],
            "Param value should be wider at {width}"
        );
        assert!(
            headers[2] >= headers[1],
            "Header value should be wider at {width}"
        );
        assert!(params.into_iter().sum::<u16>().saturating_add(5) <= width);
        assert!(headers.into_iter().sum::<u16>().saturating_add(5) <= width);
    }
}

#[test]
fn editable_values_remain_visible_at_a_common_terminal_size() {
    let mut app = test_app();
    app.select_request(1);
    app.activate_preview_tab(PreviewTab::Params);
    let params = rendered(&app, 140, 42);
    assert!(!params.contains("%E6%96%87"));
    assert!(params.contains('2'));

    app.close_dialog();
    app.select_request(2);
    app.activate_preview_tab(PreviewTab::Headers);
    let headers = rendered(&app, 140, 42);
    assert!(headers.contains("mock-secret-token"));
}

#[test]
fn sidebar_and_request_editor_render_the_workbench() {
    let app = test_app();
    let rendered = rendered(&app, 120, 40);

    assert!(rendered.contains("Requests/"));
    assert!(rendered.contains("Request"));
    assert!(rendered.contains("Body"));
    assert!(rendered.contains("+ Params"));
    assert!(rendered.contains("+ Headers"));
    assert!(rendered.contains("Variables"));
    assert!(rendered.contains("{}"));
    assert!(!rendered.contains("\"headers\""));
    assert!(rendered.contains("[--]"));
    assert!(!rendered.contains("Extract"));
    assert!(!rendered.contains("Paste"));
    assert!(!rendered.contains("Clear"));
}

#[test]
fn variables_dialog_and_body_render_their_context() {
    let mut app = test_app();
    app.open_variables();
    let variables = rendered(&app, 120, 40);
    assert!(variables.contains("Variables"));
    assert!(variables.contains("Current"));
    assert!(variables.contains("Default"));
    assert!(variables.contains("host"));
    assert_eq!(variables.matches("Current").count(), 1);

    app.close_dialog();
    let body = rendered(&app, 120, 40);
    assert!(body.contains("Body"));
    assert!(body.contains("+ Headers"));
    assert!(!body.contains("X-PostUI-Collection"));
    assert!(body.contains("Response"));
}

#[test]
fn dialog_and_inline_editors_render_without_overflow_at_compact_sizes() {
    for open_dialog in [
        App::open_variables as fn(&mut App),
        App::open_headers as fn(&mut App),
        App::open_params as fn(&mut App),
    ] {
        let mut app = test_app();
        open_dialog(&mut app);
        for (width, height) in [(120, 40), (80, 24), (48, 16), (24, 8)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("应创建测试终端");
            terminal
                .draw(|frame| draw(frame, &app))
                .expect("配置窗口应当可以渲染");
        }
    }
}

#[test]
fn clicking_variables_opens_the_collection_dialog() {
    let mut app = test_app();
    let layout = screen_layout(Rect::new(0, 0, 120, 40));
    handle_click(
        &mut app,
        layout.variables_button.x,
        layout.variables_button.y,
        layout,
    );
    assert!(matches!(app.dialog, Some(Dialog::Variables(_))));
}

#[test]
fn clicking_editable_request_content_opens_the_inline_editor() {
    let mut app = test_app();
    let layout = screen_layout(Rect::new(0, 0, 120, 40));

    for tab in [PreviewTab::Params, PreviewTab::Headers] {
        app.preview_state.active_tab = tab;
        handle_click(
            &mut app,
            layout.preview_content.x,
            layout.preview_content.y,
            layout,
        );
        assert_eq!(app.editing_preview_tab(), Some(tab));
        app.close_dialog();
    }
}

#[test]
fn header_source_column_does_not_start_value_editing() {
    let mut app = test_app();
    app.select_request(2);
    app.activate_preview_tab(PreviewTab::Headers);
    let layout = screen_layout(Rect::new(0, 0, 140, 42));
    let inline = inline_dialog_layout(layout.preview_content);
    let widths = header_table_widths(inline.rows.content.width).map(constraint_length);
    let source_column = inline
        .rows
        .content
        .x
        .saturating_add(widths[0])
        .saturating_add(widths[1])
        .saturating_add(widths[2])
        .saturating_add(3);

    handle_inline_editor_click(
        &mut app,
        source_column,
        inline.rows.content.y,
        layout.preview_content,
    );

    let Some(Dialog::Headers(dialog)) = app.dialog.as_ref() else {
        panic!("Header content should remain open");
    };
    assert!(dialog.editor.is_none());
}

#[test]
fn preview_tab_plus_adds_one_row_while_label_only_activates() {
    let mut app = test_app();
    let layout = screen_layout(Rect::new(0, 0, 120, 40));

    let params_start = preview_tab_label(PreviewTab::Body, &app).chars().count() + 2;
    handle_click(
        &mut app,
        layout
            .preview_tabs
            .x
            .saturating_add(params_start as u16 + 1),
        layout.preview_tabs.y,
        layout,
    );
    let added_count = match app.dialog.as_ref() {
        Some(Dialog::Params(dialog)) => dialog.rows.len(),
        _ => panic!("参数标签的加号应打开参数内容"),
    };

    handle_click(
        &mut app,
        layout
            .preview_tabs
            .x
            .saturating_add(params_start as u16 + 4),
        layout.preview_tabs.y,
        layout,
    );
    let activated_count = match app.dialog.as_ref() {
        Some(Dialog::Params(dialog)) => dialog.rows.len(),
        _ => panic!("参数标签文字应打开参数内容"),
    };

    assert_eq!(activated_count, added_count);
}

#[test]
fn clicking_body_structure_does_not_open_an_editor() {
    let mut app = test_app();
    let layout = screen_layout(Rect::new(0, 0, 120, 40));

    handle_click(
        &mut app,
        layout.preview_content.x,
        layout.preview_content.y,
        layout,
    );

    assert_eq!(app.editing_preview_tab(), None);
    assert!(app.body_editor().is_none());
}

#[test]
fn clicking_a_body_value_starts_inline_editing() {
    let mut app = test_app();
    app.select_request(2);
    let layout = screen_layout(Rect::new(0, 0, 120, 40));
    let body = app.body_json();
    let offset = body.find("task-from-config").expect("请求体应包含字符串值");
    let before = &body[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() as u16;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = before[line_start..].chars().count() as u16;

    handle_click(
        &mut app,
        layout.preview_content.x.saturating_add(column),
        layout.preview_content.y.saturating_add(line),
        layout,
    );
    assert!(app.body_editor().is_some());
    assert_eq!(app.editing_preview_tab(), None);
    assert_eq!(app.preview_state.active_tab, PreviewTab::Body);
}

#[test]
fn clicking_elsewhere_saves_the_active_body_value() {
    let mut app = test_app();
    app.select_request(2);
    let layout = screen_layout(Rect::new(0, 0, 120, 40));
    let body = app.body_json();
    let offset = body.find("task-from-config").unwrap();
    let before = &body[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() as u16;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = before[line_start..].chars().count() as u16;
    handle_click(
        &mut app,
        layout.preview_content.x.saturating_add(column),
        layout.preview_content.y.saturating_add(line),
        layout,
    );
    app.preview_state.editor.as_mut().unwrap().input.value = "saved-on-blur".to_string();

    handle_click(
        &mut app,
        layout.preview_tabs.x,
        layout.preview_tabs.y,
        layout,
    );

    assert!(app.body_editor().is_none());
    assert!(app.body_json().contains("\"taskId\": \"saved-on-blur\""));
}

#[test]
fn clicking_a_sidebar_request_selects_it_immediately() {
    let mut app = test_app();
    app.preview_state.active_tab = PreviewTab::Body;
    let layout = screen_layout(Rect::new(0, 0, 120, 40));
    let second_row = layout.request_list.y.saturating_add(1);

    handle_click(&mut app, layout.request_list.x, second_row, layout);

    assert_eq!(app.requests_state.selected_request, 1);
    assert_eq!(app.focus, Focus::Requests);
    assert_eq!(app.preview_state.active_tab, PreviewTab::Body);
}

#[test]
fn sidebar_shows_independent_request_statuses() {
    let mut app = test_app();
    let first_id = app.config.requests[0].id.clone();
    let second_id = app.config.requests[1].id.clone();
    app.collection_state
        .request_states
        .get_mut(&first_id)
        .unwrap()
        .status = RequestStatus::Success;
    app.collection_state
        .request_states
        .get_mut(&second_id)
        .unwrap()
        .status = RequestStatus::Failed;

    let rendered = rendered(&app, 120, 40);

    assert!(rendered.contains("[OK]"));
    assert!(rendered.contains("[ERR]"));
    assert!(rendered.contains("[--]"));
}

#[test]
fn response_body_is_the_only_response_content_shown_by_default() {
    let mut app = test_app();
    app.select_request(1);
    app.collection_state.request_states.insert(
        app.current_request().id.clone(),
        RequestRuntimeState::from_response(ResponseData {
            status: 200,
            reason: "OK".to_string(),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: r#"{"message":"hello"}"#.to_string(),
            download_path: None,
            elapsed_ms: 1,
        }),
    );

    let rendered = rendered(&app, 120, 40);

    assert!(rendered.contains("Response body"));
    assert!(rendered.contains("message"));
    assert!(!rendered.contains("Response headers"));
    assert!(!rendered.contains("content-type: application/json"));
}

#[test]
fn large_response_stays_inside_a_scrollable_viewport() {
    let mut app = test_app();
    let body = (0..2_000)
        .map(|index| format!("  {{\"index\":{index}}}"))
        .collect::<Vec<_>>()
        .join(",\n");
    app.collection_state.request_states.insert(
        app.current_request().id.clone(),
        RequestRuntimeState::from_response(ResponseData {
            status: 200,
            reason: "OK".to_string(),
            headers: Vec::new(),
            body: format!("[\n{body}\n]"),
            download_path: None,
            elapsed_ms: 1,
        }),
    );

    let area = Rect::new(0, 0, 80, 24);
    let layout = screen_layout(area);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("应创建测试终端");
    terminal
        .draw(|frame| draw(frame, &app))
        .expect("大响应应当可以渲染");
    let response_scrollbar = response_sections(layout.response).body.scrollbar;

    assert!(
        matches!(
            terminal.backend().buffer()[(response_scrollbar.x, response_scrollbar.y)].symbol(),
            "█" | "↑"
        ),
        "响应内容超出视口时应显示原生滚动条"
    );
    app.scroll_response(1);
    assert!(app.response_state.scroll.offset() > 0);
}

#[test]
fn scrollbar_position_maps_viewport_offset_to_native_state() {
    assert_eq!(scrollbar_position(0, 10, 2), 0);
    assert_eq!(scrollbar_position(8, 10, 2), 9);
    assert!(scrollbar_position(4, 10, 2) > 0);
    assert!(scrollbar_position(4, 10, 2) < 9);

    let theme = crate::settings::UiTheme::default();
    let area = Rect::new(0, 0, 1, 10);
    let thumb_top = |offset| {
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("应创建测试终端");
        terminal
            .draw(|frame| draw_scrollbar(frame, area, 10, 2, offset, &theme))
            .expect("滚动条应当可以渲染");
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .enumerate()
            .find_map(|(index, cell)| (cell.symbol() == "█").then_some(index as u16))
            .expect("应显示滚动条滑块")
    };

    let top = thumb_top(0);
    let middle = thumb_top(4);
    let bottom = thumb_top(8);
    assert!(top < middle);
    assert!(middle < bottom);
}

#[test]
fn scrollable_panel_areas_keep_content_and_scrollbar_aligned() {
    let panel = Rect::new(2, 3, 20, 10);
    let areas = panel_scroll_areas(panel);

    assert_eq!(areas.content.y, areas.scrollbar.y);
    assert_eq!(areas.content.height, areas.scrollbar.height);
    assert_eq!(areas.content.right(), areas.scrollbar.x);
    assert_eq!(areas.scrollbar.right(), panel.right().saturating_sub(1));
}

#[test]
fn focused_send_button_fills_the_complete_button_area() {
    let theme = crate::settings::UiTheme::default();
    let area = Rect::new(0, 0, PREVIEW_ACTION_WIDTH, SEND_BUTTON_HEIGHT);
    for state in [
        {
            let mut state = ButtonState::enabled();
            state.set_focused(true);
            state
        },
        ButtonState::disabled(),
    ] {
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("应创建测试终端");
        terminal
            .draw(|frame| {
                frame.render_widget(send_button_widget("Send", &state, &theme), area);
            })
            .expect("按钮应当可以渲染");

        let buffer = terminal.backend().buffer();
        let expected_background = if state.focused {
            theme.accent
        } else {
            ratatui::style::Color::Reset
        };
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                if state.focused {
                    assert_eq!(
                        buffer[(x, y)].bg,
                        expected_background,
                        "聚焦按钮的背景不完整: ({x}, {y})"
                    );
                }
            }
        }
    }
}

#[test]
fn send_button_uses_the_interactive_button_style() {
    let theme = crate::settings::UiTheme::default();
    let mut state = ButtonState::enabled();
    state.set_focused(true);
    let button = send_button_widget("Send", &state, &theme);
    assert_eq!(button.min_height(), SEND_BUTTON_HEIGHT);
    assert!(button.min_width() <= PREVIEW_ACTION_WIDTH);
}

#[test]
fn only_the_actions_focus_highlights_the_send_button() {
    let mut app = test_app();

    assert_eq!(app.focused_preview_action(), None);

    app.focus = Focus::Preview;
    app.preview_state.active_tab = PreviewTab::Headers;
    assert_eq!(app.focused_preview_action(), None);

    app.preview_state.active_tab = PreviewTab::Params;
    assert_eq!(app.focused_preview_action(), None);

    app.preview_state.active_tab = PreviewTab::Body;
    assert_eq!(app.focused_preview_action(), None);

    app.focus = Focus::Actions;
    assert_eq!(app.focused_preview_action(), Some(PreviewAction::Send));
}

#[test]
fn preview_action_button_state_encodes_focus_and_disabled() {
    let enabled_focused = preview_action_button_state(false, true);
    assert!(enabled_focused.focused);

    let disabled = preview_action_button_state(true, true);
    assert!(!disabled.focused);
}

#[test]
fn truncates_using_terminal_cell_width() {
    assert_eq!(truncate("接口", 3), "接…");
    assert_eq!(truncate("接口", 2), "…");
}
