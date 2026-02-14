// Agent fallback chain state.
//
// Contains AgentChainState and backoff computation helpers.

use serde::de::Deserializer;
use sha2::{Digest, Sha256};

/// Agent fallback chain state (explicit, not loop indices).
///
/// Tracks position in the multi-level fallback chain:
/// - Agent level (primary → fallback1 → fallback2)
/// - Model level (within each agent, try different models)
/// - Retry cycle (exhaust all agents, start over with exponential backoff)
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AgentChainState {
    /// Agent names in fallback order. Box<[String]> saves 8 bytes per instance
    /// vs Vec<String> since this collection is immutable after construction.
    pub agents: Box<[String]>,
    pub current_agent_index: usize,
    /// Models per agent. Box for immutable outer collection, Vec for model lists
    /// that need indexing during runtime selection.
    pub models_per_agent: Box<[Vec<String>]>,
    pub current_model_index: usize,
    pub retry_cycle: u32,
    pub max_cycles: u32,
    /// Base delay between retry cycles in milliseconds.
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    /// Multiplier for exponential backoff.
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
    /// Maximum backoff delay in milliseconds.
    #[serde(default = "default_max_backoff_ms")]
    pub max_backoff_ms: u64,
    /// Pending backoff delay (milliseconds) that must be waited before continuing.
    #[serde(default)]
    pub backoff_pending_ms: Option<u64>,
    pub current_role: AgentRole,
    /// Prompt context preserved from a rate-limited agent for continuation.
    ///
    /// When an agent hits 429, we save the prompt here so the next agent can
    /// continue the SAME role/task instead of starting from scratch.
    ///
    /// IMPORTANT: This must be role-scoped to prevent cross-task contamination
    /// (e.g., a developer continuation prompt overriding an analysis prompt).
    #[serde(
        default,
        deserialize_with = "deserialize_rate_limit_continuation_prompt"
    )]
    pub rate_limit_continuation_prompt: Option<RateLimitContinuationPrompt>,
    /// Session ID from the last agent response.
    ///
    /// Used for XSD retry to continue with the same session when possible.
    /// Agents that support sessions (e.g., Claude Code) emit session IDs
    /// that can be passed back for continuation.
    #[serde(default)]
    pub last_session_id: Option<String>,
}

/// Role-scoped continuation prompt captured from a rate limit (429).
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct RateLimitContinuationPrompt {
    pub role: AgentRole,
    pub prompt: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RateLimitContinuationPromptRepr {
    LegacyString(String),
    Structured { role: AgentRole, prompt: String },
}

fn deserialize_rate_limit_continuation_prompt<'de, D>(
    deserializer: D,
) -> Result<Option<RateLimitContinuationPrompt>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<RateLimitContinuationPromptRepr>::deserialize(deserializer)?;
    Ok(opt.map(|repr| match repr {
        RateLimitContinuationPromptRepr::LegacyString(prompt) => RateLimitContinuationPrompt {
            role: AgentRole::Developer,
            prompt,
        },
        RateLimitContinuationPromptRepr::Structured { role, prompt } => {
            RateLimitContinuationPrompt { role, prompt }
        }
    }))
}

const fn default_retry_delay_ms() -> u64 {
    1000
}

const fn default_backoff_multiplier() -> f64 {
    2.0
}

const fn default_max_backoff_ms() -> u64 {
    60000
}

impl AgentChainState {
    pub fn initial() -> Self {
        Self {
            agents: vec![].into_boxed_slice(),
            current_agent_index: 0,
            models_per_agent: vec![].into_boxed_slice(),
            current_model_index: 0,
            retry_cycle: 0,
            max_cycles: 3,
            retry_delay_ms: default_retry_delay_ms(),
            backoff_multiplier: default_backoff_multiplier(),
            max_backoff_ms: default_max_backoff_ms(),
            backoff_pending_ms: None,
            current_role: AgentRole::Developer,
            rate_limit_continuation_prompt: None,
            last_session_id: None,
        }
    }

    pub fn with_agents(
        mut self,
        agents: Vec<String>,
        models_per_agent: Vec<Vec<String>>,
        role: AgentRole,
    ) -> Self {
        self.agents = agents.into_boxed_slice();
        self.models_per_agent = models_per_agent.into_boxed_slice();
        self.current_role = role;
        self
    }

