use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::RequestConfig;

const CACHE_FORMAT_VERSION: u32 = 6;
const CACHE_FILE_NAME: &str = "requests.cache.json";

#[derive(Debug, Serialize, Deserialize)]
struct CacheEnvelope {
    format_version: u32,
    collection_path: String,
    fingerprint: u64,
    config: RequestConfig,
}

pub(crate) fn load(collection_path: &Path, fingerprint: &[u8]) -> Option<RequestConfig> {
    let cache_path = cache_path(collection_path);
    let bytes = match fs::read(&cache_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(path = %cache_path.display(), "请求集合缓存不存在");
            return None;
        }
        Err(error) => {
            tracing::debug!(
                path = %cache_path.display(),
                error = ?error,
                "请求集合缓存读取失败"
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
                "请求集合缓存格式无效"
            );
            return None;
        }
    };

    let expected_path = collection_identity(collection_path);
    if cache.format_version != CACHE_FORMAT_VERSION {
        tracing::debug!(
            path = %cache_path.display(),
            cached_version = cache.format_version,
            expected_version = CACHE_FORMAT_VERSION,
            "请求集合缓存版本不匹配"
        );
        return None;
    }
    if cache.collection_path != expected_path {
        tracing::debug!(
            path = %cache_path.display(),
            cached_collection_path = %cache.collection_path,
            collection_path = %expected_path,
            "请求集合缓存目录不匹配"
        );
        return None;
    }

    let expected_fingerprint = fingerprint_hash(fingerprint);
    if cache.fingerprint != expected_fingerprint {
        tracing::debug!(
            path = %cache_path.display(),
            cached_fingerprint = cache.fingerprint,
            fingerprint = expected_fingerprint,
            "请求集合缓存已过期"
        );
        return None;
    }

    tracing::debug!(
        path = %cache_path.display(),
        bytes = bytes.len(),
        request_count = cache.config.requests.len(),
        variable_count = cache.config.variables.len(),
        "命中请求集合缓存"
    );
    Some(cache.config)
}

pub(crate) fn store(
    collection_path: &Path,
    fingerprint: &[u8],
    config: &RequestConfig,
) -> Result<()> {
    let cache_path = cache_path(collection_path);
    let payload = serde_json::to_vec(&CacheEnvelope {
        format_version: CACHE_FORMAT_VERSION,
        collection_path: collection_identity(collection_path),
        fingerprint: fingerprint_hash(fingerprint),
        config: config.clone(),
    })
    .context("序列化请求集合缓存失败")?;

    let temporary_path = temporary_path(&cache_path);
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)
            .with_context(|| {
                format!("创建请求集合缓存临时文件失败: {}", temporary_path.display())
            })?;
        file.write_all(&payload)
            .with_context(|| format!("写入请求集合缓存失败: {}", temporary_path.display()))?;
        file.flush()
            .with_context(|| format!("刷新请求集合缓存失败: {}", temporary_path.display()))?;
        file.sync_all()
            .with_context(|| format!("同步请求集合缓存失败: {}", temporary_path.display()))?;
        drop(file);

        replace_cache(&temporary_path, &cache_path)
            .with_context(|| format!("替换请求集合缓存失败: {}", cache_path.display()))
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
        "写入请求集合缓存"
    );
    Ok(())
}

fn cache_path(collection_path: &Path) -> PathBuf {
    collection_path.join(CACHE_FILE_NAME)
}

fn temporary_path(cache_path: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = cache_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(CACHE_FILE_NAME);
    cache_path.with_file_name(format!("{name}.tmp-{}-{timestamp}", std::process::id()))
}

fn replace_cache(temporary_path: &Path, cache_path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    if cache_path.exists() {
        fs::remove_file(cache_path)?;
    }

    fs::rename(temporary_path, cache_path)
}

fn collection_identity(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn fingerprint_hash(fingerprint: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    for byte in fingerprint {
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
    use std::collections::{BTreeMap, BTreeSet};

    fn test_config() -> RequestConfig {
        RequestConfig {
            name: "cache-test".to_string(),
            file_directory: PathBuf::from("../test_files"),
            download_directory: PathBuf::from("../temp"),
            headers: BTreeMap::new(),
            variables: BTreeMap::from([(
                "host".to_string(),
                VariableDefinition {
                    default: Some(Value::String("example.test".to_string())),
                },
            )]),
            editable_variables: BTreeSet::from(["host".to_string()]),
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
                extracts: Vec::new(),
            }],
            timeout_seconds: 30,
        }
    }

    #[test]
    fn uses_config_directory_for_cache_file() {
        let collection_path = Path::new(".postui");
        assert_eq!(
            cache_path(collection_path),
            PathBuf::from(".postui/requests.cache.json")
        );
    }

    #[test]
    fn loads_only_when_fingerprint_matches() {
        let directory = std::env::temp_dir().join(format!(
            "postui-cache-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("应当可以创建缓存测试目录");
        let collection_path = directory.join(".postui");
        fs::create_dir_all(&collection_path).expect("应当可以创建请求集合目录");
        let fingerprint = b"config.yaml\0requests/health.http\0curl https://example.test/health";

        let config = test_config();
        store(&collection_path, fingerprint, &config).expect("应当可以写入缓存");
        let cached = load(&collection_path, fingerprint).expect("相同配置应当命中缓存");
        assert_eq!(cached.name, config.name);
        assert_eq!(cached.requests[0].url, config.requests[0].url);
        assert!(load(&collection_path, b"changed").is_none());

        fs::remove_dir_all(directory).expect("应当可以清理缓存测试目录");
    }
}
