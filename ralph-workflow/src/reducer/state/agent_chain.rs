// Agent fallback chain state.
//
// Contains AgentChainState and backoff computation helpers.
//
// # Performance Optimization
//
// AgentChainState uses Arc<[T]> for immutable collections (agents, models_per_agent)
// to enable cheap state copying during state transitions. This eliminates O(n) deep
// copy overhead and makes state transitions O(1) for collection fields.
//
// The reducer creates new state instances on every event, so this optimization
// significantly reduces memory allocations and improves performance.

use std::sync::Arc;

use sha2::{Digest, Sha256};

/// Agent fallback chain state (explicit, not loop indices).
///
/// Tracks position in the multi-level fallback chain:
/// - Agent level (primary → fallback1 → fallback2)
/// - Model level (within each agent, try different models)
/// - Retry cycle (exhaust all agents, start over with exponential backoff)
///
/// # Memory Optimization
///
/// Uses Arc<[T]> for `agents` and `models_per_agent` collections to enable
/// cheap cloning during state transitions. Since these collections are immutable
/// after construction, `Arc::clone` only increments a reference count instead of
/// deep copying the entire collection.
#[derive(Clone, Serialize, Debug)]
pub struct AgentChainState {
    /// Agent names in fallback order. Arc<[String]> enables cheap cloning
    /// via reference counting instead of deep copying the collection.
    pub agents: Arc<[String]>,
    pub current_agent_index: usize,
    /// Models per agent. Arc for immutable outer collection with cheap cloning.
    /// Inner Vec<String> is kept for runtime indexing during model selection.
    pub models_per_agent: Arc<[Vec<String>]>,
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
    #[serde(default)]
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

impl<'de> Deserialize<'de> for AgentChainState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct AgentChainStateSerde {
            agents: Arc<[String]>,
            current_agent_index: usize,
            models_per_agent: Arc<[Vec<String>]>,
            current_model_index: usize,
            retry_cycle: u32,
            max_cycles: u32,
            #[serde(default = "default_retry_delay_ms")]
            retry_delay_ms: u64,
            #[serde(default = "default_backoff_multiplier")]
            backoff_multiplier: f64,
            #[serde(default = "default_max_backoff_ms")]
            max_backoff_ms: u64,
            #[serde(default)]
            backoff_pending_ms: Option<u64>,
            current_role: AgentRole,
            #[serde(default)]
            rate_limit_continuation_prompt: Option<RateLimitContinuationPromptRepr>,
            #[serde(default)]
            last_session_id: Option<String>,
        }

        let raw = AgentChainStateSerde::deserialize(deserializer)?;

        let rate_limit_continuation_prompt = raw.rate_limit_continuation_prompt.map(|repr| {
            match repr {
                RateLimitContinuationPromptRepr::LegacyString(prompt) => {
                    // Legacy checkpoints stored only the prompt string. Scope it to the
                    // chain's current role so resume can't cross-contaminate roles.
                    RateLimitContinuationPrompt {
                        role: raw.current_role,
                        prompt,
                    }
                }
                RateLimitContinuationPromptRepr::Structured { role, prompt } => {
                    RateLimitContinuationPrompt { role, prompt }
                }
            }
        });

        Ok(Self {
            agents: raw.agents,
            current_agent_index: raw.current_agent_index,
            models_per_agent: raw.models_per_agent,
            current_model_index: raw.current_model_index,
            retry_cycle: raw.retry_cycle,
            max_cycles: raw.max_cycles,
            retry_delay_ms: raw.retry_delay_ms,
            backoff_multiplier: raw.backoff_multiplier,
            max_backoff_ms: raw.max_backoff_ms,
            backoff_pending_ms: raw.backoff_pending_ms,
            current_role: raw.current_role,
            rate_limit_continuation_prompt,
            last_session_id: raw.last_session_id,
        })
    }
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

