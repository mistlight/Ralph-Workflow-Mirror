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
# CONFIG: set these to match your CLIs
############################################
# Defaults use --dangerously-skip-permissions for autonomous operation
# (bypasses all permission checks including plan approval). Override for interactive TUIs:
# - CLAUDE_CMD=claude
# - CODEX_CMD=codex
: "${CLAUDE_CMD:=claude -p --dangerously-skip-permissions --verbose --output-format=stream-json}"
# Codex requires sandbox + approval flags for autonomous operation:
#   --full-auto             = --sandbox workspace-write + --ask-for-approval on-request
#   --sandbox danger-full-access = full filesystem access (needed if editing outside workspace)
#   --ask-for-approval never     = skip all permission prompts
# Using --yolo (alias for --dangerously-bypass-approvals-and-sandbox) matches Claude's behavior
: "${CODEX_CMD:=codex exec --json --yolo}"

# Loop counts
: "${CLAUDE_ITERS:=5}"
: "${CODEX_REVIEWS:=2}"        # after fix, how many review passes (you asked “review it twice” -> 2)

# Optional checks (script-owned). Leave empty to skip.
: "${FAST_CHECK_CMD:=}"        # e.g. "pytest -q" or "npm test --silent"
: "${FULL_CHECK_CMD:=}"        # e.g. "pytest" or "npm test"

# Many CLIs (including agent CLIs) heavily buffer and/or degrade UX when stdout/stdin
# aren't attached to a real TTY. Since we also want logs, prefer `script` which both:
# - keeps an interactive TTY for the agent process
# - records a transcript to a file
: "${RALPH_USE_PTY:=1}"
: "${RALPH_INTERACTIVE:=1}"    # 1 = keep agent in foreground (you can answer prompts), 0 = fire-and-forget
: "${RALPH_PROMPT_PATH:=.agent/last_prompt.txt}"

COMMIT_MSG="${1:-chore: apply PROMPT loop + Codex review/fix/review/review}"

############################################
# Colors & Formatting
#
# Uses ANSI escape codes for terminal coloring.
# Respects NO_COLOR env var (https://no-color.org/).
# Falls back to no colors if terminal doesn't support them.
############################################
if [[ -z "${NO_COLOR:-}" ]] && [[ -t 1 ]]; then
  # Bold/Reset
  BOLD=$'\e[1m'
  DIM=$'\e[2m'
  RESET=$'\e[0m'

  # Foreground colors
  RED=$'\e[31m'
  GREEN=$'\e[32m'
  YELLOW=$'\e[33m'
  BLUE=$'\e[34m'
  MAGENTA=$'\e[35m'
  CYAN=$'\e[36m'
  WHITE=$'\e[37m'

  # Background colors (for headers)
  BG_BLUE=$'\e[44m'
  BG_GREEN=$'\e[42m'
  BG_YELLOW=$'\e[43m'
  BG_RED=$'\e[41m'
else
  # No color mode
  BOLD="" DIM="" RESET=""
  RED="" GREEN="" YELLOW="" BLUE="" MAGENTA="" CYAN="" WHITE=""
  BG_BLUE="" BG_GREEN="" BG_YELLOW="" BG_RED=""
fi

############################################
# Box-drawing characters for visual structure
############################################
BOX_TL="╭" BOX_TR="╮" BOX_BL="╰" BOX_BR="╯"
BOX_H="─" BOX_V="│"
ARROW="→" CHECK="✓" CROSS="✗" WARN="⚠" INFO="ℹ"

############################################
# Timing utilities
############################################
typeset -g START_TIME=0
typeset -g PHASE_START=0

timer_start() {
  START_TIME=$SECONDS
  PHASE_START=$SECONDS
}

timer_phase_start() {
  PHASE_START=$SECONDS
}

timer_elapsed() {
  local elapsed=$((SECONDS - START_TIME))
  local mins=$((elapsed / 60))
  local secs=$((elapsed % 60))
  printf "%dm %02ds" "$mins" "$secs"
}

