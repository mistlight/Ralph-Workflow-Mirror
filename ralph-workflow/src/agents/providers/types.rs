//! Provider Type Enum
//!
//! Defines the `OpenCodeProviderType` enum representing all supported
//! AI providers in the OpenCode ecosystem.

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
