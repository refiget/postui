use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::RequestConfig;

const CACHE_FORMAT_VERSION: u32 = 5;
const CACHE_DIRECTORY_NAME: &str = "cache";
const WORKSPACE_CACHE_KEY: &str = "workspace-config-v5";

#[derive(Debug, Serialize, Deserialize)]
struct CacheEnvelope {
    format_version: u32,
    source_digest: String,
    config: RequestConfig,
}

pub(crate) fn load(workspace_path: &Path, fingerprint: &blake3::Hash) -> Option<RequestConfig> {
    let cache_directory = cache_directory(workspace_path);
    let bytes = match cacache::read_sync(&cache_directory, WORKSPACE_CACHE_KEY) {
        Ok(bytes) => bytes,
        Err(cacache::Error::EntryNotFound(_, _)) => {
            tracing::debug!(path = %cache_directory.display(), "工作区缓存不存在");
            return None;
        }
        Err(error) => {
            tracing::debug!(
                path = %cache_directory.display(),
                error = ?error,
                "工作区缓存读取失败"
            );
            return None;
        }
    };

    let cache: CacheEnvelope = match serde_json::from_slice(&bytes) {
        Ok(cache) => cache,
        Err(error) => {
            tracing::debug!(
                path = %cache_directory.display(),
                error = ?error,
                "工作区缓存格式无效"
            );
            return None;
        }
    };

    if cache.format_version != CACHE_FORMAT_VERSION {
        tracing::debug!(
            path = %cache_directory.display(),
            cached_version = cache.format_version,
            expected_version = CACHE_FORMAT_VERSION,
            "工作区缓存版本不匹配"
        );
        return None;
    }

    let expected_digest = source_digest(fingerprint);
    if cache.source_digest != expected_digest {
        tracing::debug!(
            path = %cache_directory.display(),
            cached_digest = %cache.source_digest,
            source_digest = %expected_digest,
            "工作区缓存已过期"
        );
        return None;
    }

    tracing::debug!(
        path = %cache_directory.display(),
        bytes = bytes.len(),
        request_count = cache.config.requests.len(),
        variable_count = cache.config.variables.len(),
        "命中工作区缓存"
    );
    Some(cache.config)
}

pub(crate) fn store(
    workspace_path: &Path,
    fingerprint: &blake3::Hash,
    config: &RequestConfig,
) -> Result<()> {
    let cache_directory = cache_directory(workspace_path);
    let payload = serde_json::to_vec(&CacheEnvelope {
        format_version: CACHE_FORMAT_VERSION,
        source_digest: source_digest(fingerprint),
        config: config.clone(),
    })
    .context("序列化工作区缓存失败")?;

    cacache::write_sync(&cache_directory, WORKSPACE_CACHE_KEY, &payload)
        .with_context(|| format!("写入工作区缓存失败: {}", cache_directory.display()))?;
    tracing::debug!(
        path = %cache_directory.display(),
        bytes = payload.len(),
        request_count = config.requests.len(),
        variable_count = config.variables.len(),
        "写入工作区缓存"
    );
    Ok(())
}

fn cache_directory(workspace_path: &Path) -> PathBuf {
    workspace_path.join(CACHE_DIRECTORY_NAME)
}

fn source_digest(fingerprint: &blake3::Hash) -> String {
    fingerprint.to_hex().to_string()
}
