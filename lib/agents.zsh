#!/usr/bin/env zsh
############################################
# Agent Abstraction Module
#
# Provides a pluggable agent system for different
# AI coding assistants (Claude, Codex, OpenCode, etc.)
#
# Configuration (env vars):
#   RALPH_DEVELOPER_AGENT   Agent to use for development (default: claude)
#   RALPH_REVIEWER_AGENT    Agent to use for review (default: codex)
#
# Each agent type has:
#   - Command template (with --json flags for streaming)
#   - Parser function for output
#   - Capability flags (can_commit, needs_json, etc.)
#
# Dependencies:
#   - lib/colors.zsh (for colored output)
#   - lib/json_parser.zsh (for parse functions)
############################################

# Guard against multiple sourcing
[[ -n "${_RALPH_AGENTS_LOADED:-}" ]] && return 0
typeset -g _RALPH_AGENTS_LOADED=1

# Get script directory for relative sourcing
typeset -g _AGENTS_LIB_DIR="${0:A:h}"

# Source dependencies if not already loaded
[[ -z "${_RALPH_COLORS_LOADED:-}" ]] && source "${_AGENTS_LIB_DIR}/colors.zsh"
[[ -z "${_RALPH_JSON_PARSER_LOADED:-}" ]] && source "${_AGENTS_LIB_DIR}/json_parser.zsh"

############################################
# Agent Registry
############################################
#
# Each agent is defined with these properties (stored as shell vars):
#   _AGENT_<name>_CMD       - Default command template
#   _AGENT_<name>_JSON_FLAG - Flag to enable JSON output
#   _AGENT_<name>_PARSER    - Function name to parse output
#   _AGENT_<name>_CAN_COMMIT - 1 if agent can run git commit
#   _AGENT_<name>_YOLO_FLAG - Flag for autonomous mode (no prompts)
############################################

# Claude agent configuration
typeset -g _AGENT_claude_CMD="claude -p"
typeset -g _AGENT_claude_JSON_FLAG="--output-format=stream-json"
typeset -g _AGENT_claude_PARSER="stream_parse_claude"
typeset -g _AGENT_claude_CAN_COMMIT=1
typeset -g _AGENT_claude_YOLO_FLAG="--dangerously-skip-permissions"
typeset -g _AGENT_claude_VERBOSE_FLAG="--verbose"

# Codex agent configuration
typeset -g _AGENT_codex_CMD="codex exec"
typeset -g _AGENT_codex_JSON_FLAG="--json"
typeset -g _AGENT_codex_PARSER="stream_parse_codex"
typeset -g _AGENT_codex_CAN_COMMIT=1
typeset -g _AGENT_codex_YOLO_FLAG="--yolo"
typeset -g _AGENT_codex_VERBOSE_FLAG=""

# OpenCode agent configuration (placeholder - adjust based on actual CLI)
typeset -g _AGENT_opencode_CMD="opencode"
typeset -g _AGENT_opencode_JSON_FLAG="--json"
typeset -g _AGENT_opencode_PARSER="stream_parse_generic"
typeset -g _AGENT_opencode_CAN_COMMIT=1
typeset -g _AGENT_opencode_YOLO_FLAG="--auto"
typeset -g _AGENT_opencode_VERBOSE_FLAG="--verbose"

# Aider agent configuration (placeholder)
typeset -g _AGENT_aider_CMD="aider"
typeset -g _AGENT_aider_JSON_FLAG=""
typeset -g _AGENT_aider_PARSER="stream_parse_generic"
typeset -g _AGENT_aider_CAN_COMMIT=1
typeset -g _AGENT_aider_YOLO_FLAG="--yes"
typeset -g _AGENT_aider_VERBOSE_FLAG="--verbose"

# List of known agents
typeset -ga _KNOWN_AGENTS=(claude codex opencode aider)

############################################
# Agent Role Defaults
############################################
: "${RALPH_DEVELOPER_AGENT:=claude}"
: "${RALPH_REVIEWER_AGENT:=codex}"

############################################
# Agent Query Functions
############################################

# Get the base command for an agent (without flags)
agent_get_cmd() {
  local agent="$1"
  local var_name="_AGENT_${agent}_CMD"
  print -r -- "${(P)var_name:-}"
}

# Get the JSON output flag for an agent
agent_get_json_flag() {
  local agent="$1"
  local var_name="_AGENT_${agent}_JSON_FLAG"
  print -r -- "${(P)var_name:-}"
}

# Get the parser function for an agent
agent_get_parser() {
  local agent="$1"
  local var_name="_AGENT_${agent}_PARSER"
  print -r -- "${(P)var_name:-stream_parse_generic}"
}

# Check if agent can commit (returns 0/1)
agent_can_commit() {
  local agent="$1"
  local var_name="_AGENT_${agent}_CAN_COMMIT"
  [[ "${(P)var_name:-0}" == "1" ]]
}

# Get the yolo/autonomous mode flag
agent_get_yolo_flag() {
  local agent="$1"
  local var_name="_AGENT_${agent}_YOLO_FLAG"
  print -r -- "${(P)var_name:-}"
}

