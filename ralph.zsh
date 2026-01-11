#!/usr/bin/env zsh
set -euo pipefail

############################################
# Ralph: PROMPT-driven agent loop for git repos
#
# Runs:
# - Claude: iterative progress against PROMPT.md
# - Codex: review → fix → review passes
# - Optional fast/full checks
# - Final `git add -A` + `git commit -m <msg>`
#
# Usage:
#   ./ralph.zsh "feat: my change"
#
# Configuration (env vars):
#   CLAUDE_CMD       Command fragment to run Claude (default includes --dangerously-skip-permissions
#                    to bypass all permission checks for autonomous operation)
#   CODEX_CMD        Command fragment to run Codex (default: "codex exec --json --yolo"
#                    where --yolo bypasses all approvals and sandboxing for autonomous operation)
#   CLAUDE_ITERS     Claude iterations (default: 5)
#   CODEX_REVIEWS    Review passes after fix (default: 2)
#   RALPH_USE_PTY    Run agent commands under a pseudo-TTY for streaming output (default: 1)
#   FAST_CHECK_CMD   Optional non-blocking check run after each Claude iter
#   FULL_CHECK_CMD   Optional blocking check run once at end
#
# Notes:
# - `CLAUDE_CMD` / `CODEX_CMD` are treated as *shell command fragments* and
#   invoked via `eval` so you can include quoting (e.g. --name='hello world').
#   Avoid untrusted input here.
# - All state files live at repo root: `PROMPT.md` and `.agent/*`.
############################################

############################################
# Load library modules
############################################
RALPH_SCRIPT_DIR="${0:A:h}"
source "${RALPH_SCRIPT_DIR}/lib/colors.zsh"
source "${RALPH_SCRIPT_DIR}/lib/timer.zsh"
source "${RALPH_SCRIPT_DIR}/lib/utils.zsh"
source "${RALPH_SCRIPT_DIR}/lib/json_parser.zsh"
source "${RALPH_SCRIPT_DIR}/lib/git_helpers.zsh"
source "${RALPH_SCRIPT_DIR}/lib/prompts.zsh"
source "${RALPH_SCRIPT_DIR}/lib/agents.zsh"

############################################
# CONFIG: set these to match your CLIs
############################################
# Agent selection - which AI assistants to use for each role
# Available agents: claude, codex, opencode, aider (or register custom ones)
: "${RALPH_DEVELOPER_AGENT:=claude}"
: "${RALPH_REVIEWER_AGENT:=codex}"

# Commands can be overridden directly, or built from agent configuration
# The defaults use autonomous mode flags for non-interactive operation.
# Override for interactive TUIs: CLAUDE_CMD=claude  CODEX_CMD=codex
: "${CLAUDE_CMD:=$(agent_developer_cmd)}"
: "${CODEX_CMD:=$(agent_reviewer_cmd)}"

# Loop counts
: "${CLAUDE_ITERS:=5}"
: "${CODEX_REVIEWS:=2}"        # after fix, how many review passes (you asked “review it twice” -> 2)

# Optional checks (script-owned). Leave empty to skip.
: "${FAST_CHECK_CMD:=}"        # e.g. "pytest -q" or "npm test --silent"
: "${FULL_CHECK_CMD:=}"        # e.g. "pytest" or "npm test"

# PTY mode: Use `script` to attach agents to a real TTY (good for interactive prompts).
# When disabled (0), JSON streams are parsed for formatted display.
# Default is 0 for autonomous operation with JSON parsing.
: "${RALPH_USE_PTY:=0}"
: "${RALPH_INTERACTIVE:=1}"    # 1 = keep agent in foreground (you can answer prompts), 0 = fire-and-forget
: "${RALPH_PROMPT_PATH:=.agent/last_prompt.txt}"

# Commit behavior:
#   RALPH_REVIEWER_COMMITS=1 (default): Reviewer (Codex) creates the final commit
#   RALPH_REVIEWER_COMMITS=0: Ralph creates the final commit after all phases
: "${RALPH_REVIEWER_COMMITS:=1}"

