//! Agent Abstraction Module
//!
//! Provides a pluggable agent system for different
//! AI coding assistants (Claude, Codex, OpenCode, Goose, Cline, etc.)
//!
//! ## Configuration
//!
//! Agents can be configured via (in order of increasing priority):
//! 1. Built-in defaults (claude, codex, opencode, aider, goose, cline, continue, amazon-q, gemini)
//! 2. Global config file (`~/.config/ralph/agents.toml`)
//! 3. Project config file (default: `.agent/agents.toml`, overridable via `RALPH_AGENTS_CONFIG`)
//! 4. Environment variables (`RALPH_DEVELOPER_CMD`, `RALPH_REVIEWER_CMD`)
//! 5. Programmatic registration via `AgentRegistry::register()`
//!
//! Config files are merged, with later sources overriding earlier ones.
//! This allows setting global defaults while customizing per-project.
//!
//! ## Agent Switching / Fallback
//!
//! Configure fallback agents for automatic switching when primary agent fails:
//! ```toml
//! [agent_chain]
//! developer = ["claude", "codex", "goose"]
//! reviewer = ["codex", "claude"]
//! max_retries = 3
//! retry_delay_ms = 1000
//! ```
//!
//! ## Example TOML Configuration
//!
//! ```toml
//! [agents.myagent]
//! cmd = "my-ai-tool run"
//! output_flag = "--json-stream"
//! yolo_flag = "--auto-fix"
//! verbose_flag = "--verbose"
//! can_commit = true
//! json_parser = "claude"  # Use Claude's JSON parser
//! ```

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Strip a leading model-flag prefix and return the raw `provider/model` string.
///
/// Supports common OpenCode CLI forms:
/// - `-m provider/model`
/// - `--model provider/model`
/// - `-m=provider/model`
/// - `--model=provider/model`
pub(crate) fn strip_model_flag_prefix(model_flag: &str) -> &str {
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

/// Get the global config directory for Ralph
///
/// Returns `~/.config/ralph` on Unix and `%APPDATA%\ralph` on Windows.
/// Returns None if the home directory cannot be determined.
pub(crate) fn global_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("ralph"))
}

/// Get the global agents.toml path
///
/// Returns `~/.config/ralph/agents.toml` on Unix.
pub(crate) fn global_agents_config_path() -> Option<PathBuf> {
    global_config_dir().map(|p| p.join("agents.toml"))
}

/// Config source for tracking where config was loaded from
#[derive(Debug, Clone)]
pub(crate) struct ConfigSource {
    pub(crate) path: PathBuf,
    pub(crate) agents_loaded: usize,
}

/// Default agents.toml template embedded at compile time
pub(crate) const DEFAULT_AGENTS_TOML: &str = include_str!("../examples/agents.toml");

/// OpenCode provider type extracted from model flag
///
/// OpenCode supports 75+ providers through the AI SDK and Models.dev.
/// This enum explicitly lists all major provider categories for clear
/// identification and provider-specific authentication guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenCodeProviderType {
    // === OpenCode Gateway Providers ===
    /// OpenCode Zen gateway (opencode/*) - routes through opencode.ai/zen
    OpenCodeZen,

    // === Chinese AI Providers ===
    /// Z.AI Direct API (zai/* or zhipuai/*) - connects to api.z.ai
    ZaiDirect,
    /// Moonshot / Kimi (moonshot/*) - Moonshot AI's Kimi K2 models
    Moonshot,
    /// MiniMax (minimax/*) - MiniMax AI models
    MiniMax,

    // === Major Cloud Providers ===
    /// Anthropic (anthropic/*) - Claude models via Anthropic API
    Anthropic,
    /// OpenAI (openai/*) - GPT models via OpenAI API
    OpenAI,
    /// Google AI Studio (google/*) - Gemini models via Google AI
    Google,
    /// Google Vertex AI (google-vertex/*) - Gemini via Vertex AI (requires project ID)
    GoogleVertex,
    /// Amazon Bedrock (amazon-bedrock/*) - AWS Bedrock (requires AWS credentials)
    AmazonBedrock,
    /// Azure OpenAI (azure-openai/*) - Azure-hosted OpenAI (requires Azure config)
    AzureOpenAI,

    // === Fast Inference Providers ===
    /// Groq (groq/*) - Ultra-fast inference for Llama, Mixtral
    Groq,
    /// Together AI (together/*) - Open-source model hosting
    Together,
    /// Fireworks AI (fireworks/*) - Fast inference platform
    Fireworks,

    // === Gateway/Aggregator Providers ===
    /// OpenRouter (openrouter/*) - Multi-provider router
    OpenRouter,
    /// Cloudflare Workers AI (cloudflare/*) - Edge AI inference
    Cloudflare,

    // === Specialized Providers ===
    /// DeepSeek (deepseek/*) - DeepSeek AI models
    DeepSeek,
    /// xAI / Grok (xai/*) - Elon Musk's xAI Grok models
    Xai,
    /// Mistral AI (mistral/*) - Mistral's models
    Mistral,
    /// Cohere (cohere/*) - Cohere's Command models
    Cohere,

    // === Local Providers ===
    /// Ollama (ollama/*) - Local LLM server
    Ollama,
    /// LM Studio (lmstudio/*) - Local model runner
    LMStudio,

    // === Additional Providers ===
    /// GitHub Copilot (copilot/*) - GitHub's AI coding assistant with GPT-4, Claude, Gemini
    GithubCopilot,
    /// Deep Infra (deep-infra/*) - Fast inference for open-source models
    DeepInfra,
    /// Hugging Face (huggingface/*) - Open-source model hub inference
    HuggingFace,
    /// Replicate (replicate/*) - Run ML models in the cloud
    Replicate,
    /// Perplexity (perplexity/*) - AI search and reasoning
    Perplexity,
    /// AI21 (ai21/*) - Jurassic and Jamba models
    AI21,
    /// Cerebras (cerebras/*) - Ultra-fast inference
    Cerebras,
    /// SambaNova (sambanova/*) - Enterprise AI platform
    SambaNova,

    // === Cloud Platform Providers ===
    /// Baseten (baseten/*) - ML inference infrastructure
    Baseten,
    /// Cortecs (cortecs/*) - AI compute platform
    Cortecs,
    /// Scaleway (scaleway/*) - European cloud AI
    Scaleway,
    /// OVHcloud (ovhcloud/*) - European cloud AI
    OVHcloud,

    // === AI Gateway Providers ===
    /// Vercel AI Gateway (vercel/*) - Vercel's AI SDK gateway
    Vercel,
    /// Helicone (helicone/*) - AI observability gateway
    Helicone,
    /// IO.NET (io-net/*) - Distributed GPU network
    IONet,
    /// Nebius (nebius/*) - Cloud AI infrastructure
    Nebius,
    /// ZenMux (zenmux/*) - AI multiplexer gateway
    ZenMux,

    // === Enterprise/Industry Providers ===
    /// SAP AI Core (sap-ai-core/*) - SAP's enterprise AI platform
    SapAICore,
    /// Azure Cognitive Services (azure-cognitive-services/*) - Azure AI services
    AzureCognitiveServices,

    // === Specialized Inference Providers ===
    /// Venice AI (venice-ai/*) - Privacy-focused AI
    VeniceAI,
    /// Ollama Cloud (ollama-cloud/*) - Cloud-hosted Ollama
    OllamaCloud,
    /// llama.cpp (llama.cpp/*) - Native llama.cpp inference
    LlamaCpp,

    /// Custom/Unknown provider - fallback for unrecognized prefixes
    Custom,
}

impl OpenCodeProviderType {
    /// Parse provider type from model flag (e.g., "opencode/glm-4.7-free" -> OpenCodeZen)
    ///
    /// Supports all major OpenCode providers. The prefix before the first '/' determines
    /// the provider type. Case-insensitive matching is used.
    pub(crate) fn from_model_flag(model_flag: &str) -> Self {
        let model = strip_model_flag_prefix(model_flag);
        let prefix = model.split('/').next().unwrap_or("");
        match prefix.to_lowercase().as_str() {
            // OpenCode Gateway
            "opencode" => OpenCodeProviderType::OpenCodeZen,

            // Chinese AI Providers
            "zai" | "zhipuai" => OpenCodeProviderType::ZaiDirect,
            "moonshot" | "kimi" => OpenCodeProviderType::Moonshot,
            "minimax" => OpenCodeProviderType::MiniMax,

            // Major Cloud Providers
            "anthropic" => OpenCodeProviderType::Anthropic,
            "openai" => OpenCodeProviderType::OpenAI,
            "google" | "gemini" => OpenCodeProviderType::Google,
            "google-vertex" | "vertex" => OpenCodeProviderType::GoogleVertex,
            "amazon-bedrock" | "bedrock" | "aws" => OpenCodeProviderType::AmazonBedrock,
            "azure-openai" | "azure" => OpenCodeProviderType::AzureOpenAI,

            // Fast Inference Providers
            "groq" => OpenCodeProviderType::Groq,
            "together" => OpenCodeProviderType::Together,
            "fireworks" => OpenCodeProviderType::Fireworks,

            // Gateway/Aggregator Providers
            "openrouter" => OpenCodeProviderType::OpenRouter,
            "cloudflare" | "cf" => OpenCodeProviderType::Cloudflare,

            // Specialized Providers
            "deepseek" => OpenCodeProviderType::DeepSeek,
            "xai" | "grok" => OpenCodeProviderType::Xai,
            "mistral" => OpenCodeProviderType::Mistral,
            "cohere" => OpenCodeProviderType::Cohere,

            // Local Providers
            "ollama" => OpenCodeProviderType::Ollama,
            "lmstudio" | "lm-studio" => OpenCodeProviderType::LMStudio,

            // Additional Providers
            "copilot" | "github-copilot" => OpenCodeProviderType::GithubCopilot,
            "deep-infra" | "deepinfra" => OpenCodeProviderType::DeepInfra,
            "huggingface" | "hf" => OpenCodeProviderType::HuggingFace,
            "replicate" => OpenCodeProviderType::Replicate,
            "perplexity" | "pplx" => OpenCodeProviderType::Perplexity,
            "ai21" => OpenCodeProviderType::AI21,
            "cerebras" => OpenCodeProviderType::Cerebras,
            "sambanova" => OpenCodeProviderType::SambaNova,

            // Cloud Platform Providers
            "baseten" => OpenCodeProviderType::Baseten,
            "cortecs" => OpenCodeProviderType::Cortecs,
            "scaleway" => OpenCodeProviderType::Scaleway,
            "ovhcloud" | "ovh" => OpenCodeProviderType::OVHcloud,

            // AI Gateway Providers
            "vercel" => OpenCodeProviderType::Vercel,
            "helicone" => OpenCodeProviderType::Helicone,
            "io-net" | "ionet" => OpenCodeProviderType::IONet,
            "nebius" => OpenCodeProviderType::Nebius,
            "zenmux" => OpenCodeProviderType::ZenMux,

            // Enterprise/Industry Providers
            "sap-ai-core" | "sap" => OpenCodeProviderType::SapAICore,
            "azure-cognitive-services" | "azure-cognitive" => {
                OpenCodeProviderType::AzureCognitiveServices
            }

            // Specialized Inference Providers
            "venice-ai" | "venice" => OpenCodeProviderType::VeniceAI,
            "ollama-cloud" => OpenCodeProviderType::OllamaCloud,
            "llama.cpp" | "llamacpp" | "llama-cpp" => OpenCodeProviderType::LlamaCpp,

            // Unknown/Custom
            _ => OpenCodeProviderType::Custom,
        }
    }