timer_phase_elapsed() {
  local elapsed=$((SECONDS - PHASE_START))
  local mins=$((elapsed / 60))
  local secs=$((elapsed % 60))
  printf "%dm %02ds" "$mins" "$secs"
}

############################################
# JSON Stream Parsing
#
# Both Claude and Codex output NDJSON (newline-delimited JSON) when
# using --output-format=stream-json (Claude) or --json (Codex).
#
# Claude event types: init, message, tool_use, tool_result, result
# Codex event types: thread.started, turn.started, turn.completed, item.started, item.completed, error
#
# We parse these streams to display formatted, colored output in real-time.
############################################

# Check if jq is available for JSON parsing
HAS_JQ=0
if command -v jq >/dev/null 2>&1; then
  HAS_JQ=1
fi

# Parse and display a single Claude JSON event
# Reads JSON from stdin, outputs formatted text
parse_claude_event() {
  local line="$1"
  [[ -z "$line" ]] && return 0
  [[ "$HAS_JQ" != "1" ]] && { print -r -- "$line"; return 0; }

  local event_type
  event_type=$(print -r -- "$line" | jq -r '.type // empty' 2>/dev/null) || { print -r -- "$line"; return 0; }

  case "$event_type" in
    init)
      local session_id
      session_id=$(print -r -- "$line" | jq -r '.session_id // "unknown"' 2>/dev/null)
      print "${DIM}[Claude]${RESET} ${CYAN}Session started${RESET} ${DIM}(${session_id:0:8}...)${RESET}"
      ;;
    message)
      local role content_text
      role=$(print -r -- "$line" | jq -r '.role // "unknown"' 2>/dev/null)
      content_text=$(print -r -- "$line" | jq -r '.content[]? | select(.type == "text") | .text // empty' 2>/dev/null | head -1)
      if [[ -n "$content_text" ]]; then
        local preview="${content_text:0:100}"
        [[ ${#content_text} -gt 100 ]] && preview="${preview}..."
        print "${DIM}[Claude]${RESET} ${BLUE}${role}${RESET}: ${preview}"
      fi
      ;;
    tool_use)
      local tool_name tool_input
      tool_name=$(print -r -- "$line" | jq -r '.name // "unknown"' 2>/dev/null)
      tool_input=$(print -r -- "$line" | jq -r '.input | keys[0] // empty' 2>/dev/null)
      print "${DIM}[Claude]${RESET} ${MAGENTA}Tool${RESET}: ${BOLD}${tool_name}${RESET}${tool_input:+ (${tool_input})}"
      ;;
    tool_result)
      local output_preview
      output_preview=$(print -r -- "$line" | jq -r '.output // empty' 2>/dev/null | head -1)
      if [[ -n "$output_preview" ]]; then
        local preview="${output_preview:0:80}"
        [[ ${#output_preview} -gt 80 ]] && preview="${preview}..."
        print "${DIM}[Claude]${RESET} ${DIM}Result:${RESET} ${preview}"
      fi
      ;;
    result)
      local status duration_ms
      status=$(print -r -- "$line" | jq -r '.status // "unknown"' 2>/dev/null)
      duration_ms=$(print -r -- "$line" | jq -r '.duration_ms // 0' 2>/dev/null)
      local duration_s=$((duration_ms / 1000))
      if [[ "$status" == "success" ]]; then
        print "${DIM}[Claude]${RESET} ${GREEN}${CHECK} Completed${RESET} ${DIM}(${duration_s}s)${RESET}"
      else
        print "${DIM}[Claude]${RESET} ${RED}${CROSS} ${status}${RESET} ${DIM}(${duration_s}s)${RESET}"
      fi
      ;;
    *)
      # Pass through unknown event types
      print "${DIM}[Claude]${RESET} ${DIM}${event_type}${RESET}"
      ;;
  esac
}

# Parse and display a single Codex JSON event
parse_codex_event() {
  local line="$1"
  [[ -z "$line" ]] && return 0
  [[ "$HAS_JQ" != "1" ]] && { print -r -- "$line"; return 0; }

  local event_type
  event_type=$(print -r -- "$line" | jq -r '.type // empty' 2>/dev/null) || { print -r -- "$line"; return 0; }

  case "$event_type" in
    thread.started)
      local thread_id
      thread_id=$(print -r -- "$line" | jq -r '.thread_id // "unknown"' 2>/dev/null)
      print "${DIM}[Codex]${RESET} ${CYAN}Thread started${RESET} ${DIM}(${thread_id:0:8}...)${RESET}"
      ;;
    turn.started)
      print "${DIM}[Codex]${RESET} ${BLUE}Turn started${RESET}"
      ;;
    turn.completed)
      local input_tokens output_tokens
      input_tokens=$(print -r -- "$line" | jq -r '.usage.input_tokens // 0' 2>/dev/null)
      output_tokens=$(print -r -- "$line" | jq -r '.usage.output_tokens // 0' 2>/dev/null)
      print "${DIM}[Codex]${RESET} ${GREEN}${CHECK} Turn completed${RESET} ${DIM}(in:${input_tokens} out:${output_tokens})${RESET}"
      ;;
    turn.failed)
      local error_msg
      error_msg=$(print -r -- "$line" | jq -r '.error // "unknown error"' 2>/dev/null)
      print "${DIM}[Codex]${RESET} ${RED}${CROSS} Turn failed:${RESET} ${error_msg}"
      ;;
    item.started)
      local item_type item_cmd
      item_type=$(print -r -- "$line" | jq -r '.item.type // "unknown"' 2>/dev/null)
      case "$item_type" in
        command_execution)
          item_cmd=$(print -r -- "$line" | jq -r '.item.command // ""' 2>/dev/null)
          print "${DIM}[Codex]${RESET} ${MAGENTA}Exec${RESET}: ${DIM}${item_cmd:0:60}${RESET}"
          ;;
        agent_message)
          print "${DIM}[Codex]${RESET} ${BLUE}Thinking...${RESET}"
          ;;
        *)
          print "${DIM}[Codex]${RESET} ${DIM}${item_type}${RESET}"
          ;;
      esac
      ;;
    item.completed)
      local item_type item_text
      item_type=$(print -r -- "$line" | jq -r '.item.type // "unknown"' 2>/dev/null)
      case "$item_type" in
        agent_message)
          item_text=$(print -r -- "$line" | jq -r '.item.text // ""' 2>/dev/null)
          if [[ -n "$item_text" ]]; then
            local preview="${item_text:0:100}"
            [[ ${#item_text} -gt 100 ]] && preview="${preview}..."
            print "${DIM}[Codex]${RESET} ${WHITE}${preview}${RESET}"
          fi
          ;;
        command_execution)
          print "${DIM}[Codex]${RESET} ${GREEN}${CHECK} Command done${RESET}"
          ;;
        file_change)
          local file_path
          file_path=$(print -r -- "$line" | jq -r '.item.path // "unknown"' 2>/dev/null)
          print "${DIM}[Codex]${RESET} ${YELLOW}File${RESET}: ${file_path}"
          ;;
        *)
          # Silently complete other item types
          ;;
      esac
      ;;
    error)
      local error_msg
      error_msg=$(print -r -- "$line" | jq -r '.message // .error // "unknown error"' 2>/dev/null)
      print "${DIM}[Codex]${RESET} ${RED}${CROSS} Error:${RESET} ${error_msg}"
      ;;
    *)
      # Pass through unknown event types
      print "${DIM}[Codex]${RESET} ${DIM}${event_type}${RESET}"
      ;;
  esac
}

