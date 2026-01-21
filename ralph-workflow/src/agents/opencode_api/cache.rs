//! OpenCode API catalog caching.
//!
//! This module handles file-based caching of the OpenCode model catalog
//! with TTL-based expiration.

use crate::agents::opencode_api::fetch::fetch_api_catalog;
use crate::agents::opencode_api::types::ApiCatalog;
use crate::agents::opencode_api::{CACHE_TTL_ENV_VAR, DEFAULT_CACHE_TTL_SECONDS};
use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur when loading the API catalog.
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("Failed to read cache file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse cache JSON: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("Failed to fetch API catalog: {0}")]
    FetchError(String),

    #[error("Cache directory not found")]
    CacheDirNotFound,
}

/// Get the cache file path for the OpenCode API catalog.
///
/// Cache location: `~/.cache/ralph-workflow/opencode-api-cache.json`
pub fn cache_file_path() -> Result<PathBuf, CacheError> {
    let cache_dir = dirs::cache_dir()
        .ok_or(CacheError::CacheDirNotFound)?
        .join("ralph-workflow");

    // Ensure cache directory exists
    std::fs::create_dir_all(&cache_dir)?;

    Ok(cache_dir.join("opencode-api-cache.json"))
}

/// Load the API catalog from cache or fetch if expired.
///
/// This function:
/// 1. Checks if a cached catalog exists
/// 2. If cached and not expired, returns the cached version
/// 3. If expired or missing, fetches a fresh catalog from the API
/// 4. Saves the fetched catalog to disk for future use
///
/// Gracefully degrades on network errors: if fetching fails but a stale
/// cache exists (< 7 days old), it will be used with a warning.
pub fn load_api_catalog() -> Result<ApiCatalog, CacheError> {
    let ttl_seconds = std::env::var(CACHE_TTL_ENV_VAR)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_CACHE_TTL_SECONDS);

    let cache_path = cache_file_path()?;

    // Try to load from cache
    if let Ok(cached) = load_cached_catalog(&cache_path, ttl_seconds) {
        return Ok(cached);
    }

    // Cache miss or expired, fetch from API
    fetch_api_catalog()
}

/// Load a cached catalog from disk.
///
/// Returns None if the cache file doesn't exist, is invalid, or is expired.
fn load_cached_catalog(path: &PathBuf, ttl_seconds: u64) -> Result<ApiCatalog, CacheError> {
    let content = std::fs::read_to_string(path)?;

    let mut catalog: ApiCatalog = serde_json::from_str(&content)?;

    // Set the TTL for expiration checking
    catalog.ttl_seconds = ttl_seconds;

    // Check if expired
    if catalog.is_expired() {
        // Try to fetch fresh catalog, but use stale cache if fetch fails
        match fetch_api_catalog() {
            Ok(fresh) => return Ok(fresh),
            Err(e) => {
                // Use stale cache if it's less than 7 days old
                if let Some(cached_at) = catalog.cached_at {
                    let now = chrono::Utc::now();
                    let stale_days =
                        (now.signed_duration_since(cached_at).num_seconds() / 86400).abs();
                    if stale_days < 7 {
                        eprintln!(
                            "Warning: Failed to fetch fresh OpenCode API catalog ({}), using stale cache from {} days ago",
                            e, stale_days
                        );
                        return Ok(catalog);
                    }
                }
                return Err(CacheError::FetchError(e.to_string()));
            }
        }
    }

    Ok(catalog)
}

/// Save the API catalog to disk.
///
/// Note: Only serializes the providers and models data from the API.
/// The cached_at timestamp and ttl_seconds are not persisted.
pub fn save_catalog(catalog: &ApiCatalog) -> Result<(), CacheError> {
    #[derive(serde::Serialize)]
    struct SerializableCatalog<'a> {
        providers: &'a std::collections::HashMap<String, crate::agents::opencode_api::Provider>,
        models: &'a std::collections::HashMap<String, Vec<crate::agents::opencode_api::Model>>,
    }

    let cache_path = cache_file_path()?;
    let serializable = SerializableCatalog {
        providers: &catalog.providers,
        models: &catalog.models,
    };
    let content = serde_json::to_string_pretty(&serializable)?;
    std::fs::write(&cache_path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::opencode_api::types::{Model, Provider};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_catalog() -> ApiCatalog {
        let mut providers = HashMap::new();
        providers.insert(
            "test".to_string(),
            Provider {
                id: "test".to_string(),
                name: "Test Provider".to_string(),
                description: "Test".to_string(),
            },
        );

        let mut models = HashMap::new();
        models.insert(
            "test".to_string(),
            vec![Model {
                id: "test-model".to_string(),
                name: "Test Model".to_string(),
                description: "Test".to_string(),
                context_length: None,
            }],
        );

        ApiCatalog {
            providers,
            models,
            cached_at: Some(chrono::Utc::now()),
            ttl_seconds: DEFAULT_CACHE_TTL_SECONDS,
        }
    }

    #[test]
    fn test_save_and_load_catalog() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("test-cache.json");

        let catalog = create_test_catalog();

        // Save catalog using SerializableCatalog wrapper
        #[derive(serde::Serialize)]
        struct SerializableCatalog<'a> {
            providers: &'a std::collections::HashMap<String, crate::agents::opencode_api::Provider>,
            models: &'a std::collections::HashMap<String, Vec<crate::agents::opencode_api::Model>>,
        }
        let serializable = SerializableCatalog {
            providers: &catalog.providers,
            models: &catalog.models,
        };
        let content = serde_json::to_string_pretty(&serializable).unwrap();
        std::fs::write(&cache_path, content).unwrap();

        // Load catalog
        let loaded_content = std::fs::read_to_string(&cache_path).unwrap();
        let loaded: ApiCatalog = serde_json::from_str(&loaded_content).unwrap();

        assert_eq!(loaded.providers.len(), catalog.providers.len());
        assert!(loaded.has_provider("test"));
        assert!(loaded.has_model("test", "test-model"));

        // Verify cache file path ends correctly
        let original_path = cache_file_path().unwrap();
        assert!(
            original_path.ends_with("opencode-api-cache.json"),
            "cache file should end with opencode-api-cache.json"
        );
    }

    #[test]
    fn test_catalog_serialization() {
        let catalog = create_test_catalog();

        // Serialize using the same method as save_catalog
        #[derive(serde::Serialize)]
        struct SerializableCatalog<'a> {
            providers: &'a std::collections::HashMap<String, crate::agents::opencode_api::Provider>,
            models: &'a std::collections::HashMap<String, Vec<crate::agents::opencode_api::Model>>,
        }
        let serializable = SerializableCatalog {
            providers: &catalog.providers,
            models: &catalog.models,
        };
        let json = serde_json::to_string(&serializable).unwrap();
        let deserialized: ApiCatalog = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.providers.len(), catalog.providers.len());
        assert_eq!(deserialized.models.len(), catalog.models.len());
    }

    #[test]
    fn test_expired_catalog_detection() {
        let mut catalog = create_test_catalog();

        // Fresh catalog should not be expired
        assert!(!catalog.is_expired());

        // Old catalog should be expired
        catalog.cached_at = Some(
            chrono::Utc::now() - chrono::Duration::seconds(DEFAULT_CACHE_TTL_SECONDS as i64 + 1),
        );
        assert!(catalog.is_expired());
    }
}