    /// Get human-readable name for this provider type
    pub(crate) fn name(&self) -> &'static str {
        match self {
            // OpenCode Gateway
            OpenCodeProviderType::OpenCodeZen => "OpenCode Zen",

            // Chinese AI Providers
            OpenCodeProviderType::ZaiDirect => "Z.AI Direct",
            OpenCodeProviderType::Moonshot => "Moonshot (Kimi)",
            OpenCodeProviderType::MiniMax => "MiniMax",

            // Major Cloud Providers
            OpenCodeProviderType::Anthropic => "Anthropic",
            OpenCodeProviderType::OpenAI => "OpenAI",
            OpenCodeProviderType::Google => "Google AI Studio",
            OpenCodeProviderType::GoogleVertex => "Google Vertex AI",
            OpenCodeProviderType::AmazonBedrock => "Amazon Bedrock",
            OpenCodeProviderType::AzureOpenAI => "Azure OpenAI",

            // Fast Inference Providers
            OpenCodeProviderType::Groq => "Groq",
            OpenCodeProviderType::Together => "Together AI",
            OpenCodeProviderType::Fireworks => "Fireworks AI",

            // Gateway/Aggregator Providers
            OpenCodeProviderType::OpenRouter => "OpenRouter",
            OpenCodeProviderType::Cloudflare => "Cloudflare Workers AI",

            // Specialized Providers
            OpenCodeProviderType::DeepSeek => "DeepSeek",
            OpenCodeProviderType::Xai => "xAI (Grok)",
            OpenCodeProviderType::Mistral => "Mistral AI",
            OpenCodeProviderType::Cohere => "Cohere",

            // Local Providers
            OpenCodeProviderType::Ollama => "Ollama",
            OpenCodeProviderType::LMStudio => "LM Studio",

            // Additional Providers
            OpenCodeProviderType::GithubCopilot => "GitHub Copilot",
            OpenCodeProviderType::DeepInfra => "Deep Infra",
            OpenCodeProviderType::HuggingFace => "Hugging Face",
            OpenCodeProviderType::Replicate => "Replicate",
            OpenCodeProviderType::Perplexity => "Perplexity",
            OpenCodeProviderType::AI21 => "AI21 Labs",
            OpenCodeProviderType::Cerebras => "Cerebras",
            OpenCodeProviderType::SambaNova => "SambaNova",

            // Cloud Platform Providers
            OpenCodeProviderType::Baseten => "Baseten",
            OpenCodeProviderType::Cortecs => "Cortecs",
            OpenCodeProviderType::Scaleway => "Scaleway",
            OpenCodeProviderType::OVHcloud => "OVHcloud",

            // AI Gateway Providers
            OpenCodeProviderType::Vercel => "Vercel AI Gateway",
            OpenCodeProviderType::Helicone => "Helicone",
            OpenCodeProviderType::IONet => "IO.NET",
            OpenCodeProviderType::Nebius => "Nebius",
            OpenCodeProviderType::ZenMux => "ZenMux",

            // Enterprise/Industry Providers
            OpenCodeProviderType::SapAICore => "SAP AI Core",
            OpenCodeProviderType::AzureCognitiveServices => "Azure Cognitive Services",

            // Specialized Inference Providers
            OpenCodeProviderType::VeniceAI => "Venice AI",
            OpenCodeProviderType::OllamaCloud => "Ollama Cloud",
            OpenCodeProviderType::LlamaCpp => "llama.cpp",

            // Custom
            OpenCodeProviderType::Custom => "Custom",
        }
    }

    /// Get authentication command for this provider type
    pub(crate) fn auth_command(&self) -> &'static str {
        match self {
            // OpenCode Gateway
            OpenCodeProviderType::OpenCodeZen => "opencode auth login -> select 'OpenCode Zen'",

            // Chinese AI Providers
            OpenCodeProviderType::ZaiDirect => {
                "opencode auth login -> select 'Z.AI' or 'Z.AI Coding Plan' (model prefix remains zai/* or zhipuai/*)"
            }
            OpenCodeProviderType::Moonshot => "opencode auth moonshot (set MOONSHOT_API_KEY)",
            OpenCodeProviderType::MiniMax => "opencode auth minimax (set MINIMAX_API_KEY)",

            // Major Cloud Providers
            OpenCodeProviderType::Anthropic => "opencode auth anthropic (set ANTHROPIC_API_KEY)",
            OpenCodeProviderType::OpenAI => "opencode auth openai (set OPENAI_API_KEY)",
            OpenCodeProviderType::Google => {
                "opencode auth google (set GOOGLE_GENERATIVE_AI_API_KEY)"
            }
            OpenCodeProviderType::GoogleVertex => {
                "gcloud auth application-default login + set GOOGLE_VERTEX_PROJECT"
            }
            OpenCodeProviderType::AmazonBedrock => "aws configure (set AWS credentials + region)",
            OpenCodeProviderType::AzureOpenAI => {
                "set AZURE_OPENAI_API_KEY, AZURE_OPENAI_ENDPOINT, AZURE_OPENAI_DEPLOYMENT"
            }

            // Fast Inference Providers
            OpenCodeProviderType::Groq => "opencode auth groq (set GROQ_API_KEY)",
            OpenCodeProviderType::Together => "opencode auth together (set TOGETHER_API_KEY)",
            OpenCodeProviderType::Fireworks => "opencode auth fireworks (set FIREWORKS_API_KEY)",

            // Gateway/Aggregator Providers
            OpenCodeProviderType::OpenRouter => "opencode auth openrouter (set OPENROUTER_API_KEY)",
            OpenCodeProviderType::Cloudflare => {
                "set CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN"
            }

            // Specialized Providers
            OpenCodeProviderType::DeepSeek => "opencode auth deepseek (set DEEPSEEK_API_KEY)",
            OpenCodeProviderType::Xai => "opencode auth xai (set XAI_API_KEY)",
            OpenCodeProviderType::Mistral => "opencode auth mistral (set MISTRAL_API_KEY)",
            OpenCodeProviderType::Cohere => "opencode auth cohere (set COHERE_API_KEY)",

            // Local Providers
            OpenCodeProviderType::Ollama => "ollama serve (no API key needed, runs locally)",
            OpenCodeProviderType::LMStudio => "Start LM Studio server (no API key needed)",

            // Additional Providers
            OpenCodeProviderType::GithubCopilot => {
                "GitHub Copilot subscription required (via VS Code or gh copilot)"
            }
            OpenCodeProviderType::DeepInfra => "set DEEPINFRA_API_KEY from https://deepinfra.com",
            OpenCodeProviderType::HuggingFace => {
                "set HF_TOKEN from https://huggingface.co/settings/tokens"
            }
            OpenCodeProviderType::Replicate => "set REPLICATE_API_TOKEN from https://replicate.com",
            OpenCodeProviderType::Perplexity => "set PERPLEXITY_API_KEY from https://perplexity.ai",
            OpenCodeProviderType::AI21 => "set AI21_API_KEY from https://studio.ai21.com",
            OpenCodeProviderType::Cerebras => "set CEREBRAS_API_KEY from https://cerebras.ai",
            OpenCodeProviderType::SambaNova => "set SAMBANOVA_API_KEY from https://sambanova.ai",

            // Cloud Platform Providers
            OpenCodeProviderType::Baseten => "opencode /connect baseten (API key via /connect)",
            OpenCodeProviderType::Cortecs => "opencode /connect cortecs (API key via /connect)",
            OpenCodeProviderType::Scaleway => "opencode /connect scaleway (API key via /connect)",
            OpenCodeProviderType::OVHcloud => "opencode /connect ovhcloud (API key via /connect)",

            // AI Gateway Providers
            OpenCodeProviderType::Vercel => "opencode /connect vercel (API key via /connect)",
            OpenCodeProviderType::Helicone => "opencode /connect helicone (API key via /connect)",
            OpenCodeProviderType::IONet => "opencode /connect io-net (API key via /connect)",
            OpenCodeProviderType::Nebius => "opencode /connect nebius (API key via /connect)",
            OpenCodeProviderType::ZenMux => "opencode /connect zenmux (API key via /connect)",

            // Enterprise/Industry Providers
            OpenCodeProviderType::SapAICore => {
                "set AICORE_SERVICE_KEY, AICORE_DEPLOYMENT_ID, AICORE_RESOURCE_GROUP"
            }
            OpenCodeProviderType::AzureCognitiveServices => {
                "set AZURE_COGNITIVE_SERVICES_RESOURCE_NAME"
            }

            // Specialized Inference Providers
            OpenCodeProviderType::VeniceAI => "opencode /connect venice-ai (API key via /connect)",
            OpenCodeProviderType::OllamaCloud => {
                "opencode /connect ollama-cloud (API key via /connect)"
            }
            OpenCodeProviderType::LlamaCpp => "llama-server (no API key needed, runs locally)",

            // Custom
            OpenCodeProviderType::Custom => "Check provider documentation for authentication",
        }
    }

    /// Get model prefix for this provider type
    pub(crate) fn prefix(&self) -> &'static str {
        match self {
            OpenCodeProviderType::OpenCodeZen => "opencode/",
            OpenCodeProviderType::ZaiDirect => "zai/ or zhipuai/",
            OpenCodeProviderType::Moonshot => "moonshot/",
            OpenCodeProviderType::MiniMax => "minimax/",
            OpenCodeProviderType::Anthropic => "anthropic/",
            OpenCodeProviderType::OpenAI => "openai/",
            OpenCodeProviderType::Google => "google/",
            OpenCodeProviderType::GoogleVertex => "google-vertex/",
            OpenCodeProviderType::AmazonBedrock => "amazon-bedrock/",
            OpenCodeProviderType::AzureOpenAI => "azure-openai/",
            OpenCodeProviderType::Groq => "groq/",
            OpenCodeProviderType::Together => "together/",
            OpenCodeProviderType::Fireworks => "fireworks/",
            OpenCodeProviderType::OpenRouter => "openrouter/",
            OpenCodeProviderType::Cloudflare => "cloudflare/",
            OpenCodeProviderType::DeepSeek => "deepseek/",
            OpenCodeProviderType::Xai => "xai/",
            OpenCodeProviderType::Mistral => "mistral/",
            OpenCodeProviderType::Cohere => "cohere/",
            OpenCodeProviderType::Ollama => "ollama/",
            OpenCodeProviderType::LMStudio => "lmstudio/",
            OpenCodeProviderType::GithubCopilot => "copilot/",
            OpenCodeProviderType::DeepInfra => "deep-infra/",
            OpenCodeProviderType::HuggingFace => "huggingface/",
            OpenCodeProviderType::Replicate => "replicate/",
            OpenCodeProviderType::Perplexity => "perplexity/",
            OpenCodeProviderType::AI21 => "ai21/",
            OpenCodeProviderType::Cerebras => "cerebras/",
            OpenCodeProviderType::SambaNova => "sambanova/",
            OpenCodeProviderType::Baseten => "baseten/",
            OpenCodeProviderType::Cortecs => "cortecs/",
            OpenCodeProviderType::Scaleway => "scaleway/",
            OpenCodeProviderType::OVHcloud => "ovhcloud/",
            OpenCodeProviderType::Vercel => "vercel/",
            OpenCodeProviderType::Helicone => "helicone/",
            OpenCodeProviderType::IONet => "io-net/",
            OpenCodeProviderType::Nebius => "nebius/",
            OpenCodeProviderType::ZenMux => "zenmux/",
            OpenCodeProviderType::SapAICore => "sap-ai-core/",
            OpenCodeProviderType::AzureCognitiveServices => "azure-cognitive-services/",
            OpenCodeProviderType::VeniceAI => "venice-ai/",
            OpenCodeProviderType::OllamaCloud => "ollama-cloud/",
            OpenCodeProviderType::LlamaCpp => "llama.cpp/",
            OpenCodeProviderType::Custom => "any other provider/*",
        }
    }

    /// Get example models for this provider type
    pub(crate) fn example_models(&self) -> &'static [&'static str] {
        match self {
            OpenCodeProviderType::OpenCodeZen => {
                &["opencode/glm-4.7-free", "opencode/claude-sonnet-4"]
            }
            OpenCodeProviderType::ZaiDirect => &["zai/glm-4.7", "zai/glm-4.5", "zhipuai/glm-4.7"],
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
            OpenCodeProviderType::Groq => &["groq/llama-3.3-70b-versatile", "groq/mixtral-8x7b"],
            OpenCodeProviderType::Together => &[
                "together/meta-llama/Llama-3-70b-chat-hf",
                "together/mistralai/Mixtral-8x7B",
            ],
            OpenCodeProviderType::Fireworks => {
                &["fireworks/accounts/fireworks/models/llama-v3p1-70b-instruct"]
            }
            OpenCodeProviderType::OpenRouter => &[
                "openrouter/anthropic/claude-3.5-sonnet",
                "openrouter/openai/gpt-4o",
            ],
            OpenCodeProviderType::Cloudflare => &[
                "cloudflare/@cf/meta/llama-3-8b-instruct",
                "cloudflare/@cf/mistral/mistral-7b",
            ],
            OpenCodeProviderType::DeepSeek => {
                &["deepseek/deepseek-chat", "deepseek/deepseek-coder"]
            }
            OpenCodeProviderType::Xai => &["xai/grok-2", "xai/grok-beta"],
            OpenCodeProviderType::Mistral => &["mistral/mistral-large", "mistral/codestral"],
            OpenCodeProviderType::Cohere => &["cohere/command-r-plus", "cohere/command-r"],
            OpenCodeProviderType::Ollama => {
                &["ollama/llama3", "ollama/codellama", "ollama/mistral"]
            }
            OpenCodeProviderType::LMStudio => &["lmstudio/local-model"],
            OpenCodeProviderType::GithubCopilot => &[
                "copilot/gpt-4o",
                "copilot/claude-3.5-sonnet",
                "copilot/gemini-2.0-flash",
            ],
            OpenCodeProviderType::DeepInfra => &[
                "deep-infra/meta-llama/Llama-3.3-70B-Instruct",
                "deep-infra/Qwen/Qwen2.5-Coder-32B",
            ],
            OpenCodeProviderType::HuggingFace => &[
                "huggingface/meta-llama/Llama-3.3-70B-Instruct",
                "huggingface/Qwen/Qwen2.5-Coder-32B",
            ],
            OpenCodeProviderType::Replicate => &["replicate/meta/llama-3-70b-instruct"],
            OpenCodeProviderType::Perplexity => &["perplexity/sonar-pro", "perplexity/sonar"],
            OpenCodeProviderType::AI21 => &["ai21/jamba-1.5-large", "ai21/jamba-1.5-mini"],
            OpenCodeProviderType::Cerebras => &["cerebras/llama3.3-70b"],
            OpenCodeProviderType::SambaNova => &["sambanova/Meta-Llama-3.3-70B-Instruct"],
            OpenCodeProviderType::Baseten => &["baseten/llama-3-70b"],
            OpenCodeProviderType::Cortecs => &["cortecs/llama-3-70b"],
            OpenCodeProviderType::Scaleway => &["scaleway/llama-3-70b"],
            OpenCodeProviderType::OVHcloud => &["ovhcloud/llama-3-70b"],
            OpenCodeProviderType::Vercel => &["vercel/gpt-4o", "vercel/claude-3.5-sonnet"],
            OpenCodeProviderType::Helicone => &["helicone/gpt-4o"],
            OpenCodeProviderType::IONet => &["io-net/llama-3-70b"],
            OpenCodeProviderType::Nebius => &["nebius/llama-3-70b"],
            OpenCodeProviderType::ZenMux => &["zenmux/gpt-4o", "zenmux/claude-3.5-sonnet"],
            OpenCodeProviderType::SapAICore => {
                &["sap-ai-core/gpt-4o", "sap-ai-core/claude-3.5-sonnet"]
            }
            OpenCodeProviderType::AzureCognitiveServices => &["azure-cognitive-services/gpt-4o"],
            OpenCodeProviderType::VeniceAI => &["venice-ai/llama-3-70b"],
            OpenCodeProviderType::OllamaCloud => &["ollama-cloud/llama3", "ollama-cloud/codellama"],
            OpenCodeProviderType::LlamaCpp => &["llama.cpp/local-model"],
            OpenCodeProviderType::Custom => &[],
        }
    }

    /// Check if this provider requires special cloud configuration
    pub(crate) fn requires_cloud_config(&self) -> bool {
        matches!(
            self,
            OpenCodeProviderType::GoogleVertex
                | OpenCodeProviderType::AmazonBedrock
                | OpenCodeProviderType::AzureOpenAI
                | OpenCodeProviderType::SapAICore
                | OpenCodeProviderType::AzureCognitiveServices
        )
    }

    /// Check if this is a local provider (no API key needed)
    pub(crate) fn is_local(&self) -> bool {
        matches!(
            self,
            OpenCodeProviderType::Ollama
                | OpenCodeProviderType::LMStudio
                | OpenCodeProviderType::LlamaCpp
        )
    }

    /// Get all provider types for enumeration
    #[cfg(test)]
    pub(crate) fn all() -> &'static [OpenCodeProviderType] {
        &[
            // OpenCode Gateway
            OpenCodeProviderType::OpenCodeZen,
            // Chinese AI Providers
            OpenCodeProviderType::ZaiDirect,
            OpenCodeProviderType::Moonshot,
            OpenCodeProviderType::MiniMax,
            // Major Cloud Providers
            OpenCodeProviderType::Anthropic,
            OpenCodeProviderType::OpenAI,
            OpenCodeProviderType::Google,
            OpenCodeProviderType::GoogleVertex,
            OpenCodeProviderType::AmazonBedrock,
            OpenCodeProviderType::AzureOpenAI,
            OpenCodeProviderType::GithubCopilot,
            // Fast Inference Providers
            OpenCodeProviderType::Groq,
            OpenCodeProviderType::Together,
            OpenCodeProviderType::Fireworks,
            OpenCodeProviderType::Cerebras,
            OpenCodeProviderType::SambaNova,
            OpenCodeProviderType::DeepInfra,
            // Gateway/Aggregator Providers
            OpenCodeProviderType::OpenRouter,
            OpenCodeProviderType::Cloudflare,
            OpenCodeProviderType::Vercel,
            OpenCodeProviderType::Helicone,
            OpenCodeProviderType::ZenMux,
            // Specialized Providers
            OpenCodeProviderType::DeepSeek,
            OpenCodeProviderType::Xai,
            OpenCodeProviderType::Mistral,
            OpenCodeProviderType::Cohere,
            OpenCodeProviderType::Perplexity,
            OpenCodeProviderType::AI21,
            OpenCodeProviderType::VeniceAI,
            // Open-Source Model Providers
            OpenCodeProviderType::HuggingFace,
            OpenCodeProviderType::Replicate,
            // Cloud Platform Providers
            OpenCodeProviderType::Baseten,
            OpenCodeProviderType::Cortecs,
            OpenCodeProviderType::Scaleway,
            OpenCodeProviderType::OVHcloud,
            OpenCodeProviderType::IONet,
            OpenCodeProviderType::Nebius,
            // Enterprise/Industry Providers
            OpenCodeProviderType::SapAICore,
            OpenCodeProviderType::AzureCognitiveServices,
            // Local Providers
            OpenCodeProviderType::Ollama,
            OpenCodeProviderType::LMStudio,
            OpenCodeProviderType::OllamaCloud,
            OpenCodeProviderType::LlamaCpp,
        ]
    }
}

/// Validate a model flag and return provider-specific warnings if any issues detected
///
/// Returns a vector of warning messages (empty if no issues).
/// This performs soft validation (warnings, not errors) to help users understand
/// provider-specific requirements without blocking execution.
pub(crate) fn validate_model_flag(model_flag: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    // Check for common mistakes
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

/// Get provider-specific authentication failure advice based on model flag
pub(crate) fn auth_failure_advice(model_flag: Option<&str>) -> String {
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

/// JSON parser type for agent output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum JsonParserType {
    /// Claude's stream-json format
    #[default]
    Claude,
    /// Codex's JSON format
    Codex,
    /// Gemini's stream-json format
    Gemini,
    /// OpenCode's JSON format
    OpenCode,
    /// Generic line-based output (no JSON parsing)
    Generic,
}

impl JsonParserType {
    /// Parse parser type from string
    pub(crate) fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "claude" => JsonParserType::Claude,
            "codex" => JsonParserType::Codex,
            "gemini" => JsonParserType::Gemini,
            "opencode" => JsonParserType::OpenCode,
            "generic" | "none" | "raw" => JsonParserType::Generic,
            _ => JsonParserType::Generic,
        }
    }
}

impl std::fmt::Display for JsonParserType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonParserType::Claude => write!(f, "claude"),
            JsonParserType::Codex => write!(f, "codex"),
            JsonParserType::Gemini => write!(f, "gemini"),
            JsonParserType::OpenCode => write!(f, "opencode"),
            JsonParserType::Generic => write!(f, "generic"),
        }
    }
}

