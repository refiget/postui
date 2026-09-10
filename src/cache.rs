use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::RequestConfig;

const CACHE_FORMAT_VERSION: u32 = 1;
const CACHE_FILE_SUFFIX: &str = ".cache.json";

#[derive(Debug, Serialize, Deserialize)]
struct CacheEnvelope {
    format_version: u32,
    source_path: String,
    source_hash: u64,
    config: RequestConfig,
}

pub(crate) fn load(source_path: &Path, source: &[u8]) -> Option<RequestConfig> {
    let cache_path = cache_path(source_path);
    let bytes = match fs::read(&cache_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(path = %cache_path.display(), "请求配置缓存不存在");
            return None;
        }
        Err(error) => {
            tracing::debug!(
                path = %cache_path.display(),
                error = ?error,
                "请求配置缓存读取失败"
            );
            return None;
        }
    };

    let cache: CacheEnvelope = match serde_json::from_slice(&bytes) {
        Ok(cache) => cache,
        Err(error) => {
            tracing::debug!(
                path = %cache_path.display(),
                error = ?error,
                "请求配置缓存格式无效"
            );
            return None;
        }
    };

    let expected_path = source_identity(source_path);
    if cache.format_version != CACHE_FORMAT_VERSION {
        tracing::debug!(
            path = %cache_path.display(),
            cached_version = cache.format_version,
            expected_version = CACHE_FORMAT_VERSION,
            "请求配置缓存版本不匹配"
        );
        return None;
    }
    if cache.source_path != expected_path {
        tracing::debug!(
            path = %cache_path.display(),
            cached_source_path = %cache.source_path,
            source_path = %expected_path,
            "请求配置缓存来源不匹配"
        );
        return None;
    }

    let expected_hash = source_hash(source);
    if cache.source_hash != expected_hash {
        tracing::debug!(
            path = %cache_path.display(),
            cached_source_hash = cache.source_hash,
            source_hash = expected_hash,
            "请求配置缓存已过期"
        );
        return None;
    }

    tracing::debug!(
        path = %cache_path.display(),
        bytes = bytes.len(),
        request_count = cache.config.requests.len(),
        variable_count = cache.config.variables.len(),
        "命中请求配置缓存"
    );
    Some(cache.config)
}

pub(crate) fn store(source_path: &Path, source: &[u8], config: &RequestConfig) -> Result<()> {
    let cache_path = cache_path(source_path);
    let payload = serde_json::to_vec(&CacheEnvelope {
        format_version: CACHE_FORMAT_VERSION,
        source_path: source_identity(source_path),
        source_hash: source_hash(source),
        config: config.clone(),
    })
    .context("序列化请求配置缓存失败")?;

    let temporary_path = temporary_path(&cache_path);
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)
            .with_context(|| {
                format!("创建请求配置缓存临时文件失败: {}", temporary_path.display())
            })?;
        file.write_all(&payload)
            .with_context(|| format!("写入请求配置缓存失败: {}", temporary_path.display()))?;
        file.flush()
            .with_context(|| format!("刷新请求配置缓存失败: {}", temporary_path.display()))?;
        file.sync_all()
            .with_context(|| format!("同步请求配置缓存失败: {}", temporary_path.display()))?;
        drop(file);

        replace_cache(&temporary_path, &cache_path)
            .with_context(|| format!("替换请求配置缓存失败: {}", cache_path.display()))
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }

    result?;
    tracing::debug!(
        path = %cache_path.display(),
        bytes = payload.len(),
        request_count = config.requests.len(),
        variable_count = config.variables.len(),
        "写入请求配置缓存"
    );
    Ok(())
}

fn cache_path(source_path: &Path) -> PathBuf {
    let parent = source_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let stem = source_path
        .file_stem()
        .map(|value| value.to_string_lossy())
        .filter(|value| !value.is_empty())
        .unwrap_or(std::borrow::Cow::Borrowed("requests"));
    parent.join(format!("{stem}{CACHE_FILE_SUFFIX}"))
}

fn temporary_path(cache_path: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = cache_path
        .file_name()
        .map(|value| value.to_string_lossy())
        .unwrap_or_else(|| std::borrow::Cow::Borrowed("requests.cache.json"));
    cache_path.with_file_name(format!("{name}.tmp-{}-{timestamp}", std::process::id()))
}

fn replace_cache(temporary_path: &Path, cache_path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    if cache_path.exists() {
        fs::remove_file(cache_path)?;
    }

    fs::rename(temporary_path, cache_path)
}

fn source_identity(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn source_hash(source: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    for byte in source {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ApiRequest, VariableDefinition};
    use serde_json::Value;
    use std::collections::BTreeMap;

    fn test_config() -> RequestConfig {
        RequestConfig {
            name: "cache-test".to_string(),
            file_directory: PathBuf::from("files"),
            download_directory: PathBuf::from("tmp"),
            variables: BTreeMap::from([(
                "host".to_string(),
                VariableDefinition {
                    default: Some(Value::String("example.test".to_string())),
                },
            )]),
            requests: vec![ApiRequest {
                id: "health".to_string(),
                name: "Health".to_string(),
                method: "GET".to_string(),
                url: "https://example.test/health".to_string(),
                timeout_seconds: 30,
                description: String::new(),
                headers: BTreeMap::new(),
                body_parts: Vec::new(),
                query_parts: Vec::new(),
                form: BTreeMap::new(),
                files: Vec::new(),
                download: None,
                extracts: Vec::new(),
            }],
            timeout_seconds: 30,
        }
    }

    #[test]
    fn uses_config_directory_for_cache_file() {
        assert_eq!(
            cache_path(Path::new(".postui/requests.yaml")),
            PathBuf::from(".postui/requests.cache.json")
        );
    }

    #[test]
    fn loads_only_when_source_matches() {
        let directory = std::env::temp_dir().join(format!(
            "postui-cache-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("应当可以创建缓存测试目录");
        let source_path = directory.join("requests.yaml");
        let source = b"requests: []";
        fs::write(&source_path, source).expect("应当可以创建缓存测试配置");

        let config = test_config();
        store(&source_path, source, &config).expect("应当可以写入缓存");
        let cached = load(&source_path, source).expect("相同配置应当命中缓存");
        assert_eq!(cached.name, config.name);
        assert_eq!(cached.requests[0].url, config.requests[0].url);
        assert!(load(&source_path, b"requests: [changed]").is_none());

        fs::remove_dir_all(directory).expect("应当可以清理缓存测试目录");
    }
}