# Stream processor: reads NDJSON from stdin, parses and displays each line
# For streaming output, we use unbuffered reading via stdbuf when available.
# Each complete JSON line is parsed and displayed in real-time.
#
# Usage: some_command | stream_parse_claude
stream_parse_claude() {
  local line
  # Read line by line - NDJSON guarantees each JSON object is on its own line
  # The || [[ -n "$line" ]] handles the case where the last line has no newline
  while IFS= read -r line || [[ -n "$line" ]]; do
    # Skip empty lines
    [[ -z "$line" ]] && continue
    parse_claude_event "$line"
    # Also write raw JSON to log file if provided
    [[ -n "${STREAM_LOGFILE:-}" ]] && print -r -- "$line" >> "$STREAM_LOGFILE"
  done
}

stream_parse_codex() {
  local line
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" ]] && continue
    parse_codex_event "$line"
    [[ -n "${STREAM_LOGFILE:-}" ]] && print -r -- "$line" >> "$STREAM_LOGFILE"
  done
}

# Detect which agent's output we're parsing based on command string
detect_agent_type() {
  local cmd="$1"
  if [[ "$cmd" == *"claude"* ]]; then
    print "claude"
  elif [[ "$cmd" == *"codex"* ]]; then
    print "codex"
  else
    print "unknown"
  fi
}

