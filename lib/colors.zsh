#!/usr/bin/env zsh
############################################
# Colors & Formatting Module
#
# Provides ANSI escape codes for terminal coloring.
# Respects NO_COLOR env var (https://no-color.org/).
# Falls back to no colors if terminal doesn't support them.
#
# Usage:
#   source lib/colors.zsh
#   print "${GREEN}Success${RESET}"
############################################

# Guard against multiple sourcing
[[ -n "${_RALPH_COLORS_LOADED:-}" ]] && return 0
typeset -g _RALPH_COLORS_LOADED=1

# Initialize color variables based on terminal capabilities
ralph_init_colors() {
  if [[ -z "${NO_COLOR:-}" ]] && [[ -t 1 ]]; then
    # Bold/Reset
    typeset -g BOLD=$'\e[1m'
    typeset -g DIM=$'\e[2m'
    typeset -g RESET=$'\e[0m'

    # Foreground colors
    typeset -g RED=$'\e[31m'
    typeset -g GREEN=$'\e[32m'
    typeset -g YELLOW=$'\e[33m'
    typeset -g BLUE=$'\e[34m'
    typeset -g MAGENTA=$'\e[35m'
    typeset -g CYAN=$'\e[36m'
    typeset -g WHITE=$'\e[37m'

    # Background colors (for headers)
    typeset -g BG_BLUE=$'\e[44m'
    typeset -g BG_GREEN=$'\e[42m'
    typeset -g BG_YELLOW=$'\e[43m'
    typeset -g BG_RED=$'\e[41m'
  else
    # No color mode
    typeset -g BOLD="" DIM="" RESET=""
    typeset -g RED="" GREEN="" YELLOW="" BLUE="" MAGENTA="" CYAN="" WHITE=""
    typeset -g BG_BLUE="" BG_GREEN="" BG_YELLOW="" BG_RED=""
  fi
}

# Box-drawing characters for visual structure
typeset -g BOX_TL="╭" BOX_TR="╮" BOX_BL="╰" BOX_BR="╯"
typeset -g BOX_H="─" BOX_V="│"
typeset -g ARROW="→" CHECK="✓" CROSS="✗" WARN="⚠" INFO="ℹ"

# Initialize colors on source
ralph_init_colors