/// Agent capabilities
#[derive(Debug, Clone)]
pub(crate) struct AgentConfig {
    /// Base command to run the agent
    pub(crate) cmd: String,
    /// Output-format flag (JSON streaming, text mode, etc.)
    pub(crate) output_flag: String,
    /// Flag for autonomous mode (no prompts)
    pub(crate) yolo_flag: String,
    /// Flag for verbose output
    pub(crate) verbose_flag: String,
    /// Whether the agent can run git commit
    pub(crate) can_commit: bool,
    /// Which JSON parser to use for this agent's output
    pub(crate) json_parser: JsonParserType,
    /// Model/provider flag for agents that support model selection (e.g., `-m provider/model`)
    /// Used for provider-level fallback within a single agent
    pub(crate) model_flag: Option<String>,
}

/// TOML configuration for an agent (for deserialization)
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AgentConfigToml {
    /// Base command to run the agent
    pub(crate) cmd: String,
    /// Output-format flag (optional, defaults to empty)
    #[serde(default)]
    pub(crate) output_flag: String,
    /// Flag for autonomous mode (optional, defaults to empty)
    #[serde(default)]
    pub(crate) yolo_flag: String,
    /// Flag for verbose output (optional, defaults to empty)
    #[serde(default)]
    pub(crate) verbose_flag: String,
    /// Whether the agent can run git commit (optional, defaults to true)
    #[serde(default = "default_can_commit")]
    pub(crate) can_commit: bool,
    /// Which JSON parser to use: "claude", "codex", or "generic" (optional, defaults to "generic")
    #[serde(default)]
    pub(crate) json_parser: String,
    /// Model/provider flag for model selection (e.g., "-m opencode/glm-4.7-free")
    /// Used for provider-level fallback within a single agent
    #[serde(default)]
    pub(crate) model_flag: Option<String>,
}

fn default_can_commit() -> bool {
    true
}

impl From<AgentConfigToml> for AgentConfig {
    fn from(toml: AgentConfigToml) -> Self {
        AgentConfig {
            cmd: toml.cmd,
            output_flag: toml.output_flag,
            yolo_flag: toml.yolo_flag,
            verbose_flag: toml.verbose_flag,
            can_commit: toml.can_commit,
            json_parser: JsonParserType::parse(&toml.json_parser),
            model_flag: toml.model_flag,
        }
    }
}

/// Root TOML configuration structure
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AgentsConfigFile {
    /// Map of agent name to configuration
    #[serde(default)]
    pub(crate) agents: HashMap<String, AgentConfigToml>,
    /// Agent chain configuration (preferred agents + fallbacks)
    #[serde(default, rename = "agent_chain")]
    pub(crate) fallback: FallbackConfig,
}

/// Error type for agent configuration loading
#[derive(Debug, thiserror::Error)]
pub(crate) enum AgentConfigError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("Built-in agents.toml template is invalid TOML: {0}")]
    DefaultTemplateToml(toml::de::Error),
}

/// Result of checking/initializing the agents config file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigInitResult {
    /// Config file already exists, no action taken
    AlreadyExists,
    /// Config file was just created from template
    Created,
}

impl AgentsConfigFile {
    /// Load agents configuration from a TOML file
    ///
    /// Returns Ok(None) if the file doesn't exist.
    /// Returns Err if the file exists but can't be parsed.
    pub(crate) fn load_from_file<P: AsRef<Path>>(
        path: P,
    ) -> Result<Option<Self>, AgentConfigError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(path)?;
        let config: AgentsConfigFile = toml::from_str(&contents)?;
        Ok(Some(config))
    }

    /// Ensure agents config file exists, creating it from template if needed.
    ///
    /// Returns:
    /// - `Ok(ConfigInitResult::AlreadyExists)` if the file already exists
    /// - `Ok(ConfigInitResult::Created)` if the file was just created from the default template
    /// - `Err` if there was an error creating the file or parent directories
    pub(crate) fn ensure_config_exists<P: AsRef<Path>>(path: P) -> io::Result<ConfigInitResult> {
        let path = path.as_ref();

        if path.exists() {
            return Ok(ConfigInitResult::AlreadyExists);
        }

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write the default template
        fs::write(path, DEFAULT_AGENTS_TOML)?;

        Ok(ConfigInitResult::Created)
    }
}

impl AgentConfig {
    /// Build full command string with specified flags
    ///
    /// Note: For Claude CLI, when using `--output-format=stream-json` (the output_flag),
    /// the `--verbose` flag is always required. This method automatically adds verbose
    /// when using Claude's stream-json format, regardless of the `verbose` parameter.
    pub(crate) fn build_cmd(&self, output: bool, yolo: bool, verbose: bool) -> String {
        self.build_cmd_with_model(output, yolo, verbose, None)
    }

    /// Build full command string with specified flags and optional model override
    ///
    /// The `model_override` parameter allows passing a custom model flag at runtime,
    /// overriding any model_flag configured in agents.toml. This is used for:
    /// - CLI flags like `--developer-model`
    /// - Provider-level fallback (trying different providers within the same agent)
    ///
    /// Example: `agent.build_cmd_with_model(true, true, true, Some("-m opencode/glm-4.7-free"))`
    pub(crate) fn build_cmd_with_model(
        &self,
        output: bool,
        yolo: bool,
        verbose: bool,
        model_override: Option<&str>,
    ) -> String {
        let mut parts = vec![self.cmd.clone()];

        if output && !self.output_flag.is_empty() {
            parts.push(self.output_flag.clone());
        }
        if yolo && !self.yolo_flag.is_empty() {
            parts.push(self.yolo_flag.clone());
        }

        // Claude CLI requires --verbose when using --output-format=stream-json
        // See: https://github.com/anthropics/claude-code
        let needs_verbose = verbose || self.requires_verbose_for_json(output);

        if needs_verbose && !self.verbose_flag.is_empty() {
            parts.push(self.verbose_flag.clone());
        }

        // Add model flag: runtime override takes precedence over config
        let effective_model = model_override.or(self.model_flag.as_deref());
        if let Some(model) = effective_model {
            if !model.is_empty() {
                parts.push(model.to_string());
            }
        }

        parts.join(" ")
    }

    /// Check if this agent requires --verbose when JSON output is enabled.
    ///
    /// Claude CLI specifically requires --verbose when using --output-format=stream-json
    /// in print mode (-p). Without it, the command fails with:
    /// "Error: When using --print, --output-format=stream-json requires --verbose"
    ///
    /// Note: This is specific to the Claude CLI binary, not just agents using the Claude parser.
    /// Other CLIs like Qwen Code may use Claude-compatible output format but don't have this requirement.
    fn requires_verbose_for_json(&self, json_enabled: bool) -> bool {
        // Only applies to Claude CLI (claude -p command), not other CLIs using Claude parser
        json_enabled && self.cmd.starts_with("claude ") && self.output_flag.contains("stream-json")
    }
}

/// Agent chain configuration for preferred agents and fallback switching
///
/// The agent chain defines both:
/// 1. The **preferred agent** (first in the list) for each role
/// 2. The **fallback agents** (remaining in the list) to try if the preferred fails
///
/// This provides a unified way to configure which agents to use and in what order.
/// Ralph automatically switches to the next agent in the chain when encountering
/// errors like rate limits or auth failures.
///
/// ## Provider-Level Fallback
///
/// In addition to agent-level fallback, you can configure provider-level fallback
/// within a single agent using the `provider_fallback` field. This is useful for
/// agents like opencode that support multiple providers/models via the `-m` flag.
///
/// Example:
/// ```toml
/// [agent_chain]
/// provider_fallback.opencode = ["-m opencode/glm-4.7-free", "-m opencode/claude-sonnet-4"]
/// ```
///
/// When the primary model fails (rate limit, token exhaustion), Ralph tries the
/// next model in the provider_fallback list before moving to the next agent.
///
/// ## Exponential Backoff and Cycling
///
/// When all fallbacks are exhausted, Ralph uses exponential backoff and cycles
/// back to the first agent in the chain:
/// - Base delay starts at `retry_delay_ms` (default: 1000ms)
/// - Each cycle multiplies by `backoff_multiplier` (default: 2.0)
/// - Capped at `max_backoff_ms` (default: 60000ms = 1 minute)
/// - Maximum cycles controlled by `max_cycles` (default: 3)
///
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FallbackConfig {
    /// Ordered list of agents for developer role (first = preferred, rest = fallbacks)
    #[serde(default)]
    pub(crate) developer: Vec<String>,
    /// Ordered list of agents for reviewer role (first = preferred, rest = fallbacks)
    #[serde(default)]
    pub(crate) reviewer: Vec<String>,
    /// Provider-level fallback: maps agent name to list of model flags to try
    /// Example: `opencode = ["-m opencode/glm-4.7-free", "-m opencode/claude-sonnet-4"]`
    /// When one model fails (rate limit, token exhaustion), the next is tried.
    #[serde(default)]
    pub(crate) provider_fallback: HashMap<String, Vec<String>>,
    /// Maximum number of retries per agent before moving to next
    #[serde(default = "default_max_retries")]
    pub(crate) max_retries: u32,
    /// Base delay between retries in milliseconds
    #[serde(default = "default_retry_delay_ms")]
    pub(crate) retry_delay_ms: u64,
    /// Multiplier for exponential backoff (default: 2.0)
    #[serde(default = "default_backoff_multiplier")]
    pub(crate) backoff_multiplier: f64,
    /// Maximum backoff delay in milliseconds (default: 60000 = 1 minute)
    #[serde(default = "default_max_backoff_ms")]
    pub(crate) max_backoff_ms: u64,
    /// Maximum number of cycles through all agents before giving up (default: 3)
    #[serde(default = "default_max_cycles")]
    pub(crate) max_cycles: u32,
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_delay_ms() -> u64 {
    1000
}

fn default_backoff_multiplier() -> f64 {
    2.0
}

fn default_max_backoff_ms() -> u64 {
    60000 // 1 minute
}

fn default_max_cycles() -> u32 {
    3
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            developer: Vec::new(),
            reviewer: Vec::new(),
            provider_fallback: HashMap::new(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
            backoff_multiplier: default_backoff_multiplier(),
            max_backoff_ms: default_max_backoff_ms(),
            max_cycles: default_max_cycles(),
        }
    }
}

impl FallbackConfig {
    /// Calculate exponential backoff delay for a given cycle
    ///
    /// Uses the formula: min(base * multiplier^cycle, max_backoff)
    pub(crate) fn calculate_backoff(&self, cycle: u32) -> u64 {
        let delay = self.retry_delay_ms as f64 * self.backoff_multiplier.powi(cycle as i32);
        (delay as u64).min(self.max_backoff_ms)
    }

    /// Get fallback agents for a role
    pub(crate) fn get_fallbacks(&self, role: AgentRole) -> &[String] {
        match role {
            AgentRole::Developer => &self.developer,
            AgentRole::Reviewer => &self.reviewer,
        }
    }

    /// Check if fallback is configured for a role
    pub(crate) fn has_fallbacks(&self, role: AgentRole) -> bool {
        !self.get_fallbacks(role).is_empty()
    }

    /// Get provider-level fallback model flags for an agent
    ///
    /// Returns the list of model flags to try for the given agent name.
    /// Empty slice if no provider fallback is configured for this agent.
    pub(crate) fn get_provider_fallbacks(&self, agent_name: &str) -> &[String] {
        self.provider_fallback
            .get(agent_name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Check if provider-level fallback is configured for an agent
    #[allow(dead_code)] // Used in tests and may be used in future features
    pub(crate) fn has_provider_fallbacks(&self, agent_name: &str) -> bool {
        self.provider_fallback
            .get(agent_name)
            .is_some_and(|v| !v.is_empty())
    }
}

/// Agent role (developer or reviewer)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentRole {
    Developer,
    Reviewer,
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentRole::Developer => write!(f, "developer"),
            AgentRole::Reviewer => write!(f, "reviewer"),
        }
    }
}

/// Error classification for agent failures (to determine if retry is appropriate)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentErrorKind {
    /// API rate limit exceeded - retry after delay
    RateLimited,
    /// Token/context limit exceeded - may need different agent
    TokenExhausted,
    /// API temporarily unavailable (server-side issue) - retry
    ApiUnavailable,
    /// Network connectivity issue (client-side) - retry
    NetworkError,
    /// Authentication failure - switch agent
    AuthFailure,
    /// Command not found - switch agent
    CommandNotFound,
    /// Disk space exhausted - cannot continue
    DiskFull,
    /// Process killed (OOM, signal) - may retry with smaller context
    ProcessKilled,
    /// Invalid JSON response from agent - may retry
    InvalidResponse,
    /// Request/response timeout - retry
    Timeout,
    /// Other transient error - retry
    Transient,
    /// Permanent failure - do not retry
    Permanent,
}

impl AgentErrorKind {
    /// Determine if this error should trigger a retry
    pub(crate) fn should_retry(&self) -> bool {
        matches!(
            self,
            AgentErrorKind::RateLimited
                | AgentErrorKind::ApiUnavailable
                | AgentErrorKind::NetworkError
                | AgentErrorKind::Timeout
                | AgentErrorKind::InvalidResponse
                | AgentErrorKind::Transient
        )
    }

    /// Determine if this error should trigger a fallback to another agent
    pub(crate) fn should_fallback(&self) -> bool {
        matches!(
            self,
            AgentErrorKind::TokenExhausted
                | AgentErrorKind::AuthFailure
                | AgentErrorKind::CommandNotFound
                | AgentErrorKind::ProcessKilled
        )
    }

    /// Determine if this error is unrecoverable (should abort)
    pub(crate) fn is_unrecoverable(&self) -> bool {
        matches!(self, AgentErrorKind::DiskFull | AgentErrorKind::Permanent)
    }

    /// Check if this is a command not found error
    pub(crate) fn is_command_not_found(&self) -> bool {
        matches!(self, AgentErrorKind::CommandNotFound)
    }

    /// Check if this is a network-related error
    pub(crate) fn is_network_error(&self) -> bool {
        matches!(self, AgentErrorKind::NetworkError | AgentErrorKind::Timeout)
    }

    /// Check if this error might be resolved by reducing context size
    pub(crate) fn suggests_smaller_context(&self) -> bool {
        matches!(
            self,
            AgentErrorKind::TokenExhausted | AgentErrorKind::ProcessKilled
        )
    }

    /// Get suggested wait time in milliseconds before retry
    pub(crate) fn suggested_wait_ms(&self) -> u64 {
        match self {
            AgentErrorKind::RateLimited => 5000, // Rate limit: wait 5 seconds
            AgentErrorKind::ApiUnavailable => 3000, // Server issue: wait 3 seconds
            AgentErrorKind::NetworkError => 2000, // Network: wait 2 seconds
            AgentErrorKind::Timeout => 1000,     // Timeout: short wait
            AgentErrorKind::InvalidResponse => 500, // Bad response: quick retry
            AgentErrorKind::Transient => 1000,   // Transient: 1 second
            _ => 0,                              // No wait for non-retryable errors
        }
    }

