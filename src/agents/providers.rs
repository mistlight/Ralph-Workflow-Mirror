//! OpenCode provider types and authentication helpers.
//!
//! This module handles provider detection from model flags and provides
//! authentication guidance for the 75+ providers supported by OpenCode.

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

/// OpenCode provider type extracted from model flag.
///
/// OpenCode supports 75+ providers through the AI SDK and Models.dev.
/// This enum explicitly lists all major provider categories for clear
/// identification and provider-specific authentication guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpenCodeProviderType {
    // ==================== OpenCode Gateway ====================
    /// OpenCode Zen - gateway for multiple providers via unified API.
    OpenCodeZen,

    // ==================== Chinese AI Providers ====================
    /// Z.AI Direct - direct access to Z.AI/ZhipuAI models.
    ZaiDirect,
    /// Z.AI Coding Plan - Z.AI coding subscription plan.
    ZaiCodingPlan,
    /// Moonshot AI / Kimi.
    Moonshot,
    /// MiniMax AI.
    MiniMax,

    // ==================== Major Cloud Providers ====================
    /// Anthropic (Claude models).
    Anthropic,
    /// OpenAI (GPT models).
    OpenAI,
    /// Google AI (Gemini models).
    Google,
    /// Google Vertex AI (enterprise Gemini).
    GoogleVertex,
    /// Amazon Bedrock.
    AmazonBedrock,
    /// Azure OpenAI Service.
    AzureOpenAI,
    /// GitHub Copilot (Chat).
    GithubCopilot,

    // ==================== Fast Inference Providers ====================
    /// Groq (fast inference).
    Groq,
    /// Together AI.
    Together,
    /// Fireworks AI.
    Fireworks,
    /// Cerebras.
    Cerebras,
    /// SambaNova.
    SambaNova,
    /// DeepInfra.
    DeepInfra,

    // ==================== Gateway / Aggregator Providers ====================
    /// OpenRouter (model aggregator).
    OpenRouter,
    /// Cloudflare Workers AI.
    Cloudflare,
    /// Vercel AI Gateway.
    Vercel,
    /// Helicone (observability + gateway).
    Helicone,
    /// ZenMux (AI gateway).
    ZenMux,

    // ==================== Specialized Providers ====================
    /// DeepSeek (coding-focused).
    DeepSeek,
    /// xAI (Grok).
    Xai,
    /// Mistral AI.
    Mistral,
    /// Cohere.
    Cohere,
    /// Perplexity (search-augmented).
    Perplexity,
    /// AI21 Labs.
    AI21,
    /// Venice AI.
    VeniceAI,

    // ==================== Open-Source Model Providers ====================
    /// HuggingFace Inference.
    HuggingFace,
    /// Replicate.
    Replicate,

    // ==================== Cloud Platform Providers ====================
    /// Baseten.
    Baseten,
    /// Cortecs.
    Cortecs,
    /// Scaleway.
    Scaleway,
    /// OVHcloud.
    OVHcloud,
    /// IO.NET.
    IONet,
    /// Nebius.
    Nebius,

    // ==================== Enterprise / Industry Providers ====================
    /// SAP AI Core.
    SapAICore,
    /// Azure Cognitive Services.
    AzureCognitiveServices,

    // ==================== Local / Self-Hosted Providers ====================
    /// Ollama (local).
    Ollama,
    /// LM Studio (local).
    LMStudio,
    /// Ollama Cloud (remote Ollama).
    OllamaCloud,
    /// llama.cpp (local).
    LlamaCpp,

    // ==================== Catch-all ====================
    /// Custom/unknown provider (OpenCode may still support it).
    Custom,
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

    /// Get the display name for this provider.
    pub fn name(&self) -> &'static str {
        match self {
            OpenCodeProviderType::OpenCodeZen => "OpenCode Zen",
            OpenCodeProviderType::ZaiDirect => "Z.AI Direct",
            OpenCodeProviderType::ZaiCodingPlan => "Z.AI Coding Plan",
            OpenCodeProviderType::Moonshot => "Moonshot AI",
            OpenCodeProviderType::MiniMax => "MiniMax",
            OpenCodeProviderType::Anthropic => "Anthropic",
            OpenCodeProviderType::OpenAI => "OpenAI",
            OpenCodeProviderType::Google => "Google AI",
            OpenCodeProviderType::GoogleVertex => "Google Vertex AI",
            OpenCodeProviderType::AmazonBedrock => "Amazon Bedrock",
            OpenCodeProviderType::AzureOpenAI => "Azure OpenAI",
            OpenCodeProviderType::GithubCopilot => "GitHub Copilot",
            OpenCodeProviderType::Groq => "Groq",
            OpenCodeProviderType::Together => "Together AI",
            OpenCodeProviderType::Fireworks => "Fireworks AI",
            OpenCodeProviderType::Cerebras => "Cerebras",
            OpenCodeProviderType::SambaNova => "SambaNova",
            OpenCodeProviderType::DeepInfra => "DeepInfra",
            OpenCodeProviderType::OpenRouter => "OpenRouter",
            OpenCodeProviderType::Cloudflare => "Cloudflare Workers AI",
            OpenCodeProviderType::Vercel => "Vercel AI Gateway",
            OpenCodeProviderType::Helicone => "Helicone",
            OpenCodeProviderType::ZenMux => "ZenMux",
            OpenCodeProviderType::DeepSeek => "DeepSeek",
            OpenCodeProviderType::Xai => "xAI",
            OpenCodeProviderType::Mistral => "Mistral AI",
            OpenCodeProviderType::Cohere => "Cohere",
            OpenCodeProviderType::Perplexity => "Perplexity",
            OpenCodeProviderType::AI21 => "AI21 Labs",
            OpenCodeProviderType::VeniceAI => "Venice AI",
            OpenCodeProviderType::HuggingFace => "HuggingFace",
            OpenCodeProviderType::Replicate => "Replicate",
            OpenCodeProviderType::Baseten => "Baseten",
            OpenCodeProviderType::Cortecs => "Cortecs",
            OpenCodeProviderType::Scaleway => "Scaleway",
            OpenCodeProviderType::OVHcloud => "OVHcloud",
            OpenCodeProviderType::IONet => "IO.NET",
            OpenCodeProviderType::Nebius => "Nebius",
            OpenCodeProviderType::SapAICore => "SAP AI Core",
            OpenCodeProviderType::AzureCognitiveServices => "Azure Cognitive Services",
            OpenCodeProviderType::Ollama => "Ollama",
            OpenCodeProviderType::LMStudio => "LM Studio",
            OpenCodeProviderType::OllamaCloud => "Ollama Cloud",
            OpenCodeProviderType::LlamaCpp => "llama.cpp",
            OpenCodeProviderType::Custom => "Custom",
        }
    }

    /// Get authentication command/instructions for this provider.
    pub fn auth_command(&self) -> &'static str {
        match self {
            OpenCodeProviderType::OpenCodeZen => {
                "Run: opencode auth login -> select 'OpenCode Zen'"
            }
            OpenCodeProviderType::ZaiDirect => {
                "Run: opencode auth login -> select 'Z.AI' or 'Z.AI Coding Plan'"
            }
            OpenCodeProviderType::ZaiCodingPlan => {
                "Run: opencode auth login -> select 'Z.AI Coding Plan'"
            }
            OpenCodeProviderType::Moonshot => {
                "Set MOONSHOT_API_KEY or run: opencode auth login -> select 'Moonshot'"
            }
            OpenCodeProviderType::MiniMax => {
                "Set MINIMAX_API_KEY or run: opencode auth login -> select 'MiniMax'"
            }
            OpenCodeProviderType::Anthropic => "Set ANTHROPIC_API_KEY environment variable",
            OpenCodeProviderType::OpenAI => "Set OPENAI_API_KEY environment variable",
            OpenCodeProviderType::Google => "Set GOOGLE_AI_API_KEY environment variable",
            OpenCodeProviderType::GoogleVertex => {
                "Configure GCP credentials: gcloud auth application-default login"
            }
            OpenCodeProviderType::AmazonBedrock => {
                "Configure AWS credentials: aws configure or set AWS_ACCESS_KEY_ID"
            }
            OpenCodeProviderType::AzureOpenAI => {
                "Set AZURE_OPENAI_API_KEY and AZURE_OPENAI_ENDPOINT"
            }
            OpenCodeProviderType::GithubCopilot => {
                "Run: gh auth login (requires GitHub Copilot subscription)"
            }
            OpenCodeProviderType::Groq => "Set GROQ_API_KEY environment variable",
            OpenCodeProviderType::Together => "Set TOGETHER_API_KEY environment variable",
            OpenCodeProviderType::Fireworks => "Set FIREWORKS_API_KEY environment variable",
            OpenCodeProviderType::Cerebras => "Set CEREBRAS_API_KEY environment variable",
            OpenCodeProviderType::SambaNova => "Set SAMBANOVA_API_KEY environment variable",
            OpenCodeProviderType::DeepInfra => "Set DEEPINFRA_API_KEY environment variable",
            OpenCodeProviderType::OpenRouter => "Set OPENROUTER_API_KEY environment variable",
            OpenCodeProviderType::Cloudflare => {
                "Set CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN"
            }
            OpenCodeProviderType::Vercel => "Set VERCEL_API_KEY environment variable",
            OpenCodeProviderType::Helicone => "Set HELICONE_API_KEY environment variable",
            OpenCodeProviderType::ZenMux => "Set ZENMUX_API_KEY environment variable",
            OpenCodeProviderType::DeepSeek => "Set DEEPSEEK_API_KEY environment variable",
            OpenCodeProviderType::Xai => "Set XAI_API_KEY environment variable",
            OpenCodeProviderType::Mistral => "Set MISTRAL_API_KEY environment variable",
            OpenCodeProviderType::Cohere => "Set COHERE_API_KEY environment variable",
            OpenCodeProviderType::Perplexity => "Set PERPLEXITY_API_KEY environment variable",
            OpenCodeProviderType::AI21 => "Set AI21_API_KEY environment variable",
            OpenCodeProviderType::VeniceAI => "Set VENICE_API_KEY environment variable",
            OpenCodeProviderType::HuggingFace => "Set HUGGINGFACE_API_KEY environment variable",
            OpenCodeProviderType::Replicate => "Set REPLICATE_API_TOKEN environment variable",
            OpenCodeProviderType::Baseten => "Set BASETEN_API_KEY environment variable",
            OpenCodeProviderType::Cortecs => "Set CORTECS_API_KEY environment variable",
            OpenCodeProviderType::Scaleway => "Set SCALEWAY_API_KEY environment variable",
            OpenCodeProviderType::OVHcloud => "Set OVHCLOUD_API_KEY environment variable",
            OpenCodeProviderType::IONet => "Set IONET_API_KEY environment variable",
            OpenCodeProviderType::Nebius => "Set NEBIUS_API_KEY environment variable",
            OpenCodeProviderType::SapAICore => {
                "Set AICORE_CLIENT_ID, AICORE_CLIENT_SECRET, AICORE_AUTH_URL, and AICORE_API_BASE"
            }
            OpenCodeProviderType::AzureCognitiveServices => {
                "Set AZURE_COGNITIVE_SERVICES_KEY and AZURE_COGNITIVE_SERVICES_ENDPOINT"
            }
            OpenCodeProviderType::Ollama => "Ollama runs locally - no API key needed",
            OpenCodeProviderType::LMStudio => "LM Studio runs locally - no API key needed",
            OpenCodeProviderType::OllamaCloud => "Set OLLAMA_CLOUD_API_KEY environment variable",
            OpenCodeProviderType::LlamaCpp => "llama.cpp runs locally - no API key needed",
            OpenCodeProviderType::Custom => {
                "Check provider documentation or run: opencode /connect"
            }
        }
    }

    /// Get the model prefix for this provider.
    pub fn prefix(&self) -> &'static str {
        match self {
            OpenCodeProviderType::OpenCodeZen => "opencode/",
            OpenCodeProviderType::ZaiDirect => "zai/",
            OpenCodeProviderType::ZaiCodingPlan => "zai-coding/",
            OpenCodeProviderType::Moonshot => "moonshot/",
            OpenCodeProviderType::MiniMax => "minimax/",
            OpenCodeProviderType::Anthropic => "anthropic/",
            OpenCodeProviderType::OpenAI => "openai/",
            OpenCodeProviderType::Google => "google/",
            OpenCodeProviderType::GoogleVertex => "google-vertex/",
            OpenCodeProviderType::AmazonBedrock => "amazon-bedrock/",
            OpenCodeProviderType::AzureOpenAI => "azure-openai/",
            OpenCodeProviderType::GithubCopilot => "copilot/",
            OpenCodeProviderType::Groq => "groq/",
            OpenCodeProviderType::Together => "together/",
            OpenCodeProviderType::Fireworks => "fireworks/",
            OpenCodeProviderType::Cerebras => "cerebras/",
            OpenCodeProviderType::SambaNova => "sambanova/",
            OpenCodeProviderType::DeepInfra => "deep-infra/",
            OpenCodeProviderType::OpenRouter => "openrouter/",
            OpenCodeProviderType::Cloudflare => "cloudflare/",
            OpenCodeProviderType::Vercel => "vercel/",
            OpenCodeProviderType::Helicone => "helicone/",
            OpenCodeProviderType::ZenMux => "zenmux/",
            OpenCodeProviderType::DeepSeek => "deepseek/",
            OpenCodeProviderType::Xai => "xai/",
            OpenCodeProviderType::Mistral => "mistral/",
            OpenCodeProviderType::Cohere => "cohere/",
            OpenCodeProviderType::Perplexity => "perplexity/",
            OpenCodeProviderType::AI21 => "ai21/",
            OpenCodeProviderType::VeniceAI => "venice-ai/",
            OpenCodeProviderType::HuggingFace => "huggingface/",
            OpenCodeProviderType::Replicate => "replicate/",
            OpenCodeProviderType::Baseten => "baseten/",
            OpenCodeProviderType::Cortecs => "cortecs/",
            OpenCodeProviderType::Scaleway => "scaleway/",
            OpenCodeProviderType::OVHcloud => "ovhcloud/",
            OpenCodeProviderType::IONet => "io-net/",
            OpenCodeProviderType::Nebius => "nebius/",
            OpenCodeProviderType::SapAICore => "sap-ai-core/",
            OpenCodeProviderType::AzureCognitiveServices => "azure-cognitive-services/",
            OpenCodeProviderType::Ollama => "ollama/",
            OpenCodeProviderType::LMStudio => "lmstudio/",
            OpenCodeProviderType::OllamaCloud => "ollama-cloud/",
            OpenCodeProviderType::LlamaCpp => "llama.cpp/",
            OpenCodeProviderType::Custom => "any other provider/*",
        }
    }

    /// Get example models for this provider type.
    pub fn example_models(&self) -> &'static [&'static str] {
        match self {
            OpenCodeProviderType::OpenCodeZen => {
                &["opencode/glm-4.7-free", "opencode/claude-sonnet-4"]
            }
            OpenCodeProviderType::ZaiDirect => &["zai/glm-4.7", "zai/glm-4.5", "zhipuai/glm-4.7"],
            OpenCodeProviderType::ZaiCodingPlan => &["zai/glm-4.7", "zai/glm-4.5"],
            OpenCodeProviderType::Moonshot => &["moonshot/kimi-k2", "moonshot/moonshot-v1-128k"],
            OpenCodeProviderType::MiniMax => &["minimax/abab6.5-chat", "minimax/abab5.5-chat"],
            OpenCodeProviderType::Anthropic => {
                &["anthropic/claude-sonnet-4", "anthropic/claude-opus-4"]
            }
            OpenCodeProviderType::OpenAI => &["openai/gpt-4o", "openai/gpt-4-turbo", "openai/o1"],
            OpenCodeProviderType::Google => &["google/gemini-2.0-flash", "google/gemini-1.5-pro"],
            OpenCodeProviderType::GoogleVertex => &[
                "google-vertex/gemini-2.0-flash",
                "google-vertex/gemini-1.5-pro",
            ],
            OpenCodeProviderType::AmazonBedrock => &[
                "amazon-bedrock/anthropic.claude-3-5-sonnet",
                "amazon-bedrock/meta.llama3-70b-instruct",
            ],
            OpenCodeProviderType::AzureOpenAI => {
                &["azure-openai/gpt-4o", "azure-openai/gpt-4-turbo"]
            }
            OpenCodeProviderType::GithubCopilot => &[
                "copilot/gpt-4o",
                "copilot/claude-3.5-sonnet",
                "copilot/gemini-2.0-flash",
            ],
            OpenCodeProviderType::Groq => &["groq/llama-3.3-70b-versatile", "groq/mixtral-8x7b"],
            OpenCodeProviderType::Together => &[
                "together/meta-llama/Llama-3-70b-chat-hf",
                "together/mistralai/Mixtral-8x7B",
            ],
            OpenCodeProviderType::Fireworks => {
                &["fireworks/accounts/fireworks/models/llama-v3p1-70b-instruct"]
            }
            OpenCodeProviderType::Cerebras => &["cerebras/llama3.3-70b"],
            OpenCodeProviderType::SambaNova => &["sambanova/Meta-Llama-3.3-70B-Instruct"],
            OpenCodeProviderType::DeepInfra => &[
                "deep-infra/meta-llama/Llama-3.3-70B-Instruct",
                "deep-infra/Qwen/Qwen2.5-Coder-32B",
            ],
            OpenCodeProviderType::OpenRouter => &[
                "openrouter/anthropic/claude-3.5-sonnet",
                "openrouter/openai/gpt-4o",
            ],
            OpenCodeProviderType::Cloudflare => &[
                "cloudflare/@cf/meta/llama-3-8b-instruct",
                "cloudflare/@cf/mistral/mistral-7b",
            ],
            OpenCodeProviderType::Vercel => &["vercel/gpt-4o", "vercel/claude-3.5-sonnet"],
            OpenCodeProviderType::Helicone => &["helicone/gpt-4o"],
            OpenCodeProviderType::ZenMux => &["zenmux/gpt-4o", "zenmux/claude-3.5-sonnet"],
            OpenCodeProviderType::DeepSeek => {
                &["deepseek/deepseek-chat", "deepseek/deepseek-coder"]
            }
            OpenCodeProviderType::Xai => &["xai/grok-2", "xai/grok-beta"],
            OpenCodeProviderType::Mistral => &["mistral/mistral-large", "mistral/codestral"],
            OpenCodeProviderType::Cohere => &["cohere/command-r-plus", "cohere/command-r"],
            OpenCodeProviderType::Perplexity => &["perplexity/sonar-pro", "perplexity/sonar"],
            OpenCodeProviderType::AI21 => &["ai21/jamba-1.5-large", "ai21/jamba-1.5-mini"],
            OpenCodeProviderType::VeniceAI => &["venice-ai/llama-3-70b"],
            OpenCodeProviderType::HuggingFace => &[
                "huggingface/meta-llama/Llama-3.3-70B-Instruct",
                "huggingface/Qwen/Qwen2.5-Coder-32B",
            ],
            OpenCodeProviderType::Replicate => &["replicate/meta/llama-3-70b-instruct"],
            OpenCodeProviderType::Baseten => &["baseten/llama-3-70b"],
            OpenCodeProviderType::Cortecs => &["cortecs/llama-3-70b"],
            OpenCodeProviderType::Scaleway => &["scaleway/llama-3-70b"],
            OpenCodeProviderType::OVHcloud => &["ovhcloud/llama-3-70b"],
            OpenCodeProviderType::IONet => &["io-net/llama-3-70b"],
            OpenCodeProviderType::Nebius => &["nebius/llama-3-70b"],
            OpenCodeProviderType::SapAICore => {
                &["sap-ai-core/gpt-4o", "sap-ai-core/claude-3.5-sonnet"]
            }
            OpenCodeProviderType::AzureCognitiveServices => &["azure-cognitive-services/gpt-4o"],
            OpenCodeProviderType::Ollama => {
                &["ollama/llama3", "ollama/codellama", "ollama/mistral"]
            }
            OpenCodeProviderType::LMStudio => &["lmstudio/local-model"],
            OpenCodeProviderType::OllamaCloud => &["ollama-cloud/llama3", "ollama-cloud/codellama"],
            OpenCodeProviderType::LlamaCpp => &["llama.cpp/local-model"],
            OpenCodeProviderType::Custom => &[],
        }
    }

    /// Check if this provider requires special cloud configuration.
    pub fn requires_cloud_config(&self) -> bool {
        matches!(
            self,
            OpenCodeProviderType::GoogleVertex
                | OpenCodeProviderType::AmazonBedrock
                | OpenCodeProviderType::AzureOpenAI
                | OpenCodeProviderType::SapAICore
                | OpenCodeProviderType::AzureCognitiveServices
        )
    }

    /// Check if this is a local provider (no API key needed).
    pub fn is_local(&self) -> bool {
        matches!(
            self,
            OpenCodeProviderType::Ollama
                | OpenCodeProviderType::LMStudio
                | OpenCodeProviderType::LlamaCpp
        )
    }
}