############################################
# Output helpers with visual formatting
############################################
# Print a section header with box drawing
print_header() {
  local title="$1"
  local color="${2:-$CYAN}"
  local width=60
  local title_len=${#title}
  local padding=$(( (width - title_len - 2) / 2 ))

  print ""
  print "${color}${BOLD}${BOX_TL}$(printf '%*s' "$width" | tr ' ' "$BOX_H")${BOX_TR}${RESET}"
  print "${color}${BOLD}${BOX_V}$(printf '%*s' "$padding" '')${WHITE}${title}${color}$(printf '%*s' "$((width - padding - title_len))" '')${BOX_V}${RESET}"
  print "${color}${BOLD}${BOX_BL}$(printf '%*s' "$width" | tr ' ' "$BOX_H")${BOX_BR}${RESET}"
}

# Print a sub-header (less prominent)
print_subheader() {
  local title="$1"
  print ""
  print "${BOLD}${BLUE}${ARROW} ${title}${RESET}"
  print "${DIM}$(printf '%*s' "${#title}" | tr ' ' '─')──${RESET}"
}

# Print progress bar: [████████░░░░░░░░] 50%
print_progress() {
  local current="$1" total="$2" label="${3:-Progress}"
  local pct=$((current * 100 / total))
  local bar_width=20
  local filled=$((current * bar_width / total))
  local empty=$((bar_width - filled))

  local bar=""
  for ((k=0; k<filled; k++)); do bar+="█"; done
  for ((k=0; k<empty; k++)); do bar+="░"; done

  print "${DIM}${label}:${RESET} ${CYAN}[${bar}]${RESET} ${BOLD}${pct}%${RESET} (${current}/${total})"
}

# Timestamped log line with icon
log_info()    { print "${DIM}[$(ts)]${RESET} ${BLUE}${INFO}${RESET}  $*"; }
log_success() { print "${DIM}[$(ts)]${RESET} ${GREEN}${CHECK}${RESET}  ${GREEN}$*${RESET}"; }
log_warn()    { print "${DIM}[$(ts)]${RESET} ${YELLOW}${WARN}${RESET}  ${YELLOW}$*${RESET}"; }
log_error()   { print "${DIM}[$(ts)]${RESET} ${RED}${CROSS}${RESET}  ${RED}$*${RESET}"; }
log_step()    { print "${DIM}[$(ts)]${RESET} ${MAGENTA}${ARROW}${RESET}  $*"; }

# Also log to file (strips ANSI codes for clean log files)
log_to_file() {
  local msg="$1"
  local logfile="${2:-.agent/logs/pipeline.log}"
  # Strip ANSI escape sequences for log file
  print -r -- "$msg" | sed 's/\x1b\[[0-9;]*m//g' >> "$logfile"
}

# Combined: print to terminal with colors, log to file without
tlog_info()    { log_info "$@"; log_to_file "[$(ts)] [INFO] $*"; }
tlog_success() { log_success "$@"; log_to_file "[$(ts)] [OK] $*"; }
tlog_warn()    { log_warn "$@"; log_to_file "[$(ts)] [WARN] $*"; }
tlog_error()   { log_error "$@"; log_to_file "[$(ts)] [ERROR] $*"; }
tlog_step()    { log_step "$@"; log_to_file "[$(ts)] [STEP] $*"; }

############################################
# Helpers
############################################
fail() { log_error "$*" >&2; exit 1; }
ts()   { date +"%Y-%m-%d %H:%M:%S"; }

typeset -g REPO_ROOT=""

require_git_repo() {
  git rev-parse --is-inside-work-tree >/dev/null 2>&1 || fail "Not inside a git repo."
}

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
    if [[ "${RALPH_INTERACTIVE}" == "1" ]]; then
      tlog_warn "\`script\` not found; running without transcript logging"
      eval "$full_cmd" || exit_code=$?
    else
      # Non-interactive mode: parse JSON streams for neat display
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
  fi

  if [[ "$exit_code" -ne 0 ]]; then
    log_warn "Command exited with code $exit_code (continuing anyway)"
    log_to_file "[$(ts)] [WARN] $label exited with code $exit_code"
  fi
  log_success "Completed in $(timer_phase_elapsed)"
}

git_snapshot() {
  git status --porcelain=v1
}

############################################
# Hard enforcement: block commits during agent phase
############################################
REAL_GIT="$(command -v git)"
WRAPDIR=""
HOOK_MARKER="RALPH_ZSH_MANAGED_HOOK"

file_contains_marker() {
  local file="$1" marker="$2"
  if command -v rg >/dev/null 2>&1; then
    rg -n --fixed-strings -- "$marker" "$file" >/dev/null 2>&1
    return $?
  fi
  grep -Fq -- "$marker" "$file" >/dev/null 2>&1
}

install_hook() {
  local hook_name="$1"
  local hook_path="$2"
  local orig_path="${hook_path}.ralph.orig"

  mkdir -p "${hook_path:h}"

  if [[ -f "$hook_path" ]] && ! file_contains_marker "$hook_path" "$HOOK_MARKER"; then
    cp -f "$hook_path" "$orig_path"
    chmod +x "$orig_path" || true
  fi

  cat > "$hook_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
# ${HOOK_MARKER} - generated by ralph.zsh

repo_root="\$(command git rev-parse --show-toplevel 2>/dev/null || pwd)"
if [[ -f "\$repo_root/.no_agent_commit" ]]; then
  echo "✋ ${hook_name} blocked (agent phase): .no_agent_commit exists."
  exit 1
fi

orig="${orig_path}"
if [[ -f "\$orig" ]]; then
  exec "\$orig" "\$@"
fi

exit 0
EOF
  chmod +x "$hook_path"
}

install_hooks() {
  local hooks_dir
  hooks_dir="$(git rev-parse --git-path hooks)"
  [[ -n "$hooks_dir" ]] || fail "Unable to resolve git hooks directory."
  mkdir -p "$hooks_dir"
  install_hook "Commit" "$hooks_dir/pre-commit"
  install_hook "Push" "$hooks_dir/pre-push"
}

enable_git_wrapper() {
  WRAPDIR="$(mktemp -d)"
  cat > "$WRAPDIR/git" <<EOF
#!/usr/bin/env bash
set -euo pipefail
repo_root="\$("$REAL_GIT" rev-parse --show-toplevel 2>/dev/null || pwd)"
if [[ -f "\$repo_root/.no_agent_commit" ]]; then
  subcmd="\${1:-}"
  case "\$subcmd" in
    commit|push|tag)
      echo "✋ Blocked: git \$subcmd disabled during agent phase (.no_agent_commit present)."
      exit 1
      ;;
  esac
fi
exec "$REAL_GIT" "\$@"
EOF
  chmod +x "$WRAPDIR/git"
  export PATH="$WRAPDIR:$PATH"
}