COMMIT_MSG="${1:-chore: apply PROMPT loop + Codex review/fix/review/review}"

############################################
# Helpers (additional to lib modules)
############################################

typeset -g REPO_ROOT=""

ensure_files() {
  mkdir -p .agent/logs

  [[ -f PROMPT.md ]] || cat > PROMPT.md <<'EOF'
# PROMPT

## Goal
(Write what you want done)

## Acceptance checks
- (List tests/lint/behaviors that must pass)

## Notes / constraints
- (Optional)
EOF

  [[ -f .agent/STATUS.md ]] || cat > .agent/STATUS.md <<'EOF'
# STATUS
- Last action: none
- Blockers: none
- Next action: TBD
EOF

  [[ -f .agent/NOTES.md  ]] || : > .agent/NOTES.md
  [[ -f .agent/ISSUES.md ]] || : > .agent/ISSUES.md
}

run_with_prompt_arg() {
  local label="$1" cmdstr="$2" prompt="$3" logfile="$4"
  timer_phase_start

  tlog_step "${BOLD}$label${RESET}"

  local prompt_quoted
  prompt_quoted="${(q)prompt}"

  print -r -- "$prompt" > "$RALPH_PROMPT_PATH"
  log_info "Prompt saved to ${CYAN}$RALPH_PROMPT_PATH${RESET}"

  if [[ "${RALPH_INTERACTIVE}" == "1" ]] && command -v pbcopy >/dev/null 2>&1; then
    print -r -- "$prompt" | pbcopy || true
    log_info "Prompt copied to clipboard ${DIM}(pbpaste to view)${RESET}"
  fi

  local full_cmd="$cmdstr $prompt_quoted"
  log_info "Executing: ${DIM}${full_cmd:0:80}...${RESET}"

  # Detect if command outputs JSON and which agent type
  local agent_type uses_json=0
  agent_type=$(detect_agent_type "$cmdstr")
  if [[ "$cmdstr" == *"--output-format=stream-json"* ]] || [[ "$cmdstr" == *"--json"* ]]; then
    uses_json=1
  fi

  local exit_code=0
  if [[ "${RALPH_USE_PTY}" == "1" ]] && command -v script >/dev/null 2>&1; then
    # `script` keeps the agent attached to a TTY while recording a transcript to $logfile.
    # This is critical for interactive UX (answering Claude/Codex questions mid-run).
    # Note: PTY mode cannot easily parse JSON streams; raw output goes to logfile.
    script -aq "$logfile" zsh -c "$full_cmd" || exit_code=$?
  else
    # Non-PTY mode: parse JSON streams for formatted display
    if [[ "$uses_json" == "1" ]] && [[ "$HAS_JQ" == "1" ]]; then
      log_info "Parsing ${agent_type} JSON stream..."
      export STREAM_LOGFILE="$logfile"
      # Use stdbuf to disable output buffering if available, ensuring
      # JSON lines are passed to the parser as soon as they're written
      local stdbuf_prefix=""
      if command -v stdbuf >/dev/null 2>&1; then
        stdbuf_prefix="stdbuf -oL "
      fi
      case "$agent_type" in
        claude)
          eval "${stdbuf_prefix}$full_cmd" 2>&1 | stream_parse_claude || exit_code=$?
          ;;
        codex)
          eval "${stdbuf_prefix}$full_cmd" 2>&1 | stream_parse_codex || exit_code=$?
          ;;
        *)
          eval "${stdbuf_prefix}$full_cmd" 2>&1 | tee -a "$logfile" || exit_code=$?
          ;;
      esac
      unset STREAM_LOGFILE
    else
      eval "$full_cmd" 2>&1 | tee -a "$logfile" || exit_code=$?
    fi
  fi

  if [[ "$exit_code" -ne 0 ]]; then
    log_warn "Command exited with code $exit_code (continuing anyway)"
    log_to_file "[$(ts)] [WARN] $label exited with code $exit_code"
  fi
  log_success "Completed in $(timer_phase_elapsed)"
}