    /// Get a user-friendly description of this error type
    pub(crate) fn description(&self) -> &'static str {
        match self {
            AgentErrorKind::RateLimited => "API rate limit exceeded",
            AgentErrorKind::TokenExhausted => "Token/context limit exceeded",
            AgentErrorKind::ApiUnavailable => "API service temporarily unavailable",
            AgentErrorKind::NetworkError => "Network connectivity issue",
            AgentErrorKind::AuthFailure => "Authentication failure",
            AgentErrorKind::CommandNotFound => "Command not found",
            AgentErrorKind::DiskFull => "Disk space exhausted",
            AgentErrorKind::ProcessKilled => "Process terminated (possibly OOM)",
            AgentErrorKind::InvalidResponse => "Invalid response from agent",
            AgentErrorKind::Timeout => "Request timed out",
            AgentErrorKind::Transient => "Transient error",
            AgentErrorKind::Permanent => "Permanent error",
        }
    }

    /// Get recovery advice for this error type
    pub(crate) fn recovery_advice(&self) -> &'static str {
        match self {
            AgentErrorKind::RateLimited => {
                "Will retry after delay. Consider reducing request frequency."
            }
            AgentErrorKind::TokenExhausted => {
                "Switching to alternative agent. Consider reducing context size."
            }
            AgentErrorKind::ApiUnavailable => "API server issue. Will retry automatically.",
            AgentErrorKind::NetworkError => {
                "Check your internet connection. Will retry automatically."
            }
            AgentErrorKind::AuthFailure => "Check API key or run 'agent auth' to authenticate.",
            AgentErrorKind::CommandNotFound => {
                "Agent binary not installed. See installation guidance."
            }
            AgentErrorKind::DiskFull => "Free up disk space and try again.",
            AgentErrorKind::ProcessKilled => {
                "Process was killed (possible OOM). Trying with smaller context."
            }
            AgentErrorKind::InvalidResponse => "Received malformed response. Retrying...",
            AgentErrorKind::Timeout => "Request timed out. Will retry with longer timeout.",
            AgentErrorKind::Transient => "Temporary issue. Will retry automatically.",
            AgentErrorKind::Permanent => "Unrecoverable error. Check agent logs for details.",
        }
    }

    /// Classify an error from exit code and output
    pub(crate) fn classify(exit_code: i32, stderr: &str) -> Self {
        let stderr_lower = stderr.to_lowercase();

        // Rate limiting indicators (API-side)
        if stderr_lower.contains("rate limit")
            || stderr_lower.contains("too many requests")
            || stderr_lower.contains("429")
            || stderr_lower.contains("quota exceeded")
        {
            return AgentErrorKind::RateLimited;
        }

        // Token/context exhaustion (API-side)
        if stderr_lower.contains("token")
            || stderr_lower.contains("context length")
            || stderr_lower.contains("maximum context")
            || stderr_lower.contains("too long")
            || stderr_lower.contains("input too large")
        {
            return AgentErrorKind::TokenExhausted;
        }

        // Network errors (client-side connectivity issues)
        // These indicate the request couldn't reach the API at all
        if stderr_lower.contains("connection refused")
            || stderr_lower.contains("network unreachable")
            || stderr_lower.contains("dns resolution")
            || stderr_lower.contains("name resolution")
            || stderr_lower.contains("no route to host")
            || stderr_lower.contains("network is down")
            || stderr_lower.contains("host unreachable")
            || stderr_lower.contains("connection reset")
            || stderr_lower.contains("broken pipe")
            || stderr_lower.contains("econnrefused")
            || stderr_lower.contains("enetunreach")
        {
            return AgentErrorKind::NetworkError;
        }

        // API unavailable (server-side issues - the request reached the server but it's having issues)
        if stderr_lower.contains("service unavailable")
            || stderr_lower.contains("503")
            || stderr_lower.contains("502")
            || stderr_lower.contains("504")
            || stderr_lower.contains("500")
            || stderr_lower.contains("internal server error")
            || stderr_lower.contains("bad gateway")
            || stderr_lower.contains("gateway timeout")
            || stderr_lower.contains("overloaded")
            || stderr_lower.contains("maintenance")
        {
            return AgentErrorKind::ApiUnavailable;
        }

        // Request timeout - specific error type for better handling
        if stderr_lower.contains("timeout")
            || stderr_lower.contains("timed out")
            || stderr_lower.contains("request timeout")
            || stderr_lower.contains("deadline exceeded")
        {
            return AgentErrorKind::Timeout;
        }

        // Auth failures
        if stderr_lower.contains("unauthorized")
            || stderr_lower.contains("authentication")
            || stderr_lower.contains("401")
            || stderr_lower.contains("api key")
            || stderr_lower.contains("invalid token")
            || stderr_lower.contains("forbidden")
            || stderr_lower.contains("403")
            || stderr_lower.contains("access denied")
        {
            return AgentErrorKind::AuthFailure;
        }

        // Disk space exhaustion
        if stderr_lower.contains("no space left")
            || stderr_lower.contains("disk full")
            || stderr_lower.contains("enospc")
            || stderr_lower.contains("out of disk")
            || stderr_lower.contains("insufficient storage")
        {
            return AgentErrorKind::DiskFull;
        }

        // Process killed (OOM or signals)
        // Exit code 137 = 128 + 9 (SIGKILL), 139 = 128 + 11 (SIGSEGV)
        if exit_code == 137
            || exit_code == 139
            || exit_code == -9
            || stderr_lower.contains("killed")
            || stderr_lower.contains("oom")
            || stderr_lower.contains("out of memory")
            || stderr_lower.contains("memory exhausted")
            || stderr_lower.contains("cannot allocate")
            || stderr_lower.contains("segmentation fault")
            || stderr_lower.contains("sigsegv")
            || stderr_lower.contains("sigkill")
        {
            return AgentErrorKind::ProcessKilled;
        }

        // Invalid JSON response
        if stderr_lower.contains("invalid json")
            || stderr_lower.contains("json parse")
            || stderr_lower.contains("unexpected token")
            || stderr_lower.contains("malformed")
            || stderr_lower.contains("truncated response")
            || stderr_lower.contains("incomplete response")
        {
            return AgentErrorKind::InvalidResponse;
        }

        // Command not found
        if exit_code == 127
            || exit_code == 126
            || stderr_lower.contains("command not found")
            || stderr_lower.contains("not found")
            || stderr_lower.contains("no such file")
            || stderr_lower.contains("permission denied")
            || stderr_lower.contains("operation not permitted")
        {
            return AgentErrorKind::CommandNotFound;
        }

        // Transient errors (exit codes that might succeed on retry)
        if exit_code == 1 && stderr_lower.contains("error") {
            return AgentErrorKind::Transient;
        }

        AgentErrorKind::Permanent
    }
}

/// Agent registry
pub(crate) struct AgentRegistry {
    agents: HashMap<String, AgentConfig>,
    fallback: FallbackConfig,
}

impl AgentRegistry {
    /// Create a new registry with default agents
    pub(crate) fn new() -> Result<Self, AgentConfigError> {
        let AgentsConfigFile { agents, fallback } =
            toml::from_str(DEFAULT_AGENTS_TOML).map_err(AgentConfigError::DefaultTemplateToml)?;

        let mut registry = Self {
            agents: HashMap::new(),
            fallback,
        };

        for (name, agent_toml) in agents {
            registry.register(&name, agent_toml.into());
        }

        Ok(registry)
    }

    /// Register a new agent
    pub(crate) fn register(&mut self, name: &str, config: AgentConfig) {
        self.agents.insert(name.to_string(), config);
    }

    /// Get agent configuration
    pub(crate) fn get(&self, name: &str) -> Option<&AgentConfig> {
        self.agents.get(name)
    }

    /// Check if agent exists
    #[cfg(test)]
    pub(crate) fn is_known(&self, name: &str) -> bool {
        self.agents.contains_key(name)
    }

    /// List all registered agents
    pub(crate) fn list(&self) -> Vec<(&str, &AgentConfig)> {
        self.agents.iter().map(|(k, v)| (k.as_str(), v)).collect()
    }

    /// Get command for developer role
    pub(crate) fn developer_cmd(&self, agent_name: &str) -> Option<String> {
        self.get(agent_name).map(|c| c.build_cmd(true, true, true))
    }

    /// Get command for reviewer role
    pub(crate) fn reviewer_cmd(&self, agent_name: &str) -> Option<String> {
        self.get(agent_name).map(|c| c.build_cmd(true, true, false))
    }

    /// Get the JSON parser type for an agent
    #[cfg(test)]
    pub(crate) fn parser_type(&self, agent_name: &str) -> JsonParserType {
        self.get(agent_name)
            .map(|c| c.json_parser)
            .unwrap_or(JsonParserType::Generic)
    }

    /// Load custom agents from a TOML configuration file
    ///
    /// Custom agents override built-in defaults if they have the same name.
    /// Returns the number of agents loaded, or an error if the file can't be parsed.
    pub(crate) fn load_from_file<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<usize, AgentConfigError> {
        match AgentsConfigFile::load_from_file(path)? {
            Some(config) => {
                let count = config.agents.len();
                for (name, agent_toml) in config.agents {
                    self.register(&name, agent_toml.into());
                }
                // Load fallback configuration
                self.fallback = config.fallback;
                Ok(count)
            }
            None => Ok(0),
        }
    }

    /// Create a new registry with default agents, then load custom agents from file
    ///
    /// This is the recommended way to create a registry for production use.
    /// Custom agents in the file will override built-in defaults.
    #[cfg(test)]
    pub(crate) fn with_config_file<P: AsRef<Path>>(path: P) -> Result<Self, AgentConfigError> {
        let mut registry = Self::new()?;
        registry.load_from_file(path)?;
        Ok(registry)
    }

    /// Create a new registry with merged config from multiple sources
    ///
    /// Loads config in order of increasing priority:
    /// 1. Built-in defaults
    /// 2. Global config (`~/.config/ralph/agents.toml`)
    /// 3. Per-repository config (`.agent/agents.toml` or `local_path`)
    ///
    /// Later sources override earlier ones. Returns a list of loaded config sources.
    pub(crate) fn with_merged_configs<P: AsRef<Path>>(
        local_path: P,
    ) -> Result<(Self, Vec<ConfigSource>, Vec<String>), AgentConfigError> {
        let mut registry = Self::new()?;
        let mut sources = Vec::new();
        let mut warnings = Vec::new();

        // 1. Try global config
        if let Some(global_path) = global_agents_config_path() {
            if global_path.exists() {
                match registry.load_from_file(&global_path) {
                    Ok(count) => {
                        sources.push(ConfigSource {
                            path: global_path,
                            agents_loaded: count,
                        });
                    }
                    Err(e) => {
                        // Global config is optional: continue, but return a warning for the caller
                        warnings.push(format!(
                            "Failed to load global config from {}: {}",
                            global_path.display(),
                            e
                        ));
                    }
                }
            }
        }

        // 2. Try local (per-repo) config
        let local_path = local_path.as_ref();
        if local_path.exists() {
            let count = registry.load_from_file(local_path)?;
            sources.push(ConfigSource {
                path: local_path.to_path_buf(),
                agents_loaded: count,
            });
        }

        Ok((registry, sources, warnings))
    }

    /// Get the fallback configuration
    pub(crate) fn fallback_config(&self) -> &FallbackConfig {
        &self.fallback
    }

    /// Set the fallback configuration
    #[cfg(test)]
    pub(crate) fn set_fallback(&mut self, fallback: FallbackConfig) {
        self.fallback = fallback;
    }

    /// Get all fallback agents for a role that are registered in this registry
    pub(crate) fn available_fallbacks(&self, role: AgentRole) -> Vec<&str> {
        self.fallback
            .get_fallbacks(role)
            .iter()
            .filter(|name| self.is_agent_available(name))
            // Agents with can_commit=false are chat-only / non-tool agents and will stall Ralph.
            .filter(|name| self.get(name.as_str()).is_some_and(|cfg| cfg.can_commit))
            .map(|s| s.as_str())
            .collect()
    }

    /// Validate that agent chains are configured for both roles.
    ///
    /// Returns Ok(()) if both developer and reviewer chains are configured,
    /// or an Err with a helpful error message if not.
    pub(crate) fn validate_agent_chains(&self) -> Result<(), String> {
        let has_developer = self.fallback.has_fallbacks(AgentRole::Developer);
        let has_reviewer = self.fallback.has_fallbacks(AgentRole::Reviewer);

        if !has_developer && !has_reviewer {
            return Err("No agent chain configured.\n\
                Please add an [agent_chain] section to your agents.toml file.\n\
                Run 'ralph --init' to create a default configuration."
                .to_string());
        }

        if !has_developer {
            return Err("No developer agent chain configured.\n\
                Add 'developer = [\"your-agent\", ...]' to your [agent_chain] section.\n\
                Use --list-agents to see available agents."
                .to_string());
        }

        if !has_reviewer {
            return Err("No reviewer agent chain configured.\n\
                Add 'reviewer = [\"your-agent\", ...]' to your [agent_chain] section.\n\
                Use --list-agents to see available agents."
                .to_string());
        }

        // Sanity check: ensure there is at least one workflow-capable agent per role.
        // Agents with can_commit=false are chat-only / non-tool agents and will stall Ralph.
        for role in [AgentRole::Developer, AgentRole::Reviewer] {
            let chain = self.fallback.get_fallbacks(role);
            let has_capable = chain
                .iter()
                .any(|name| self.get(name).is_some_and(|cfg| cfg.can_commit));
            if !has_capable {
                return Err(format!(
                    "No workflow-capable agents found for {}.\n\
	                    All agents in the {} chain have can_commit=false.\n\
                    Fix: set can_commit=true for at least one agent or update [agent_chain].",
                    role, role
                ));
            }
        }

        Ok(())
    }

    /// Check if an agent is available (command exists and is executable)
    pub(crate) fn is_agent_available(&self, name: &str) -> bool {
        if let Some(config) = self.get(name) {
            let Ok(parts) = crate::utils::split_command(&config.cmd) else {
                return false;
            };
            let Some(base_cmd) = parts.first() else {
                return false;
            };

            // Check if the command exists in PATH (portable; avoids shelling out)
            which::which(base_cmd).is_ok()
        } else {
            false
        }
    }

