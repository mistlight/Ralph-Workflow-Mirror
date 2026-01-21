//! OpenCode API catalog data structures.
//!
//! This module defines the types for parsing and representing the OpenCode
//! model catalog from https://models.dev/api.json.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::agents::opencode_api::DEFAULT_CACHE_TTL_SECONDS;

/// OpenCode API catalog containing all available providers and models.
#[derive(Debug, Clone, Deserialize)]
#[serde(from = "ApiCatalogRaw")]
pub struct ApiCatalog {
    /// All providers supported by OpenCode.
    pub providers: HashMap<String, Provider>,
    /// All models supported by OpenCode, indexed by provider name.
    pub models: HashMap<String, Vec<Model>>,
    /// When this catalog was cached (for TTL tracking).
    pub cached_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Time-to-live in seconds for this catalog.
    pub ttl_seconds: u64,
}

/// Raw representation for deserializing from JSON.
#[derive(Debug, Clone, Deserialize)]
struct ApiCatalogRaw {
    #[serde(default)]
    providers: HashMap<String, Provider>,
    #[serde(default)]
    models: HashMap<String, Vec<Model>>,
}

impl From<ApiCatalogRaw> for ApiCatalog {
    fn from(raw: ApiCatalogRaw) -> Self {
        Self {
            providers: raw.providers,
            models: raw.models,
            cached_at: None,
            ttl_seconds: DEFAULT_CACHE_TTL_SECONDS,
        }
    }
}

impl ApiCatalog {
    /// Check if the catalog is expired based on its cached_at timestamp and TTL.
    pub fn is_expired(&self) -> bool {
        if let Some(cached_at) = self.cached_at {
            let now = chrono::Utc::now();
            let elapsed = now.signed_duration_since(cached_at);
            elapsed.num_seconds() as u64 > self.ttl_seconds
        } else {
            // No cache timestamp means it should be refreshed
            true
        }
    }

    /// Check if a provider exists in the catalog.
    pub fn has_provider(&self, provider: &str) -> bool {
        self.providers.contains_key(provider)
    }

    /// Check if a specific model exists for a provider.
    pub fn has_model(&self, provider: &str, model: &str) -> bool {
        self.models
            .get(provider)
            .is_some_and(|models| models.iter().any(|m| m.id == model))
    }

    /// Get all model IDs for a provider.
    pub fn get_model_ids(&self, provider: &str) -> Vec<String> {
        self.models
            .get(provider)
            .map(|models| models.iter().map(|m| m.id.clone()).collect())
            .unwrap_or_default()
    }

