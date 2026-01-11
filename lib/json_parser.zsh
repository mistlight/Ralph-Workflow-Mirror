#!/usr/bin/env zsh
############################################
# JSON Stream Parsing Module
#
# Functions for parsing NDJSON (newline-delimited JSON)
# streams from Claude and Codex CLI tools.
#
# Dependencies:
#   - lib/colors.zsh (for colored output)
#   - jq (optional, for JSON parsing - falls back to raw output)
############################################

# Guard against multiple sourcing
[[ -n "${_RALPH_JSON_PARSER_LOADED:-}" ]] && return 0
typeset -g _RALPH_JSON_PARSER_LOADED=1

# Get script directory for relative sourcing
typeset -g _JSON_PARSER_LIB_DIR="${0:A:h}"

# Source dependencies if not already loaded
[[ -z "${_RALPH_COLORS_LOADED:-}" ]] && source "${_JSON_PARSER_LIB_DIR}/colors.zsh"

############################################
# Configuration
############################################

# Check if jq is available for JSON parsing
typeset -g HAS_JQ=0
if command -v jq >/dev/null 2>&1; then
  HAS_JQ=1
fi

# Log file for raw JSON stream (set externally)
typeset -g STREAM_LOGFILE="${STREAM_LOGFILE:-}"

# Verbosity level for output display (set externally)
# 0 = quiet (minimal output)
# 1 = normal (default, moderate truncation)
# 2 = verbose (expanded output with higher limits)
# 3 = full (no truncation, show complete content)
typeset -g RALPH_VERBOSITY="${RALPH_VERBOSITY:-1}"

# Get truncation limits based on verbosity level
# Returns the character limit for the given content type
_get_truncate_limit() {
  local content_type="$1"
  case "$RALPH_VERBOSITY" in
    0)  # quiet - very aggressive truncation
      case "$content_type" in
        text)        print 60 ;;
        tool_result) print 40 ;;
        user)        print 30 ;;
        result)      print 200 ;;
        command)     print 40 ;;
        agent_msg)   print 50 ;;
        *)           print 50 ;;
      esac
      ;;
    1)  # normal (default) - current behavior
      case "$content_type" in
        text)        print 120 ;;
        tool_result) print 80 ;;
        user)        print 60 ;;
        result)      print 500 ;;
        command)     print 60 ;;
        agent_msg)   print 100 ;;
        *)           print 80 ;;
      esac
      ;;
    2)  # verbose - expanded limits
      case "$content_type" in
        text)        print 500 ;;
        tool_result) print 300 ;;
        user)        print 200 ;;
        result)      print 2000 ;;
        command)     print 200 ;;
        agent_msg)   print 400 ;;
        *)           print 300 ;;
      esac
      ;;
    3|*)  # full - no truncation (use very high limit)
      print 999999
      ;;
  esac
}

