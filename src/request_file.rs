use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub struct RequestFileStore {
    workspace_path: PathBuf,
}

impl RequestFileStore {
    pub fn new(workspace_path: PathBuf) -> Self {
        Self { workspace_path }
    }

    pub fn workspace_path(&self) -> &Path {
        &self.workspace_path
    }

    pub fn delete(&self, request_id: &str) -> Result<()> {
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