# Get verbose flag
agent_get_verbose_flag() {
  local agent="$1"
  local var_name="_AGENT_${agent}_VERBOSE_FLAG"
  print -r -- "${(P)var_name:-}"
}

# Check if agent is known/registered
agent_is_known() {
  local agent="$1"
  [[ " ${_KNOWN_AGENTS[*]} " == *" $agent "* ]]
}

############################################
# Command Building
############################################

# Build full command string for an agent with all needed flags
# Usage: agent_build_cmd <agent> [--json] [--yolo] [--verbose]
agent_build_cmd() {
  local agent="$1"
  shift

  local cmd parts=()
  cmd=$(agent_get_cmd "$agent")
  [[ -z "$cmd" ]] && { print "unknown_agent"; return 1; }

  parts+=("$cmd")

  # Process optional flags
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --json)
        local json_flag
        json_flag=$(agent_get_json_flag "$agent")
        [[ -n "$json_flag" ]] && parts+=("$json_flag")
        ;;
      --yolo)
        local yolo_flag
        yolo_flag=$(agent_get_yolo_flag "$agent")
        [[ -n "$yolo_flag" ]] && parts+=("$yolo_flag")
        ;;
      --verbose)
        local verbose_flag
        verbose_flag=$(agent_get_verbose_flag "$agent")
        [[ -n "$verbose_flag" ]] && parts+=("$verbose_flag")
        ;;
    esac
    shift
  done

  print -r -- "${parts[*]}"
}

# Build the default autonomous command for developer role
agent_developer_cmd() {
  local agent="${RALPH_DEVELOPER_AGENT}"
  agent_build_cmd "$agent" --json --yolo --verbose
}

# Build the default autonomous command for reviewer role
agent_reviewer_cmd() {
  local agent="${RALPH_REVIEWER_AGENT}"
  agent_build_cmd "$agent" --json --yolo
}

############################################
# Generic Stream Parser
############################################
#
# Fallback parser for agents without specialized parsing.
# Simply passes through output, optionally logging to file.
############################################

stream_parse_generic() {
  local line
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" ]] && continue
    # Try to pretty-print JSON, fall back to raw output
    if [[ "$HAS_JQ" == "1" ]]; then
      local parsed
      parsed=$(print -r -- "$line" | jq -r '
        if .type then
          "[" + .type + "] " + (.message // .text // .content // "" | tostring)[:100]
        else
          .[:100]
        end
      ' 2>/dev/null)
      if [[ -n "$parsed" ]]; then
        print "${DIM}[Agent]${RESET} ${parsed}"
      else
        print "${DIM}[Agent]${RESET} ${line:0:100}"
      fi
    else
      print "${DIM}[Agent]${RESET} ${line:0:100}"
    fi
    [[ -n "${STREAM_LOGFILE:-}" ]] && print -r -- "$line" >> "$STREAM_LOGFILE"
  done
}

############################################
# Agent Detection & Dispatch
############################################

# Detect agent type from command string (enhanced version)
# Returns the agent name if recognized
detect_agent_from_cmd() {
  local cmd="$1"
  local agent
  for agent in "${_KNOWN_AGENTS[@]}"; do
    if [[ "$cmd" == *"$agent"* ]]; then
      print "$agent"
      return 0
    fi
  done
  print "unknown"
}

# Get the appropriate parser for a command string
get_parser_for_cmd() {
  local cmd="$1"
  local agent
  agent=$(detect_agent_from_cmd "$cmd")
  agent_get_parser "$agent"
}

# Run output through the appropriate parser for an agent
# Usage: some_command | dispatch_parser <agent>
dispatch_parser() {
  local agent="$1"
  local parser
  parser=$(agent_get_parser "$agent")
  "$parser"
}

############################################
# Agent Registration (for custom agents)
############################################

# Register a new agent with custom configuration
# Usage: register_agent <name> <cmd> <json_flag> <parser> <can_commit> <yolo_flag> [verbose_flag]
register_agent() {
  local name="$1"
  local cmd="$2"
  local json_flag="$3"
  local parser="$4"
  local can_commit="$5"
  local yolo_flag="$6"
  local verbose_flag="${7:-}"

  eval "_AGENT_${name}_CMD=\"$cmd\""
  eval "_AGENT_${name}_JSON_FLAG=\"$json_flag\""
  eval "_AGENT_${name}_PARSER=\"$parser\""
  eval "_AGENT_${name}_CAN_COMMIT=\"$can_commit\""
  eval "_AGENT_${name}_YOLO_FLAG=\"$yolo_flag\""
  eval "_AGENT_${name}_VERBOSE_FLAG=\"$verbose_flag\""

  # Add to known agents if not already there
  if ! agent_is_known "$name"; then
    _KNOWN_AGENTS+=("$name")
  fi
}

# List all registered agents
list_agents() {
  for agent in "${_KNOWN_AGENTS[@]}"; do
    local cmd can_commit
    cmd=$(agent_get_cmd "$agent")
    agent_can_commit "$agent" && can_commit="yes" || can_commit="no"
    print "${BOLD}$agent${RESET}: $cmd ${DIM}(can_commit: $can_commit)${RESET}"
  done
}
