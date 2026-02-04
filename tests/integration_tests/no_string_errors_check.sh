#!/bin/bash
# Compliance check: ensure no string-based errors in effect handlers
#
# Effect handlers must return error events from the Error namespace, not string errors.
# String errors bypass the reducer and prevent recovery logic from handling different
# failure modes appropriately.

set -e

echo "Checking for string-based errors in effect handlers..."

# Check for anyhow::anyhow! in handler modules
if rg -q 'return Err\(anyhow::anyhow!' ralph-workflow/src/reducer/handler/; then
    echo "ERROR: Found string-based errors in effect handlers:"
    rg 'return Err\(anyhow::anyhow!' ralph-workflow/src/reducer/handler/
    echo ""
    echo "Effect handlers must return error events from the Error namespace, not string errors."
    echo "See ErrorEvent enum in ralph-workflow/src/reducer/event/error.rs"
    exit 1
fi

echo "✓ No string-based errors in effect handlers"