############################################
# Prompts (using lib/prompts.zsh module)
############################################
# These wrapper functions maintain backward compatibility while
# delegating to the prompts module for context-controlled output.
#
# Configuration (env vars):
#   RALPH_REVIEWER_CONTEXT  0=fresh eyes (default), 1=normal context
#   RALPH_DEVELOPER_CONTEXT 0=minimal, 1=normal (default)
############################################
claude_prompt() {
  local i="$1"
  prompt_claude_iteration "$i" "$CLAUDE_ITERS"
}

codex_review_prompt() {
  prompt_codex_review
}

codex_fix_prompt() {
  prompt_codex_fix
}

codex_review_again_prompt() {
  prompt_codex_review_again
}

codex_commit_prompt() {
  prompt_commit "$COMMIT_MSG"
}

############################################
# Statistics tracking
############################################
typeset -g CHANGES_DETECTED=0
typeset -g CLAUDE_RUNS_COMPLETED=0
typeset -g CODEX_RUNS_COMPLETED=0

############################################
# Status file updates (continuous visibility)
############################################
update_status() {
  local last_action="$1"
  local blockers="${2:-none}"
  local next_action="${3:-TBD}"
  cat > .agent/STATUS.md <<EOF
# STATUS
- Last action: ${last_action}
- Blockers: ${blockers}
- Next action: ${next_action}
- Updated at: $(ts)
EOF
}

############################################
# Main
############################################
require_git_repo
REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"
ensure_files

cleanup() {
  end_agent_phase || true
  disable_git_wrapper || true
}
trap cleanup EXIT

start_agent_phase
timer_start

# Welcome banner
print ""
print "${BOLD}${CYAN}╭────────────────────────────────────────────────────────────╮${RESET}"
print "${BOLD}${CYAN}│${RESET}  ${BOLD}${WHITE}🤖 Ralph${RESET} ${DIM}─ PROMPT-driven agent orchestrator${RESET}              ${BOLD}${CYAN}│${RESET}"
print "${BOLD}${CYAN}│${RESET}  ${DIM}Claude × Codex pipeline for autonomous development${RESET}       ${BOLD}${CYAN}│${RESET}"
print "${BOLD}${CYAN}╰────────────────────────────────────────────────────────────╯${RESET}"
print ""
log_info "Working directory: ${CYAN}$REPO_ROOT${RESET}"
log_info "Commit message: ${CYAN}$COMMIT_MSG${RESET}"
print ""

############################################
# Phase 1: Claude iterations
############################################
print_header "PHASE 1: Claude Development" "$BLUE"
log_to_file "=== PHASE 1: Claude Development ==="
log_info "Running ${BOLD}$CLAUDE_ITERS${RESET} Claude iterations"

prev_snap="$(git_snapshot)"
for i in $(seq 1 "$CLAUDE_ITERS"); do
  # HARD REQUIREMENT: This loop MUST complete all iterations.
  # Wrap entire iteration body to prevent ANY failure from breaking the loop.
  {
    print_subheader "Claude Iteration $i of $CLAUDE_ITERS"
    print_progress "$i" "$CLAUDE_ITERS" "Overall"

    # Update status BEFORE starting iteration
    update_status "Starting Claude iteration ${i}/${CLAUDE_ITERS}" "none" "Complete Claude iteration ${i}"
    tlog_info ">>> ITERATION ${i}/${CLAUDE_ITERS} STARTING <<<"

    run_with_prompt_arg "Claude run #$i" "$CLAUDE_CMD" "$(claude_prompt "$i")" ".agent/logs/claude_${i}.log"
    ((CLAUDE_RUNS_COMPLETED++))

    # Update status AFTER completing iteration
    tlog_info ">>> ITERATION ${i}/${CLAUDE_ITERS} COMPLETED (total completed: ${CLAUDE_RUNS_COMPLETED}) <<<"
    update_status "Completed Claude iteration ${i}/${CLAUDE_ITERS}" "none" "Continue to iteration $((i+1)) or Phase 2"

    snap="$(git_snapshot)"
    if [[ "$snap" == "$prev_snap" ]]; then
      log_warn "No git-status change detected"
    else
      log_success "Repository modified"
      ((CHANGES_DETECTED++))
    fi
    prev_snap="$snap"

    if [[ -n "$FAST_CHECK_CMD" ]]; then
      log_info "Running fast check: ${DIM}$FAST_CHECK_CMD${RESET}"
      if (eval "$FAST_CHECK_CMD" 2>&1 | tee -a ".agent/logs/fast_check_${i}.log"); then
        log_success "Fast check passed"
      else
        log_warn "Fast check had issues (non-blocking)"
      fi
    fi
  } || {
    # If anything in the iteration block failed, log it but CONTINUE to next iteration
    log_error "Iteration ${i} encountered an error but loop MUST continue"
    tlog_error ">>> ITERATION ${i} FAILED BUT CONTINUING <<<"
  }