    /// List all available (installed) agents
    pub(crate) fn list_available(&self) -> Vec<&str> {
        self.agents
            .keys()
            .filter(|name| self.is_agent_available(name))
            .map(|s| s.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn write_stub_executable(dir: &Path, name: &str) {
        #[cfg(windows)]
        {
            let path = dir.join(format!("{}.cmd", name));
            std::fs::write(&path, "@echo off\r\nexit /b 0\r\n").unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = dir.join(name);
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).unwrap();
        }
    }

    #[test]
    fn test_agent_registry_defaults() {
        let registry = AgentRegistry::new().unwrap();

        // Original agents
        assert!(registry.is_known("claude"));
        assert!(registry.is_known("codex"));
        assert!(registry.is_known("opencode"));
        assert!(registry.is_known("aider"));

        // New agents
        assert!(registry.is_known("goose"));
        assert!(registry.is_known("cline"));
        assert!(registry.is_known("continue"));
        assert!(registry.is_known("amazon-q"));
        assert!(registry.is_known("gemini"));

        // Lower-cost / open-source agents
        assert!(registry.is_known("qwen"));
        assert!(registry.is_known("vibe"));
        assert!(registry.is_known("llama-cli"));
        assert!(registry.is_known("aichat"));

        // Additional popular CLI tools
        assert!(registry.is_known("cursor"));
        assert!(registry.is_known("plandex"));
        assert!(registry.is_known("ollama"));

        assert!(!registry.is_known("unknown_agent"));
    }

    #[test]
    fn test_agent_get_cmd() {
        let registry = AgentRegistry::new().unwrap();

        let claude = registry.get("claude").unwrap();
        assert!(claude.cmd.contains("claude"));

        let codex = registry.get("codex").unwrap();
        assert!(codex.cmd.contains("codex"));
    }

    #[test]
    fn test_agent_build_cmd() {
        let registry = AgentRegistry::new().unwrap();
        let codex = registry.get("codex").unwrap();

        // Codex doesn't require verbose with JSON, so verbose=false should exclude it
        let cmd = codex.build_cmd(true, true, false);
        assert!(cmd.contains("codex"));
        assert!(cmd.contains("json"));
        assert!(cmd.contains("full-auto")); // Codex uses --full-auto for automatic execution
        assert!(!cmd.contains("verbose"));

        // With verbose=true, it should be included
        let cmd_verbose = codex.build_cmd(true, true, true);
        // Codex has empty verbose_flag, so still no verbose in output
        assert!(!cmd_verbose.contains("verbose"));
    }

    #[test]
    fn test_claude_requires_verbose_with_stream_json() {
        let registry = AgentRegistry::new().unwrap();
        let claude = registry.get("claude").unwrap();

        // Claude requires --verbose when using --output-format=stream-json
        // Even when verbose=false is passed, it should include --verbose
        let cmd = claude.build_cmd(true, true, false);
        assert!(cmd.contains("claude"));
        assert!(cmd.contains("stream-json"));
        assert!(cmd.contains("skip-permissions"));
        assert!(
            cmd.contains("verbose"),
            "Claude should always include --verbose with stream-json"
        );

        // With verbose=true, it should also be included
        let cmd_verbose = claude.build_cmd(true, true, true);
        assert!(cmd_verbose.contains("verbose"));

        // Without JSON, verbose should follow the parameter
        let cmd_no_json = claude.build_cmd(false, true, false);
        assert!(!cmd_no_json.contains("verbose"));
        assert!(!cmd_no_json.contains("stream-json"));

        let cmd_no_json_verbose = claude.build_cmd(false, true, true);
        assert!(cmd_no_json_verbose.contains("verbose"));
    }

    #[test]
    fn test_agent_developer_cmd() {
        let registry = AgentRegistry::new().unwrap();
        let cmd = registry.developer_cmd("claude").unwrap();
        assert!(cmd.contains("claude"));
        assert!(cmd.contains("json"));
    }

    #[test]
    fn test_agent_reviewer_cmd() {
        let registry = AgentRegistry::new().unwrap();
        let cmd = registry.reviewer_cmd("codex").unwrap();
        assert!(cmd.contains("codex"));
        assert!(cmd.contains("json"));
    }

    #[test]
    fn test_claude_reviewer_cmd_includes_verbose() {
        // Regression test: Claude as reviewer must include --verbose with stream-json
        // See: "Error: When using --print, --output-format=stream-json requires --verbose"
        let registry = AgentRegistry::new().unwrap();
        let cmd = registry.reviewer_cmd("claude").unwrap();
        assert!(cmd.contains("claude"));
        assert!(cmd.contains("stream-json"));
        assert!(
            cmd.contains("verbose"),
            "Claude reviewer must include --verbose for stream-json to work"
        );
    }

    #[test]
    fn test_register_custom_agent() {
        let mut registry = AgentRegistry::new().unwrap();

        registry.register(
            "testbot",
            AgentConfig {
                cmd: "testbot run".to_string(),
                output_flag: "--output-json".to_string(),
                yolo_flag: "--auto".to_string(),
                verbose_flag: String::new(),
                can_commit: true,
                json_parser: JsonParserType::Claude,
                model_flag: None,
            },
        );

        assert!(registry.is_known("testbot"));
        let config = registry.get("testbot").unwrap();
        assert_eq!(config.cmd, "testbot run");
        assert_eq!(config.json_parser, JsonParserType::Claude);
    }

    #[test]
    fn test_can_commit() {
        let registry = AgentRegistry::new().unwrap();

        let claude = registry.get("claude").unwrap();
        assert!(claude.can_commit);

        let codex = registry.get("codex").unwrap();
        assert!(codex.can_commit);
    }

    #[test]
    fn test_json_parser_type_parse() {
        assert_eq!(JsonParserType::parse("claude"), JsonParserType::Claude);
        assert_eq!(JsonParserType::parse("CLAUDE"), JsonParserType::Claude);
        assert_eq!(JsonParserType::parse("codex"), JsonParserType::Codex);
        assert_eq!(JsonParserType::parse("gemini"), JsonParserType::Gemini);
        assert_eq!(JsonParserType::parse("GEMINI"), JsonParserType::Gemini);
        assert_eq!(JsonParserType::parse("generic"), JsonParserType::Generic);
        assert_eq!(JsonParserType::parse("none"), JsonParserType::Generic);
        assert_eq!(JsonParserType::parse("raw"), JsonParserType::Generic);
        assert_eq!(JsonParserType::parse("unknown"), JsonParserType::Generic);
    }

    #[test]
    fn test_json_parser_type_display() {
        assert_eq!(format!("{}", JsonParserType::Claude), "claude");
        assert_eq!(format!("{}", JsonParserType::Codex), "codex");
        assert_eq!(format!("{}", JsonParserType::Gemini), "gemini");
        assert_eq!(format!("{}", JsonParserType::Generic), "generic");
    }

    #[test]
    fn test_default_agent_parser_types() {
        let registry = AgentRegistry::new().unwrap();

        assert_eq!(registry.parser_type("claude"), JsonParserType::Claude);
        assert_eq!(registry.parser_type("codex"), JsonParserType::Codex);
        assert_eq!(registry.parser_type("gemini"), JsonParserType::Gemini);
        assert_eq!(registry.parser_type("opencode"), JsonParserType::OpenCode);
        assert_eq!(registry.parser_type("aider"), JsonParserType::Generic);
        assert_eq!(registry.parser_type("unknown"), JsonParserType::Generic);
    }

    #[test]
    fn test_agent_config_from_toml() {
        let toml = AgentConfigToml {
            cmd: "myagent run".to_string(),
            output_flag: "--json".to_string(),
            yolo_flag: "--auto".to_string(),
            verbose_flag: "--verbose".to_string(),
            can_commit: false,
            json_parser: "claude".to_string(),
            model_flag: Some("-m provider/model".to_string()),
        };

        let config: AgentConfig = toml.into();
        assert_eq!(config.cmd, "myagent run");
        assert_eq!(config.output_flag, "--json");
        assert_eq!(config.yolo_flag, "--auto");
        assert_eq!(config.verbose_flag, "--verbose");
        assert!(!config.can_commit);
        assert_eq!(config.json_parser, JsonParserType::Claude);
        assert_eq!(config.model_flag, Some("-m provider/model".to_string()));
    }

    #[test]
    fn test_agent_config_toml_defaults() {
        // Test that serde defaults work correctly
        let toml_str = r#"cmd = "myagent""#;
        let config: AgentConfigToml = toml::from_str(toml_str).unwrap();

        assert_eq!(config.cmd, "myagent");
        assert_eq!(config.output_flag, "");
        assert_eq!(config.yolo_flag, "");
        assert_eq!(config.verbose_flag, "");
        assert!(config.can_commit); // default is true
        assert_eq!(config.json_parser, "");
    }

    #[test]
    fn test_agents_config_file_parse() {
        let toml_str = r#"
[agents.custom1]
cmd = "custom1-cli"
output_flag = "--json"
yolo_flag = "--yes"
can_commit = true
json_parser = "codex"

[agents.custom2]
cmd = "custom2-tool run"
json_parser = "claude"
"#;
        let config: AgentsConfigFile = toml::from_str(toml_str).unwrap();

        assert_eq!(config.agents.len(), 2);
        assert!(config.agents.contains_key("custom1"));
        assert!(config.agents.contains_key("custom2"));

        let custom1 = &config.agents["custom1"];
        assert_eq!(custom1.cmd, "custom1-cli");
        assert_eq!(custom1.output_flag, "--json");
        assert_eq!(custom1.json_parser, "codex");

        let custom2 = &config.agents["custom2"];
        assert_eq!(custom2.cmd, "custom2-tool run");
        assert!(custom2.can_commit); // default
        assert_eq!(custom2.json_parser, "claude");
    }

    #[test]
    fn test_load_from_file_nonexistent() {
        let mut registry = AgentRegistry::new().unwrap();
        let result = registry.load_from_file("/nonexistent/path/agents.toml");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_load_from_file_with_temp() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("agents.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[agents.testbot]
cmd = "testbot exec"
output_flag = "--output-json"
yolo_flag = "--auto"
json_parser = "codex"
"#
        )
        .unwrap();

        let mut registry = AgentRegistry::new().unwrap();
        let loaded = registry.load_from_file(&config_path).unwrap();

        assert_eq!(loaded, 1);
        assert!(registry.is_known("testbot"));

        let config = registry.get("testbot").unwrap();
        assert_eq!(config.cmd, "testbot exec");
        assert_eq!(config.output_flag, "--output-json");
        assert_eq!(config.json_parser, JsonParserType::Codex);
    }

    #[test]
    fn test_with_config_file_overrides_defaults() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("agents.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[agents.claude]
cmd = "claude-custom -p"
output_flag = "--custom-json"
yolo_flag = "--skip"
json_parser = "codex"
"#
        )
        .unwrap();

        let registry = AgentRegistry::with_config_file(&config_path).unwrap();

        let config = registry.get("claude").unwrap();
        assert_eq!(config.cmd, "claude-custom -p");
        assert_eq!(config.output_flag, "--custom-json");
        assert_eq!(config.json_parser, JsonParserType::Codex);
    }

    #[test]
    fn test_new_agent_configs() {
        let registry = AgentRegistry::new().unwrap();

        // Test Goose config
        let goose = registry.get("goose").unwrap();
        assert!(goose.cmd.contains("goose"));
        assert_eq!(goose.json_parser, JsonParserType::Generic);

        // Test Cline config
        let cline = registry.get("cline").unwrap();
        assert!(cline.cmd.contains("cline"));

        // Test Continue config
        let cont = registry.get("continue").unwrap();
        assert!(cont.cmd.contains("cn"));

        // Test Amazon Q config
        let q = registry.get("amazon-q").unwrap();
        assert!(q.cmd.contains("q"));
        assert!(q.yolo_flag.contains("trust"));

        // Test Gemini config
        let gemini = registry.get("gemini").unwrap();
        assert!(gemini.cmd.contains("gemini"));
    }

    #[test]
    fn test_lower_cost_agent_configs() {
        let registry = AgentRegistry::new().unwrap();

        // Test Qwen Code config (Alibaba's Qwen3-Coder)
        let qwen = registry.get("qwen").unwrap();
        assert!(qwen.cmd.contains("qwen"));
        assert_eq!(qwen.cmd, "qwen -p");
        assert!(qwen.output_flag.contains("stream-json"));
        assert_eq!(qwen.yolo_flag, "--yolo");
        assert_eq!(qwen.verbose_flag, "--debug");
        assert!(qwen.can_commit);
        // Qwen uses Claude parser (forked from Gemini CLI with Claude-compatible output)
        assert_eq!(qwen.json_parser, JsonParserType::Claude);

        // Test Mistral Vibe config
        let vibe = registry.get("vibe").unwrap();
        assert!(vibe.cmd.contains("vibe"));
        assert_eq!(vibe.cmd, "vibe --prompt");
        assert_eq!(vibe.output_flag, ""); // No JSON streaming support
        assert_eq!(vibe.yolo_flag, "--auto-approve");
        assert!(vibe.can_commit);
        assert_eq!(vibe.json_parser, JsonParserType::Generic);

        // Test llama-cli config (llama.cpp)
        let llama = registry.get("llama-cli").unwrap();
        assert!(llama.cmd.contains("llama-cli"));
        assert!(llama.cmd.contains("-m")); // Local model path (no auto-download)
        assert!(llama.cmd.contains("-cnv")); // Conversation mode
        assert_eq!(llama.output_flag, ""); // No native JSON streaming
        assert_eq!(llama.yolo_flag, ""); // No autonomous mode
        assert_eq!(llama.verbose_flag, "-v");
        assert!(!llama.can_commit); // Local model without tool use
        assert_eq!(llama.json_parser, JsonParserType::Generic);

        // Test AIChat config (multi-provider LLM CLI)
        let aichat = registry.get("aichat").unwrap();
        assert!(aichat.cmd.contains("aichat"));
        assert_eq!(aichat.output_flag, ""); // No CLI JSON output
        assert_eq!(aichat.yolo_flag, ""); // No autonomous mode
        assert!(!aichat.can_commit); // General chat, no tool use
        assert_eq!(aichat.json_parser, JsonParserType::Generic);
    }

    #[test]
    fn test_qwen_build_cmd() {
        let registry = AgentRegistry::new().unwrap();
        let qwen = registry.get("qwen").unwrap();

        // Build developer command (json=true, yolo=true, verbose=true)
        let dev_cmd = qwen.build_cmd(true, true, true);
        assert!(dev_cmd.contains("qwen -p"));
        assert!(dev_cmd.contains("--output-format stream-json"));
        assert!(dev_cmd.contains("--yolo"));
        assert!(dev_cmd.contains("--debug"));

        // Build reviewer command (json=true, yolo=true, verbose=false)
        // Qwen doesn't require verbose with stream-json (unlike Claude)
        let rev_cmd = qwen.build_cmd(true, true, false);
        assert!(rev_cmd.contains("qwen -p"));
        assert!(rev_cmd.contains("--output-format stream-json"));
        assert!(rev_cmd.contains("--yolo"));
        assert!(!rev_cmd.contains("--debug"));
    }

    #[test]
    fn test_vibe_build_cmd() {
        let registry = AgentRegistry::new().unwrap();
        let vibe = registry.get("vibe").unwrap();

        // Build command with yolo mode
        let cmd = vibe.build_cmd(true, true, true);
        assert!(cmd.contains("vibe --prompt"));
        assert!(cmd.contains("--auto-approve"));
        // No output_flag or verbose_flag for vibe
        assert!(!cmd.contains("--json"));
    }

    #[test]
    fn test_lower_cost_agents_in_chain() {
        let registry = AgentRegistry::new().unwrap();
        let fallback = registry.fallback_config();

        // Verify new agents are in the developer fallback chain
        assert!(
            fallback.developer.contains(&"qwen".to_string()),
            "qwen should be in developer fallback chain"
        );
        assert!(
            fallback.developer.contains(&"vibe".to_string()),
            "vibe should be in developer fallback chain"
        );
        assert!(
            !fallback.developer.contains(&"llama-cli".to_string()),
            "llama-cli should not be in developer chain by default (can_commit=false)"
        );
        assert!(
            !fallback.developer.contains(&"aichat".to_string()),
            "aichat should not be in developer chain by default (can_commit=false)"
        );

        // Verify qwen and vibe are in reviewer chain (they support tool use)
        assert!(
            fallback.reviewer.contains(&"qwen".to_string()),
            "qwen should be in reviewer fallback chain"
        );
        assert!(
            fallback.reviewer.contains(&"vibe".to_string()),
            "vibe should be in reviewer fallback chain"
        );

        // llama-cli and aichat are NOT in reviewer chain (no tool use)
        assert!(
            !fallback.reviewer.contains(&"llama-cli".to_string()),
            "llama-cli should not be in reviewer chain (no tool use)"
        );
        assert!(
            !fallback.reviewer.contains(&"aichat".to_string()),
            "aichat should not be in reviewer chain (no tool use)"
        );
    }

    #[test]
    fn test_cursor_agent_config() {
        let registry = AgentRegistry::new().unwrap();
        let cursor = registry.get("cursor").unwrap();
        assert!(cursor.cmd.contains("agent"));
        assert!(cursor.cmd.contains("-p"));
        assert_eq!(cursor.output_flag, "--output-format text");
        assert!(cursor.can_commit);
        assert_eq!(cursor.json_parser, JsonParserType::Generic);
    }

    #[test]
    fn test_plandex_agent_config() {
        let registry = AgentRegistry::new().unwrap();
        let plandex = registry.get("plandex").unwrap();
        assert!(plandex.cmd.contains("plandex"));
        assert!(plandex.cmd.contains("tell"));
        assert!(plandex.yolo_flag.contains("--apply"));
        assert!(plandex.yolo_flag.contains("--commit"));
        assert!(plandex.can_commit);
        assert_eq!(plandex.json_parser, JsonParserType::Generic);
    }

    #[test]
    fn test_ollama_agent_config() {
        let registry = AgentRegistry::new().unwrap();
        let ollama = registry.get("ollama").unwrap();
        assert!(ollama.cmd.contains("ollama"));
        assert!(ollama.cmd.contains("run"));
        assert!(!ollama.can_commit); // Local model, no tool use
        assert_eq!(ollama.json_parser, JsonParserType::Generic);
    }

    #[test]
    fn test_new_agents_in_chain() {
        let registry = AgentRegistry::new().unwrap();
        let fallback = registry.fallback_config();

        // cursor and plandex should be in both chains (they support tool use)
        assert!(
            fallback.developer.contains(&"cursor".to_string()),
            "cursor should be in developer fallback chain"
        );
        assert!(
            fallback.developer.contains(&"plandex".to_string()),
            "plandex should be in developer fallback chain"
        );
        assert!(
            fallback.reviewer.contains(&"cursor".to_string()),
            "cursor should be in reviewer fallback chain"
        );
        assert!(
            fallback.reviewer.contains(&"plandex".to_string()),
            "plandex should be in reviewer fallback chain"
        );

        // Chat-only / local-model agents are not included by default
        assert!(
            !fallback.developer.contains(&"ollama".to_string()),
            "ollama should not be in developer chain by default (can_commit=false)"
        );
        assert!(
            !fallback.reviewer.contains(&"ollama".to_string()),
            "ollama should not be in reviewer chain (can_commit=false)"
        );
    }

