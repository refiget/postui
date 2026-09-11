use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::*;
use crate::{
    app::{App, RequestRuntimeState},
    config::load,
    http::ResponseData,
};

fn test_app() -> App {
    let global_config = crate::settings::GlobalConfig {
        language: crate::settings::Language::English,
        ..Default::default()
    };
    App::new(
        load(Path::new("mock/.postui")).expect("mock 请求配置应当可以加载"),
        PathBuf::from("mock/.postui"),
        global_config,
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
            layout.header_content,
            layout.requests,
            layout.collection_label,
            layout.variables_button,
            layout.request_list,
            layout.request_scrollbar,
            layout.preview,
            layout.preview_details,
            layout.preview_tabs,
            layout.preview_content,
            layout.send_button,
            layout.response,
            layout.response_menu_button,
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
fn send_button_is_aligned_to_the_top_right_of_the_app_header() {
    let layout = screen_layout(Rect::new(0, 0, 120, 40));

    assert_eq!(layout.send_button.y, layout.header.y.saturating_add(1));
    assert_eq!(
        layout.send_button.right(),
        layout.header.right().saturating_sub(1)
    );
}

#[test]
fn editable_tables_reserve_most_space_for_values() {
    for width in [30, 48, 60, 80] {
        let params = inline_param_table_widths(width).map(constraint_length);
        let headers = inline_header_table_widths(width).map(constraint_length);

        assert!(
            params[1] >= params[0],
            "Param value should be wider at {width}"
        );
        assert!(
            headers[1] >= headers[0],
            "Header value should be wider at {width}"
        );
        let available = width.saturating_sub(TABLE_HIGHLIGHT_WIDTH + TABLE_COLUMN_SPACING);
        assert_eq!(params.into_iter().sum::<u16>(), available);
        assert_eq!(headers.into_iter().sum::<u16>(), available);
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
fn params_preview_renders_table_with_truncation_and_clickable_hints() {
    let mut app = test_app();
    let request_index = app
        .config
        .requests
        .iter()
        .position(|request| request.name == "查询参数")
        .unwrap_or(1);
    app.select_request(request_index);
    app.activate_preview_tab(PreviewTab::Params);
    app.close_dialog();
    app.collection_state.variables.insert(
        "search_term".to_string(),
        "search-value-with-extra-long-tail-that-cuts".to_string(),
    );
    app.collection_state.variables.insert(
        "page".to_string(),
        "page-value-with-extra-long-tail-that-cuts".to_string(),
    );
    let output = rendered(&app, 72, 40);

    assert!(output.contains("Name"));
    assert!(output.contains("Value"));
    assert!(output.contains('…'));
    assert!(
        !output.contains("search-value-with-extra-long-tail-that-cuts"),
        "查询参数值应在窄宽下截断"
    );
}

#[test]
fn headers_preview_renders_table_with_truncation_and_clickable_hints() {
    let mut app = test_app();
    let request_index = app
        .config
        .requests
        .iter()
        .position(|request| request.name == "查询参数")
        .unwrap_or(1);
    app.select_request(request_index);
    app.activate_preview_tab(PreviewTab::Headers);
    app.close_dialog();
    app.collection_state.variables.insert(
        "token".to_string(),
        "token-value-with-extra-long-tail-that-cuts".to_string(),
    );
    let output = rendered(&app, 72, 40);

    assert!(output.contains("Name"));
    assert!(output.contains("Value"));
    assert!(output.contains('…'));
    assert!(
        !output.contains("token-value-with-extra-long-tail-that-cuts"),
        "请求头值应在窄宽下截断"
    );
}

#[test]
fn editable_params_and_headers_tables_render_without_an_open_dialog() {
    let mut app = test_app();
    app.select_request(1);

    for tab in [PreviewTab::Params, PreviewTab::Headers] {
        app.dialog = None;
        app.preview_state.active_tab = tab;
        let output = rendered(&app, 120, 40);

        assert!(output.contains("Name"));
        assert!(output.contains("Value"));
        assert!(output.contains("▸"));
    }
}

#[test]
fn clicking_a_direct_table_cell_opens_the_matching_editor() {
    let mut app = test_app();
    app.select_request(1);
    app.preview_state.active_tab = PreviewTab::Params;
    app.dialog = None;

    let layout = screen_layout(Rect::new(0, 0, 120, 40));
    let row_count = match app.preview_dialog(PreviewTab::Params) {
        Some(Dialog::Params(dialog)) => dialog.rows.len(),
        _ => 0,
    };
    let inline = inline_dialog_layout(layout.preview_content, row_count);
    let widths = inline_param_table_widths(inline.rows.content.width).map(constraint_length);
    let value_x = inline
        .rows
        .content
        .x
        .saturating_add(TABLE_HIGHLIGHT_WIDTH)
        .saturating_add(widths[0])
        .saturating_add(TABLE_COLUMN_SPACING);
    let row_y = inline.rows.content.y;

    handle_click(&mut app, value_x, row_y, layout);

    assert!(matches!(
        app.dialog.as_ref(),
        Some(Dialog::Params(dialog)) if dialog.editor.is_some()
    ));
}

#[test]
fn inline_tables_do_not_disable_send_or_global_tab_navigation() {
    let mut app = test_app();
    app.select_request(1);
    app.activate_preview_tab(PreviewTab::Params);

    assert!(app.can_execute_preview_action(PreviewAction::Send));

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::Actions);
}

#[test]
fn inline_tables_keep_global_shortcuts_available() {
    let mut app = test_app();
    app.activate_preview_tab(PreviewTab::Params);

    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));

    assert!(matches!(app.dialog, Some(Dialog::Variables(_))));
}

#[test]
fn request_urls_are_shown_after_variable_resolution() {
    let app = test_app();
    let output = rendered(&app, 140, 42);

    assert!(output.contains("http://127.0.0.1:18080/v1/health"));
    assert!(!output.contains("http://{{host}}/v1/health"));
}

#[test]
fn long_request_urls_wrap_without_truncating_the_tail() {
    let mut app = test_app();
    app.config.requests[0].url = format!(
        "http://{{{{host}}}}/v1/health/{}",
        "long-path-segment-".repeat(8)
    );
    let area = Rect::new(0, 0, 120, 40);
    let layout = screen_layout_for_app(area, &app);
    assert!(layout.preview_summary.height > 2);

    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("应创建测试终端");
    terminal
        .draw(|frame| draw(frame, &app))
        .expect("长 URL 应当可以渲染");
    let summary = layout.preview_summary;
    let output = (summary.y..summary.bottom())
        .flat_map(|row| (summary.x..summary.right()).map(move |column| (column, row)))
        .map(|(column, row)| {
            terminal
                .backend()
                .buffer()
                .cell((column, row))
                .unwrap()
                .symbol()
        })
        .collect::<String>()
        .replace(' ', "");
    assert!(output.contains("long-path-segment-long-path-segment"));
}

#[test]
fn get_url_variables_are_shown_as_editable_content_fields() {
    let mut app = test_app();
    app.select_request(3);
    let output = rendered(&app, 140, 42);

    assert!(output.contains("task_id: task-from-config"));
}

#[test]
fn request_variable_names_use_the_variable_highlight_style() {
    let app = test_app();
    let line = request_variable_line(&app, "task_id", &app.global_config.theme);

    assert_eq!(
        line.spans[0].style.fg,
        Some(app.global_config.theme.variable)
    );
    assert!(line.spans[0].style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn clicking_a_get_variable_field_starts_editing_and_saves_on_blur() {
    let mut app = test_app();
    app.select_request(3);
    app.collection_state
        .variables
        .insert("task_id".to_string(), String::new());
    let area = Rect::new(0, 0, 120, 40);
    let layout = screen_layout_for_app(area, &app);

    handle_click(
        &mut app,
        layout.preview_content.x.saturating_add(9),
        layout.preview_content.y,
        layout,
    );
    assert_eq!(
        app.variable_editor().map(|editor| editor.variable.as_str()),
        Some("task_id")
    );

    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    for character in "task-from-click".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.collection_state.variables.get("task_id"),
        Some(&"task-from-click".to_string())
    );
    assert!(
        app.resolved_url(app.current_request())
            .contains("task-from-click")
    );
}

#[test]
fn clicking_the_request_url_does_not_start_variable_editing() {
    let mut app = test_app();
    app.select_request(3);
    let area = Rect::new(0, 0, 120, 40);
    let layout = screen_layout_for_app(area, &app);

    handle_click(
        &mut app,
        layout.preview_summary.x,
        layout.preview_summary.y,
        layout,
    );

    assert!(app.variable_editor().is_none());
}

#[test]
fn multipart_content_shows_form_fields_and_resolved_files() {
    let mut app = test_app();
    app.select_request(8);
    let output = rendered(&app, 140, 42);

    assert!(output.contains("Content"));
    assert!(output.contains("Form"));
    assert!(output.contains("Files"));
    assert!(output.contains("sample.txt"));
    assert!(output.contains("second.txt"));
    assert!(!output.contains("File not found"));
}

#[test]
fn missing_upload_file_is_not_prevalidated_in_the_request_content() {
    let mut app = test_app();
    app.select_request(15);
    let output = rendered(&app, 140, 42);

    assert!(output.contains("does-not-exist.txt"));
    assert!(!output.contains("File not found"));
}

#[test]
fn clicking_an_upload_file_value_starts_inline_editing() {
    let mut app = test_app();
    app.select_request(8);
    let layout = screen_layout(Rect::new(0, 0, 120, 40));

    handle_click(
        &mut app,
        layout.preview_content.x.saturating_add(6),
        layout.preview_content.y.saturating_add(5),
        layout,
    );

    let editor = app.file_editor().expect("点击文件值后应开始编辑");
    assert_eq!(editor.file_index, 0);
    assert_eq!(editor.input.value, "{{upload_file}}");

    app.preview_state.file_editor.as_mut().unwrap().input.value = "replacement.txt".to_string();
    app.commit_active_editors();
    assert_eq!(
        app.current_resolved_request().files[0].path,
        "replacement.txt"
    );
}

#[test]
fn an_empty_upload_file_edit_restores_the_configured_default() {
    let mut app = test_app();
    app.select_request(8);
    app.start_body_edit(5, 6);
    app.preview_state
        .file_editor
        .as_mut()
        .unwrap()
        .input
        .value
        .clear();

    app.commit_active_editors();

    assert_eq!(app.current_resolved_request().files[0].path, "sample.txt");
}

#[test]
fn sidebar_and_request_editor_render_the_workbench() {
    let app = test_app();
    let rendered = rendered(&app, 120, 40);

    assert!(rendered.contains("Requests"));
    assert!(rendered.contains("Request"));
    assert!(rendered.contains("Content"));
    assert!(rendered.contains("Params"));
    assert!(rendered.contains("Headers"));
    assert!(rendered.contains("Variables"));
    assert!(rendered.contains("No request content"));
    assert!(!rendered.contains("\"headers\""));
    assert!(rendered.contains("●"));
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
    assert!(body.contains("Content"));
    assert!(body.contains("Headers"));
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
fn clicking_collection_opens_an_anchored_dropdown() {
    let mut app = test_app();
    let layout = screen_layout(Rect::new(0, 0, 120, 40));
    let closed = rendered(&app, 120, 40);

    assert_eq!(layout.collection_label.height, 2);
    assert!(closed.contains("◆ .postui ▾"));

    handle_click(
        &mut app,
        layout.collection_label.x,
        layout.collection_label.y,
        layout,
    );

    assert!(app.collection_menu_open);
    assert_eq!(app.focus, Focus::Collection);
    let output = rendered(&app, 120, 40);
    assert!(output.contains(".postui"));
    assert!(output.contains("▴"));
}

#[test]
fn collection_menu_highlight_follows_the_mouse_cursor() {
    let mut app = test_app();
    app.collections.push(crate::app::CollectionChoice {
        name: "Troubleshooting".to_string(),
        path: PathBuf::from("mock/.postui/collections/troubleshooting"),
    });
    app.open_collection_menu();

    let area = Rect::new(0, 0, 120, 40);
    let layout = screen_layout(area);
    let content = collection_menu_area(layout, &app).inner(Margin::new(1, 1));
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: content.x,
            row: content.y + 1,
            modifiers: crossterm::event::KeyModifiers::NONE,
        },
        area,
    );

    assert_eq!(app.selected_collection, 1);
    assert!(app.collection_menu_open);
}

#[test]
fn clicking_a_variable_value_starts_inline_editing() {
    let mut app = test_app();
    app.open_variables();
    let area = Rect::new(0, 0, 120, 40);
    let row_count = match app.dialog.as_ref() {
        Some(Dialog::Variables(dialog)) => dialog.rows.len(),
        _ => panic!("变量窗口应当打开"),
    };
    let layout = dialog_layout(area, row_count);
    let widths = variable_table_widths(layout.rows.content.width);
    let value_x = layout
        .rows
        .content
        .x
        .saturating_add(TABLE_HIGHLIGHT_WIDTH)
        .saturating_add(constraint_length(widths[0]))
        .saturating_add(TABLE_COLUMN_SPACING);
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: value_x,
        row: layout.rows.content.y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };

    handle_dialog_mouse(&mut app, event, area);

    assert!(matches!(
        app.dialog.as_ref(),
        Some(Dialog::Variables(dialog)) if dialog.editor.is_some()
    ));
}

