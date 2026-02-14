// Serialization and deserialization logic for checkpoint state.
//
// This file contains workspace-based checkpoint functions for loading,
// saving, and validating checkpoints.

/// Load a checkpoint from a string.
///
/// Only v3 (current) checkpoint format is supported. Legacy formats (v1, v2, pre-v1)
/// and legacy phases (Fix, ReviewAgain) are no longer supported and will result in an error.
fn load_checkpoint_with_fallback(
    content: &str,
) -> Result<PipelineCheckpoint, Box<dyn std::error::Error>> {
    // Only accept v3 format (current)
    match serde_json::from_str::<PipelineCheckpoint>(content) {
        Ok(checkpoint) => {
            // Accept v3 (current) or higher
            if checkpoint.version >= 3 {
                return Ok(checkpoint);
            }
            // Reject older versions
            Err(format!(
                "Invalid checkpoint format: version {} is no longer supported. \
                 Only version 3 (current) is accepted. \
                 Delete .agent/checkpoint.json and start a fresh pipeline run.",
                checkpoint.version
            )
            .into())
        }
        Err(e) => {
            // Parsing failed - likely legacy format or legacy phase
            Err(format!(
                "Invalid checkpoint format: {}. \
                 Legacy checkpoint formats are no longer supported. \
                 Delete .agent/checkpoint.json and start a fresh pipeline run.",
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

    // SAFETY: serde_json guarantees valid UTF-8
    let json = unsafe { String::from_utf8_unchecked(buf) };

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
    // Base size: metadata + config + snapshots
    const BASE_SIZE: usize = 10_000;
    // Average bytes per execution history entry (includes JSON overhead)
    const BYTES_PER_ENTRY: usize = 400;

    let history_len = checkpoint
        .execution_history
        .as_ref()
        .map(|h| h.steps.len())
        .unwrap_or(0);

    BASE_SIZE + (history_len * BYTES_PER_ENTRY)
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
