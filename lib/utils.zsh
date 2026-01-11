#!/usr/bin/env zsh
############################################
# General Utilities Module
#
# Common helper functions used throughout Ralph.
#
# Dependencies:
#   - lib/colors.zsh (for colored output)
############################################

# Guard against multiple sourcing
[[ -n "${_RALPH_UTILS_LOADED:-}" ]] && return 0
typeset -g _RALPH_UTILS_LOADED=1

# Get script directory for relative sourcing
typeset -g RALPH_LIB_DIR="${0:A:h}"

# Source dependencies if not already loaded
[[ -z "${_RALPH_COLORS_LOADED:-}" ]] && source "${RALPH_LIB_DIR}/colors.zsh"
[[ -z "${_RALPH_TIMER_LOADED:-}" ]] && source "${RALPH_LIB_DIR}/timer.zsh"

############################################
# Logging functions
############################################

# Get current timestamp
ts() {
  date +"%Y-%m-%d %H:%M:%S"
}

# Exit with error message
fail() {
  print "${RED}${CROSS}${RESET} $*" >&2
  exit 1
}

# Timestamped log line with icon
log_info() {
  print "${DIM}[$(ts)]${RESET} ${BLUE}${INFO}${RESET}  $*"
}

log_success() {
  print "${DIM}[$(ts)]${RESET} ${GREEN}${CHECK}${RESET}  ${GREEN}$*${RESET}"
}

log_warn() {
  print "${DIM}[$(ts)]${RESET} ${YELLOW}${WARN}${RESET}  ${YELLOW}$*${RESET}"
}

log_error() {
  print "${DIM}[$(ts)]${RESET} ${RED}${CROSS}${RESET}  ${RED}$*${RESET}"
}

log_step() {
  print "${DIM}[$(ts)]${RESET} ${MAGENTA}${ARROW}${RESET}  $*"
}

############################################
# File logging (strips ANSI codes)
############################################

# Log to file (strips ANSI escape sequences for clean log files)
log_to_file() {
  local msg="$1"
  local logfile="${2:-.agent/logs/pipeline.log}"
  # Strip ANSI escape sequences for log file
  print -r -- "$msg" | sed 's/\x1b\[[0-9;]*m//g' >> "$logfile"
}

# Combined: print to terminal with colors, log to file without
tlog_info() {
  log_info "$@"
  log_to_file "[$(ts)] [INFO] $*"
}

tlog_success() {
  log_success "$@"
  log_to_file "[$(ts)] [OK] $*"
}

tlog_warn() {
  log_warn "$@"
  log_to_file "[$(ts)] [WARN] $*"
}

tlog_error() {
  log_error "$@"
  log_to_file "[$(ts)] [ERROR] $*"
}

tlog_step() {
  log_step "$@"
  log_to_file "[$(ts)] [STEP] $*"
}

############################################
# Visual output helpers
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
  local current="$1"
  local total="$2"
  local label="${3:-Progress}"
  local pct=$((current * 100 / total))
  local bar_width=20
  local filled=$((current * bar_width / total))
  local empty=$((bar_width - filled))

  local bar=""
  local k
  for ((k=0; k<filled; k++)); do bar+="█"; done
  for ((k=0; k<empty; k++)); do bar+="░"; done

  print "${DIM}${label}:${RESET} ${CYAN}[${bar}]${RESET} ${BOLD}${pct}%${RESET} (${current}/${total})"
}

############################################
# File utilities
############################################

# Check if a file contains a specific marker string
# Returns 0 if found, 1 if not found
file_contains_marker() {
  local file="$1"
  local marker="$2"

  [[ ! -f "$file" ]] && return 1

  if command -v rg >/dev/null 2>&1; then
    rg -n --fixed-strings -- "$marker" "$file" >/dev/null 2>&1
    return $?
  fi
  grep -Fq -- "$marker" "$file" >/dev/null 2>&1
}

############################################
# Context cleanup utilities
############################################
# These functions help prevent "context pollution" between agent phases
# by archiving/clearing context files that might bias subsequent agents.

# Archive a context file to .agent/archive/ with timestamp
# Args: $1 = file path (e.g., .agent/STATUS.md)
archive_context_file() {
  local file="$1"
  [[ ! -f "$file" ]] && return 0

  local archive_dir=".agent/archive"
  local timestamp="$(date +%Y%m%d_%H%M%S)"
  local basename="${file:t}"

  mkdir -p "$archive_dir"
  cp "$file" "${archive_dir}/${basename%.md}_${timestamp}.md"
}

# Clear context file by truncating it (preserves file, removes content)
# Args: $1 = file path
clear_context_file() {
  local file="$1"
  [[ ! -f "$file" ]] && return 0
  : > "$file"
}

# Clean context before reviewer phase
# Archives STATUS.md, NOTES.md, and ISSUES.md, then clears them so reviewer has fresh eyes
# This prevents "context pollution" where reviewer sees developer's notes or stale issues
clean_context_for_reviewer() {
  log_info "Cleaning context for reviewer (fresh eyes)..."

  # Archive current context files (including ISSUES.md to prevent stale issues)
  archive_context_file ".agent/STATUS.md"
  archive_context_file ".agent/NOTES.md"
  archive_context_file ".agent/ISSUES.md"

  # Reset STATUS.md to minimal state
  cat > .agent/STATUS.md <<'EOF'
# STATUS
- Last action: Development phase complete
- Blockers: none
- Next action: Review phase starting
EOF

  # Clear NOTES.md (reviewer should not see developer notes)
  : > .agent/NOTES.md

  # Clear ISSUES.md (reviewer should find issues fresh, not see stale ones)
  : > .agent/ISSUES.md

  log_success "Context cleaned for reviewer"
}

# Reset context between iterations (lighter cleanup)
# Only resets STATUS.md, preserves NOTES.md for continuity
reset_iteration_context() {
  local iteration="$1"
  local next_action="${2:-Continue development}"

  cat > .agent/STATUS.md <<EOF
# STATUS
- Last action: Starting iteration ${iteration}
- Blockers: none
- Next action: ${next_action}
- Updated at: $(ts)
EOF
}