done

# Explicit notification that all Claude iterations are complete
tlog_info ">>> ALL ${CLAUDE_ITERS} CLAUDE ITERATIONS COMPLETED <<<"
update_status "All ${CLAUDE_ITERS} Claude iterations complete" "none" "Start Phase 2: Codex review"

############################################
# Phase 2: Codex review/fix cycle
############################################
print_header "PHASE 2: Codex Review & Fix" "$MAGENTA"
log_to_file "=== PHASE 2: Codex Review & Fix ==="
log_info "Running review ${ARROW} fix ${ARROW} review×${BOLD}$CODEX_REVIEWS${RESET} cycle"
tlog_info ">>> ENTERING CODEX PHASE <<<"

print_subheader "Initial Review"
update_status "Starting Codex initial review" "none" "Complete Codex review"
run_with_prompt_arg "Codex review (initial)" "$CODEX_CMD" "$(codex_review_prompt)" ".agent/logs/codex_review_1.log"
((CODEX_RUNS_COMPLETED++))
tlog_info ">>> CODEX INITIAL REVIEW COMPLETED <<<"

print_subheader "Applying Fixes"
update_status "Starting Codex fix phase" "none" "Apply fixes from review"
run_with_prompt_arg "Codex fix" "$CODEX_CMD" "$(codex_fix_prompt)" ".agent/logs/codex_fix.log"
((CODEX_RUNS_COMPLETED++))
tlog_info ">>> CODEX FIX PHASE COMPLETED <<<"

for j in $(seq 1 "$CODEX_REVIEWS"); do
  print_subheader "Verification Review $j of $CODEX_REVIEWS"
  print_progress "$j" "$CODEX_REVIEWS" "Review passes"
  update_status "Starting Codex verification review ${j}/${CODEX_REVIEWS}" "none" "Complete verification review"
  run_with_prompt_arg "Codex re-review #$j" "$CODEX_CMD" "$(codex_review_again_prompt)" ".agent/logs/codex_review_$((j+1)).log"
  ((CODEX_RUNS_COMPLETED++))
  tlog_info ">>> CODEX VERIFICATION REVIEW ${j}/${CODEX_REVIEWS} COMPLETED <<<"
done

# Reviewer commit phase: let the reviewer (Codex) create the final commit
if [[ "${RALPH_REVIEWER_COMMITS}" == "1" ]]; then
  print_subheader "Reviewer Commit"
  update_status "Reviewer creating commit" "none" "Commit all changes"

  # Lift commit block so reviewer can commit
  allow_reviewer_commit

  run_with_prompt_arg "Codex commit" "$CODEX_CMD" "$(codex_commit_prompt)" ".agent/logs/codex_commit.log"
  ((CODEX_RUNS_COMPLETED++))
  tlog_info ">>> CODEX COMMIT PHASE COMPLETED <<<"
fi