    /// Get all provider names.
    pub fn provider_names(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

/// Helper methods for ApiCatalog (test-only).
#[cfg(test)]
impl ApiCatalog {
    /// Get a model by provider and model ID.
    pub fn get_model(&self, provider: &str, model_id: &str) -> Option<&Model> {
        self.models
            .get(provider)
            .and_then(|models| models.iter().find(|m| m.id == model_id))
    }

    /// Find providers that start with the given prefix.
    pub fn find_providers_by_prefix(&self, prefix: &str) -> Vec<String> {
        let prefix_lower = prefix.to_lowercase();
        self.provider_names()
            .into_iter()
            .filter(|p| p.to_lowercase().starts_with(&prefix_lower))
            .collect()
    }

    /// Find models for a provider that start with the given prefix.
    pub fn find_models_by_prefix(&self, provider: &str, prefix: &str) -> Vec<String> {
        let prefix_lower = prefix.to_lowercase();
        self.get_model_ids(provider)
            .into_iter()
            .filter(|m| m.to_lowercase().starts_with(&prefix_lower))
            .collect()
    }
}

/// A provider supported by OpenCode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    /// Unique provider identifier.
    pub id: String,
    /// Display name for the provider.
    pub name: String,
    /// Optional description of the provider.
    #[serde(default)]
    pub description: String,
}

/// A model available from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// Unique model identifier (e.g., "claude-sonnet-4-5").
    pub id: String,
    /// Display name for the model.
    pub name: String,
    /// Optional description of the model.
    #[serde(default)]
    pub description: String,
    /// Optional context window size.
    #[serde(default)]
    pub context_length: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_catalog() -> ApiCatalog {
        let mut providers = HashMap::new();
        providers.insert(
            "anthropic".to_string(),
            Provider {
                id: "anthropic".to_string(),
                name: "Anthropic".to_string(),
                description: "Anthropic Claude models".to_string(),
            },
        );
        providers.insert(
            "openai".to_string(),
            Provider {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                description: "OpenAI GPT models".to_string(),
            },
        );

        let mut models = HashMap::new();
        models.insert(
            "anthropic".to_string(),
            vec![
                Model {
                    id: "claude-sonnet-4-5".to_string(),
                    name: "Claude Sonnet 4.5".to_string(),
                    description: "Latest Claude Sonnet".to_string(),
                    context_length: Some(200000),
                },
                Model {
                    id: "claude-opus-4".to_string(),
                    name: "Claude Opus 4".to_string(),
                    description: "Most capable Claude".to_string(),
                    context_length: Some(200000),
                },
            ],
        );
        models.insert(
            "openai".to_string(),
            vec![Model {
                id: "gpt-4".to_string(),
                name: "GPT-4".to_string(),
                description: "OpenAI's GPT-4".to_string(),
                context_length: Some(8192),
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
    fn test_catalog_not_expired_when_fresh() {
        let catalog = create_test_catalog();
        assert!(!catalog.is_expired());
    }

    #[test]
    fn test_catalog_expired_when_old() {
        let mut catalog = create_test_catalog();
        catalog.cached_at = Some(
            chrono::Utc::now() - chrono::Duration::seconds(DEFAULT_CACHE_TTL_SECONDS as i64 + 1),
        );
        assert!(catalog.is_expired());
    }

    #[test]
    fn test_catalog_expired_without_timestamp() {
        let mut catalog = create_test_catalog();
        catalog.cached_at = None;
        assert!(catalog.is_expired());
    }

    #[test]
    fn test_has_provider() {
        let catalog = create_test_catalog();
        assert!(catalog.has_provider("anthropic"));
        assert!(catalog.has_provider("openai"));
        assert!(!catalog.has_provider("nonexistent"));
    }

    #[test]
    fn test_has_model() {
        let catalog = create_test_catalog();
        assert!(catalog.has_model("anthropic", "claude-sonnet-4-5"));
        assert!(catalog.has_model("anthropic", "claude-opus-4"));
        assert!(catalog.has_model("openai", "gpt-4"));
        assert!(!catalog.has_model("anthropic", "gpt-4"));
        assert!(!catalog.has_model("nonexistent", "any-model"));
    }

    #[test]
    fn test_get_model_ids() {
        let catalog = create_test_catalog();
        let anthropic_models = catalog.get_model_ids("anthropic");
        assert_eq!(anthropic_models.len(), 2);
        assert!(anthropic_models.contains(&"claude-sonnet-4-5".to_string()));
        assert!(anthropic_models.contains(&"claude-opus-4".to_string()));

        let openai_models = catalog.get_model_ids("openai");
        assert_eq!(openai_models.len(), 1);
        assert!(openai_models.contains(&"gpt-4".to_string()));

        let nonexistent_models = catalog.get_model_ids("nonexistent");
        assert!(nonexistent_models.is_empty());
    }

    #[test]
    fn test_get_model() {
        let catalog = create_test_catalog();
        let model = catalog.get_model("anthropic", "claude-sonnet-4-5");
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "claude-sonnet-4-5");

        assert!(catalog.get_model("nonexistent", "any").is_none());
        assert!(catalog.get_model("anthropic", "nonexistent").is_none());
    }

    #[test]
    fn test_provider_names() {
        let catalog = create_test_catalog();
        let names = catalog.provider_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"anthropic".to_string()));
        assert!(names.contains(&"openai".to_string()));
    }

    #[test]
    fn test_find_providers_by_prefix() {
        let catalog = create_test_catalog();
        let results = catalog.find_providers_by_prefix("anth");
        assert_eq!(results, vec!["anthropic".to_string()]);

        let results = catalog.find_providers_by_prefix("a");
        assert_eq!(results, vec!["anthropic".to_string()]);

        let results = catalog.find_providers_by_prefix("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_find_models_by_prefix() {
        let catalog = create_test_catalog();
        let results = catalog.find_models_by_prefix("anthropic", "claude-son");
        assert_eq!(results, vec!["claude-sonnet-4-5".to_string()]);

        let results = catalog.find_models_by_prefix("anthropic", "claude");
        assert_eq!(results.len(), 2);

        let results = catalog.find_models_by_prefix("nonexistent", "any");
        assert!(results.is_empty());
    }
}
