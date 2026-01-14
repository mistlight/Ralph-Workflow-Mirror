//! Preset configurations for common agent combinations.
//!
//! Presets allow users to quickly configure Ralph for common use cases
//! without specifying individual agent options.

use crate::config::{Config, ReviewDepth};
use crate::logger::Colors;
use clap::ValueEnum;

/// Preset configurations for common agent combinations.
#[derive(Clone, Debug, ValueEnum)]
pub enum Preset {
    /// Use `agent_chain` defaults (no explicit agent override)
    Default,
    /// Use opencode for both developer and reviewer
    Opencode,
}

/// Apply CLI arguments to the configuration.
///
/// This function merges CLI arguments into the existing config, handling:
/// - Verbosity flags (--quiet, --full, --debug, -v LEVEL)
/// - Preset configurations (--preset)
/// - Quick mode (--quick)
/// - Agent and model overrides
/// - Isolation mode
pub fn apply_args_to_config(args: &super::Args, config: &mut Config, colors: Colors) {
    // Handle verbosity shorthand flags (--quiet, --full, --debug take precedence)
    let base_verbosity = config.verbosity;
    config.verbosity = if args.quiet {
        crate::config::Verbosity::Quiet
    } else if args.debug {
        crate::config::Verbosity::Debug
    } else if args.full {
        crate::config::Verbosity::Full
    } else if let Some(v) = args.verbosity {
        v.into()
    } else {
        base_verbosity
    };

    // Apply preset (CLI/env preset overrides env-selected agents, but can be overridden by
    // explicit --developer-agent/--reviewer-agent flags below).
    if let Some(preset) = args.preset.clone() {
        match preset {
            Preset::Default => {
                // No override; use agent_chain defaults from the unified config / built-ins
            }
            Preset::Opencode => {
                config.developer_agent = Some("opencode".to_string());
                config.reviewer_agent = Some("opencode".to_string());
            }
        }
    }

    // Quick mode: 1 developer iteration, 1 review pass (explicit flags override)
    if args.quick {
        if args.developer_iters.is_none() {
            config.developer_iters = 1;
        }
        if args.reviewer_reviews.is_none() {
            config.reviewer_reviews = 1;
        }
    }

    // Rapid mode: 2 developer iterations, 1 review pass (explicit flags override)
    if args.rapid {
        if args.developer_iters.is_none() {
            config.developer_iters = 2;
        }
        if args.reviewer_reviews.is_none() {
            config.reviewer_reviews = 1;
        }
    }

    if let Some(iters) = args.developer_iters {
        config.developer_iters = iters;
    }
    if let Some(reviews) = args.reviewer_reviews {
        config.reviewer_reviews = reviews;
    }
    if let Some(agent) = args.developer_agent.clone() {
        config.developer_agent = Some(agent);
    }
    if let Some(agent) = args.reviewer_agent.clone() {
        config.reviewer_agent = Some(agent);
    }
    if let Some(model) = args.developer_model.clone() {
        config.developer_model = Some(model);
    }
    if let Some(model) = args.reviewer_model.clone() {
        config.reviewer_model = Some(model);
    }
    if let Some(provider) = args.developer_provider.clone() {
        config.developer_provider = Some(provider);
    }
    if let Some(provider) = args.reviewer_provider.clone() {
        config.reviewer_provider = Some(provider);
    }
    if let Some(parser) = args.reviewer_json_parser.clone() {
        config.reviewer_json_parser = Some(parser);
    }
    if let Some(depth) = args.review_depth.clone() {
        if let Some(parsed) = ReviewDepth::from_str(&depth) {
            config.review_depth = parsed;
        } else {
            eprintln!(
                "{}{}Warning:{} Unknown review depth '{}'. Using default (standard).",
                colors.bold(),
                colors.yellow(),
                colors.reset(),
                depth
            );
            eprintln!("Valid options: standard, comprehensive, security, incremental");
        }
    }

    // Handle --no-isolation flag (CLI overrides env var)
    if args.no_isolation {
        config.isolation_mode = false;
    }

    // Container mode flags (--container-mode and --no-container-mode)
    // --container-mode explicitly enables, --no-container-mode explicitly disables
    if args.container_mode {
        config.container_mode = true;
    }
    if args.no_container_mode {
        config.container_mode = false;
    }
    // Container engine override
    if let Some(engine) = args.container_engine.clone() {
        let engine = engine.trim();
        if !engine.is_empty() {
            config.container_engine = Some(engine.to_string());
        }
    }
    // Container image override
    if let Some(image) = args.container_image.clone() {
        let image = image.trim();
        if !image.is_empty() {
            config.container_image = Some(image.to_string());
        }
    }
    // Container network flag (--no-network disables network in containers)
    if args.no_network {
        config.container_network = false;
    }

    // Git user identity (CLI args have highest priority)
    if let Some(name) = args.git_user_name.clone() {
        let name = name.trim();
        if !name.is_empty() {
            config.git_user_name = Some(name.to_string());
        }
    }
    if let Some(email) = args.git_user_email.clone() {
        let email = email.trim();
        if !email.is_empty() {
            config.git_user_email = Some(email.to_string());
        }
    }
}