/// Validate a model flag and return provider-specific warnings if any issues detected.
///
/// Returns a vector of warning messages (empty if no issues).
pub fn validate_model_flag(model_flag: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    let model = strip_model_flag_prefix(model_flag);
    if model.is_empty() {
        return warnings;
    }

    // Ensure model flag has provider prefix
    if !model.contains('/') {
        warnings.push(format!(
            "Model '{}' has no provider prefix. Expected format: 'provider/model' (e.g., 'opencode/glm-4.7-free')",
            model
        ));
        return warnings;
    }

    let provider_type = OpenCodeProviderType::from_model_flag(model);

    // Warn about Z.AI vs Zen confusion
    if provider_type == OpenCodeProviderType::OpenCodeZen && model.to_lowercase().contains("zai") {
        warnings.push(
            "Model flag uses 'opencode/' prefix but contains 'zai'. \
             For Z.AI Direct access, use 'zai/' prefix instead."
                .to_string(),
        );
    }

    // Warn about providers requiring cloud configuration
    if provider_type.requires_cloud_config() {
        warnings.push(format!(
            "{} provider requires cloud configuration. {}",
            provider_type.name(),
            provider_type.auth_command()
        ));
    }

    // Warn about custom/unknown providers
    if provider_type == OpenCodeProviderType::Custom {
        let prefix = model.split('/').next().unwrap_or("");
        warnings.push(format!(
            "Unknown provider prefix '{}'. This may work if OpenCode supports it. \
             Run 'ralph --list-providers' to see known providers.",
            prefix
        ));
    }

    // Info about local providers
    if provider_type.is_local() {
        warnings.push(format!(
            "{} is a local provider. {}",
            provider_type.name(),
            provider_type.auth_command()
        ));
    }

    warnings
}

