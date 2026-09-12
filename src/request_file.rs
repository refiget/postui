use std::{
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::config::{ApiRequest, BodyPart};

#[derive(Debug, Clone)]
pub(crate) struct RequestFileStore {
    workspace_path: PathBuf,
}

impl RequestFileStore {
    pub(crate) fn new(workspace_path: PathBuf) -> Self {
        Self { workspace_path }
    }

    pub(crate) fn workspace_path(&self) -> &Path {
        &self.workspace_path
    }

    pub(crate) fn save(&self, request: &ApiRequest) -> Result<PathBuf> {
        let path = self.path_for_id(&request.id)?;
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("请求路径缺少父目录"))?;
        fs::create_dir_all(parent)
            .with_context(|| format!("无法创建请求目录: {}", parent.display()))?;

        let temporary = temporary_path(&path)?;
        if let Err(error) = fs::write(&temporary, serialize_request(request)) {
            let _ = fs::remove_file(&temporary);
            return Err(error)
                .with_context(|| format!("无法写入请求临时文件: {}", temporary.display()));
        }

        #[cfg(windows)]
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("无法替换请求文件: {}", path.display()))?;
        }

        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            return Err(error).with_context(|| format!("无法保存请求文件: {}", path.display()));
        }
        tracing::debug!(path = %path.display(), "保存请求文件");
        Ok(path)
    }

    pub(crate) fn delete(&self, request_id: &str) -> Result<()> {
        let path = self.path_for_id(request_id)?;
        fs::remove_file(&path).with_context(|| format!("无法删除请求文件: {}", path.display()))?;
        tracing::debug!(path = %path.display(), "删除请求文件");
        Ok(())
    }

    fn path_for_id(&self, request_id: &str) -> Result<PathBuf> {
        let relative = request_id
            .strip_prefix("requests/")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("请求没有可写入的源文件"))?;
        let relative_path = Path::new(relative);
        if relative_path.is_absolute()
            || relative_path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            bail!("请求源路径无效")
        }
        Ok(self.workspace_path.join("requests").join(relative_path))
    }
}

fn temporary_path(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("请求路径缺少父目录"))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("请求路径缺少文件名"))?;
    let mut temporary_name = OsString::from(".");
    temporary_name.push(file_name);
    temporary_name.push(".postui.tmp");
    Ok(parent.join(temporary_name))
}

fn serialize_request(request: &ApiRequest) -> String {
    let mut metadata = vec![format!("# @name {}", request.name)];
    if !request.description.is_empty() {
        metadata.push(format!("# @description {}", request.description));
    }
    let mut command = vec![format!(
        "curl --request {} --url {}",
        request.method,
        shell_quote(&request.url)
    )];
    for header in &request.headers {
        command.push(format!(
            "  --header {}",
            shell_quote(&format!("{}: {}", header.name, header.value))
        ));
    }
    for part in &request.query_parts {
        let option = match part {
            BodyPart::Raw(_) => "--data-raw",
            BodyPart::UrlEncoded(_) => "--data-urlencode",
        };
        command.push(format!(
            "  {option} {}",
            shell_quote(crate::template::body_part_value(part))
        ));
    }
    if !request.query_parts.is_empty() {
        command.push("  --get".to_string());
    }
    for part in &request.body_parts {
        let option = match part {
            BodyPart::Raw(_) => "--data-raw",
            BodyPart::UrlEncoded(_) => "--data-urlencode",
        };
        command.push(format!(
            "  {option} {}",
            shell_quote(crate::template::body_part_value(part))
        ));
    }
    for field in &request.form {
        command.push(format!(
            "  --form-string {}",
            shell_quote(&format!("{}={}", field.name, field.value))
        ));
    }
    for file in &request.files {
        let mut value = format!("{}=@{}", file.field, file.path);
        if let Some(content_type) = &file.content_type {
            value.push_str(&format!(";type={content_type}"));
        }
        if let Some(filename) = &file.filename {
            value.push_str(&format!(";filename={filename}"));
        }
        command.push(format!("  --form {}", shell_quote(&value)));
    }
    format!("{}\n{}\n", metadata.join("\n"), command.join(" \\\n"))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
