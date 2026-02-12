fn build_commit_prompt(
    prompt_key: &str,
    template_context: &TemplateContext,
    working_diff: &str,
    workspace: &dyn Workspace,
    prompt_history: &HashMap<String, String>,
) -> (String, bool, Option<crate::prompts::SubstitutionLog>) {
    if let Some(stored_prompt) = prompt_history.get(prompt_key) {
        (stored_prompt.clone(), true, None)
    } else {
        let rendered = crate::prompts::prompt_generate_commit_message_with_diff_with_log(
            template_context,
            working_diff,
            workspace,
            "commit_message_xml",
        );
        (rendered.content, false, Some(rendered.log))
    }
}

fn stderr_contains_auth_error(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("authentication")
        || lower.contains("api key")
        || lower.contains("invalid key")
        || lower.contains("unauthorized")
        || lower.contains("permission denied")
}
