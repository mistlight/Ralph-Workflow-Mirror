// Serialization and deserialization logic for checkpoint state.
//
// This file contains workspace-based checkpoint functions for loading,
// saving, and validating checkpoints.

/// Load a checkpoint from a string.
///
/// Load a checkpoint from a string, with minimal compatibility handling.
///
/// Supported versions:
/// - v3 (current)
/// - v2 (migrated in-memory to v3 by bumping `version`; v3-only fields remain empty)
///
/// Legacy formats (v1, pre-v1) and legacy phases (Fix, ReviewAgain) are not supported.
fn load_checkpoint_with_fallback(
    content: &str,
) -> Result<PipelineCheckpoint, Box<dyn std::error::Error>> {
    // Parse using the current struct shape; serde will default missing Option fields.
    match serde_json::from_str::<PipelineCheckpoint>(content) {
        Ok(mut checkpoint) => {
            // v2 -> v3 migration (in-memory)
            if checkpoint.version == 2 {
                checkpoint.version = 3;
                return Ok(checkpoint);
            }

            // Accept only the current version.
            if checkpoint.version == 3 {
                return Ok(checkpoint);
            }

            // Fail closed on newer versions; future formats may be incompatible.
            if checkpoint.version > 3 {
                return Err(format!(
                    "Invalid checkpoint format: version {} is newer than this binary supports. \
                     Supported versions: 2 (migrated) and 3 (current). \
                     Please upgrade Ralph Workflow to resume this checkpoint.",
                    checkpoint.version
                )
                .into());
            }

            Err(format!(
                "Invalid checkpoint format: version {} is no longer supported. \
                 Supported versions: 2 (migrated) and 3 (current). \
                 To start fresh without data loss: cp .agent/checkpoint.json .agent/checkpoint.backup.json && rm .agent/checkpoint.json",
                checkpoint.version
            )
            .into())
        }
        Err(e) => {
            // Parsing failed - likely legacy format or legacy phase
            Err(format!(
                "Invalid checkpoint format: {}. \
                 Legacy checkpoint formats are no longer supported. \
                 To start fresh without data loss: cp .agent/checkpoint.json .agent/checkpoint.backup.json && rm .agent/checkpoint.json",
                e
            )
            .into())
        }
    }
}

// ============================================================================
// Workspace-based checkpoint functions (for testability with MemoryWorkspace)
// ============================================================================

/// Calculate SHA-256 checksum of a file using the workspace.
///
/// # Arguments
///
/// * `workspace` - The workspace for file operations
/// * `path` - Relative path within the workspace
///
/// Returns `None` if the file doesn't exist or cannot be read.
pub fn calculate_file_checksum_with_workspace(
    workspace: &dyn Workspace,
    path: &Path,
) -> Option<String> {
    let content = workspace.read_bytes(path).ok()?;
    Some(calculate_checksum_from_bytes(&content))
}

/// Save a pipeline checkpoint using the workspace.
///
/// # Arguments
///
/// * `workspace` - The workspace for file operations
/// * `checkpoint` - The checkpoint to save
///
/// # Performance
///
/// Uses optimized serialization with pre-allocated buffer and compact JSON
/// encoding (no pretty-printing) to minimize serialization time.
pub fn save_checkpoint_with_workspace(
    workspace: &dyn Workspace,
    checkpoint: &PipelineCheckpoint,
) -> io::Result<()> {
    // Estimate serialized size to pre-allocate buffer and avoid reallocation
    let estimated_size = estimate_checkpoint_size(checkpoint);
    let mut buf = Vec::with_capacity(estimated_size);

    // Use compact serialization (no pretty printing) with pre-sized buffer
    serde_json::to_writer(&mut buf, checkpoint).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize checkpoint: {e}"),
        )
    })?;

    let json = String::from_utf8(buf).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Checkpoint JSON was not valid UTF-8: {e}"),
        )
    })?;

    // Ensure the .agent directory exists
    workspace.create_dir_all(Path::new(AGENT_DIR))?;

    // Write checkpoint file atomically
    workspace.write_atomic(Path::new(&checkpoint_path()), &json)
}

/// Estimate the serialized JSON size of a checkpoint for buffer pre-allocation.
///
/// This heuristic is based on observed checkpoint sizes:
/// - Base overhead: ~10KB for metadata, config snapshots, and structure
/// - Per-entry cost: ~400 bytes for execution history entries
///
/// The estimate is conservative (slightly over) to avoid reallocation while
/// not wasting excessive memory.
fn estimate_checkpoint_size(checkpoint: &PipelineCheckpoint) -> usize {
    let history_len = checkpoint
        .execution_history
        .as_ref()
        .map(|h| h.steps.len())
        .unwrap_or(0);

    estimate_checkpoint_size_from_history_len(history_len)
}

const MAX_CHECKPOINT_ESTIMATE_BYTES: usize = 50 * 1024 * 1024;

fn estimate_checkpoint_size_from_history_len(history_len: usize) -> usize {
    // Base size: metadata + config + snapshots
    const BASE_SIZE: usize = 10_000;
    // Average bytes per execution history entry (includes JSON overhead)
    const BYTES_PER_ENTRY: usize = 400;

    BASE_SIZE
        .saturating_add(history_len.saturating_mul(BYTES_PER_ENTRY))
        .min(MAX_CHECKPOINT_ESTIMATE_BYTES)
}

/// Load an existing checkpoint using the workspace.
///
/// Returns `Ok(Some(checkpoint))` if a valid checkpoint was loaded,
/// `Ok(None)` if no checkpoint file exists, or an error if the file
/// exists but cannot be parsed.
pub fn load_checkpoint_with_workspace(
    workspace: &dyn Workspace,
) -> io::Result<Option<PipelineCheckpoint>> {
    let checkpoint_path_str = checkpoint_path();
    let checkpoint_file = Path::new(&checkpoint_path_str);

    if !workspace.exists(checkpoint_file) {
        return Ok(None);
    }

    let content = workspace.read(checkpoint_file)?;
    let loaded_checkpoint = load_checkpoint_with_fallback(&content).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse checkpoint: {e}"),
        )
    })?;

    Ok(Some(loaded_checkpoint))
}

/// Delete the checkpoint file using the workspace.
///
/// Does nothing if the checkpoint file doesn't exist.
pub fn clear_checkpoint_with_workspace(workspace: &dyn Workspace) -> io::Result<()> {
    let checkpoint_path_str = checkpoint_path();
    let checkpoint_file = Path::new(&checkpoint_path_str);

    if workspace.exists(checkpoint_file) {
        workspace.remove(checkpoint_file)?;
    }
    Ok(())
}

/// Check if a checkpoint exists using the workspace.
pub fn checkpoint_exists_with_workspace(workspace: &dyn Workspace) -> bool {
    workspace.exists(Path::new(&checkpoint_path()))
}