const fn agent_role_signature_tag(role: AgentRole) -> &'static [u8] {
    match role {
        AgentRole::Developer => b"developer\n",
        AgentRole::Reviewer => b"reviewer\n",
        AgentRole::Commit => b"commit\n",
        AgentRole::Analysis => b"analysis\n",
    }
}

impl AgentChainState {
    #[must_use] 
    pub fn initial() -> Self {
        Self {
            agents: Arc::from(vec![]),
            current_agent_index: 0,
            models_per_agent: Arc::from(vec![]),
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

    #[must_use] 
    pub fn with_agents(
        mut self,
        agents: Vec<String>,
        models_per_agent: Vec<Vec<String>>,
        role: AgentRole,
    ) -> Self {
        self.agents = Arc::from(agents);
        self.models_per_agent = Arc::from(models_per_agent);
        self.current_role = role;
        self
    }

    /// Builder method to set the maximum number of retry cycles.
    ///
    /// A retry cycle is when all agents have been exhausted and we start
    /// over with exponential backoff.
    #[must_use] 
    pub const fn with_max_cycles(mut self, max_cycles: u32) -> Self {
        self.max_cycles = max_cycles;
        self
    }

    #[must_use] 
    pub const fn with_backoff_policy(
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

    #[must_use] 
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
    #[must_use] 
    pub fn consumer_signature_sha256(&self) -> String {
        let mut pairs: Vec<(&str, &[String])> = self
            .agents
            .iter()
            .enumerate()
            .map(|(idx, agent)| {
                let models: &[String] = self
                    .models_per_agent
                    .get(idx)
                    .map_or(&[], std::vec::Vec::as_slice);
                (agent.as_str(), models)
            })
            .collect();

        // Sort so the signature is stable even if callers reorder the configured
        // consumer set.
        pairs.sort_by(|(agent_a, models_a), (agent_b, models_b)| {
            use std::cmp::Ordering;

            let agent_ord = agent_a.cmp(agent_b);
            if agent_ord != Ordering::Equal {
                return agent_ord;
            }

            for (a, b) in models_a.iter().zip(models_b.iter()) {
                let ord = a.cmp(b);
                if ord != Ordering::Equal {
                    return ord;
                }
            }

            models_a.len().cmp(&models_b.len())
        });

        let mut hasher = Sha256::new();
        hasher.update(agent_role_signature_tag(self.current_role));
        for (agent, models) in pairs {
            hasher.update(agent.as_bytes());
            hasher.update(b"|");
            for (idx, model) in models.iter().enumerate() {
                if idx > 0 {
                    hasher.update(b",");
                }
                hasher.update(model.as_bytes());
            }
            hasher.update(b"\n");
        }
        let digest = hasher.finalize();
        digest.iter().fold(String::new(), |mut s, b| {
            use std::fmt::Write;
            write!(&mut s, "{b:02x}").unwrap();
            s
        })
    }

    #[cfg(test)]
    fn legacy_consumer_signature_sha256_for_test(&self) -> String {
        let mut rendered: Vec<String> = self
            .agents
            .iter()
            .enumerate()
            .map(|(idx, agent)| {
                let models = self
                    .models_per_agent
                    .get(idx)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);
                format!("{}|{}", agent, models.join(","))
            })
            .collect();

        rendered.sort();

        let mut hasher = Sha256::new();
        hasher.update(agent_role_signature_tag(self.current_role));
        for line in rendered {
            hasher.update(line.as_bytes());
            hasher.update(b"\n");
        }
        let digest = hasher.finalize();
        digest.iter().fold(String::new(), |mut s, b| {
            use std::fmt::Write;
            write!(&mut s, "{b:02x}").unwrap();
            s
        })
    }

    /// Get the currently selected model for the current agent.
    ///
    /// Returns `None` if:
    /// - No models are configured
    /// - The current agent index is out of bounds
    /// - The current model index is out of bounds
    #[must_use] 
    pub fn current_model(&self) -> Option<&String> {
        self.models_per_agent
            .get(self.current_agent_index)
            .and_then(|models| models.get(self.current_model_index))
    }

    #[must_use] 
    pub const fn is_exhausted(&self) -> bool {
        self.retry_cycle >= self.max_cycles
            && self.current_agent_index == 0
            && self.current_model_index == 0
    }

    #[must_use] 
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
                        agents: Arc::clone(&self.agents),
                        current_agent_index: self.current_agent_index,
                        models_per_agent: Arc::clone(&self.models_per_agent),
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

    #[must_use] 
    pub fn switch_to_next_agent(&self) -> Self {
        if self.current_agent_index + 1 < self.agents.len() {
            // Advance to next agent
            Self {
                agents: Arc::clone(&self.agents),
                current_agent_index: self.current_agent_index + 1,
                models_per_agent: Arc::clone(&self.models_per_agent),
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
                    agents: Arc::clone(&self.agents),
                    current_agent_index: 0,
                    models_per_agent: Arc::clone(&self.models_per_agent),
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
                agents: Arc::clone(&self.agents),
                current_agent_index: 0,
                models_per_agent: Arc::clone(&self.models_per_agent),
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
    #[must_use] 
    pub fn switch_to_agent_named(&self, to_agent: &str) -> Self {
        let Some(target_index) = self.agents.iter().position(|a| a == to_agent) else {
            return self.switch_to_next_agent();
        };

        if target_index == self.current_agent_index {
            // Same agent - just reset model index
            return Self {
                agents: Arc::clone(&self.agents),
                current_agent_index: self.current_agent_index,
                models_per_agent: Arc::clone(&self.models_per_agent),
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
            let new_backoff_pending_ms = if new_retry_cycle >= self.max_cycles && target_index == 0
            {
                None
            } else {
                // Create temporary state to calculate backoff
                let temp = Self {
                    agents: Arc::clone(&self.agents),
                    current_agent_index: target_index,
                    models_per_agent: Arc::clone(&self.models_per_agent),
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
                agents: Arc::clone(&self.agents),
                current_agent_index: target_index,
                models_per_agent: Arc::clone(&self.models_per_agent),
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
                agents: Arc::clone(&self.agents),
                current_agent_index: target_index,
                models_per_agent: Arc::clone(&self.models_per_agent),
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
    #[must_use] 
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
    #[must_use] 
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
    #[must_use] 
    pub fn clear_continuation_prompt(&self) -> Self {
        Self {
            agents: Arc::clone(&self.agents),
            current_agent_index: self.current_agent_index,
            models_per_agent: Arc::clone(&self.models_per_agent),
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

    #[must_use] 
    pub fn reset_for_role(&self, role: AgentRole) -> Self {
        Self {
            agents: Arc::clone(&self.agents),
            current_agent_index: 0,
            models_per_agent: Arc::clone(&self.models_per_agent),
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

    #[must_use] 
    pub fn reset(&self) -> Self {
        Self {
            agents: Arc::clone(&self.agents),
            current_agent_index: 0,
            models_per_agent: Arc::clone(&self.models_per_agent),
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
    #[must_use] 
    pub fn with_session_id(&self, session_id: Option<String>) -> Self {
        Self {
            agents: Arc::clone(&self.agents),
            current_agent_index: self.current_agent_index,
            models_per_agent: Arc::clone(&self.models_per_agent),
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
    #[must_use] 
    pub fn clear_session_id(&self) -> Self {
        Self {
            agents: Arc::clone(&self.agents),
            current_agent_index: self.current_agent_index,
            models_per_agent: Arc::clone(&self.models_per_agent),
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

    #[must_use] 
    pub fn start_retry_cycle(&self) -> Self {
        let new_retry_cycle = self.retry_cycle + 1;
        let new_backoff_pending_ms = if new_retry_cycle >= self.max_cycles {
            None
        } else {
            // Create temporary state to calculate backoff
            let temp = Self {
                agents: Arc::clone(&self.agents),
                current_agent_index: 0,
                models_per_agent: Arc::clone(&self.models_per_agent),
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
            agents: Arc::clone(&self.agents),
            current_agent_index: 0,
            models_per_agent: Arc::clone(&self.models_per_agent),
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

    #[must_use] 
    pub fn clear_backoff_pending(&self) -> Self {
        Self {
            agents: Arc::clone(&self.agents),
            current_agent_index: self.current_agent_index,
            models_per_agent: Arc::clone(&self.models_per_agent),
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

#[cfg(test)]
mod consumer_signature_tests {
    use super::*;

    #[test]
    fn test_consumer_signature_sorting_matches_legacy_rendered_pair_ordering() {
        // This regression test locks in the pre-optimization signature ordering:
        // sort by the lexicographic ordering of the rendered `agent|models_csv` strings.
        //
        // A length-first models compare changes ordering when the first model differs.
        // Example: "a,z" must sort before "b" even though it is longer.
        let state = AgentChainState::initial().with_agents(
            vec!["agent".to_string(), "agent".to_string()],
            vec![
                vec!["b".to_string()],
                vec!["a".to_string(), "z".to_string()],
            ],
            AgentRole::Developer,
        );

        assert_eq!(
            state.consumer_signature_sha256(),
            state.legacy_consumer_signature_sha256_for_test(),
            "consumer signature ordering must remain stable for the same configured consumers"
        );
    }

    #[test]
    fn test_consumer_signature_uses_stable_role_encoding() {
        // The consumer signature is persisted in reducer state and used for dedupe.
        // It must not depend on Debug formatting (variant renames would change the hash).
        // Instead, it should use a stable, explicit role tag.
        let state = AgentChainState::initial().with_agents(
            vec!["agent-a".to_string()],
            vec![vec!["m1".to_string(), "m2".to_string()]],
            AgentRole::Reviewer,
        );

        let mut hasher = Sha256::new();
        hasher.update(b"reviewer\n");
        hasher.update(b"agent-a");
        hasher.update(b"|");
        hasher.update(b"m1");
        hasher.update(b",");
        hasher.update(b"m2");
        hasher.update(b"\n");
        let expected = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();

        assert_eq!(
            state.consumer_signature_sha256(),
            expected,
            "role encoding must be stable and explicit"
        );
    }
}

#[cfg(test)]
mod legacy_rate_limit_prompt_tests {
    use super::*;

    #[test]
    fn test_legacy_rate_limit_continuation_prompt_uses_current_role_on_deserialize() {
        // Legacy checkpoints stored `rate_limit_continuation_prompt` as a bare string.
        // When resuming, we must scope that prompt to the chain's `current_role`
        // (the role the checkpoint was executing) instead of defaulting to Developer.
        let state = AgentChainState::initial().with_agents(
            vec!["a".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        );

        let mut v = serde_json::to_value(&state).expect("serialize AgentChainState");
        v["rate_limit_continuation_prompt"] = serde_json::Value::String("legacy prompt".into());

        let json = serde_json::to_string(&v).expect("serialize JSON value");
        let decoded: AgentChainState =
            serde_json::from_str(&json).expect("deserialize AgentChainState");

        let prompt = decoded
            .rate_limit_continuation_prompt
            .expect("expected legacy prompt to deserialize");
        assert_eq!(prompt.role, AgentRole::Reviewer);
        assert_eq!(prompt.prompt, "legacy prompt");
    }
}

#[cfg(test)]
mod backoff_semantics_tests {
    use super::*;

    #[test]
    fn test_switch_to_agent_named_preserves_backoff_when_retry_cycle_hits_max_but_state_is_not_exhausted(
    ) {
        let mut state = AgentChainState::initial().with_agents(
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
            vec![vec![], vec![], vec![]],
            AgentRole::Developer,
        );
        state.max_cycles = 2;
        state.retry_cycle = 1;
        state.current_agent_index = 2;

        let next = state.switch_to_agent_named("b");

        assert_eq!(next.current_agent_index, 1);
        assert_eq!(next.retry_cycle, 2);
        assert!(
            next.backoff_pending_ms.is_some(),
            "backoff should remain pending unless the state is fully exhausted"
        );
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
