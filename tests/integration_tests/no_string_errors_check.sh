#!/bin/bash
# Compliance check: ensure no string-based errors in effect handlers
#
# Effect handlers must return error events from the Error namespace, not string errors.
# String errors bypass the reducer and prevent recovery logic from handling different
# failure modes appropriately.

set -e

echo "Checking for string-based errors in effect handlers..."

# Check for any `anyhow!` string-construction in production handler modules.
#
# We intentionally scan for the macro usage itself (not just `return Err(...)`) because
# string-based errors can be introduced via:
# - `.map_err(|_| anyhow::anyhow!(...))?`
# - `.ok_or_else(|| anyhow::anyhow!(...))?`
# - `return Err(anyhow::anyhow!(...))`
#
# Handler tests are excluded; only production handler code matters.
if rg -n -q --glob '!**/tests/**' '\banyhow::anyhow!\(|\banyhow!\(' ralph-workflow/src/reducer/handler/; then
    echo "ERROR: Found string-based errors in effect handlers:"
    rg -n --glob '!**/tests/**' '\banyhow::anyhow!\(|\banyhow!\(' ralph-workflow/src/reducer/handler/
    echo ""
    echo "Effect handlers must return error events from the Error namespace, not string errors."
    echo "See ErrorEvent enum in ralph-workflow/src/reducer/event/error.rs"
    exit 1
fi

echo "✓ No string-based errors in effect handlers"
