//! Provider Detection
//!
//! Logic for detecting provider type from model flag strings.

use super::types::OpenCodeProviderType;

/// Strip a leading model-flag prefix and return the raw `provider/model` string.
///
/// Supports common OpenCode CLI forms:
/// - `-m provider/model`
/// - `--model provider/model`
/// - `-m=provider/model`
/// - `--model=provider/model`
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
    pub fn from_model_flag(model_flag: &str) -> Self {
        let model = strip_model_flag_prefix(model_flag);
        let prefix = model.split('/').next().unwrap_or("").to_lowercase();

        match prefix.as_str() {
            // OpenCode Gateway
            "opencode" => OpenCodeProviderType::OpenCodeZen,

            // Chinese AI Providers
            "zai" | "zhipuai" => OpenCodeProviderType::ZaiDirect,
            "zai-coding" | "zai-plan" => OpenCodeProviderType::ZaiCodingPlan,
            "moonshot" | "kimi" => OpenCodeProviderType::Moonshot,
            "minimax" => OpenCodeProviderType::MiniMax,

            // Major Cloud Providers
            "anthropic" => OpenCodeProviderType::Anthropic,
            "openai" => OpenCodeProviderType::OpenAI,
            "google" => OpenCodeProviderType::Google,
            "google-vertex" | "vertex" => OpenCodeProviderType::GoogleVertex,
            "amazon-bedrock" | "bedrock" => OpenCodeProviderType::AmazonBedrock,
            "azure-openai" | "azure" => OpenCodeProviderType::AzureOpenAI,
            "copilot" | "github-copilot" => OpenCodeProviderType::GithubCopilot,

            // Fast Inference Providers
            "groq" => OpenCodeProviderType::Groq,
            "together" => OpenCodeProviderType::Together,
            "fireworks" => OpenCodeProviderType::Fireworks,
            "cerebras" => OpenCodeProviderType::Cerebras,
            "sambanova" => OpenCodeProviderType::SambaNova,
            "deep-infra" | "deepinfra" => OpenCodeProviderType::DeepInfra,

            // Gateway / Aggregator Providers
            "openrouter" => OpenCodeProviderType::OpenRouter,
            "cloudflare" | "cf" => OpenCodeProviderType::Cloudflare,
            "vercel" => OpenCodeProviderType::Vercel,
            "helicone" => OpenCodeProviderType::Helicone,
            "zenmux" => OpenCodeProviderType::ZenMux,

            // Specialized Providers
            "deepseek" => OpenCodeProviderType::DeepSeek,
            "xai" | "grok" => OpenCodeProviderType::Xai,
            "mistral" => OpenCodeProviderType::Mistral,
            "cohere" => OpenCodeProviderType::Cohere,
            "perplexity" => OpenCodeProviderType::Perplexity,
            "ai21" => OpenCodeProviderType::AI21,
            "venice-ai" | "venice" => OpenCodeProviderType::VeniceAI,

            // Open-Source Model Providers
            "huggingface" | "hf" => OpenCodeProviderType::HuggingFace,
            "replicate" => OpenCodeProviderType::Replicate,

            // Cloud Platform Providers
            "baseten" => OpenCodeProviderType::Baseten,
            "cortecs" => OpenCodeProviderType::Cortecs,
            "scaleway" => OpenCodeProviderType::Scaleway,
            "ovhcloud" | "ovh" => OpenCodeProviderType::OVHcloud,
            "io-net" | "ionet" => OpenCodeProviderType::IONet,
            "nebius" => OpenCodeProviderType::Nebius,

            // Enterprise / Industry Providers
            "sap-ai-core" | "sap" => OpenCodeProviderType::SapAICore,
            "azure-cognitive-services" | "azure-cognitive" => {
                OpenCodeProviderType::AzureCognitiveServices
            }

            // Local / Self-Hosted Providers
            "ollama" => OpenCodeProviderType::Ollama,
            "lmstudio" => OpenCodeProviderType::LMStudio,
            "ollama-cloud" => OpenCodeProviderType::OllamaCloud,
            "llama.cpp" | "llamacpp" | "llama-cpp" => OpenCodeProviderType::LlamaCpp,

            _ => OpenCodeProviderType::Custom,
        }
    }
}
