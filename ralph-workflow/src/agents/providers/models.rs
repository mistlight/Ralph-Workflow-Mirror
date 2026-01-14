//! Example Models
//!
//! Example model identifiers for each provider type.

use super::types::OpenCodeProviderType;

impl OpenCodeProviderType {
    /// Get example models for this provider type.
    pub const fn example_models(self) -> &'static [&'static str] {
        match self {
            Self::OpenCodeZen => &["opencode/glm-4.7-free", "opencode/claude-sonnet-4"],
            Self::ZaiDirect => &["zai/glm-4.7", "zai/glm-4.5", "zhipuai/glm-4.7"],
            Self::ZaiCodingPlan => &["zai/glm-4.7", "zai/glm-4.5"],
            Self::Moonshot => &["moonshot/kimi-k2", "moonshot/moonshot-v1-128k"],
            Self::MiniMax => &["minimax/abab6.5-chat", "minimax/abab5.5-chat"],
            Self::Anthropic => &["anthropic/claude-sonnet-4", "anthropic/claude-opus-4"],
            Self::OpenAI => &["openai/gpt-4o", "openai/gpt-4-turbo", "openai/o1"],
            Self::Google => &["google/gemini-2.0-flash", "google/gemini-1.5-pro"],
            Self::GoogleVertex => &[
                "google-vertex/gemini-2.0-flash",
                "google-vertex/gemini-1.5-pro",
            ],
            Self::AmazonBedrock => &[
                "amazon-bedrock/anthropic.claude-3-5-sonnet",
                "amazon-bedrock/meta.llama3-70b-instruct",
            ],
            Self::AzureOpenAI => &["azure-openai/gpt-4o", "azure-openai/gpt-4-turbo"],
            Self::GithubCopilot => &[
                "copilot/gpt-4o",
                "copilot/claude-3.5-sonnet",
                "copilot/gemini-2.0-flash",
            ],
            Self::Groq => &["groq/llama-3.3-70b-versatile", "groq/mixtral-8x7b"],
            Self::Together => &[
                "together/meta-llama/Llama-3-70b-chat-hf",
                "together/mistralai/Mixtral-8x7B",
            ],
            Self::Fireworks => &["fireworks/accounts/fireworks/models/llama-v3p1-70b-instruct"],
            Self::Cerebras => &["cerebras/llama3.3-70b"],
            Self::SambaNova => &["sambanova/Meta-Llama-3.3-70B-Instruct"],
            Self::DeepInfra => &[
                "deep-infra/meta-llama/Llama-3.3-70B-Instruct",
                "deep-infra/Qwen/Qwen2.5-Coder-32B",
            ],
            Self::OpenRouter => &[
                "openrouter/anthropic/claude-3.5-sonnet",
                "openrouter/openai/gpt-4o",
            ],
            Self::Cloudflare => &[
                "cloudflare/@cf/meta/llama-3-8b-instruct",
                "cloudflare/@cf/mistral/mistral-7b",
            ],
            Self::Vercel => &["vercel/gpt-4o", "vercel/claude-3.5-sonnet"],
            Self::Helicone => &["helicone/gpt-4o"],
            Self::ZenMux => &["zenmux/gpt-4o", "zenmux/claude-3.5-sonnet"],
            Self::DeepSeek => &["deepseek/deepseek-chat", "deepseek/deepseek-coder"],
            Self::Xai => &["xai/grok-2", "xai/grok-beta"],
            Self::Mistral => &["mistral/mistral-large", "mistral/codestral"],
            Self::Cohere => &["cohere/command-r-plus", "cohere/command-r"],
            Self::Perplexity => &["perplexity/sonar-pro", "perplexity/sonar"],
            Self::AI21 => &["ai21/jamba-1.5-large", "ai21/jamba-1.5-mini"],
            Self::VeniceAI => &["venice-ai/llama-3-70b"],
            Self::HuggingFace => &[
                "huggingface/meta-llama/Llama-3.3-70B-Instruct",
                "huggingface/Qwen/Qwen2.5-Coder-32B",
            ],
            Self::Replicate => &["replicate/meta/llama-3-70b-instruct"],
            Self::Baseten => &["baseten/llama-3-70b"],
            Self::Cortecs => &["cortecs/llama-3-70b"],
            Self::Scaleway => &["scaleway/llama-3-70b"],
            Self::OVHcloud => &["ovhcloud/llama-3-70b"],
            Self::IONet => &["io-net/llama-3-70b"],
            Self::Nebius => &["nebius/llama-3-70b"],
            Self::SapAICore => &["sap-ai-core/gpt-4o", "sap-ai-core/claude-3.5-sonnet"],
            Self::AzureCognitiveServices => &["azure-cognitive-services/gpt-4o"],
            Self::Ollama => &["ollama/llama3", "ollama/codellama", "ollama/mistral"],
            Self::LMStudio => &["lmstudio/local-model"],
            Self::OllamaCloud => &["ollama-cloud/llama3", "ollama-cloud/codellama"],
            Self::LlamaCpp => &["llama.cpp/local-model"],
            Self::Custom => &[],
        }
    }
}