tlog_info ">>> ALL CODEX PHASES COMPLETED (total runs: ${CODEX_RUNS_COMPLETED}) <<<"
update_status "All Codex phases complete" "none" "Proceed to final checks or commit"

############################################
# Phase 3: Final checks (if configured)
############################################
if [[ -n "$FULL_CHECK_CMD" ]]; then
  print_header "PHASE 3: Final Validation" "$YELLOW"
  log_to_file "=== PHASE 3: Final Validation ==="
  log_info "Running full check: ${DIM}$FULL_CHECK_CMD${RESET}"
  if eval "$FULL_CHECK_CMD" 2>&1 | tee -a ".agent/logs/full_check.log"; then
    log_success "Full check passed"
  else
    log_error "Full check failed"
  fi
fi

############################################
# Phase 4: Commit (only if Ralph commits, not reviewer)
############################################
# Allow commit now (cleanup for reviewer commit case too)
end_agent_phase
disable_git_wrapper
trap - EXIT

if [[ "${RALPH_REVIEWER_COMMITS}" != "1" ]]; then
  print_header "PHASE 4: Commit Changes" "$GREEN"
  log_to_file "=== PHASE 4: Commit ==="

  log_info "Staging all changes..."
  git add -A

  # Show what we're committing
  print ""
  print "${BOLD}Changes to commit:${RESET}"
  git status --short | head -20 | while read line; do
    print "  ${DIM}${line}${RESET}"
  done
  print ""

  log_info "Creating commit..."
  if git commit -m "$COMMIT_MSG"; then
    log_success "Commit created successfully"
  else
    log_warn "Nothing to commit (working tree clean)"
  fi
else
  # Reviewer already committed; show status
  print_header "PHASE 4: Verify Commit" "$GREEN"
  log_to_file "=== PHASE 4: Verify Commit ==="

  # Check if reviewer made a commit
  local last_commit_msg
  last_commit_msg="$(git log -1 --pretty=%s 2>/dev/null || echo "")"
  if [[ -n "$last_commit_msg" ]]; then
    log_success "Reviewer created commit: ${CYAN}${last_commit_msg}${RESET}"
  else
    log_warn "No commit found - reviewer may have failed to commit"
    # Fallback: create commit ourselves
    log_info "Fallback: staging and committing changes..."
    git add -A
    if git commit -m "$COMMIT_MSG"; then
      log_success "Fallback commit created"
    else
      log_warn "Nothing to commit (working tree clean)"
    fi
  fi
fi

############################################
# Final summary
############################################
print_header "Pipeline Complete" "$GREEN"

print ""
print "${BOLD}${WHITE}📊 Summary${RESET}"
print "${DIM}──────────────────────────────────${RESET}"
print "  ${CYAN}⏱${RESET}  Total time:      ${BOLD}$(timer_elapsed)${RESET}"
print "  ${BLUE}🔄${RESET}  Claude runs:     ${BOLD}$CLAUDE_RUNS_COMPLETED${RESET}/${CLAUDE_ITERS}"
print "  ${MAGENTA}🔍${RESET}  Codex runs:      ${BOLD}$CODEX_RUNS_COMPLETED${RESET}"
print "  ${GREEN}📝${RESET}  Changes detected: ${BOLD}$CHANGES_DETECTED${RESET}"
print ""

print "${BOLD}${WHITE}📁 Output Files${RESET}"
print "${DIM}──────────────────────────────────${RESET}"
print "  ${ARROW} ${CYAN}PROMPT.md${RESET}           Goal definition"
print "  ${ARROW} ${CYAN}.agent/STATUS.md${RESET}    Current status"
print "  ${ARROW} ${CYAN}.agent/ISSUES.md${RESET}    Review findings"
print "  ${ARROW} ${CYAN}.agent/NOTES.md${RESET}     Progress notes"
print "  ${ARROW} ${CYAN}.agent/logs/${RESET}        Detailed logs"
print ""

log_success "Ralph pipeline completed successfully!"
log_to_file "Pipeline completed in $(timer_elapsed)"
