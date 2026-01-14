//! Provider listing and information display.
//!
//! Contains functions for displaying `OpenCode` provider information.

use crate::agents::OpenCodeProviderType;
use crate::colors::Colors;

/// Provider category for display grouping
struct ProviderCategory {
    name: &'static str,
    providers: &'static [(OpenCodeProviderType, &'static str)],
}

/// Helper function to print provider information for --list-providers.
fn print_provider_info(colors: Colors, provider: OpenCodeProviderType, agent_alias: &str) {
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

/// Print a category of providers
fn print_provider_category(colors: Colors, category: &ProviderCategory) {
    println!(
        "{}═══ {} ═══{}",
        colors.bold(),
        category.name,
        colors.reset()
    );
    for (provider, alias) in category.providers {
        print_provider_info(colors, *provider, alias);
    }
    println!();
}

/// Provider categories for listing
const PROVIDER_CATEGORIES: &[ProviderCategory] = &[
    ProviderCategory {
        name: "OPENCODE GATEWAY",
        providers: &[(OpenCodeProviderType::OpenCodeZen, "opencode-zen-glm")],
    },
    ProviderCategory {
        name: "CHINESE AI PROVIDERS",
        providers: &[
            (OpenCodeProviderType::ZaiDirect, "opencode-zai-glm"),
            (
                OpenCodeProviderType::ZaiCodingPlan,
                "opencode-zai-glm-codingplan",
            ),
            (OpenCodeProviderType::Moonshot, "opencode-moonshot"),
            (OpenCodeProviderType::MiniMax, "opencode-minimax"),
        ],
    },
    ProviderCategory {
        name: "MAJOR CLOUD PROVIDERS",
        providers: &[
            (OpenCodeProviderType::Anthropic, "opencode-direct-claude"),
            (OpenCodeProviderType::OpenAI, "opencode-openai"),
            (OpenCodeProviderType::Google, "opencode-google"),
            (OpenCodeProviderType::GoogleVertex, "opencode-vertex"),
            (OpenCodeProviderType::AmazonBedrock, "opencode-bedrock"),
            (OpenCodeProviderType::AzureOpenAI, "opencode-azure"),
            (OpenCodeProviderType::GithubCopilot, "opencode-copilot"),
        ],
    },
    ProviderCategory {
        name: "FAST INFERENCE PROVIDERS",
        providers: &[
            (OpenCodeProviderType::Groq, "opencode-groq"),
            (OpenCodeProviderType::Together, "opencode-together"),
            (OpenCodeProviderType::Fireworks, "opencode-fireworks"),
            (OpenCodeProviderType::Cerebras, "opencode-cerebras"),
            (OpenCodeProviderType::SambaNova, "opencode-sambanova"),
            (OpenCodeProviderType::DeepInfra, "opencode-deepinfra"),
        ],
    },
    ProviderCategory {
        name: "GATEWAY PROVIDERS",
        providers: &[
            (OpenCodeProviderType::OpenRouter, "opencode-openrouter"),
            (OpenCodeProviderType::Cloudflare, "opencode-cloudflare"),
        ],
    },
    ProviderCategory {
        name: "SPECIALIZED PROVIDERS",
        providers: &[
            (OpenCodeProviderType::DeepSeek, "opencode-deepseek"),
            (OpenCodeProviderType::Xai, "opencode-xai"),
            (OpenCodeProviderType::Mistral, "opencode-mistral"),
            (OpenCodeProviderType::Cohere, "opencode-cohere"),
            (OpenCodeProviderType::Perplexity, "opencode-perplexity"),
            (OpenCodeProviderType::AI21, "opencode-ai21"),
            (OpenCodeProviderType::VeniceAI, "opencode-venice"),
        ],
    },
    ProviderCategory {
        name: "OPEN-SOURCE MODEL PROVIDERS",
        providers: &[
            (OpenCodeProviderType::HuggingFace, "opencode-huggingface"),
            (OpenCodeProviderType::Replicate, "opencode-replicate"),
        ],
    },
    ProviderCategory {
        name: "CLOUD PLATFORM PROVIDERS",
        providers: &[
            (OpenCodeProviderType::Baseten, "opencode-baseten"),
            (OpenCodeProviderType::Cortecs, "opencode-cortecs"),
            (OpenCodeProviderType::Scaleway, "opencode-scaleway"),
            (OpenCodeProviderType::OVHcloud, "opencode-ovhcloud"),
            (OpenCodeProviderType::IONet, "opencode-ionet"),
            (OpenCodeProviderType::Nebius, "opencode-nebius"),
        ],
    },
    ProviderCategory {
        name: "AI GATEWAY PROVIDERS",
        providers: &[
            (OpenCodeProviderType::Vercel, "opencode-vercel"),
            (OpenCodeProviderType::Helicone, "opencode-helicone"),
            (OpenCodeProviderType::ZenMux, "opencode-zenmux"),
        ],
    },
    ProviderCategory {
        name: "ENTERPRISE PROVIDERS",
        providers: &[
            (OpenCodeProviderType::SapAICore, "opencode-sap"),
            (
                OpenCodeProviderType::AzureCognitiveServices,
                "opencode-azure-cognitive",
            ),
        ],
    },
    ProviderCategory {
        name: "LOCAL PROVIDERS",
        providers: &[
            (OpenCodeProviderType::Ollama, "opencode-ollama"),
            (OpenCodeProviderType::LMStudio, "opencode-lmstudio"),
            (OpenCodeProviderType::OllamaCloud, "opencode-ollama-cloud"),
            (OpenCodeProviderType::LlamaCpp, "opencode-llamacpp"),
        ],
    },
    ProviderCategory {
        name: "CUSTOM",
        providers: &[(OpenCodeProviderType::Custom, "(custom)")],
    },
];

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

    // Print all provider categories
    for category in PROVIDER_CATEGORIES {
        print_provider_category(colors, category);
    }

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