    #[test]
    fn test_qwen_developer_and_reviewer_cmd() {
        let registry = AgentRegistry::new().unwrap();

        // Developer cmd should include all flags
        let dev_cmd = registry.developer_cmd("qwen").unwrap();
        assert!(dev_cmd.contains("qwen -p"));
        assert!(dev_cmd.contains("stream-json"));
        assert!(dev_cmd.contains("--yolo"));

        // Reviewer cmd should also work
        let rev_cmd = registry.reviewer_cmd("qwen").unwrap();
        assert!(rev_cmd.contains("qwen -p"));
        assert!(rev_cmd.contains("stream-json"));
    }

    #[test]
    fn test_vibe_developer_and_reviewer_cmd() {
        let registry = AgentRegistry::new().unwrap();

        let dev_cmd = registry.developer_cmd("vibe").unwrap();
        assert!(dev_cmd.contains("vibe --prompt"));
        assert!(dev_cmd.contains("--auto-approve"));

        let rev_cmd = registry.reviewer_cmd("vibe").unwrap();
        assert!(rev_cmd.contains("vibe --prompt"));
        assert!(rev_cmd.contains("--auto-approve"));
    }

    #[test]
    fn test_fallback_config_defaults() {
        let config = FallbackConfig::default();
        assert!(config.developer.is_empty());
        assert!(config.reviewer.is_empty());
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
        assert_eq!(config.max_backoff_ms, 60000);
        assert_eq!(config.max_cycles, 3);
    }

    #[test]
    fn test_fallback_config_calculate_backoff() {
        let config = FallbackConfig {
            retry_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_backoff_ms: 60000,
            ..Default::default()
        };

        // Cycle 0: 1000 * 2^0 = 1000
        assert_eq!(config.calculate_backoff(0), 1000);
        // Cycle 1: 1000 * 2^1 = 2000
        assert_eq!(config.calculate_backoff(1), 2000);
        // Cycle 2: 1000 * 2^2 = 4000
        assert_eq!(config.calculate_backoff(2), 4000);
        // Cycle 3: 1000 * 2^3 = 8000
        assert_eq!(config.calculate_backoff(3), 8000);
        // Cycle 6: 1000 * 2^6 = 64000, capped at 60000
        assert_eq!(config.calculate_backoff(6), 60000);
    }

    #[test]
    fn test_fallback_config_get_fallbacks() {
        let config = FallbackConfig {
            developer: vec!["claude".to_string(), "codex".to_string()],
            reviewer: vec!["codex".to_string(), "goose".to_string()],
            ..Default::default()
        };

        assert_eq!(
            config.get_fallbacks(AgentRole::Developer),
            &["claude", "codex"]
        );
        assert_eq!(
            config.get_fallbacks(AgentRole::Reviewer),
            &["codex", "goose"]
        );
    }

    #[test]
    fn test_fallback_config_has_fallbacks() {
        let mut config = FallbackConfig::default();
        assert!(!config.has_fallbacks(AgentRole::Developer));
        assert!(!config.has_fallbacks(AgentRole::Reviewer));

        config.developer = vec!["claude".to_string()];
        assert!(config.has_fallbacks(AgentRole::Developer));
        assert!(!config.has_fallbacks(AgentRole::Reviewer));
    }

    #[test]
    fn test_agent_error_kind_classify() {
        // Rate limiting
        assert_eq!(
            AgentErrorKind::classify(1, "rate limit exceeded"),
            AgentErrorKind::RateLimited
        );
        assert_eq!(
            AgentErrorKind::classify(1, "Error: 429 Too Many Requests"),
            AgentErrorKind::RateLimited
        );
        assert_eq!(
            AgentErrorKind::classify(1, "quota exceeded"),
            AgentErrorKind::RateLimited
        );

        // Token exhaustion
        assert_eq!(
            AgentErrorKind::classify(1, "context length exceeded"),
            AgentErrorKind::TokenExhausted
        );
        assert_eq!(
            AgentErrorKind::classify(1, "maximum token limit"),
            AgentErrorKind::TokenExhausted
        );

        // Network errors (client-side connectivity)
        assert_eq!(
            AgentErrorKind::classify(1, "connection refused"),
            AgentErrorKind::NetworkError
        );
        assert_eq!(
            AgentErrorKind::classify(1, "network unreachable"),
            AgentErrorKind::NetworkError
        );
        assert_eq!(
            AgentErrorKind::classify(1, "DNS resolution failed"),
            AgentErrorKind::NetworkError
        );
        assert_eq!(
            AgentErrorKind::classify(1, "ECONNREFUSED"),
            AgentErrorKind::NetworkError
        );

        // API unavailable (server-side)
        assert_eq!(
            AgentErrorKind::classify(1, "service unavailable"),
            AgentErrorKind::ApiUnavailable
        );
        assert_eq!(
            AgentErrorKind::classify(1, "503 Service Unavailable"),
            AgentErrorKind::ApiUnavailable
        );
        assert_eq!(
            AgentErrorKind::classify(1, "internal server error"),
            AgentErrorKind::ApiUnavailable
        );

        // Timeouts have their own type now
        assert_eq!(
            AgentErrorKind::classify(1, "request timeout"),
            AgentErrorKind::Timeout
        );
        assert_eq!(
            AgentErrorKind::classify(1, "deadline exceeded"),
            AgentErrorKind::Timeout
        );

        // Disk space exhaustion
        assert_eq!(
            AgentErrorKind::classify(1, "no space left on device"),
            AgentErrorKind::DiskFull
        );
        assert_eq!(
            AgentErrorKind::classify(1, "ENOSPC"),
            AgentErrorKind::DiskFull
        );

        // Process killed (OOM)
        assert_eq!(
            AgentErrorKind::classify(137, ""),
            AgentErrorKind::ProcessKilled
        );
        assert_eq!(
            AgentErrorKind::classify(1, "out of memory"),
            AgentErrorKind::ProcessKilled
        );
        assert_eq!(
            AgentErrorKind::classify(1, "killed by signal"),
            AgentErrorKind::ProcessKilled
        );

        // Invalid response
        assert_eq!(
            AgentErrorKind::classify(1, "invalid json response"),
            AgentErrorKind::InvalidResponse
        );
        assert_eq!(
            AgentErrorKind::classify(1, "truncated response"),
            AgentErrorKind::InvalidResponse
        );

        // Auth failures
        assert_eq!(
            AgentErrorKind::classify(1, "unauthorized"),
            AgentErrorKind::AuthFailure
        );
        assert_eq!(
            AgentErrorKind::classify(1, "invalid api key"),
            AgentErrorKind::AuthFailure
        );
        assert_eq!(
            AgentErrorKind::classify(1, "403 forbidden"),
            AgentErrorKind::AuthFailure
        );

        // Command not found
        assert_eq!(
            AgentErrorKind::classify(127, ""),
            AgentErrorKind::CommandNotFound
        );
        assert_eq!(
            AgentErrorKind::classify(126, "permission denied"),
            AgentErrorKind::CommandNotFound
        );
        assert_eq!(
            AgentErrorKind::classify(1, "command not found"),
            AgentErrorKind::CommandNotFound
        );
    }

    #[test]
    fn test_agent_error_kind_should_retry() {
        assert!(AgentErrorKind::RateLimited.should_retry());
        assert!(AgentErrorKind::ApiUnavailable.should_retry());
        assert!(AgentErrorKind::NetworkError.should_retry());
        assert!(AgentErrorKind::Timeout.should_retry());
        assert!(AgentErrorKind::InvalidResponse.should_retry());
        assert!(AgentErrorKind::Transient.should_retry());

        assert!(!AgentErrorKind::TokenExhausted.should_retry());
        assert!(!AgentErrorKind::AuthFailure.should_retry());
        assert!(!AgentErrorKind::CommandNotFound.should_retry());
        assert!(!AgentErrorKind::DiskFull.should_retry());
        assert!(!AgentErrorKind::ProcessKilled.should_retry());
        assert!(!AgentErrorKind::Permanent.should_retry());
    }

    #[test]
    fn test_agent_error_kind_should_fallback() {
        assert!(AgentErrorKind::TokenExhausted.should_fallback());
        assert!(AgentErrorKind::AuthFailure.should_fallback());
        assert!(AgentErrorKind::CommandNotFound.should_fallback());
        assert!(AgentErrorKind::ProcessKilled.should_fallback());

        assert!(!AgentErrorKind::RateLimited.should_fallback());
        assert!(!AgentErrorKind::ApiUnavailable.should_fallback());
        assert!(!AgentErrorKind::NetworkError.should_fallback());
        assert!(!AgentErrorKind::Transient.should_fallback());
        assert!(!AgentErrorKind::DiskFull.should_fallback());
        assert!(!AgentErrorKind::Permanent.should_fallback());
    }

    #[test]
    fn test_agent_error_kind_is_unrecoverable() {
        assert!(AgentErrorKind::DiskFull.is_unrecoverable());
        assert!(AgentErrorKind::Permanent.is_unrecoverable());

        assert!(!AgentErrorKind::RateLimited.is_unrecoverable());
        assert!(!AgentErrorKind::NetworkError.is_unrecoverable());
        assert!(!AgentErrorKind::CommandNotFound.is_unrecoverable());
    }

    #[test]
    fn test_agent_error_kind_suggests_smaller_context() {
        assert!(AgentErrorKind::TokenExhausted.suggests_smaller_context());
        assert!(AgentErrorKind::ProcessKilled.suggests_smaller_context());

        assert!(!AgentErrorKind::RateLimited.suggests_smaller_context());
        assert!(!AgentErrorKind::NetworkError.suggests_smaller_context());
    }

    #[test]
    fn test_agent_error_kind_suggested_wait_ms() {
        assert!(AgentErrorKind::RateLimited.suggested_wait_ms() > 0);
        assert!(AgentErrorKind::ApiUnavailable.suggested_wait_ms() > 0);
        assert!(AgentErrorKind::NetworkError.suggested_wait_ms() > 0);
        assert!(AgentErrorKind::Timeout.suggested_wait_ms() > 0);
        assert_eq!(AgentErrorKind::DiskFull.suggested_wait_ms(), 0);
        assert_eq!(AgentErrorKind::Permanent.suggested_wait_ms(), 0);
    }

    #[test]
    fn test_agent_error_kind_description_and_advice() {
        // Verify all error kinds have descriptions and advice
        let all_kinds = [
            AgentErrorKind::RateLimited,
            AgentErrorKind::TokenExhausted,
            AgentErrorKind::ApiUnavailable,
            AgentErrorKind::NetworkError,
            AgentErrorKind::AuthFailure,
            AgentErrorKind::CommandNotFound,
            AgentErrorKind::DiskFull,
            AgentErrorKind::ProcessKilled,
            AgentErrorKind::InvalidResponse,
            AgentErrorKind::Timeout,
            AgentErrorKind::Transient,
            AgentErrorKind::Permanent,
        ];

        for kind in all_kinds {
            assert!(!kind.description().is_empty());
            assert!(!kind.recovery_advice().is_empty());
        }

        // Specific checks
        assert!(AgentErrorKind::NetworkError.is_network_error());
        assert!(AgentErrorKind::Timeout.is_network_error());
        assert!(!AgentErrorKind::ApiUnavailable.is_network_error());
        assert!(AgentErrorKind::CommandNotFound.is_command_not_found());
        assert!(!AgentErrorKind::NetworkError.is_command_not_found());
    }

    #[test]
    fn test_registry_available_fallbacks() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let original_path = std::env::var_os("PATH");
        let dir = tempfile::tempdir().unwrap();

        write_stub_executable(dir.path(), "claude");
        write_stub_executable(dir.path(), "codex");

        let mut new_paths = vec![dir.path().to_path_buf()];
        if let Some(p) = &original_path {
            new_paths.extend(std::env::split_paths(p));
        }
        let joined = std::env::join_paths(new_paths).unwrap();
        std::env::set_var("PATH", &joined);

        let mut registry = AgentRegistry::new().unwrap();
        registry.set_fallback(FallbackConfig {
            developer: vec![
                "claude".to_string(),
                "nonexistent".to_string(),
                "codex".to_string(),
            ],
            reviewer: vec![],
            ..Default::default()
        });

        let fallbacks = registry.available_fallbacks(AgentRole::Developer);
        assert!(fallbacks.contains(&"claude"));
        assert!(fallbacks.contains(&"codex"));
        assert!(!fallbacks.contains(&"nonexistent"));

        if let Some(p) = original_path {
            std::env::set_var("PATH", p);
        } else {
            std::env::remove_var("PATH");
        }
    }

    #[test]
    fn test_fallback_config_from_toml() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("agents.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[agent_chain]
developer = ["claude", "codex", "goose"]
reviewer = ["codex", "claude"]
max_retries = 5
retry_delay_ms = 2000

[agents.testbot]
cmd = "testbot exec"
"#
        )
        .unwrap();

        let registry = AgentRegistry::with_config_file(&config_path).unwrap();
        let fallback = registry.fallback_config();

        assert_eq!(fallback.developer, vec!["claude", "codex", "goose"]);
        assert_eq!(fallback.reviewer, vec!["codex", "claude"]);
        assert_eq!(fallback.max_retries, 5);
        assert_eq!(fallback.retry_delay_ms, 2000);
    }

    #[test]
    fn test_agent_role_display() {
        assert_eq!(format!("{}", AgentRole::Developer), "developer");
        assert_eq!(format!("{}", AgentRole::Reviewer), "reviewer");
    }

    #[test]
    fn test_agent_chain_config_loading() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("agents.toml");

        // Use the new [agent_chain] section name
        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[agent_chain]
