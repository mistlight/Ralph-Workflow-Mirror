//! Provider Metadata
//!
//! Display names, prefixes, and authentication commands for each provider.

use super::types::OpenCodeProviderType;

impl OpenCodeProviderType {
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
