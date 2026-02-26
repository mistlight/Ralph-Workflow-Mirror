//! Provider Metadata
//!
//! Display names, prefixes, and authentication commands for each provider.

use super::types::OpenCodeProviderType;

impl OpenCodeProviderType {
    /// Get the display name for this provider.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OpenCodeZen => "OpenCode Zen",
            Self::ZaiDirect => "Z.AI Direct",
            Self::ZaiCodingPlan => "Z.AI Coding Plan",
            Self::Moonshot => "Moonshot AI",
            Self::MiniMax => "MiniMax",
            Self::Anthropic => "Anthropic",
            Self::OpenAI => "OpenAI",
            Self::Google => "Google AI",
            Self::GoogleVertex => "Google Vertex AI",
            Self::AmazonBedrock => "Amazon Bedrock",
            Self::AzureOpenAI => "Azure OpenAI",
            Self::GithubCopilot => "GitHub Copilot",
            Self::Groq => "Groq",
            Self::Together => "Together AI",
            Self::Fireworks => "Fireworks AI",
            Self::Cerebras => "Cerebras",
            Self::SambaNova => "SambaNova",
            Self::DeepInfra => "DeepInfra",
            Self::OpenRouter => "OpenRouter",
            Self::Cloudflare => "Cloudflare Workers AI",
            Self::Vercel => "Vercel AI Gateway",
            Self::Helicone => "Helicone",
            Self::ZenMux => "ZenMux",
            Self::DeepSeek => "DeepSeek",
            Self::Xai => "xAI",
            Self::Mistral => "Mistral AI",
            Self::Cohere => "Cohere",
            Self::Perplexity => "Perplexity",
            Self::AI21 => "AI21 Labs",
            Self::VeniceAI => "Venice AI",
            Self::HuggingFace => "HuggingFace",
            Self::Replicate => "Replicate",
            Self::Baseten => "Baseten",
            Self::Cortecs => "Cortecs",
            Self::Scaleway => "Scaleway",
            Self::OVHcloud => "OVHcloud",
            Self::IONet => "IO.NET",
            Self::Nebius => "Nebius",
            Self::SapAICore => "SAP AI Core",
            Self::AzureCognitiveServices => "Azure Cognitive Services",
            Self::Ollama => "Ollama",
            Self::LMStudio => "LM Studio",
            Self::OllamaCloud => "Ollama Cloud",
            Self::LlamaCpp => "llama.cpp",
            Self::Custom => "Custom",
        }
    }

    /// Get authentication command/instructions for this provider.
    #[must_use]
    pub const fn auth_command(self) -> &'static str {
        match self {
            Self::OpenCodeZen => "Run: opencode auth login -> select 'OpenCode Zen'",
            Self::ZaiDirect => "Run: opencode auth login -> select 'Z.AI' or 'Z.AI Coding Plan'",
            Self::ZaiCodingPlan => "Run: opencode auth login -> select 'Z.AI Coding Plan'",
            Self::Moonshot => {
                "Set MOONSHOT_API_KEY or run: opencode auth login -> select 'Moonshot'"
            }
            Self::MiniMax => "Set MINIMAX_API_KEY or run: opencode auth login -> select 'MiniMax'",
            Self::Anthropic => "Set ANTHROPIC_API_KEY environment variable",
            Self::OpenAI => "Set OPENAI_API_KEY environment variable",
            Self::Google => "Set GOOGLE_AI_API_KEY environment variable",
            Self::GoogleVertex => {
                "Configure GCP credentials: gcloud auth application-default login"
            }
            Self::AmazonBedrock => {
                "Configure AWS credentials: aws configure or set AWS_ACCESS_KEY_ID"
            }
            Self::AzureOpenAI => "Set AZURE_OPENAI_API_KEY and AZURE_OPENAI_ENDPOINT",
            Self::GithubCopilot => "Run: gh auth login (requires GitHub Copilot subscription)",
            Self::Groq => "Set GROQ_API_KEY environment variable",
            Self::Together => "Set TOGETHER_API_KEY environment variable",
            Self::Fireworks => "Set FIREWORKS_API_KEY environment variable",
            Self::Cerebras => "Set CEREBRAS_API_KEY environment variable",
            Self::SambaNova => "Set SAMBANOVA_API_KEY environment variable",
            Self::DeepInfra => "Set DEEPINFRA_API_KEY environment variable",
            Self::OpenRouter => "Set OPENROUTER_API_KEY environment variable",
            Self::Cloudflare => "Set CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN",
            Self::Vercel => "Set VERCEL_API_KEY environment variable",
            Self::Helicone => "Set HELICONE_API_KEY environment variable",
            Self::ZenMux => "Set ZENMUX_API_KEY environment variable",
            Self::DeepSeek => "Set DEEPSEEK_API_KEY environment variable",
            Self::Xai => "Set XAI_API_KEY environment variable",
            Self::Mistral => "Set MISTRAL_API_KEY environment variable",
            Self::Cohere => "Set COHERE_API_KEY environment variable",
            Self::Perplexity => "Set PERPLEXITY_API_KEY environment variable",
            Self::AI21 => "Set AI21_API_KEY environment variable",
            Self::VeniceAI => "Set VENICE_API_KEY environment variable",
            Self::HuggingFace => "Set HUGGINGFACE_API_KEY environment variable",
            Self::Replicate => "Set REPLICATE_API_TOKEN environment variable",
            Self::Baseten => "Set BASETEN_API_KEY environment variable",
            Self::Cortecs => "Set CORTECS_API_KEY environment variable",
            Self::Scaleway => "Set SCALEWAY_API_KEY environment variable",
            Self::OVHcloud => "Set OVHCLOUD_API_KEY environment variable",
            Self::IONet => "Set IONET_API_KEY environment variable",
            Self::Nebius => "Set NEBIUS_API_KEY environment variable",
            Self::SapAICore => {
                "Set AICORE_CLIENT_ID, AICORE_CLIENT_SECRET, AICORE_AUTH_URL, and AICORE_API_BASE"
            }
            Self::AzureCognitiveServices => {
                "Set AZURE_COGNITIVE_SERVICES_KEY and AZURE_COGNITIVE_SERVICES_ENDPOINT"
            }
            Self::Ollama => "Ollama runs locally - no API key needed",
            Self::LMStudio => "LM Studio runs locally - no API key needed",
            Self::OllamaCloud => "Set OLLAMA_CLOUD_API_KEY environment variable",
            Self::LlamaCpp => "llama.cpp runs locally - no API key needed",
            Self::Custom => "Check provider documentation or run: opencode /connect",
        }
    }

    /// Get the model prefix for this provider.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::OpenCodeZen => "opencode/",
            Self::ZaiDirect => "zai/",
            Self::ZaiCodingPlan => "zai-coding/",
            Self::Moonshot => "moonshot/",
            Self::MiniMax => "minimax/",
            Self::Anthropic => "anthropic/",
            Self::OpenAI => "openai/",
            Self::Google => "google/",
            Self::GoogleVertex => "google-vertex/",
            Self::AmazonBedrock => "amazon-bedrock/",
            Self::AzureOpenAI => "azure-openai/",
            Self::GithubCopilot => "copilot/",
            Self::Groq => "groq/",
            Self::Together => "together/",
            Self::Fireworks => "fireworks/",
            Self::Cerebras => "cerebras/",
            Self::SambaNova => "sambanova/",
            Self::DeepInfra => "deep-infra/",
            Self::OpenRouter => "openrouter/",
            Self::Cloudflare => "cloudflare/",
            Self::Vercel => "vercel/",
            Self::Helicone => "helicone/",
            Self::ZenMux => "zenmux/",
            Self::DeepSeek => "deepseek/",
            Self::Xai => "xai/",
            Self::Mistral => "mistral/",
            Self::Cohere => "cohere/",
            Self::Perplexity => "perplexity/",
            Self::AI21 => "ai21/",
            Self::VeniceAI => "venice-ai/",
            Self::HuggingFace => "huggingface/",
            Self::Replicate => "replicate/",
            Self::Baseten => "baseten/",
            Self::Cortecs => "cortecs/",
            Self::Scaleway => "scaleway/",
            Self::OVHcloud => "ovhcloud/",
            Self::IONet => "io-net/",
            Self::Nebius => "nebius/",
            Self::SapAICore => "sap-ai-core/",
            Self::AzureCognitiveServices => "azure-cognitive-services/",
            Self::Ollama => "ollama/",
            Self::LMStudio => "lmstudio/",
            Self::OllamaCloud => "ollama-cloud/",
            Self::LlamaCpp => "llama.cpp/",
            Self::Custom => "any other provider/*",
        }
    }

    /// Check if this provider requires special cloud configuration.
    #[must_use]
    pub const fn requires_cloud(self) -> bool {
        matches!(
            self,
            Self::GoogleVertex
                | Self::AmazonBedrock
                | Self::AzureOpenAI
                | Self::SapAICore
                | Self::AzureCognitiveServices
        )
    }

    /// Check if this is a local provider (no API key needed).
    #[must_use]
    pub const fn is_local(self) -> bool {
        matches!(self, Self::Ollama | Self::LMStudio | Self::LlamaCpp)
    }
}