# Truncate text to limit with ellipsis indicator
# Usage: _truncate_text "long text" limit
_truncate_text() {
  local text="$1"
  local limit="$2"
  if [[ ${#text} -gt $limit ]]; then
    print "${text:0:$limit}..."
  else
    print "$text"
  fi
}

############################################
# Claude Event Parsing
############################################
#
# Claude Code stream-json format includes:
# - {"type":"system", "subtype":"init", ...} - session init
# - {"type":"assistant", "message":{...}} - assistant messages with content
# - {"type":"user", ...} - user messages
# - {"type":"result", "subtype":"success"|"error", ...} - final result with stats
#
# Content blocks within messages can be:
# - {"type":"text", "text":"..."} - plain text
# - {"type":"tool_use", "name":"...", "input":{...}} - tool invocations
# - {"type":"tool_result", ...} - tool outputs
############################################

# Parse and display a single Claude JSON event
# Reads JSON from stdin, outputs formatted text
parse_claude_event() {
  local line="$1"
  [[ -z "$line" ]] && return 0
  [[ "$HAS_JQ" != "1" ]] && { print -r -- "$line"; return 0; }

  local event_type subtype
  event_type=$(print -r -- "$line" | jq -r '.type // empty' 2>/dev/null) || { print -r -- "$line"; return 0; }
  subtype=$(print -r -- "$line" | jq -r '.subtype // empty' 2>/dev/null)

  case "$event_type" in
    system)
      # System events: init, session info
      if [[ "$subtype" == "init" ]]; then
        local session_id cwd
        session_id=$(print -r -- "$line" | jq -r '.session_id // "unknown"' 2>/dev/null)
        cwd=$(print -r -- "$line" | jq -r '.cwd // ""' 2>/dev/null)
        print "${DIM}[Claude]${RESET} ${CYAN}Session started${RESET} ${DIM}(${session_id:0:8}...)${RESET}"
        [[ -n "$cwd" ]] && print "${DIM}[Claude]${RESET} ${DIM}Working dir: ${cwd}${RESET}"
      else
        print "${DIM}[Claude]${RESET} ${CYAN}${subtype:-system}${RESET}"
      fi
      ;;
    assistant)
      # Assistant message with content blocks
      # Parse each content block: text, tool_use, etc.
      local content_count
      content_count=$(print -r -- "$line" | jq -r '.message.content | length // 0' 2>/dev/null)
      if [[ "$content_count" -gt 0 ]]; then
        # Iterate through content blocks
        local idx=0
        while [[ $idx -lt $content_count ]]; do
          local block_type block_text tool_name
          block_type=$(print -r -- "$line" | jq -r ".message.content[$idx].type // empty" 2>/dev/null)
          case "$block_type" in
            text)
              block_text=$(print -r -- "$line" | jq -r ".message.content[$idx].text // empty" 2>/dev/null)
              if [[ -n "$block_text" ]]; then
                local limit=$(_get_truncate_limit "text")
                local preview=$(_truncate_text "$block_text" "$limit")
                print "${DIM}[Claude]${RESET} ${WHITE}${preview}${RESET}"
              fi
              ;;
            tool_use)
              tool_name=$(print -r -- "$line" | jq -r ".message.content[$idx].name // \"unknown\"" 2>/dev/null)
              local tool_input_key
              tool_input_key=$(print -r -- "$line" | jq -r ".message.content[$idx].input | keys[0] // empty" 2>/dev/null)
              print "${DIM}[Claude]${RESET} ${MAGENTA}Tool${RESET}: ${BOLD}${tool_name}${RESET}${tool_input_key:+ (${tool_input_key})}"
              ;;
            tool_result)
              local result_preview
              result_preview=$(print -r -- "$line" | jq -r ".message.content[$idx].content // empty" 2>/dev/null | head -1)
              if [[ -n "$result_preview" ]]; then
                local limit=$(_get_truncate_limit "tool_result")
                local preview=$(_truncate_text "$result_preview" "$limit")
                print "${DIM}[Claude]${RESET} ${DIM}Result:${RESET} ${preview}"
              fi
              ;;
          esac
          ((idx++))
        done
      fi
      ;;
    user)
      # User messages (usually the prompts we send)
      local user_text
      user_text=$(print -r -- "$line" | jq -r '.message.content[0].text // empty' 2>/dev/null)
      if [[ -n "$user_text" ]]; then
        local limit=$(_get_truncate_limit "user")
        local preview=$(_truncate_text "$user_text" "$limit")
        print "${DIM}[Claude]${RESET} ${BLUE}User${RESET}: ${DIM}${preview}${RESET}"
      fi
      ;;
    result)
      # Final result with statistics
      local result_subtype duration_ms cost num_turns
      result_subtype=$(print -r -- "$line" | jq -r '.subtype // "unknown"' 2>/dev/null)
      duration_ms=$(print -r -- "$line" | jq -r '.duration_ms // 0' 2>/dev/null)
      cost=$(print -r -- "$line" | jq -r '.total_cost_usd // 0' 2>/dev/null)
      num_turns=$(print -r -- "$line" | jq -r '.num_turns // 0' 2>/dev/null)
      local duration_s=$((duration_ms / 1000))
      local duration_m=$((duration_s / 60))
      local duration_s_rem=$((duration_s % 60))
      if [[ "$result_subtype" == "success" ]]; then
        print "${DIM}[Claude]${RESET} ${GREEN}${CHECK} Completed${RESET} ${DIM}(${duration_m}m ${duration_s_rem}s, ${num_turns} turns, \$${cost})${RESET}"
      else
        local error_msg
        error_msg=$(print -r -- "$line" | jq -r '.error // "unknown error"' 2>/dev/null)
        print "${DIM}[Claude]${RESET} ${RED}${CROSS} ${result_subtype}${RESET}: ${error_msg} ${DIM}(${duration_m}m ${duration_s_rem}s)${RESET}"
      fi
      # Also show the result summary if available
      local result_text
      result_text=$(print -r -- "$line" | jq -r '.result // empty' 2>/dev/null)
      if [[ -n "$result_text" ]]; then
        print ""
        print "${BOLD}Result summary:${RESET}"
        local limit=$(_get_truncate_limit "result")
        local result_preview=$(_truncate_text "$result_text" "$limit")
        print "${DIM}${result_preview}${RESET}"
      fi
      ;;
    *)
      # Pass through unknown event types with their subtype if available
      if [[ -n "$subtype" ]]; then
        print "${DIM}[Claude]${RESET} ${DIM}${event_type}:${subtype}${RESET}"
      else
        print "${DIM}[Claude]${RESET} ${DIM}${event_type}${RESET}"
      fi
      ;;
  esac
}

############################################
# Codex Event Parsing
############################################
#
# Codex event types:
# - thread.started - new thread
# - turn.started - turn begins
# - turn.completed - turn ends with usage stats
# - turn.failed - turn error
# - item.started - item (command, message, etc.) starts
# - item.completed - item finishes
# - error - error message
############################################

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
          local cmd_limit=$(_get_truncate_limit "command")
          local cmd_preview=$(_truncate_text "$item_cmd" "$cmd_limit")
          print "${DIM}[Codex]${RESET} ${MAGENTA}Exec${RESET}: ${DIM}${cmd_preview}${RESET}"
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
            local msg_limit=$(_get_truncate_limit "agent_msg")
            local preview=$(_truncate_text "$item_text" "$msg_limit")
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

############################################
# Stream Processors
############################################
#
# These read NDJSON from stdin, parsing and displaying each line.
# For streaming output, we use unbuffered reading when available.
# Each complete JSON line is parsed and displayed in real-time.
#
# Usage: some_command | stream_parse_claude
############################################

stream_parse_claude() {
  local line
  # Read line by line - NDJSON guarantees each JSON object is on its own line
  # The || [[ -n "$line" ]] handles the case where the last line has no newline
  while IFS= read -r line || [[ -n "$line" ]]; do
    # Skip empty lines
    [[ -z "$line" ]] && continue
    parse_claude_event "$line" || true  # Don't fail on parse errors
    # Also write raw JSON to log file if provided
    [[ -n "${STREAM_LOGFILE:-}" ]] && print -r -- "$line" >> "$STREAM_LOGFILE"
  done
}

stream_parse_codex() {
  local line
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" ]] && continue
    parse_codex_event "$line" || true  # Don't fail on parse errors
    [[ -n "${STREAM_LOGFILE:-}" ]] && print -r -- "$line" >> "$STREAM_LOGFILE"
  done
}

############################################
# Utility Functions
############################################

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
