use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde_json::{Value, json};

pub(super) fn create(
    directory: &Path,
    base_url: &str,
    theme: &str,
    language: &str,
) -> Result<PathBuf> {
    let workspace = directory.join(".postui");
    for child in [
        ".postui",
        ".postui/requests",
        ".postui/scenarios",
        "uploads",
        "downloads",
    ] {
        let path = directory.join(child);
        fs::create_dir(&path).with_context(|| format!("创建预览目录失败：{}", path.display()))?;
    }
    write_json(
        &directory.join("config.yaml"),
        &json!({"theme": theme, "language": language}),
    )?;
    write_json(
        &workspace.join("postui.yaml"),
        &json!({
            "name": "Studio · 前端预览",
            "directories": {"uploads": "uploads", "downloads": "downloads"},
            "default_scenario": "local",
            "timeout": 10,
            "headers": {"Accept": "application/json", "X-Client": "postui-preview"},
            "variables": {
                "base_url": {"value": base_url, "temporary": false},
                "project_name": "Terminal Studio",
                "owner": "Lin",
                "environment": "local"
            }
        }),
    )?;
    for (name, environment) in [
        ("local", "local"),
        ("staging", "staging"),
        ("sandbox", "sandbox"),
    ] {
        write_json(
            &workspace.join("scenarios").join(format!("{name}.yaml")),
            &json!({"variables": {"environment": environment}, "headers": {"X-Environment": environment}}),
        )?;
    }
    let project_body = serde_json::to_string_pretty(&json!({
        "name": "{{project_name}}",
        "owner": "{{owner}}",
        "environment": "{{environment}}",
        "visibility": "private",
        "notifications": true,
        "tags": ["terminal", "design", "rust"]
    }))?;
    let requests = [
        (
            "01-create-project",
            json!({
                "name": "创建项目", "description": "项目资料 · JSON", "method": "POST",
                "url": "{{base_url}}/projects", "headers": {"Content-Type": "application/json"},
                "body": project_body
            }),
        ),
        (
            "02-projects",
            json!({
                "name": "项目列表", "description": "24 条项目记录 · 分页参数", "url": "{{base_url}}/projects",
                "params": [{"name": "limit", "value": "24"}, {"name": "status", "value": "active"}]
            }),
        ),
        (
            "03-health",
            json!({
                "name": "服务状态", "description": "服务信息 · 运行状态", "url": "{{base_url}}/health"
            }),
        ),
        (
            "04-update-project",
            json!({
                "name": "更新项目", "description": "项目设置 · JSON", "method": "PATCH",
                "url": "{{base_url}}/projects/p-1047", "headers": {"Content-Type": "application/json"},
                "body": "{\n  \"name\": \"Terminal Studio\",\n  \"visibility\": \"public\"\n}"
            }),
        ),
        (
            "05-validation",
            json!({
                "name": "校验结果", "description": "HTTP 422 · 字段校验", "url": "{{base_url}}/status/422"
            }),
        ),
        (
            "06-slow",
            json!({
                "name": "延迟响应", "description": "1800 ms · 发送中可停止", "url": "{{base_url}}/slow"
            }),
        ),
        (
            "07-upload",
            json!({
                "name": "上传文件", "description": "Multipart · 示例文本文件", "method": "POST",
                "url": "{{base_url}}/upload", "form": [{"name": "collection", "value": "studio"}],
                "files": [{"field": "file", "path": "studio-note.txt", "content_type": "text/plain"}]
            }),
        ),
        (
            "08-export",
            json!({
                "name": "导出项目", "description": "JSON 文件 · 响应菜单下载", "url": "{{base_url}}/export"
            }),
        ),
    ];
    for (name, document) in requests {
        write_json(
            &workspace.join("requests").join(format!("{name}.yaml")),
            &document,
        )?;
    }
    write_new(
        &directory.join("uploads/studio-note.txt"),
        b"PostUI Studio\nSample upload file\nTerminal interface preview\n",
    )?;
    Ok(workspace)
}

fn write_json(path: &Path, document: &Value) -> Result<()> {
    write_new(path, &serde_json::to_vec_pretty(document)?)
}

fn write_new(path: &Path, contents: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("创建预览文件失败：{}", path.display()))?;
    file.write_all(contents)
        .with_context(|| format!("写入预览文件失败：{}", path.display()))
}