#[test]
fn variables_uses_a_single_line_button_surface() {
    let app = test_app();
    let layout = screen_layout(Rect::new(0, 0, 120, 40));
    let output = rendered(&app, 120, 40);

    assert_eq!(layout.variables_button.height, 1);
    assert!(output.contains("◇ Variables"));

    let theme = crate::settings::UiTheme::default();
    let state = ButtonState::enabled();
    let button = variables_button_widget("Variables (2)", &state, &theme);
    assert_eq!(button.min_height(), 1);

    let backend = TestBackend::new(24, 1);
    let mut terminal = Terminal::new(backend).expect("应创建测试终端");
    terminal
        .draw(|frame| frame.render_widget(button, frame.area()))
        .expect("变量按钮应当可以渲染");
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .any(|cell| cell.bg == theme.accent)
    );
}

#[test]
fn sidebar_groups_collection_variables_and_requests_with_blank_rows() {
    let layout = screen_layout(Rect::new(0, 0, 120, 40));

    assert_eq!(
        layout.variables_button.y,
        layout.collection_label.bottom().saturating_add(1)
    );
    assert_eq!(
        layout.request_list.y,
        layout.variables_button.bottom().saturating_add(1)
    );
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
fn inline_header_click_targets_name_or_value_column() {
    let mut app = test_app();
    app.select_request(9);
    app.activate_preview_tab(PreviewTab::Headers);
    let layout = screen_layout(Rect::new(0, 0, 140, 42));
    handle_click(
        &mut app,
        layout.preview_content.x,
        layout.preview_content.y,
        layout,
    );
    let request_row = app
        .dialog
        .as_ref()
        .and_then(|dialog| match dialog {
            Dialog::Headers(dialog) => dialog
                .rows
                .iter()
                .position(|row| row.source == HeaderSource::Request),
            _ => None,
        })
        .unwrap_or(0);
    let inline = inline_dialog_layout(
        layout.preview_content,
        app.dialog
            .as_ref()
            .and_then(|dialog| match dialog {
                Dialog::Headers(dialog) => Some(dialog.rows.len()),
                _ => None,
            })
            .unwrap_or(0),
    );
    let widths = inline_header_table_widths(inline.rows.content.width).map(constraint_length);
    let name_column = inline
        .rows
        .content
        .x
        .saturating_add(TABLE_HIGHLIGHT_WIDTH)
        .saturating_add(widths[0].saturating_div(2));
    let value_column = inline
        .rows
        .content
        .x
        .saturating_add(TABLE_HIGHLIGHT_WIDTH)
        .saturating_add(widths[0])
        .saturating_add(TABLE_COLUMN_SPACING);
    let row_y = inline.rows.content.y.saturating_add(request_row as u16);

    handle_inline_editor_click(&mut app, name_column, row_y, layout.preview_content);
    let Some(Dialog::Headers(dialog)) = app.dialog.as_ref() else {
        panic!("Header content should remain open");
    };
    assert_eq!(dialog.field, HeaderField::Name);

    handle_inline_editor_click(&mut app, value_column, row_y, layout.preview_content);
    let Some(Dialog::Headers(dialog)) = app.dialog.as_ref() else {
        panic!("Header content should remain open");
    };
    assert_eq!(dialog.field, HeaderField::Value);
    assert!(dialog.editor.is_some());
}

#[test]
fn inline_add_row_is_below_the_last_item_and_clickable() {
    let mut app = test_app();
    let layout = screen_layout(Rect::new(0, 0, 120, 40));
    app.activate_preview_tab(PreviewTab::Params);
    let before = match app.dialog.as_ref() {
        Some(Dialog::Params(dialog)) => dialog.rows.len(),
        _ => panic!("参数表格应当打开"),
    };
    let inline = inline_dialog_layout(layout.preview_content, before);
    assert_eq!(
        inline.add_button.y,
        layout.preview_content.y.saturating_add(1)
    );
    assert_eq!(inline.add_button.height, 3);

    handle_click(
        &mut app,
        inline
            .add_button
            .x
            .saturating_add(inline.add_button.width / 2),
        inline.add_button.y,
        layout,
    );

    let after = match app.dialog.as_ref() {
        Some(Dialog::Params(dialog)) => dialog.rows.len(),
        _ => panic!("参数表格应当保持打开"),
    };
    assert_eq!(after, before + 1);
    let moved = inline_dialog_layout(layout.preview_content, after);
    assert_eq!(moved.add_button.y, inline.add_button.y.saturating_add(1));
}

#[test]
fn inline_add_control_only_renders_a_single_plus() {
    let mut app = test_app();
    app.activate_preview_tab(PreviewTab::Params);

    let output = rendered(&app, 120, 40);

    assert!(!output.contains("Add"));
    assert!(!output.contains("A+D"));
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
fn sidebar_request_item_only_shows_the_request_name() {
    let app = test_app();
    let request = &app.config.requests[0];
    let backend = TestBackend::new(80, 1);
    let mut terminal = Terminal::new(backend).expect("应创建测试终端");

    terminal
        .draw(|frame| {
            frame.render_widget(
                List::new(vec![request_item(
                    request,
                    &app.global_config.theme,
                    frame.area().width,
                )]),
                frame.area(),
            );
        })
        .expect("接口列表项应当可以渲染");

    let output = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(
        request
            .name
            .chars()
            .filter(|character| !character.is_whitespace())
            .all(|character| output.contains(character))
    );
    assert!(!output.contains(&request.method));
    assert!(!output.contains(&request.url));
    assert!(!output.contains('●'));
}

#[test]
fn request_status_visuals_use_spinner_and_semantic_colors() {
    let theme = crate::settings::UiTheme::default();

    assert_ne!(
        request_status_symbol(RequestStatus::Sending, 0),
        request_status_symbol(RequestStatus::Sending, 1)
    );
    assert_eq!(
        request_status_style(RequestStatus::Failed, &theme).fg,
        Some(theme.error)
    );
    assert_eq!(
        request_status_style(RequestStatus::Timeout, &theme).fg,
        Some(theme.warning)
    );
    assert_eq!(
        request_status_style(RequestStatus::Success, &theme).fg,
        Some(theme.success)
    );
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
            body_bytes: br#"{"message":"hello"}"#.to_vec(),
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
fn response_actions_open_as_a_dropdown_after_a_response_arrives() {
    let mut app = test_app();
    app.collection_state.request_states.insert(
        app.current_request().id.clone(),
        RequestRuntimeState::from_response(ResponseData {
            status: 200,
            reason: "OK".to_string(),
            headers: Vec::new(),
            body: "hello".to_string(),
            body_bytes: b"hello".to_vec(),
            elapsed_ms: 1,
        }),
    );
    let area = Rect::new(0, 0, 120, 40);
    let layout = screen_layout_for_app(area, &app);

    handle_click(
        &mut app,
        layout.response_menu_button.x,
        layout.response_menu_button.y,
        layout,
    );

    assert!(app.response_state.menu_open);
    let output = rendered(&app, area.width, area.height);
    assert!(output.contains("Actions"));
    assert!(output.contains("Download"));
    assert!(output.contains("Copy"));
}

#[test]
fn response_actions_are_clickable_when_request_failed() {
    let mut app = test_app();
    let request_id = app.current_request().id.clone();
    if let Some(state) = app.collection_state.request_states.get_mut(&request_id) {
        state.status = RequestStatus::Failed;
        state.response = None;
        state.error = Some("timeout from server".to_string());
    }
    let area = Rect::new(0, 0, 120, 40);
    let layout = screen_layout_for_app(area, &app);

    handle_click(
        &mut app,
        layout.response_menu_button.x,
        layout.response_menu_button.y,
        layout,
    );

    assert!(app.response_state.menu_open);
    let output = rendered(&app, area.width, area.height);
    assert!(output.contains("Actions"));
}

#[test]
fn response_actions_are_clickable_when_request_timeout() {
    let mut app = test_app();
    let request_id = app.current_request().id.clone();
    if let Some(state) = app.collection_state.request_states.get_mut(&request_id) {
        state.status = RequestStatus::Timeout;
        state.response = None;
        state.error = Some("timeout".to_string());
    }
    let area = Rect::new(0, 0, 120, 40);
    let layout = screen_layout_for_app(area, &app);

    handle_click(
        &mut app,
        layout.response_menu_button.x,
        layout.response_menu_button.y,
        layout,
    );

    assert!(app.response_state.menu_open);
}

#[test]
fn response_actions_click_does_not_switch_focus() {
    let mut app = test_app();
    app.collection_state.request_states.insert(
        app.current_request().id.clone(),
        RequestRuntimeState::from_response(ResponseData {
            status: 200,
            reason: "OK".to_string(),
            headers: Vec::new(),
            body: r#"{"message":"ok"}"#.to_string(),
            body_bytes: br#"{"message":"ok"}"#.to_vec(),
            elapsed_ms: 1,
        }),
    );
    app.focus = Focus::Preview;
    let area = Rect::new(0, 0, 120, 40);
    let layout = screen_layout_for_app(area, &app);

    handle_click(
        &mut app,
        layout.response_menu_button.x,
        layout.response_menu_button.y,
        layout,
    );

    assert!(app.response_state.menu_open);
    assert_eq!(app.focus, Focus::Preview);
}

#[test]
fn response_dropdown_follows_hover_and_downloads_the_cached_response() {
    let mut app = test_app();
    app.collection_state.request_states.insert(
        app.current_request().id.clone(),
        RequestRuntimeState::from_response(ResponseData {
            status: 200,
            reason: "OK".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: r#"{"ok":true}"#.to_string(),
            body_bytes: br#"{"ok":true}"#.to_vec(),
            elapsed_ms: 1,
        }),
    );
    let directory = std::env::temp_dir().join(format!(
        "postui-ui-response-download-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    app.config.download_directory = directory.clone();
    app.open_response_menu();

    let area = Rect::new(0, 0, 120, 40);
    let layout = screen_layout_for_app(area, &app);
    let menu = response_menu_area(layout.response, layout.response_menu_button);
    let content = menu.inner(Margin::new(1, 1));
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: content.x,
            row: content.y + 1,
            modifiers: crossterm::event::KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(
        app.response_state.menu_selected,
        ResponseMenuAction::Copy as usize
    );

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: content.x,
            row: content.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        },
        area,
    );

    assert!(!app.response_state.menu_open);
    assert_eq!(
        std::fs::read(directory.join("requests-01-health.http.json"))
            .expect("下载动作应保存当前响应"),
        br#"{"ok":true}"#
    );
    assert!(app.status.contains("Response saved to"));
    std::fs::remove_dir_all(directory).expect("应清理响应目录");
}

#[test]
fn response_dropdown_is_available_before_a_request_finishes() {
    let mut app = test_app();
    app.open_response_menu();

    assert!(app.response_state.menu_open);
    app.activate_selected_response_action();
    assert!(!app.response_state.menu_open);
    assert_eq!(app.status, "No response to act on");
}

#[test]
fn response_actions_always_use_the_focus_highlight() {
    let app = test_app();
    assert_ne!(app.focus, Focus::Actions);
    let theme = app.global_config.theme.clone();
    let backend = TestBackend::new(16, 1);
    let mut terminal = Terminal::new(backend).expect("应创建测试终端");

    terminal
        .draw(|frame| draw_response_menu_button(frame, frame.area(), &app))
        .expect("Actions 按钮应当可以渲染");

    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .all(|cell| cell.bg == theme.selection)
    );
}

#[test]
fn response_actions_button_is_inside_response_panel_top_right() {
    let app = test_app();
    let layout = screen_layout_for_app(Rect::new(0, 0, 120, 40), &app);
    let panel = layout.response;
    let button = layout.response_menu_button;

    assert!(button.x > panel.x);
    assert!(button.y > panel.y);
    assert_eq!(button.y, panel.y.saturating_add(1));
    assert!(button.x.saturating_add(button.width).saturating_add(1) <= panel.right());
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
            body_bytes: format!("[\n{body}\n]").into_bytes(),
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
    let response_scrollbar = response_sections(layout.response, layout.response_menu_button)
        .body
        .scrollbar;

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
fn send_button_uses_a_quiet_terminal_style() {
    let theme = crate::settings::UiTheme::default();
    let area = Rect::new(0, 0, PREVIEW_ACTION_WIDTH, SEND_BUTTON_HEIGHT);
    for state in [
        {
            let mut state = ButtonState::enabled();
            state.set_focused(true);
            state
        },
        ButtonState::enabled(),
    ] {
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("应创建测试终端");
        terminal
            .draw(|frame| {
                frame.render_widget(send_button_widget("Send", &state, &theme), area);
            })
            .expect("按钮应当可以渲染");

        let buffer = terminal.backend().buffer();
        let button_background = if state.focused {
            theme.selection
        } else {
            theme.surface
        };
        let highlighted = buffer
            .content
            .iter()
            .filter(|cell| cell.bg == button_background)
            .count();
        assert!(highlighted > 0);
    }
}

#[test]
fn send_button_label_uses_terminal_brackets_instead_of_an_arrow() {
    let app = test_app();
    let output = rendered(&app, 120, 40);

    assert!(output.contains("[ Send ]"));
    assert!(!output.contains("↑ Send"));
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
fn disabled_send_button_ignores_mouse_clicks() {
    let mut app = test_app();
    let request_id = app.current_request().id.clone();
    app.collection_state
        .request_states
        .get_mut(&request_id)
        .unwrap()
        .status = RequestStatus::Sending;
    app.status = "unchanged".to_string();
    let layout = screen_layout(Rect::new(0, 0, 120, 40));

    handle_click(&mut app, layout.send_button.x, layout.send_button.y, layout);

    assert_eq!(app.status, "unchanged");
    assert_eq!(app.focus, Focus::Requests);
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
