// Phase-specific validated outcome types.
//
// These structures capture the validated results from each pipeline phase
// after XML parsing and schema validation. They represent the contract
// between agent output and reducer state.

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewValidatedOutcome {
    pub pass: u32,
    pub issues_found: bool,
    pub clean_no_issues: bool,
    /// Issues found during review. Box<[String]> saves 8 bytes per instance
    /// vs Vec<String> (no separate capacity field) since this collection
    /// never grows after construction.
    #[serde(default)]
    pub issues: Box<[String]>,
    #[serde(default)]
    pub no_issues_found: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanningValidatedOutcome {
    pub iteration: u32,
    pub valid: bool,
    #[serde(default)]
    pub markdown: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DevelopmentValidatedOutcome {
    pub iteration: u32,
    pub status: DevelopmentStatus,
    pub summary: String,
    /// Files changed during development. Option<Box<[String]>> saves 8 bytes
    /// per instance vs Option<Vec<String>> when Some, and is None when empty
    /// to avoid allocation entirely.
    pub files_changed: Option<Box<[String]>>,
    pub next_steps: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FixValidatedOutcome {
    pub pass: u32,
    pub status: FixStatus,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitValidatedOutcome {
    pub attempt: u32,
    pub message: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct PromptInputsState {
    #[serde(default)]
    pub planning: Option<MaterializedPlanningInputs>,
    #[serde(default)]
    pub development: Option<MaterializedDevelopmentInputs>,
    #[serde(default)]
    pub review: Option<MaterializedReviewInputs>,
    #[serde(default)]
    pub commit: Option<MaterializedCommitInputs>,
    /// Materialized last invalid XML output for XSD retry prompts.
    ///
    /// This is used to dedupe retries and keep oversize handling reducer-visible.
    #[serde(default)]
    pub xsd_retry_last_output: Option<MaterializedXsdRetryLastOutput>,
}

impl PromptInputsState {
    /// Clear commit inputs without cloning other fields.
    /// Uses consuming builder pattern for zero-cost state updates.
    #[must_use] 
    pub fn with_commit_cleared(mut self) -> Self {
        self.commit = None;
        self
    }

    /// Clear planning inputs without cloning other fields.
    #[must_use] 
    pub fn with_planning_cleared(mut self) -> Self {
        self.planning = None;
        self
    }

    /// Clear development inputs without cloning other fields.
    #[must_use] 
    pub fn with_development_cleared(mut self) -> Self {
        self.development = None;
        self
    }

    /// Clear review inputs without cloning other fields.
    #[must_use] 
    pub fn with_review_cleared(mut self) -> Self {
        self.review = None;
        self
    }

    /// Clear XSD retry last output without cloning other fields.
    #[must_use] 
    pub fn with_xsd_retry_cleared(mut self) -> Self {
        self.xsd_retry_last_output = None;
        self
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct MaterializedPlanningInputs {
    pub iteration: u32,
    pub prompt: MaterializedPromptInput,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct MaterializedDevelopmentInputs {
    pub iteration: u32,
    pub prompt: MaterializedPromptInput,
    pub plan: MaterializedPromptInput,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct MaterializedReviewInputs {
    pub pass: u32,
    pub plan: MaterializedPromptInput,
    pub diff: MaterializedPromptInput,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct MaterializedCommitInputs {
    pub attempt: u32,
    pub diff: MaterializedPromptInput,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct MaterializedXsdRetryLastOutput {
    pub phase: PipelinePhase,
    pub scope_id: u32,
    pub last_output: MaterializedPromptInput,
}
