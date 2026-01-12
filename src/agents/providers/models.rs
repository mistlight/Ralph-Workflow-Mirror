//! Example Models
//!
//! Example model identifiers for each provider type.

use super::types::OpenCodeProviderType;

impl OpenCodeProviderType {
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
}