/// Get provider-specific authentication failure advice based on model flag.
pub fn auth_failure_advice(model_flag: Option<&str>) -> String {
    match model_flag {
        Some(flag) => {
            let model = strip_model_flag_prefix(flag);
            let prefix = model.split('/').next().unwrap_or("").to_lowercase();
            if matches!(prefix.as_str(), "zai" | "zhipuai") {
                return "Authentication failed for Z.AI provider. Run: opencode auth login -> select 'Z.AI' or 'Z.AI Coding Plan'".to_string();
            }
            let provider = OpenCodeProviderType::from_model_flag(flag);
            format!(
                "Authentication failed for {} provider. Run: {}",
                provider.name(),
                provider.auth_command()
            )
        }
        None => "Check API key or run 'opencode auth login' to authenticate.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_model_flag_prefix() {
        assert_eq!(
            strip_model_flag_prefix("-m opencode/glm-4.7-free"),
            "opencode/glm-4.7-free"
        );
        assert_eq!(
            strip_model_flag_prefix("--model opencode/glm-4.7-free"),
            "opencode/glm-4.7-free"
        );
        assert_eq!(
            strip_model_flag_prefix("-m=opencode/glm-4.7-free"),
            "opencode/glm-4.7-free"
        );
        assert_eq!(
            strip_model_flag_prefix("opencode/glm-4.7-free"),
            "opencode/glm-4.7-free"
        );
    }

    #[test]
    fn test_provider_type_from_model_flag() {
        assert_eq!(
            OpenCodeProviderType::from_model_flag("opencode/glm-4.7-free"),
            OpenCodeProviderType::OpenCodeZen
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("zai/glm-4.7"),
            OpenCodeProviderType::ZaiDirect
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("anthropic/claude-sonnet-4"),
            OpenCodeProviderType::Anthropic
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("unknown/model"),
            OpenCodeProviderType::Custom
        );
    }

    #[test]
    fn test_validate_model_flag() {
        // Valid flag
        let warnings = validate_model_flag("opencode/glm-4.7-free");
        assert!(warnings.is_empty());

        // Missing prefix
        let warnings = validate_model_flag("glm-4.7-free");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("no provider prefix"));

        // Unknown provider
        let warnings = validate_model_flag("unknown/model");
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_auth_failure_advice() {
        let advice = auth_failure_advice(Some("anthropic/claude-sonnet-4"));
        assert!(advice.contains("Anthropic"));
        assert!(advice.contains("ANTHROPIC_API_KEY"));

        let advice = auth_failure_advice(None);
        assert!(advice.contains("opencode auth login"));
    }
}