disable_git_wrapper() {
  [[ -n "${WRAPDIR:-}" && -d "$WRAPDIR" ]] && rm -rf "$WRAPDIR" || true
}

start_agent_phase() {
  touch .no_agent_commit
  install_hooks
  enable_git_wrapper
}

end_agent_phase() {
  rm -f .no_agent_commit
}

############################################
# Prompts (script cycles PROMPT.md; agents can update STATUS.md)
############################################
claude_prompt() {
  local i="$1"
  cat <<EOF
Iteration ${i}/${CLAUDE_ITERS}.

Read PROMPT.md and .agent/STATUS.md.
Make the next best progress step toward PROMPT.md's Goal and Acceptance checks.
Update .agent/STATUS.md (last action, blockers, next action).
Append brief bullets to .agent/NOTES.md.

Then stop.
EOF
}

codex_review_prompt() {
  cat <<'EOF'
Review the repository against PROMPT.md (Goal + Acceptance checks).
Write findings into .agent/ISSUES.md as a prioritized checklist.
EOF
}

codex_fix_prompt() {
  cat <<'EOF'
Fix everything in .agent/ISSUES.md.
Update .agent/ISSUES.md to mark items resolved.
Append brief bullets to .agent/NOTES.md.
EOF
}

codex_review_again_prompt() {
  cat <<'EOF'
Re-review the repository after fixes against PROMPT.md.
If issues remain, fix them and update .agent/ISSUES.md.
EOF
}

