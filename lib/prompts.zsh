#!/usr/bin/env zsh
############################################
# Prompt Templates Module
#
# Provides context-controlled prompts for agents.
# Key design: reviewers get minimal context for "fresh eyes" perspective.
#
# Dependencies:
#   - None (standalone module)
############################################

# Guard against multiple sourcing
[[ -n "${_RALPH_PROMPTS_LOADED:-}" ]] && return 0
typeset -g _RALPH_PROMPTS_LOADED=1

############################################
# Configuration
############################################

# Context level controls how much history agents see
# 0 = minimal (fresh eyes - just the goal)
# 1 = normal (goal + current status)
# 2 = full (goal + status + notes + issues)
typeset -g RALPH_CONTEXT_LEVEL="${RALPH_CONTEXT_LEVEL:-1}"

# Role-specific context defaults
# Developers (Claude) get more context to continue work
# Reviewers (Codex) get minimal context for unbiased review
typeset -g RALPH_DEVELOPER_CONTEXT="${RALPH_DEVELOPER_CONTEXT:-1}"  # normal context
typeset -g RALPH_REVIEWER_CONTEXT="${RALPH_REVIEWER_CONTEXT:-0}"    # minimal context

############################################
# Claude (Developer) Prompts
############################################

# Generate Claude iteration prompt
# Args: $1 = iteration number, $2 = total iterations
prompt_claude_iteration() {
  local i="$1"
  local total="${2:-5}"
  local context_level="${RALPH_DEVELOPER_CONTEXT}"

  cat <<EOF
Iteration ${i}/${total}.

Read PROMPT.md and .agent/STATUS.md.
Make the next best progress step toward PROMPT.md's Goal and Acceptance checks.
Update .agent/STATUS.md (last action, blockers, next action).
Append brief bullets to .agent/NOTES.md.

Then stop.
EOF
}

############################################
# Codex (Reviewer) Prompts
############################################

# Generate Codex review prompt with minimal context
# Reviewer should NOT see what was done - just evaluate the code against requirements
prompt_codex_review() {
  local context_level="${RALPH_REVIEWER_CONTEXT}"

  if [[ "$context_level" -eq 0 ]]; then
    # Fresh eyes: only see the goal, not what was done
    cat <<'EOF'
You are reviewing this repository with fresh eyes.

Read ONLY PROMPT.md to understand the Goal and Acceptance checks.
DO NOT read .agent/STATUS.md or .agent/NOTES.md - you need an unbiased perspective.

Evaluate the codebase against the requirements:
1. Does the code meet the Goal?
2. Do all Acceptance checks pass?
3. Are there quality issues (bugs, code smells, missing tests)?

Write your findings into .agent/ISSUES.md as a prioritized checklist.
Be specific about file paths and line numbers.
EOF
  else
    # Normal context: can see status
    cat <<'EOF'
Review the repository against PROMPT.md (Goal + Acceptance checks).
Write findings into .agent/ISSUES.md as a prioritized checklist.
EOF
  fi
}

# Generate Codex fix prompt
prompt_codex_fix() {
  cat <<'EOF'
Fix everything in .agent/ISSUES.md.
Update .agent/ISSUES.md to mark items resolved.
Append brief bullets to .agent/NOTES.md.
EOF
}

# Generate Codex re-review prompt with minimal context
prompt_codex_review_again() {
  local context_level="${RALPH_REVIEWER_CONTEXT}"

  if [[ "$context_level" -eq 0 ]]; then
    # Fresh eyes: don't look at what was fixed, just verify the code
    cat <<'EOF'
Re-review the repository with fresh eyes.

Read ONLY PROMPT.md to verify the Goal and Acceptance checks are met.
DO NOT assume previous issues were fixed - verify independently.

If issues remain:
1. Fix them directly
2. Update .agent/ISSUES.md with what was found and fixed

Be thorough but efficient.
EOF
  else
    cat <<'EOF'
Re-review the repository after fixes against PROMPT.md.
If issues remain, fix them and update .agent/ISSUES.md.
EOF
  fi
}

############################################
# Commit Prompt (for reviewer to commit)
############################################

# Generate commit prompt for reviewer
# Args: $1 = commit message
prompt_commit() {
  local msg="${1:-chore: apply changes}"
  cat <<EOF
All work is complete. Create a git commit with all changes.

Run:
  git add -A
  git commit -m "${msg}"

If commit hooks fail, fix the issues and try again.
Report success or failure.
EOF
}

############################################
# Generic Agent Prompts
############################################

# Generate a prompt for any agent type
# Args: $1 = role (developer|reviewer), $2 = action, $3 = extra args...
prompt_for_agent() {
  local role="$1"
  local action="$2"
  shift 2

  case "${role}:${action}" in
    developer:iterate)
      prompt_claude_iteration "$@"
      ;;
    reviewer:review)
      prompt_codex_review
      ;;
    reviewer:fix)
      prompt_codex_fix
      ;;
    reviewer:review_again)
      prompt_codex_review_again
      ;;
    *:commit)
      prompt_commit "$@"
      ;;
    *)
      print "Unknown prompt: ${role}:${action}" >&2
      return 1
      ;;
  esac
}
