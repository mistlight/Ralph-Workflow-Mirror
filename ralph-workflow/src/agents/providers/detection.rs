//! Provider Detection
//!
//! Logic for detecting provider type from model flag strings.

use super::types::OpenCodeProviderType;

/// Strip a leading model-flag prefix and return the raw `provider/model` string.
///
/// Supports common `OpenCode` CLI forms:
/// - `-m provider/model`
/// - `--model provider/model`
/// - `-m=provider/model`
/// - `--model=provider/model`
#[must_use]
pub fn strip_model_flag_prefix(model_flag: &str) -> &str {
    let s = model_flag.trim();

    // Equals-style flags first.
    if let Some(rest) = s.strip_prefix("-m=") {
        return rest.trim();
    }
    if let Some(rest) = s.strip_prefix("--model=") {
        return rest.trim();
    }

    // Space-style flags.
    if s == "-m" {
        return "";
    }
    if let Some(rest) = s.strip_prefix("-m ") {
        return rest.trim();
    }
    if let Some(rest) = s.strip_prefix("-m\t") {
        return rest.trim();
    }
    if s == "--model" {
        return "";
    }
    if let Some(rest) = s.strip_prefix("--model ") {
        return rest.trim();
    }
    if let Some(rest) = s.strip_prefix("--model\t") {
        return rest.trim();
    }

    s
}

impl OpenCodeProviderType {
    /// Detect provider type from a model flag string.
    #[must_use]
    pub fn from_model_flag(model_flag: &str) -> Self {
        let model = strip_model_flag_prefix(model_flag);
        let prefix = model.split('/').next().unwrap_or("").to_lowercase();

        match prefix.as_str() {
            // OpenCode Gateway
            "opencode" => Self::OpenCodeZen,

            // Chinese AI Providers
            "zai" | "zhipuai" => Self::ZaiDirect,
            "zai-coding" | "zai-plan" => Self::ZaiCodingPlan,
            "moonshot" | "kimi" => Self::Moonshot,
            "minimax" => Self::MiniMax,

            // Major Cloud Providers
            "anthropic" => Self::Anthropic,
            "openai" => Self::OpenAI,
            "google" => Self::Google,
            "google-vertex" | "vertex" => Self::GoogleVertex,
            "amazon-bedrock" | "bedrock" => Self::AmazonBedrock,
            "azure-openai" | "azure" => Self::AzureOpenAI,
            "copilot" | "github-copilot" => Self::GithubCopilot,

            // Fast Inference Providers
            "groq" => Self::Groq,
            "together" => Self::Together,
            "fireworks" => Self::Fireworks,
            "cerebras" => Self::Cerebras,
            "sambanova" => Self::SambaNova,
            "deep-infra" | "deepinfra" => Self::DeepInfra,

            // Gateway / Aggregator Providers
            "openrouter" => Self::OpenRouter,
            "cloudflare" | "cf" => Self::Cloudflare,
            "vercel" => Self::Vercel,
            "helicone" => Self::Helicone,
            "zenmux" => Self::ZenMux,

            // Specialized Providers
            "deepseek" => Self::DeepSeek,
            "xai" | "grok" => Self::Xai,
            "mistral" => Self::Mistral,
            "cohere" => Self::Cohere,
            "perplexity" => Self::Perplexity,
            "ai21" => Self::AI21,
            "venice-ai" | "venice" => Self::VeniceAI,

            // Open-Source Model Providers
            "huggingface" | "hf" => Self::HuggingFace,
            "replicate" => Self::Replicate,

            // Cloud Platform Providers
            "baseten" => Self::Baseten,
            "cortecs" => Self::Cortecs,
            "scaleway" => Self::Scaleway,
            "ovhcloud" | "ovh" => Self::OVHcloud,
            "io-net" | "ionet" => Self::IONet,
            "nebius" => Self::Nebius,

            // Enterprise / Industry Providers
            "sap-ai-core" | "sap" => Self::SapAICore,
            "azure-cognitive-services" | "azure-cognitive" => Self::AzureCognitiveServices,

            // Local / Self-Hosted Providers
            "ollama" => Self::Ollama,
            "lmstudio" => Self::LMStudio,
            "ollama-cloud" => Self::OllamaCloud,
            "llama.cpp" | "llamacpp" | "llama-cpp" => Self::LlamaCpp,

            _ => Self::Custom,
        }
    }
}