############################################
# Statistics tracking
############################################
typeset -g CHANGES_DETECTED=0
typeset -g CLAUDE_RUNS_COMPLETED=0
typeset -g CODEX_RUNS_COMPLETED=0

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
  print_subheader "Claude Iteration $i of $CLAUDE_ITERS"
  print_progress "$i" "$CLAUDE_ITERS" "Overall"

  run_with_prompt_arg "Claude run #$i" "$CLAUDE_CMD" "$(claude_prompt "$i")" ".agent/logs/claude_${i}.log"
  ((CLAUDE_RUNS_COMPLETED++))

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
done

############################################
# Phase 2: Codex review/fix cycle
############################################
print_header "PHASE 2: Codex Review & Fix" "$MAGENTA"
log_to_file "=== PHASE 2: Codex Review & Fix ==="
log_info "Running review ${ARROW} fix ${ARROW} review×${BOLD}$CODEX_REVIEWS${RESET} cycle"

print_subheader "Initial Review"
run_with_prompt_arg "Codex review (initial)" "$CODEX_CMD" "$(codex_review_prompt)" ".agent/logs/codex_review_1.log"
((CODEX_RUNS_COMPLETED++))

print_subheader "Applying Fixes"
run_with_prompt_arg "Codex fix" "$CODEX_CMD" "$(codex_fix_prompt)" ".agent/logs/codex_fix.log"
((CODEX_RUNS_COMPLETED++))

for j in $(seq 1 "$CODEX_REVIEWS"); do
  print_subheader "Verification Review $j of $CODEX_REVIEWS"
  print_progress "$j" "$CODEX_REVIEWS" "Review passes"
  run_with_prompt_arg "Codex re-review #$j" "$CODEX_CMD" "$(codex_review_again_prompt)" ".agent/logs/codex_review_$((j+1)).log"
  ((CODEX_RUNS_COMPLETED++))
done

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
# Phase 4: Commit
############################################
# Allow commit now
end_agent_phase
disable_git_wrapper
trap - EXIT

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