developer = ["opencode", "claude", "codex"]
reviewer = ["claude", "codex"]
max_retries = 2
retry_delay_ms = 500
"#
        )
        .unwrap();

        let registry = AgentRegistry::with_config_file(&config_path).unwrap();
        let fallback = registry.fallback_config();

        // Loads agent_chain configuration
        assert_eq!(fallback.developer, vec!["opencode", "claude", "codex"]);
        assert_eq!(fallback.reviewer, vec!["claude", "codex"]);
        assert_eq!(fallback.max_retries, 2);
        assert_eq!(fallback.retry_delay_ms, 500);
    }

    #[test]
    fn test_ensure_config_exists_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".agent/agents.toml");

        // File should not exist initially
        assert!(!config_path.exists());

        // ensure_config_exists should create it
        let result = AgentsConfigFile::ensure_config_exists(&config_path).unwrap();
        assert_eq!(result, ConfigInitResult::Created);

        // File should now exist
        assert!(config_path.exists());

        // Content should match the default template
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Ralph Agents Configuration File"));
        assert!(content.contains("[agents.claude]"));
        assert!(content.contains("[agents.codex]"));
    }

    #[test]
    fn test_ensure_config_exists_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let agent_dir = dir.path().join(".agent");
        fs::create_dir_all(&agent_dir).unwrap();
        let config_path = agent_dir.join("agents.toml");

        // Create an existing file
        fs::write(&config_path, "# Custom config\n").unwrap();

        // ensure_config_exists should return AlreadyExists
        let result = AgentsConfigFile::ensure_config_exists(&config_path).unwrap();
        assert_eq!(result, ConfigInitResult::AlreadyExists);

        // Content should be unchanged
        let content = fs::read_to_string(&config_path).unwrap();
        assert_eq!(content, "# Custom config\n");
    }

    #[test]
    fn test_ensure_config_exists_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("deep/nested/path/.agent/agents.toml");

        // Parent directories don't exist
        assert!(!config_path.parent().unwrap().exists());

        // ensure_config_exists should create parent directories
        let result = AgentsConfigFile::ensure_config_exists(&config_path).unwrap();
        assert_eq!(result, ConfigInitResult::Created);

        // Both file and parent directories should exist
        assert!(config_path.exists());
        assert!(config_path.parent().unwrap().exists());
    }

    #[test]
    fn test_default_agents_toml_is_valid() {
        // Verify the embedded default template can be parsed
        let config: AgentsConfigFile = toml::from_str(DEFAULT_AGENTS_TOML).unwrap();

        // Check that all expected agents are present
        assert!(config.agents.contains_key("claude"));
        assert!(config.agents.contains_key("codex"));
        assert!(config.agents.contains_key("opencode"));
        assert!(config.agents.contains_key("aider"));
        assert!(config.agents.contains_key("goose"));
        assert!(config.agents.contains_key("cline"));
        assert!(config.agents.contains_key("continue"));
        assert!(config.agents.contains_key("amazon-q"));
        assert!(config.agents.contains_key("gemini"));

        // Check lower-cost / open-source agents
        assert!(config.agents.contains_key("qwen"));
        assert!(config.agents.contains_key("vibe"));
        assert!(config.agents.contains_key("llama-cli"));
        assert!(config.agents.contains_key("aichat"));

        // Check additional popular CLI tools
        assert!(config.agents.contains_key("cursor"));
        assert!(config.agents.contains_key("plandex"));
        assert!(config.agents.contains_key("ollama"));

        // Verify Claude config is correct
        let claude = &config.agents["claude"];
        assert_eq!(claude.cmd, "claude -p");
        assert_eq!(claude.json_parser, "claude");

        // Verify Qwen config is correct
        let qwen = &config.agents["qwen"];
        assert_eq!(qwen.cmd, "qwen -p");
        assert_eq!(qwen.json_parser, "claude"); // Uses Claude parser
        assert!(qwen.output_flag.contains("stream-json"));

        // Verify Vibe config is correct
        let vibe = &config.agents["vibe"];
        assert_eq!(vibe.cmd, "vibe --prompt");
        assert_eq!(vibe.json_parser, "generic"); // Generic parser (no JSON streaming)

        // Verify Cursor config is correct
        let cursor = &config.agents["cursor"];
        assert_eq!(cursor.cmd, "agent -p");
        assert_eq!(cursor.json_parser, "generic");

        // Verify Plandex config is correct
        let plandex = &config.agents["plandex"];
        assert_eq!(plandex.cmd, "plandex tell");
        assert!(plandex.yolo_flag.contains("--apply"));

        // Verify Ollama config is correct
        let ollama = &config.agents["ollama"];
        assert!(ollama.cmd.contains("ollama run"));
        assert!(!ollama.can_commit);
    }

    #[test]
    fn test_registry_defaults_come_from_default_toml() {
        let config: AgentsConfigFile = toml::from_str(DEFAULT_AGENTS_TOML).unwrap();
        let registry = AgentRegistry::new().unwrap();

        let mut expected_names: Vec<String> = config.agents.keys().cloned().collect();
        expected_names.sort();

        let mut actual_names: Vec<String> = registry.agents.keys().cloned().collect();
        actual_names.sort();

        assert_eq!(expected_names, actual_names);

        for (name, cfg_toml) in config.agents {
            let expected: AgentConfig = cfg_toml.into();
            let actual = registry.get(&name).unwrap();
            assert_eq!(actual.cmd, expected.cmd);
            assert_eq!(actual.output_flag, expected.output_flag);
            assert_eq!(actual.yolo_flag, expected.yolo_flag);
            assert_eq!(actual.verbose_flag, expected.verbose_flag);
            assert_eq!(actual.can_commit, expected.can_commit);
            assert_eq!(actual.json_parser, expected.json_parser);
        }
    }

    #[test]
    fn test_with_merged_configs_no_files() {
        let dir = tempfile::tempdir().unwrap();
        let local_path = dir.path().join("nonexistent/agents.toml");

        let (registry, sources, warnings) =
            AgentRegistry::with_merged_configs(&local_path).unwrap();

        // Should have built-in defaults
        assert!(registry.is_known("claude"));
        assert!(registry.is_known("codex"));

        // No sources loaded (no files existed)
        assert!(sources.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_with_merged_configs_local_only() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let local_path = dir.path().join("agents.toml");

        // Create a local config that overrides claude
        let mut file = std::fs::File::create(&local_path).unwrap();
        writeln!(
            file,
            r#"
[agents.claude]
cmd = "claude-custom"
json_parser = "generic"

[agents.mybot]
cmd = "mybot run"
"#
        )
        .unwrap();

        let (registry, sources, warnings) =
            AgentRegistry::with_merged_configs(&local_path).unwrap();
        assert!(warnings.is_empty());

        // Should have both built-in and custom agents
        assert!(registry.is_known("codex")); // Built-in
        assert!(registry.is_known("mybot")); // Custom

        // Claude should be overridden
        let claude = registry.get("claude").unwrap();
        assert_eq!(claude.cmd, "claude-custom");
        assert_eq!(claude.json_parser, JsonParserType::Generic);

        // One source should be loaded
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].path, local_path);
        assert_eq!(sources[0].agents_loaded, 2); // claude + mybot
    }

    #[test]
    fn test_global_config_dir_returns_some() {
        // Should return Some on most systems (may fail in very minimal environments)
        // This is more of a smoke test
        if let Some(path) = global_config_dir() {
            assert!(path.ends_with("ralph") || path.to_string_lossy().contains("ralph"));
        }
    }

    #[test]
    fn test_global_agents_config_path() {
        if let Some(path) = global_agents_config_path() {
            assert!(path.ends_with("agents.toml"));
            assert!(path.to_string_lossy().contains("ralph"));
        }
    }

    #[test]
    fn test_config_source_struct() {
        let source = ConfigSource {
            path: PathBuf::from("/test/agents.toml"),
            agents_loaded: 5,
        };
        assert_eq!(source.path, PathBuf::from("/test/agents.toml"));
        assert_eq!(source.agents_loaded, 5);
    }

    #[test]
    fn test_validate_agent_chains_empty() {
        let mut registry = AgentRegistry::new().unwrap();
        registry.set_fallback(FallbackConfig::default());
        assert!(registry.validate_agent_chains().is_err());
        let err = registry.validate_agent_chains().unwrap_err();
        assert!(err.contains("No agent chain configured"));
    }

    #[test]
    fn test_validate_agent_chains_developer_only() {
        let mut registry = AgentRegistry::new().unwrap();
        registry.set_fallback(FallbackConfig {
            developer: vec!["claude".to_string()],
            reviewer: vec![],
            ..Default::default()
        });
        assert!(registry.validate_agent_chains().is_err());
        let err = registry.validate_agent_chains().unwrap_err();
        assert!(err.contains("No reviewer agent chain"));
    }

    #[test]
    fn test_validate_agent_chains_reviewer_only() {
        let mut registry = AgentRegistry::new().unwrap();
        registry.set_fallback(FallbackConfig {
            developer: vec![],
            reviewer: vec!["codex".to_string()],
            ..Default::default()
        });
        assert!(registry.validate_agent_chains().is_err());
        let err = registry.validate_agent_chains().unwrap_err();
        assert!(err.contains("No developer agent chain"));
    }

    #[test]
    fn test_validate_agent_chains_complete() {
        let mut registry = AgentRegistry::new().unwrap();
        registry.set_fallback(FallbackConfig {
            developer: vec!["claude".to_string()],
            reviewer: vec!["codex".to_string()],
            ..Default::default()
        });
        assert!(registry.validate_agent_chains().is_ok());
    }

    // =========================================================================
    // Tests for model_flag functionality
    // =========================================================================

    #[test]
    fn test_agent_config_with_model_flag() {
        let agent = AgentConfig {
            cmd: "opencode run".to_string(),
            output_flag: "--format json".to_string(),
            yolo_flag: "".to_string(),
            verbose_flag: "--log-level DEBUG".to_string(),
            can_commit: true,
            json_parser: JsonParserType::Generic,
            model_flag: Some("-m opencode/glm-4.7-free".to_string()),
        };

        // Build command should include the model flag
        let cmd = agent.build_cmd(true, true, true);
        assert!(cmd.contains("opencode run"));
        assert!(cmd.contains("--format json"));
        assert!(cmd.contains("-m opencode/glm-4.7-free"));
    }

    #[test]
    fn test_agent_config_without_model_flag() {
        let agent = AgentConfig {
            cmd: "opencode run".to_string(),
            output_flag: "--format json".to_string(),
            yolo_flag: "".to_string(),
            verbose_flag: "".to_string(),
            can_commit: true,
            json_parser: JsonParserType::Generic,
            model_flag: None,
        };

        // Build command should not include any model flag
        let cmd = agent.build_cmd(true, true, true);
        assert!(cmd.contains("opencode run"));
        assert!(cmd.contains("--format json"));
        assert!(!cmd.contains("-m"));
    }

    #[test]
    fn test_build_cmd_with_model_override() {
        let agent = AgentConfig {
            cmd: "opencode run".to_string(),
            output_flag: "--format json".to_string(),
            yolo_flag: "".to_string(),
            verbose_flag: "".to_string(),
            can_commit: true,
            json_parser: JsonParserType::Generic,
            model_flag: Some("-m opencode/default-model".to_string()),
        };

        // Override the configured model_flag with a runtime override
        let cmd = agent.build_cmd_with_model(true, true, true, Some("-m opencode/override-model"));
        assert!(cmd.contains("-m opencode/override-model"));
        // The configured model_flag should NOT be present (override takes precedence)
        assert!(!cmd.contains("default-model"));
    }

    #[test]
    fn test_build_cmd_with_model_no_override() {
        let agent = AgentConfig {
            cmd: "opencode run".to_string(),
            output_flag: "--format json".to_string(),
            yolo_flag: "".to_string(),
            verbose_flag: "".to_string(),
            can_commit: true,
            json_parser: JsonParserType::Generic,
            model_flag: Some("-m opencode/configured-model".to_string()),
        };

        // Without an override, the configured model_flag should be used
        let cmd = agent.build_cmd_with_model(true, true, true, None);
        assert!(cmd.contains("-m opencode/configured-model"));
    }

    #[test]
    fn test_agent_config_toml_model_flag_parsing() {
        let toml_str = r#"
cmd = "opencode run"
output_flag = "--format json"
model_flag = "-m opencode/glm-4.7-free"
"#;
        let config: AgentConfigToml = toml::from_str(toml_str).unwrap();

        assert_eq!(config.cmd, "opencode run");
        assert_eq!(
            config.model_flag,
            Some("-m opencode/glm-4.7-free".to_string())
        );

        let agent: AgentConfig = config.into();
        assert_eq!(
            agent.model_flag,
            Some("-m opencode/glm-4.7-free".to_string())
        );
    }

    #[test]
    fn test_agent_config_toml_model_flag_default() {
        // model_flag should default to None when not specified
        let toml_str = r#"cmd = "opencode run""#;
        let config: AgentConfigToml = toml::from_str(toml_str).unwrap();

        assert!(config.model_flag.is_none());

        let agent: AgentConfig = config.into();
        assert!(agent.model_flag.is_none());
    }

    #[test]
    fn test_provider_fallback_config() {
        let mut provider_fallback = HashMap::new();
        provider_fallback.insert(
            "opencode".to_string(),
            vec![
                "-m opencode/glm-4.7-free".to_string(),
                "-m opencode/claude-sonnet-4".to_string(),
            ],
        );

        let config = FallbackConfig {
            developer: vec!["opencode".to_string()],
            reviewer: vec!["opencode".to_string()],
            provider_fallback,
            ..Default::default()
        };

        // Check provider fallback methods
        assert!(config.has_provider_fallbacks("opencode"));
        assert!(!config.has_provider_fallbacks("claude"));

        let fallbacks = config.get_provider_fallbacks("opencode");
        assert_eq!(fallbacks.len(), 2);
        assert_eq!(fallbacks[0], "-m opencode/glm-4.7-free");
        assert_eq!(fallbacks[1], "-m opencode/claude-sonnet-4");

        // Non-existent agent returns empty slice
        let empty = config.get_provider_fallbacks("nonexistent");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_provider_fallback_from_toml() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("agents.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[agent_chain]
developer = ["opencode", "claude"]
reviewer = ["claude", "opencode"]
max_retries = 3

[agent_chain.provider_fallback]
opencode = ["-m opencode/glm-4.7-free", "-m opencode/claude-sonnet-4"]

[agents.testbot]
cmd = "testbot exec"
"#
        )
        .unwrap();

        let registry = AgentRegistry::with_config_file(&config_path).unwrap();
        let fallback = registry.fallback_config();

        // Check provider fallback was parsed correctly
        assert!(fallback.has_provider_fallbacks("opencode"));
        let provider_fallbacks = fallback.get_provider_fallbacks("opencode");
        assert_eq!(provider_fallbacks.len(), 2);
        assert!(provider_fallbacks[0].contains("glm-4.7-free"));
        assert!(provider_fallbacks[1].contains("claude-sonnet-4"));
    }

    #[test]
    fn test_fallback_config_defaults_provider_fallback() {
        let config = FallbackConfig::default();
        assert!(config.provider_fallback.is_empty());
        assert!(!config.has_provider_fallbacks("opencode"));
        assert!(config.get_provider_fallbacks("any").is_empty());
    }

    #[test]
    fn test_opencode_provider_type_from_model_flag() {
        // OpenCode Zen (opencode/*)
        assert_eq!(
            OpenCodeProviderType::from_model_flag("opencode/glm-4.7-free"),
            OpenCodeProviderType::OpenCodeZen
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("-m opencode/glm-4.7-free"),
            OpenCodeProviderType::OpenCodeZen
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("--model opencode/glm-4.7-free"),
            OpenCodeProviderType::OpenCodeZen
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("opencode/claude-sonnet-4"),
            OpenCodeProviderType::OpenCodeZen
        );

        // Z.AI Direct (zai/* or zhipuai/*)
        assert_eq!(
            OpenCodeProviderType::from_model_flag("zai/glm-4.7"),
            OpenCodeProviderType::ZaiDirect
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("-m zai/glm-4.7"),
            OpenCodeProviderType::ZaiDirect
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("zai/glm-4.5"),
            OpenCodeProviderType::ZaiDirect
        );
        // zhipuai is an alias for Z.AI
        assert_eq!(
            OpenCodeProviderType::from_model_flag("zhipuai/glm-4.7"),
            OpenCodeProviderType::ZaiDirect
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("zhipuai/glm-4.5"),
            OpenCodeProviderType::ZaiDirect
        );

        // Direct API providers - now have distinct types
        assert_eq!(
            OpenCodeProviderType::from_model_flag("anthropic/claude-sonnet-4"),
            OpenCodeProviderType::Anthropic
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("openai/gpt-4o"),
            OpenCodeProviderType::OpenAI
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("google/gemini-pro"),
            OpenCodeProviderType::Google
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("groq/llama-3.3-70b"),
            OpenCodeProviderType::Groq
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("deepseek/deepseek-chat"),
            OpenCodeProviderType::DeepSeek
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("mistral/mistral-large"),
            OpenCodeProviderType::Mistral
        );

        // Custom/Unknown provider
        assert_eq!(
            OpenCodeProviderType::from_model_flag("unknown/some-model"),
            OpenCodeProviderType::Custom
        );

        // Case insensitivity
        assert_eq!(
            OpenCodeProviderType::from_model_flag("OPENCODE/glm-4.7-free"),
            OpenCodeProviderType::OpenCodeZen
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("ZAI/glm-4.7"),
            OpenCodeProviderType::ZaiDirect
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("ZHIPUAI/glm-4.7"),
            OpenCodeProviderType::ZaiDirect
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("ZhipuAI/glm-4.5"),
            OpenCodeProviderType::ZaiDirect
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("GROQ/llama"),
            OpenCodeProviderType::Groq
        );
    }

    #[test]
    fn test_opencode_provider_type_names_and_auth() {
        assert_eq!(OpenCodeProviderType::OpenCodeZen.name(), "OpenCode Zen");
        assert_eq!(OpenCodeProviderType::ZaiDirect.name(), "Z.AI Direct");
        assert_eq!(OpenCodeProviderType::Anthropic.name(), "Anthropic");
        assert_eq!(OpenCodeProviderType::OpenAI.name(), "OpenAI");
        assert_eq!(OpenCodeProviderType::Groq.name(), "Groq");
        assert_eq!(OpenCodeProviderType::Custom.name(), "Custom");

        // Auth commands should be non-empty
        assert!(!OpenCodeProviderType::OpenCodeZen.auth_command().is_empty());
        assert!(!OpenCodeProviderType::ZaiDirect.auth_command().is_empty());
        assert!(!OpenCodeProviderType::Anthropic.auth_command().is_empty());
        assert!(!OpenCodeProviderType::Custom.auth_command().is_empty());

        // Verify specific auth command content
        assert!(OpenCodeProviderType::OpenCodeZen
            .auth_command()
            .contains("OpenCode Zen"));
        assert!(OpenCodeProviderType::ZaiDirect
            .auth_command()
            .contains("Z.AI"));
        assert!(OpenCodeProviderType::Anthropic
            .auth_command()
            .contains("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn test_validate_model_flag_valid_flags() {
        // Valid flags with provider prefix should not warn
        let warnings = validate_model_flag("opencode/glm-4.7-free");
        assert!(warnings.is_empty());

        let warnings = validate_model_flag("zai/glm-4.7");
        assert!(warnings.is_empty());

        // Empty string should not warn
        let warnings = validate_model_flag("");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_validate_model_flag_missing_prefix() {
        // Model without provider prefix should warn
        let warnings = validate_model_flag("glm-4.7-free");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("no provider prefix"));
    }

    #[test]
    fn test_validate_model_flag_zai_confusion() {
        // Using opencode/ prefix with "zai" in the model name should warn
        let warnings = validate_model_flag("opencode/zai-model");
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("zai")));
    }

    #[test]
    fn test_validate_model_flag_cloud_config_warning() {
        // Cloud providers should get a warning about additional config
        let warnings = validate_model_flag("amazon-bedrock/anthropic.claude-3");
        assert!(!warnings.is_empty());
        assert!(warnings
            .iter()
            .any(|w| w.contains("Amazon Bedrock") || w.contains("cloud configuration")));

        let warnings = validate_model_flag("azure-openai/gpt-4o");
        assert!(!warnings.is_empty());
        assert!(warnings
            .iter()
            .any(|w| w.contains("Azure") || w.contains("cloud configuration")));

        let warnings = validate_model_flag("google-vertex/gemini-pro");
        assert!(!warnings.is_empty());
        assert!(warnings
            .iter()
            .any(|w| w.contains("Vertex") || w.contains("cloud configuration")));
    }

    #[test]
    fn test_validate_model_flag_custom_provider_warning() {
        // Unknown/custom providers should get a warning
        let warnings = validate_model_flag("unknownprovider/some-model");
        assert!(!warnings.is_empty());
        assert!(warnings
            .iter()
            .any(|w| w.contains("Unknown provider") || w.contains("unknownprovider")));
    }

    #[test]
    fn test_validate_model_flag_known_providers_no_warning() {
        // Known providers without special requirements should have no warnings
        // (Anthropic, OpenAI, Groq, etc. are straightforward API key providers)
        let warnings = validate_model_flag("anthropic/claude-sonnet-4");
        assert!(warnings.is_empty());
        let warnings = validate_model_flag("-m anthropic/claude-sonnet-4");
        assert!(warnings.is_empty());

        let warnings = validate_model_flag("openai/gpt-4o");
        assert!(warnings.is_empty());

        let warnings = validate_model_flag("groq/llama-3.3-70b");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_auth_failure_advice_with_model_flag() {
        // With OpenCode Zen model
        let advice = auth_failure_advice(Some("opencode/glm-4.7-free"));
        assert!(advice.contains("OpenCode Zen"));

        // With Z.AI model prefix (tier can't be inferred from the prefix)
        let advice = auth_failure_advice(Some("zai/glm-4.7"));
        assert!(advice.contains("Z.AI"));
        assert!(advice.contains("Coding Plan"));

        // With Anthropic model - now shows specific provider
        let advice = auth_failure_advice(Some("anthropic/claude-sonnet-4"));
        assert!(advice.contains("Anthropic"));
        assert!(advice.contains("ANTHROPIC_API_KEY"));

        // With OpenAI model - now shows specific provider
        let advice = auth_failure_advice(Some("openai/gpt-4o"));
        assert!(advice.contains("OpenAI"));
        assert!(advice.contains("OPENAI_API_KEY"));

        // With Groq model
        let advice = auth_failure_advice(Some("groq/llama"));
        assert!(advice.contains("Groq"));
        assert!(advice.contains("GROQ_API_KEY"));

        // Without model flag
        let advice = auth_failure_advice(None);
        assert!(advice.contains("opencode auth login"));
    }

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
            strip_model_flag_prefix("--model=opencode/glm-4.7-free"),
            "opencode/glm-4.7-free"
        );
        assert_eq!(
            strip_model_flag_prefix("opencode/glm-4.7-free"),
            "opencode/glm-4.7-free"
        );
    }

    #[test]
    fn test_new_opencode_provider_types_parsing() {
        // Cloud Platform Providers
        assert_eq!(
            OpenCodeProviderType::from_model_flag("baseten/llama-3-70b"),
            OpenCodeProviderType::Baseten
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("cortecs/llama-3-70b"),
            OpenCodeProviderType::Cortecs
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("scaleway/llama-3-70b"),
            OpenCodeProviderType::Scaleway
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("ovhcloud/llama-3-70b"),
            OpenCodeProviderType::OVHcloud
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("ovh/llama-3-70b"),
            OpenCodeProviderType::OVHcloud
        );

        // AI Gateway Providers
        assert_eq!(
            OpenCodeProviderType::from_model_flag("vercel/gpt-4o"),
            OpenCodeProviderType::Vercel
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("helicone/gpt-4o"),
            OpenCodeProviderType::Helicone
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("io-net/llama-3-70b"),
            OpenCodeProviderType::IONet
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("ionet/llama-3-70b"),
            OpenCodeProviderType::IONet
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("nebius/llama-3-70b"),
            OpenCodeProviderType::Nebius
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("zenmux/gpt-4o"),
            OpenCodeProviderType::ZenMux
        );

        // Enterprise/Industry Providers
        assert_eq!(
            OpenCodeProviderType::from_model_flag("sap-ai-core/gpt-4o"),
            OpenCodeProviderType::SapAICore
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("sap/gpt-4o"),
            OpenCodeProviderType::SapAICore
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("azure-cognitive-services/gpt-4o"),
            OpenCodeProviderType::AzureCognitiveServices
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("azure-cognitive/gpt-4o"),
            OpenCodeProviderType::AzureCognitiveServices
        );

        // Specialized Inference Providers
        assert_eq!(
            OpenCodeProviderType::from_model_flag("venice-ai/llama-3-70b"),
            OpenCodeProviderType::VeniceAI
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("venice/llama-3-70b"),
            OpenCodeProviderType::VeniceAI
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("ollama-cloud/llama3"),
            OpenCodeProviderType::OllamaCloud
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("llama.cpp/local-model"),
            OpenCodeProviderType::LlamaCpp
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("llamacpp/local-model"),
            OpenCodeProviderType::LlamaCpp
        );
        assert_eq!(
            OpenCodeProviderType::from_model_flag("llama-cpp/local-model"),
            OpenCodeProviderType::LlamaCpp
        );
    }

    #[test]
    fn test_new_provider_types_names() {
        // Cloud Platform Providers
        assert_eq!(OpenCodeProviderType::Baseten.name(), "Baseten");
        assert_eq!(OpenCodeProviderType::Cortecs.name(), "Cortecs");
        assert_eq!(OpenCodeProviderType::Scaleway.name(), "Scaleway");
        assert_eq!(OpenCodeProviderType::OVHcloud.name(), "OVHcloud");

        // AI Gateway Providers
        assert_eq!(OpenCodeProviderType::Vercel.name(), "Vercel AI Gateway");
        assert_eq!(OpenCodeProviderType::Helicone.name(), "Helicone");
        assert_eq!(OpenCodeProviderType::IONet.name(), "IO.NET");
        assert_eq!(OpenCodeProviderType::Nebius.name(), "Nebius");
        assert_eq!(OpenCodeProviderType::ZenMux.name(), "ZenMux");

        // Enterprise/Industry Providers
        assert_eq!(OpenCodeProviderType::SapAICore.name(), "SAP AI Core");
        assert_eq!(
            OpenCodeProviderType::AzureCognitiveServices.name(),
            "Azure Cognitive Services"
        );

        // Specialized Inference Providers
        assert_eq!(OpenCodeProviderType::VeniceAI.name(), "Venice AI");
        assert_eq!(OpenCodeProviderType::OllamaCloud.name(), "Ollama Cloud");
        assert_eq!(OpenCodeProviderType::LlamaCpp.name(), "llama.cpp");
    }

    #[test]
    fn test_new_provider_types_auth_commands() {
        // All new providers should have non-empty auth commands
        assert!(!OpenCodeProviderType::Baseten.auth_command().is_empty());
        assert!(!OpenCodeProviderType::Cortecs.auth_command().is_empty());
        assert!(!OpenCodeProviderType::Scaleway.auth_command().is_empty());
        assert!(!OpenCodeProviderType::OVHcloud.auth_command().is_empty());
        assert!(!OpenCodeProviderType::Vercel.auth_command().is_empty());
        assert!(!OpenCodeProviderType::Helicone.auth_command().is_empty());
        assert!(!OpenCodeProviderType::IONet.auth_command().is_empty());
        assert!(!OpenCodeProviderType::Nebius.auth_command().is_empty());
        assert!(!OpenCodeProviderType::ZenMux.auth_command().is_empty());
        assert!(!OpenCodeProviderType::SapAICore.auth_command().is_empty());
        assert!(!OpenCodeProviderType::AzureCognitiveServices
            .auth_command()
            .is_empty());
        assert!(!OpenCodeProviderType::VeniceAI.auth_command().is_empty());
        assert!(!OpenCodeProviderType::OllamaCloud.auth_command().is_empty());
        assert!(!OpenCodeProviderType::LlamaCpp.auth_command().is_empty());

        // Check specific auth content
        assert!(OpenCodeProviderType::SapAICore
            .auth_command()
            .contains("AICORE"));
        assert!(OpenCodeProviderType::AzureCognitiveServices
            .auth_command()
            .contains("AZURE_COGNITIVE"));
        assert!(OpenCodeProviderType::LlamaCpp
            .auth_command()
            .contains("locally"));
    }

    #[test]
    fn test_new_provider_types_prefixes() {
        // All new providers should have correct prefixes
        assert_eq!(OpenCodeProviderType::Baseten.prefix(), "baseten/");
        assert_eq!(OpenCodeProviderType::Cortecs.prefix(), "cortecs/");
        assert_eq!(OpenCodeProviderType::Scaleway.prefix(), "scaleway/");
        assert_eq!(OpenCodeProviderType::OVHcloud.prefix(), "ovhcloud/");
        assert_eq!(OpenCodeProviderType::Vercel.prefix(), "vercel/");
        assert_eq!(OpenCodeProviderType::Helicone.prefix(), "helicone/");
        assert_eq!(OpenCodeProviderType::IONet.prefix(), "io-net/");
        assert_eq!(OpenCodeProviderType::Nebius.prefix(), "nebius/");
        assert_eq!(OpenCodeProviderType::ZenMux.prefix(), "zenmux/");
        assert_eq!(OpenCodeProviderType::SapAICore.prefix(), "sap-ai-core/");
        assert_eq!(
            OpenCodeProviderType::AzureCognitiveServices.prefix(),
            "azure-cognitive-services/"
        );
        assert_eq!(OpenCodeProviderType::VeniceAI.prefix(), "venice-ai/");
        assert_eq!(OpenCodeProviderType::OllamaCloud.prefix(), "ollama-cloud/");
        assert_eq!(OpenCodeProviderType::LlamaCpp.prefix(), "llama.cpp/");
        assert_eq!(
            OpenCodeProviderType::Custom.prefix(),
            "any other provider/*"
        );
    }

    #[test]
    fn test_new_provider_types_example_models() {
        // All new providers should have at least one example model
        assert!(!OpenCodeProviderType::Baseten.example_models().is_empty());
        assert!(!OpenCodeProviderType::Cortecs.example_models().is_empty());
        assert!(!OpenCodeProviderType::Scaleway.example_models().is_empty());
        assert!(!OpenCodeProviderType::OVHcloud.example_models().is_empty());
        assert!(!OpenCodeProviderType::Vercel.example_models().is_empty());
        assert!(!OpenCodeProviderType::Helicone.example_models().is_empty());
        assert!(!OpenCodeProviderType::IONet.example_models().is_empty());
        assert!(!OpenCodeProviderType::Nebius.example_models().is_empty());
        assert!(!OpenCodeProviderType::ZenMux.example_models().is_empty());
        assert!(!OpenCodeProviderType::SapAICore.example_models().is_empty());
        assert!(!OpenCodeProviderType::AzureCognitiveServices
            .example_models()
            .is_empty());
        assert!(!OpenCodeProviderType::VeniceAI.example_models().is_empty());
        assert!(!OpenCodeProviderType::OllamaCloud
            .example_models()
            .is_empty());
        assert!(!OpenCodeProviderType::LlamaCpp.example_models().is_empty());
    }

    #[test]
    fn test_new_provider_types_requires_cloud_config() {
        // Enterprise providers require cloud config
        assert!(OpenCodeProviderType::SapAICore.requires_cloud_config());
        assert!(OpenCodeProviderType::AzureCognitiveServices.requires_cloud_config());

        // Cloud platform providers do NOT require cloud config (they use /connect)
        assert!(!OpenCodeProviderType::Baseten.requires_cloud_config());
        assert!(!OpenCodeProviderType::Cortecs.requires_cloud_config());
        assert!(!OpenCodeProviderType::Scaleway.requires_cloud_config());
        assert!(!OpenCodeProviderType::OVHcloud.requires_cloud_config());

        // Gateway providers do NOT require cloud config
        assert!(!OpenCodeProviderType::Vercel.requires_cloud_config());
        assert!(!OpenCodeProviderType::Helicone.requires_cloud_config());
        assert!(!OpenCodeProviderType::ZenMux.requires_cloud_config());
    }

    #[test]
    fn test_new_provider_types_is_local() {
        // llama.cpp is a local provider
        assert!(OpenCodeProviderType::LlamaCpp.is_local());

        // Ollama Cloud is NOT local (it's cloud-hosted)
        assert!(!OpenCodeProviderType::OllamaCloud.is_local());

        // Cloud platform providers are NOT local
        assert!(!OpenCodeProviderType::Baseten.is_local());
        assert!(!OpenCodeProviderType::Scaleway.is_local());
        assert!(!OpenCodeProviderType::OVHcloud.is_local());
    }

    #[test]
    fn test_all_providers_include_new_types() {
        let all = OpenCodeProviderType::all();

        // Verify all new providers are in the all() list
        assert!(all.contains(&OpenCodeProviderType::Baseten));
        assert!(all.contains(&OpenCodeProviderType::Cortecs));
        assert!(all.contains(&OpenCodeProviderType::Scaleway));
        assert!(all.contains(&OpenCodeProviderType::OVHcloud));
        assert!(all.contains(&OpenCodeProviderType::Vercel));
        assert!(all.contains(&OpenCodeProviderType::Helicone));
        assert!(all.contains(&OpenCodeProviderType::IONet));
        assert!(all.contains(&OpenCodeProviderType::Nebius));
        assert!(all.contains(&OpenCodeProviderType::ZenMux));
        assert!(all.contains(&OpenCodeProviderType::SapAICore));
        assert!(all.contains(&OpenCodeProviderType::AzureCognitiveServices));
        assert!(all.contains(&OpenCodeProviderType::VeniceAI));
        assert!(all.contains(&OpenCodeProviderType::OllamaCloud));
        assert!(all.contains(&OpenCodeProviderType::LlamaCpp));

        // Custom should NOT be in the all() list
        assert!(!all.contains(&OpenCodeProviderType::Custom));
    }

    #[test]
    fn test_validate_model_flag_new_enterprise_providers() {
        // SAP AI Core should warn about cloud config requirement
        let warnings = validate_model_flag("sap-ai-core/gpt-4o");
        assert!(!warnings.is_empty());
        assert!(warnings
            .iter()
            .any(|w| w.contains("SAP AI Core") || w.contains("cloud configuration")));

        // Azure Cognitive Services should warn about cloud config requirement
        let warnings = validate_model_flag("azure-cognitive-services/gpt-4o");
        assert!(!warnings.is_empty());
        assert!(warnings
            .iter()
            .any(|w| w.contains("Azure Cognitive Services") || w.contains("cloud configuration")));
    }

    #[test]
    fn test_validate_model_flag_new_local_providers() {
        // llama.cpp is local, should give info message
        let warnings = validate_model_flag("llama.cpp/local-model");
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("local provider")));
    }
}