    /// Builder method to set the maximum number of retry cycles.
    ///
    /// A retry cycle is when all agents have been exhausted and we start
    /// over with exponential backoff.
    pub fn with_max_cycles(mut self, max_cycles: u32) -> Self {
        self.max_cycles = max_cycles;
        self
    }

    pub fn with_backoff_policy(
        mut self,
        retry_delay_ms: u64,
        backoff_multiplier: f64,
        max_backoff_ms: u64,
    ) -> Self {
        self.retry_delay_ms = retry_delay_ms;
        self.backoff_multiplier = backoff_multiplier;
        self.max_backoff_ms = max_backoff_ms;
        self
    }

    pub fn current_agent(&self) -> Option<&String> {
        self.agents.get(self.current_agent_index)
    }

    /// Stable signature of the current consumer set (agents + configured models + role).
    ///
    /// This is used to dedupe oversize materialization decisions across reducer retries.
    /// The signature is stable under:
    /// - switching the current agent/model index
    /// - retry cycles
    ///
    /// It changes only when the configured consumer set changes.
    pub fn consumer_signature_sha256(&self) -> String {
        let mut pairs: Vec<String> = self
            .agents
            .iter()
            .enumerate()
            .map(|(idx, agent)| {
                let models = self
                    .models_per_agent
                    .get(idx)
                    .cloned()
                    .unwrap_or_default()
                    .join(",");
                format!("{agent}|{models}")
            })
            .collect();
        pairs.sort();

        let mut hasher = Sha256::new();
        hasher.update(format!("{:?}\n", self.current_role).as_bytes());
        for pair in pairs {
            hasher.update(pair.as_bytes());
            hasher.update(b"\n");
        }
        let digest = hasher.finalize();
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Get the currently selected model for the current agent.
    ///
    /// Returns `None` if:
    /// - No models are configured
    /// - The current agent index is out of bounds
    /// - The current model index is out of bounds
    pub fn current_model(&self) -> Option<&String> {
        self.models_per_agent
            .get(self.current_agent_index)
            .and_then(|models| models.get(self.current_model_index))
    }

    pub fn is_exhausted(&self) -> bool {
        self.retry_cycle >= self.max_cycles
            && self.current_agent_index == 0
            && self.current_model_index == 0
    }

    pub fn advance_to_next_model(&self) -> Self {
        let start_agent_index = self.current_agent_index;

        // When models are configured, we try each model for the current agent once.
        // If the models list is exhausted, advance to the next agent/retry cycle
        // instead of looping models indefinitely.
        let mut next = match self.models_per_agent.get(self.current_agent_index) {
            Some(models) if !models.is_empty() => {
                if self.current_model_index + 1 < models.len() {
                    // Simple model advance - only increment model index
                    Self {
                        agents: self.agents.clone(),
                        current_agent_index: self.current_agent_index,
                        models_per_agent: self.models_per_agent.clone(),
                        current_model_index: self.current_model_index + 1,
                        retry_cycle: self.retry_cycle,
                        max_cycles: self.max_cycles,
                        retry_delay_ms: self.retry_delay_ms,
                        backoff_multiplier: self.backoff_multiplier,
                        max_backoff_ms: self.max_backoff_ms,
                        backoff_pending_ms: self.backoff_pending_ms,
                        current_role: self.current_role,
                        rate_limit_continuation_prompt: self.rate_limit_continuation_prompt.clone(),
                        last_session_id: self.last_session_id.clone(),
                    }
                } else {
                    self.switch_to_next_agent()
                }
            }
            _ => self.switch_to_next_agent(),
        };

        if next.current_agent_index != start_agent_index {
            next.last_session_id = None;
        }

        next
    }

    pub fn switch_to_next_agent(&self) -> Self {
        if self.current_agent_index + 1 < self.agents.len() {
            // Advance to next agent
            Self {
                agents: self.agents.clone(),
                current_agent_index: self.current_agent_index + 1,
                models_per_agent: self.models_per_agent.clone(),
                current_model_index: 0,
                retry_cycle: self.retry_cycle,
                max_cycles: self.max_cycles,
                retry_delay_ms: self.retry_delay_ms,
                backoff_multiplier: self.backoff_multiplier,
                max_backoff_ms: self.max_backoff_ms,
                backoff_pending_ms: None,
                current_role: self.current_role,
                rate_limit_continuation_prompt: self.rate_limit_continuation_prompt.clone(),
                last_session_id: self.last_session_id.clone(),
            }
        } else {
            // Wrap around to first agent and increment retry cycle
            let new_retry_cycle = self.retry_cycle + 1;
            let new_backoff_pending_ms = if new_retry_cycle >= self.max_cycles {
                None
            } else {
                // Create temporary state to calculate backoff
                let temp = Self {
                    agents: self.agents.clone(),
                    current_agent_index: 0,
                    models_per_agent: self.models_per_agent.clone(),
                    current_model_index: 0,
                    retry_cycle: new_retry_cycle,
                    max_cycles: self.max_cycles,
                    retry_delay_ms: self.retry_delay_ms,
                    backoff_multiplier: self.backoff_multiplier,
                    max_backoff_ms: self.max_backoff_ms,
                    backoff_pending_ms: None,
                    current_role: self.current_role,
                    rate_limit_continuation_prompt: None,
                    last_session_id: None,
                };
                Some(temp.calculate_backoff_delay_ms_for_retry_cycle())
            };

            Self {
                agents: self.agents.clone(),
                current_agent_index: 0,
                models_per_agent: self.models_per_agent.clone(),
                current_model_index: 0,
                retry_cycle: new_retry_cycle,
                max_cycles: self.max_cycles,
                retry_delay_ms: self.retry_delay_ms,
                backoff_multiplier: self.backoff_multiplier,
                max_backoff_ms: self.max_backoff_ms,
                backoff_pending_ms: new_backoff_pending_ms,
                current_role: self.current_role,
                rate_limit_continuation_prompt: self.rate_limit_continuation_prompt.clone(),
                last_session_id: self.last_session_id.clone(),
            }
        }
    }

    /// Switch to a specific agent by name.
    ///
    /// If `to_agent` is unknown, falls back to `switch_to_next_agent()` to keep the
    /// reducer deterministic.
    pub fn switch_to_agent_named(&self, to_agent: &str) -> Self {
        let Some(target_index) = self.agents.iter().position(|a| a == to_agent) else {
            return self.switch_to_next_agent();
        };

        if target_index == self.current_agent_index {
            // Same agent - just reset model index
            return Self {
                agents: self.agents.clone(),
                current_agent_index: self.current_agent_index,
                models_per_agent: self.models_per_agent.clone(),
                current_model_index: 0,
                retry_cycle: self.retry_cycle,
                max_cycles: self.max_cycles,
                retry_delay_ms: self.retry_delay_ms,
                backoff_multiplier: self.backoff_multiplier,
                max_backoff_ms: self.max_backoff_ms,
                backoff_pending_ms: None,
                current_role: self.current_role,
                rate_limit_continuation_prompt: self.rate_limit_continuation_prompt.clone(),
                last_session_id: self.last_session_id.clone(),
            };
        }

        if target_index <= self.current_agent_index {
            // Treat switching to an earlier agent as starting a new retry cycle.
            let new_retry_cycle = self.retry_cycle + 1;
            let new_backoff_pending_ms = if new_retry_cycle >= self.max_cycles {
                None
            } else {
                // Create temporary state to calculate backoff
                let temp = Self {
                    agents: self.agents.clone(),
                    current_agent_index: target_index,
                    models_per_agent: self.models_per_agent.clone(),
                    current_model_index: 0,
                    retry_cycle: new_retry_cycle,
                    max_cycles: self.max_cycles,
                    retry_delay_ms: self.retry_delay_ms,
                    backoff_multiplier: self.backoff_multiplier,
                    max_backoff_ms: self.max_backoff_ms,
                    backoff_pending_ms: None,
                    current_role: self.current_role,
                    rate_limit_continuation_prompt: None,
                    last_session_id: None,
                };
                Some(temp.calculate_backoff_delay_ms_for_retry_cycle())
            };

            Self {
                agents: self.agents.clone(),
                current_agent_index: target_index,
                models_per_agent: self.models_per_agent.clone(),
                current_model_index: 0,
                retry_cycle: new_retry_cycle,
                max_cycles: self.max_cycles,
                retry_delay_ms: self.retry_delay_ms,
                backoff_multiplier: self.backoff_multiplier,
                max_backoff_ms: self.max_backoff_ms,
                backoff_pending_ms: new_backoff_pending_ms,
                current_role: self.current_role,
                rate_limit_continuation_prompt: self.rate_limit_continuation_prompt.clone(),
                last_session_id: self.last_session_id.clone(),
            }
        } else {
            // Advancing to later agent
            Self {
                agents: self.agents.clone(),
                current_agent_index: target_index,
                models_per_agent: self.models_per_agent.clone(),
                current_model_index: 0,
                retry_cycle: self.retry_cycle,
                max_cycles: self.max_cycles,
                retry_delay_ms: self.retry_delay_ms,
                backoff_multiplier: self.backoff_multiplier,
                max_backoff_ms: self.max_backoff_ms,
                backoff_pending_ms: None,
                current_role: self.current_role,
                rate_limit_continuation_prompt: self.rate_limit_continuation_prompt.clone(),
                last_session_id: self.last_session_id.clone(),
            }
        }
    }

    /// Switch to next agent after rate limit, preserving prompt for continuation.
    ///
    /// This is used when an agent hits a 429 rate limit error. Instead of
    /// retrying with the same agent (which would likely hit rate limits again),
    /// we switch to the next agent and preserve the prompt so the new agent
    /// can continue the same work.
    pub fn switch_to_next_agent_with_prompt(&self, prompt: Option<String>) -> Self {
        let base = self.switch_to_next_agent();
        // Back-compat: older callers didn't track role. Preserve prompt only.
        Self {
            agents: base.agents,
            current_agent_index: base.current_agent_index,
            models_per_agent: base.models_per_agent,
            current_model_index: base.current_model_index,
            retry_cycle: base.retry_cycle,
            max_cycles: base.max_cycles,
            retry_delay_ms: base.retry_delay_ms,
            backoff_multiplier: base.backoff_multiplier,
            max_backoff_ms: base.max_backoff_ms,
            backoff_pending_ms: base.backoff_pending_ms,
            current_role: base.current_role,
            rate_limit_continuation_prompt: prompt.map(|p| RateLimitContinuationPrompt {
                role: base.current_role,
                prompt: p,
            }),
            last_session_id: base.last_session_id,
        }
    }

    /// Switch to next agent after rate limit, preserving prompt for continuation (role-scoped).
    pub fn switch_to_next_agent_with_prompt_for_role(
        &self,
        role: AgentRole,
        prompt: Option<String>,
    ) -> Self {
        let base = self.switch_to_next_agent();
        Self {
            agents: base.agents,
            current_agent_index: base.current_agent_index,
            models_per_agent: base.models_per_agent,
            current_model_index: base.current_model_index,
            retry_cycle: base.retry_cycle,
            max_cycles: base.max_cycles,
            retry_delay_ms: base.retry_delay_ms,
            backoff_multiplier: base.backoff_multiplier,
            max_backoff_ms: base.max_backoff_ms,
            backoff_pending_ms: base.backoff_pending_ms,
            current_role: base.current_role,
            rate_limit_continuation_prompt: prompt
                .map(|p| RateLimitContinuationPrompt { role, prompt: p }),
            last_session_id: base.last_session_id,
        }
    }

    /// Clear continuation prompt after successful execution.
    ///
    /// Called when an agent successfully completes its task, clearing any
    /// saved prompt context from previous rate-limited agents.
    pub fn clear_continuation_prompt(&self) -> Self {
        Self {
            agents: self.agents.clone(),
            current_agent_index: self.current_agent_index,
            models_per_agent: self.models_per_agent.clone(),
            current_model_index: self.current_model_index,
            retry_cycle: self.retry_cycle,
            max_cycles: self.max_cycles,
            retry_delay_ms: self.retry_delay_ms,
            backoff_multiplier: self.backoff_multiplier,
            max_backoff_ms: self.max_backoff_ms,
            backoff_pending_ms: self.backoff_pending_ms,
            current_role: self.current_role,
            rate_limit_continuation_prompt: None,
            last_session_id: self.last_session_id.clone(),
        }
    }

    pub fn reset_for_role(&self, role: AgentRole) -> Self {
        Self {
            agents: self.agents.clone(),
            current_agent_index: 0,
            models_per_agent: self.models_per_agent.clone(),
            current_model_index: 0,
            retry_cycle: 0,
            max_cycles: self.max_cycles,
            retry_delay_ms: self.retry_delay_ms,
            backoff_multiplier: self.backoff_multiplier,
            max_backoff_ms: self.max_backoff_ms,
            backoff_pending_ms: None,
            current_role: role,
            rate_limit_continuation_prompt: None,
            last_session_id: None,
        }
    }

    pub fn reset(&self) -> Self {
        Self {
            agents: self.agents.clone(),
            current_agent_index: 0,
            models_per_agent: self.models_per_agent.clone(),
            current_model_index: 0,
            retry_cycle: self.retry_cycle,
            max_cycles: self.max_cycles,
            retry_delay_ms: self.retry_delay_ms,
            backoff_multiplier: self.backoff_multiplier,
            max_backoff_ms: self.max_backoff_ms,
            backoff_pending_ms: None,
            current_role: self.current_role,
            rate_limit_continuation_prompt: None,
            last_session_id: None,
        }
    }

    /// Store session ID from agent response for potential reuse.
    pub fn with_session_id(&self, session_id: Option<String>) -> Self {
        Self {
            agents: self.agents.clone(),
            current_agent_index: self.current_agent_index,
            models_per_agent: self.models_per_agent.clone(),
            current_model_index: self.current_model_index,
            retry_cycle: self.retry_cycle,
            max_cycles: self.max_cycles,
            retry_delay_ms: self.retry_delay_ms,
            backoff_multiplier: self.backoff_multiplier,
            max_backoff_ms: self.max_backoff_ms,
            backoff_pending_ms: self.backoff_pending_ms,
            current_role: self.current_role,
            rate_limit_continuation_prompt: self.rate_limit_continuation_prompt.clone(),
            last_session_id: session_id,
        }
    }

    /// Clear session ID (e.g., when switching agents or starting new work).
    pub fn clear_session_id(&self) -> Self {
        Self {
            agents: self.agents.clone(),
            current_agent_index: self.current_agent_index,
            models_per_agent: self.models_per_agent.clone(),
            current_model_index: self.current_model_index,
            retry_cycle: self.retry_cycle,
            max_cycles: self.max_cycles,
            retry_delay_ms: self.retry_delay_ms,
            backoff_multiplier: self.backoff_multiplier,
            max_backoff_ms: self.max_backoff_ms,
            backoff_pending_ms: self.backoff_pending_ms,
            current_role: self.current_role,
            rate_limit_continuation_prompt: self.rate_limit_continuation_prompt.clone(),
            last_session_id: None,
        }
    }

    pub fn start_retry_cycle(&self) -> Self {
        let new_retry_cycle = self.retry_cycle + 1;
        let new_backoff_pending_ms = if new_retry_cycle >= self.max_cycles {
            None
        } else {
            // Create temporary state to calculate backoff
            let temp = Self {
                agents: self.agents.clone(),
                current_agent_index: 0,
                models_per_agent: self.models_per_agent.clone(),
                current_model_index: 0,
                retry_cycle: new_retry_cycle,
                max_cycles: self.max_cycles,
                retry_delay_ms: self.retry_delay_ms,
                backoff_multiplier: self.backoff_multiplier,
                max_backoff_ms: self.max_backoff_ms,
                backoff_pending_ms: None,
                current_role: self.current_role,
                rate_limit_continuation_prompt: None,
                last_session_id: None,
            };
            Some(temp.calculate_backoff_delay_ms_for_retry_cycle())
        };

        Self {
            agents: self.agents.clone(),
            current_agent_index: 0,
            models_per_agent: self.models_per_agent.clone(),
            current_model_index: 0,
            retry_cycle: new_retry_cycle,
            max_cycles: self.max_cycles,
            retry_delay_ms: self.retry_delay_ms,
            backoff_multiplier: self.backoff_multiplier,
            max_backoff_ms: self.max_backoff_ms,
            backoff_pending_ms: new_backoff_pending_ms,
            current_role: self.current_role,
            rate_limit_continuation_prompt: self.rate_limit_continuation_prompt.clone(),
            last_session_id: self.last_session_id.clone(),
        }
    }

    pub fn clear_backoff_pending(&self) -> Self {
        Self {
            agents: self.agents.clone(),
            current_agent_index: self.current_agent_index,
            models_per_agent: self.models_per_agent.clone(),
            current_model_index: self.current_model_index,
            retry_cycle: self.retry_cycle,
            max_cycles: self.max_cycles,
            retry_delay_ms: self.retry_delay_ms,
            backoff_multiplier: self.backoff_multiplier,
            max_backoff_ms: self.max_backoff_ms,
            backoff_pending_ms: None,
            current_role: self.current_role,
            rate_limit_continuation_prompt: self.rate_limit_continuation_prompt.clone(),
            last_session_id: self.last_session_id.clone(),
        }
    }

    fn calculate_backoff_delay_ms_for_retry_cycle(&self) -> u64 {
        // The first retry cycle should use the base delay.
        let cycle_index = self.retry_cycle.saturating_sub(1);
        calculate_backoff_delay_ms(
            self.retry_delay_ms,
            self.backoff_multiplier,
            self.max_backoff_ms,
            cycle_index,
        )
    }
}

// Backoff computation helpers.
// These mirror the semantics in `crate::agents::fallback::FallbackConfig::calculate_backoff`
// but live in reducer state so orchestration can derive BackoffWait effects purely.

const IEEE_754_EXP_BIAS: i32 = 1023;
const IEEE_754_EXP_MASK: u64 = 0x7FF;
const IEEE_754_MANTISSA_MASK: u64 = 0x000F_FFFF_FFFF_FFFF;
const IEEE_754_IMPLICIT_ONE: u64 = 1u64 << 52;

fn f64_to_u64_via_bits(value: f64) -> u64 {
    if !value.is_finite() || value < 0.0 {
        return 0;
    }
    let bits = value.to_bits();
    let exp_biased = ((bits >> 52) & IEEE_754_EXP_MASK) as i32;
    let mantissa = bits & IEEE_754_MANTISSA_MASK;
    if exp_biased == 0 {
        return 0;
    }
    let exp = exp_biased - IEEE_754_EXP_BIAS;
    if exp < 0 {
        return 0;
    }
    let full_mantissa = mantissa | IEEE_754_IMPLICIT_ONE;
    let shift = 52i32 - exp;
    if shift <= 0 {
        u64::MAX
    } else if shift < 64 {
        full_mantissa >> shift
    } else {
        0
    }
}

fn multiplier_hundredths(backoff_multiplier: f64) -> u64 {
    const EPSILON: f64 = 0.0001;
    let m = backoff_multiplier;
    if (m - 1.0).abs() < EPSILON {
        return 100;
    } else if (m - 1.5).abs() < EPSILON {
        return 150;
    } else if (m - 2.0).abs() < EPSILON {
        return 200;
    } else if (m - 2.5).abs() < EPSILON {
        return 250;
    } else if (m - 3.0).abs() < EPSILON {
        return 300;
    } else if (m - 4.0).abs() < EPSILON {
        return 400;
    } else if (m - 5.0).abs() < EPSILON {
        return 500;
    } else if (m - 10.0).abs() < EPSILON {
        return 1000;
    }

    let clamped = m.clamp(0.0, 1000.0);
    let multiplied = clamped * 100.0;
    let rounded = multiplied.round();
    f64_to_u64_via_bits(rounded)
}

fn calculate_backoff_delay_ms(
    retry_delay_ms: u64,
    backoff_multiplier: f64,
    max_backoff_ms: u64,
    cycle: u32,
) -> u64 {
    let mult_hundredths = multiplier_hundredths(backoff_multiplier);
    let mut delay_hundredths = retry_delay_ms.saturating_mul(100);
    for _ in 0..cycle {
        delay_hundredths = delay_hundredths.saturating_mul(mult_hundredths);
        delay_hundredths = delay_hundredths.saturating_div(100);
    }
    delay_hundredths.div_euclid(100).min(max_backoff_ms)
}
