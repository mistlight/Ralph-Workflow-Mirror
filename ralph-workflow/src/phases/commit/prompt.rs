fn build_commit_prompt(
    prompt_key: &str,
    template_context: &TemplateContext,
    working_diff: &str,
    workspace: &dyn Workspace,
    prompt_history: &HashMap<String, String>,
) -> (String, bool) {
    get_stored_or_generate_prompt(prompt_key, prompt_history, || {
        prompt_generate_commit_message_with_diff_with_context(
            template_context,
            working_diff,
            workspace,
        )
    })
}

fn stderr_contains_auth_error(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("authentication")
        || lower.contains("api key")
        || lower.contains("invalid key")
        || lower.contains("unauthorized")
        || lower.contains("permission denied")
}
