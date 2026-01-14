//! Provider listing and information display.
//!
//! Contains functions for displaying `OpenCode` provider information.

#![expect(clippy::too_many_lines)]
use crate::agents::OpenCodeProviderType;
use crate::colors::Colors;

/// Helper function to print provider information for --list-providers.
pub fn print_provider_info(colors: Colors, provider: OpenCodeProviderType, agent_alias: &str) {
    let examples = provider.example_models();
    let example_str = if examples.is_empty() {
        String::new()
    } else {
        format!(" (e.g., {})", examples[0])
    };

    println!("{}{}{}", colors.bold(), provider.name(), colors.reset());
    println!("  Prefix: {}{}", provider.prefix(), example_str);
    println!("  Auth: {}", provider.auth_command());
    println!("  Agent: {agent_alias}");
}

/// Handle --list-providers command.
///
/// Displays a categorized list of all `OpenCode` provider types with their
/// model prefixes, authentication commands, and example agent aliases.
pub fn handle_list_providers(colors: Colors) {
    println!("{}OpenCode Provider Types{}", colors.bold(), colors.reset());
    println!();
    println!("Ralph includes built-in guidance for major OpenCode provider prefixes (plus a custom fallback).");
    println!("OpenCode may support additional providers; consult OpenCode docs for the full set.");
    println!();

    // Category: OpenCode Gateway
    println!(
        "{}═══ OPENCODE GATEWAY ═══{}",
        colors.bold(),
        colors.reset()
    );
    print_provider_info(
        colors,
        OpenCodeProviderType::OpenCodeZen,
        "opencode-zen-glm",
    );
    println!();

    // Category: Chinese AI Providers
    println!(
        "{}═══ CHINESE AI PROVIDERS ═══{}",
        colors.bold(),
        colors.reset()
    );
    print_provider_info(colors, OpenCodeProviderType::ZaiDirect, "opencode-zai-glm");
    print_provider_info(
        colors,
        OpenCodeProviderType::ZaiCodingPlan,
        "opencode-zai-glm-codingplan",
    );
    print_provider_info(colors, OpenCodeProviderType::Moonshot, "opencode-moonshot");
    print_provider_info(colors, OpenCodeProviderType::MiniMax, "opencode-minimax");
    println!();

    // Category: Major Cloud Providers
    println!(
        "{}═══ MAJOR CLOUD PROVIDERS ═══{}",
        colors.bold(),
        colors.reset()
    );
    print_provider_info(
        colors,
        OpenCodeProviderType::Anthropic,
        "opencode-direct-claude",
    );
    print_provider_info(colors, OpenCodeProviderType::OpenAI, "opencode-openai");
    print_provider_info(colors, OpenCodeProviderType::Google, "opencode-google");
    print_provider_info(
        colors,
        OpenCodeProviderType::GoogleVertex,
        "opencode-vertex",
    );
    print_provider_info(
        colors,
        OpenCodeProviderType::AmazonBedrock,
        "opencode-bedrock",
    );
    print_provider_info(colors, OpenCodeProviderType::AzureOpenAI, "opencode-azure");
    print_provider_info(
        colors,
        OpenCodeProviderType::GithubCopilot,
        "opencode-copilot",
    );
    println!();

    // Category: Fast Inference Providers
    println!(
        "{}═══ FAST INFERENCE PROVIDERS ═══{}",
        colors.bold(),
        colors.reset()
    );
    print_provider_info(colors, OpenCodeProviderType::Groq, "opencode-groq");
    print_provider_info(colors, OpenCodeProviderType::Together, "opencode-together");
    print_provider_info(
        colors,
        OpenCodeProviderType::Fireworks,
        "opencode-fireworks",
    );
    print_provider_info(colors, OpenCodeProviderType::Cerebras, "opencode-cerebras");
    print_provider_info(
        colors,
        OpenCodeProviderType::SambaNova,
        "opencode-sambanova",
    );
    print_provider_info(
        colors,
        OpenCodeProviderType::DeepInfra,
        "opencode-deepinfra",
    );
    println!();

    // Category: Gateway/Aggregator Providers
    println!(
        "{}═══ GATEWAY PROVIDERS ═══{}",
        colors.bold(),
        colors.reset()
    );
    print_provider_info(
        colors,
        OpenCodeProviderType::OpenRouter,
        "opencode-openrouter",
    );
    print_provider_info(
        colors,
        OpenCodeProviderType::Cloudflare,
        "opencode-cloudflare",
    );
    println!();

    // Category: Specialized Providers
    println!(
        "{}═══ SPECIALIZED PROVIDERS ═══{}",
        colors.bold(),
        colors.reset()
    );
    print_provider_info(colors, OpenCodeProviderType::DeepSeek, "opencode-deepseek");
    print_provider_info(colors, OpenCodeProviderType::Xai, "opencode-xai");
    print_provider_info(colors, OpenCodeProviderType::Mistral, "opencode-mistral");
    print_provider_info(colors, OpenCodeProviderType::Cohere, "opencode-cohere");
    print_provider_info(
        colors,
        OpenCodeProviderType::Perplexity,
        "opencode-perplexity",
    );
    print_provider_info(colors, OpenCodeProviderType::AI21, "opencode-ai21");
    print_provider_info(colors, OpenCodeProviderType::VeniceAI, "opencode-venice");
    println!();

    // Category: Open-Source Model Providers
    println!(
        "{}═══ OPEN-SOURCE MODEL PROVIDERS ═══{}",
        colors.bold(),
        colors.reset()
    );
    print_provider_info(
        colors,
        OpenCodeProviderType::HuggingFace,
        "opencode-huggingface",
    );
    print_provider_info(
        colors,
        OpenCodeProviderType::Replicate,
        "opencode-replicate",
    );
    println!();

    // Category: Cloud Platform Providers
    println!(
        "{}═══ CLOUD PLATFORM PROVIDERS ═══{}",
        colors.bold(),
        colors.reset()
    );
    print_provider_info(colors, OpenCodeProviderType::Baseten, "opencode-baseten");
    print_provider_info(colors, OpenCodeProviderType::Cortecs, "opencode-cortecs");
    print_provider_info(colors, OpenCodeProviderType::Scaleway, "opencode-scaleway");
    print_provider_info(colors, OpenCodeProviderType::OVHcloud, "opencode-ovhcloud");
    print_provider_info(colors, OpenCodeProviderType::IONet, "opencode-ionet");
    print_provider_info(colors, OpenCodeProviderType::Nebius, "opencode-nebius");
    println!();

    // Category: AI Gateway Providers
    println!(
        "{}═══ AI GATEWAY PROVIDERS ═══{}",
        colors.bold(),
        colors.reset()
    );
    print_provider_info(colors, OpenCodeProviderType::Vercel, "opencode-vercel");
    print_provider_info(colors, OpenCodeProviderType::Helicone, "opencode-helicone");
    print_provider_info(colors, OpenCodeProviderType::ZenMux, "opencode-zenmux");
    println!();

    // Category: Enterprise/Industry Providers
    println!(
        "{}═══ ENTERPRISE PROVIDERS ═══{}",
        colors.bold(),
        colors.reset()
    );
    print_provider_info(colors, OpenCodeProviderType::SapAICore, "opencode-sap");
    print_provider_info(
        colors,
        OpenCodeProviderType::AzureCognitiveServices,
        "opencode-azure-cognitive",
    );
    println!();

    // Category: Local Providers
    println!("{}═══ LOCAL PROVIDERS ═══{}", colors.bold(), colors.reset());
    print_provider_info(colors, OpenCodeProviderType::Ollama, "opencode-ollama");
    print_provider_info(colors, OpenCodeProviderType::LMStudio, "opencode-lmstudio");
    print_provider_info(
        colors,
        OpenCodeProviderType::OllamaCloud,
        "opencode-ollama-cloud",
    );
    print_provider_info(colors, OpenCodeProviderType::LlamaCpp, "opencode-llamacpp");
    println!();

    // Category: Custom
    println!("{}═══ CUSTOM ═══{}", colors.bold(), colors.reset());
    print_provider_info(colors, OpenCodeProviderType::Custom, "(custom)");
    println!();

    // Important notes
    println!("{}═══ IMPORTANT NOTES ═══{}", colors.bold(), colors.reset());
    println!(
        "• OpenCode Zen (opencode/*) and Z.AI Direct (zai/* or zhipuai/*) are SEPARATE endpoints!"
    );
    println!("  - opencode/* routes through OpenCode's Zen gateway at opencode.ai");
    println!("  - zai/* or zhipuai/* connects directly to Z.AI's API at api.z.ai");
    println!("  - Z.AI Coding Plan is an auth tier; model prefix remains zai/* or zhipuai/*");
    println!("• Cloud providers (Vertex, Bedrock, Azure, SAP) require additional configuration");
    println!(
        "• Local providers (Ollama, LM Studio, llama.cpp) run on your hardware - no API key needed"
    );
    println!("• Use clear naming: opencode-zen-*, opencode-zai-*, opencode-direct-* aliases");
    println!();
}
