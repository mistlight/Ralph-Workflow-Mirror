# Template Customization Guide

This guide explains how to customize the prompt templates used for AI agent communication in the Ralph workflow system.

## Table of Contents

1. [Overview](#overview)
2. [Template Features](#template-features)
3. [Template Directory Structure](#template-directory-structure)
4. [Template Reference](#template-reference)
5. [Advanced Features](#advanced-features)
6. [Customization Examples](#customization-examples)

## Overview

The Ralph workflow system uses a template-based approach for all AI agent communications. Each template is a `.txt` file that defines the prompt structure, with support for:

- **Variable substitution** - Dynamic content injection
- **Conditionals** - Show/hide content based on variable values
- **Loops** - Iterate over lists of items
- **Partials** - Reusable template components
- **Defaults** - Fallback values for missing variables

All templates are located in `ralph-workflow/src/prompts/templates/`.

## Template Features

### Variable Substitution

Variables are referenced using double curly braces:

```
{{VARIABLE_NAME}}
```

Variables are replaced with their values at render time. If a variable is required but not provided, the template will fail to render with a clear error message.

### Default Values

You can provide default values for variables:

```
{{VARIABLE|default="fallback value"}}
```

If `VARIABLE` is not provided or is empty, the default value will be used instead.

### Conditionals

Show or hide content based on variable values:

```
{% if VARIABLE %}
This content is shown when VARIABLE is truthy (non-empty).
{% endif %}

{% if !VARIABLE %}
This content is shown when VARIABLE is falsy (empty or missing).
{% endif %}
```

A variable is considered "truthy" if it exists and is non-empty.

### Loops

Iterate over comma-separated values:

```
{% for item in ITEMS %}
- {{item}}
{% endfor %}
```

If `ITEMS = "apple,banana,cherry"`, this renders as:
```
- apple
- banana
- cherry
```

### Partials

Include shared template sections:

```
{{> shared/_critical_header}}
```

Partials are templates that can be included in other templates. They are useful for sharing common sections across multiple templates.

### Comments

Add documentation to your templates that won't appear in the final output:

```
{# This is a comment and will be stripped from the output #}
```

## Template Directory Structure

```
ralph-workflow/src/prompts/templates/
├── shared/                          # Reusable partials
│   ├── _critical_header.txt         # Critical security/context header
│   ├── _context_section.txt         # PROMPT.md/PLAN.md context
│   ├── _diff_section.txt            # Diff display format
│   └── _output_checklist.txt        # Output format checklist
├── commit_*.txt                     # Commit message generation templates
├── conflict_resolution.txt          # Merge conflict resolution
├── conflict_resolution_fallback.txt # Fallback for template errors
├── developer_iteration.txt          # Developer iteration prompts
├── fix_mode.txt                     # Fix mode prompts
└── planning.txt                     # Planning prompts
```

### Shared Partials

Partials in the `shared/` directory (prefixed with `_`) are reusable components:

- **`_critical_header.txt`** - Agent role and behavior constraints
- **`_context_section.txt`** - Task context from PROMPT.md and PLAN.md
- **`_diff_section.txt`** - Formatted diff display
- **`_output_checklist.txt`** - Required output format checklist

## Template Reference

### Commit Message Templates

| Template | When Used | Variables |
|----------|-----------|-----------|
| `commit_message_xml.txt` | Normal strategy (standard commit generation) | `DIFF`, `FILES_CHANGED`, `BRANCH_NAME` |
| `commit_simplified.txt` | Simplified strategy (direct instructions) | `DIFF`, `FILES_CHANGED`, `BRANCH_NAME` |
| `commit_xsd_retry.txt` | XSD validation retry (in-session) | `DIFF`, `FILES_CHANGED`, `BRANCH_NAME`, `XSD_ERROR` |
| `commit_message_fallback.txt` | Fallback when template rendering fails | `DIFF`, `FILES_CHANGED` |

**Trigger**: Used when `ralph commit` is run.

### Developer Templates

| Template | When Used | Variables |
|----------|-----------|-----------|
| `developer_iteration.txt` | Developer iteration mode | `ITERATION`, `TOTAL_ITERATIONS`, `CONTEXT_LEVEL`, `PROMPT_MD`, `PLAN_MD` |
| `planning.txt` | Planning mode | `PROMPT_MD` |

**Trigger**: Used during `ralph plan` or developer iteration phases.

### Fix Mode Template

| Template | When Used | Variables |
|----------|-----------|-----------|
| `fix_mode.txt` | Fix mode for addressing issues | `PROMPT_MD`, `PLAN_MD` |

**Trigger**: Used when `ralph fix` is run.

### Conflict Resolution Templates

| Template | When Used | Variables |
|----------|-----------|-----------|
| `conflict_resolution.txt` | Merge conflict resolution | `CONTEXT`, `CONFLICTS` |
| `conflict_resolution_fallback.txt` | Fallback if template fails | `CONTEXT`, `CONFLICTS` |

**Trigger**: Used automatically during rebase when conflicts occur.

## Advanced Features

### Nested Conditionals

You can nest conditionals for more complex logic:

```
{% if FEATURE_A %}
Feature A is enabled
  {% if FEATURE_B %}
  Both A and B are enabled
  {% endif %}
{% endif %}
```

### Loops with Conditionals

Combine loops with conditionals for filtering:

```
{% for file in FILES %}
  {% if file %}
Processing: {{file}}
  {% endif %}
{% endfor %}
```

This will skip empty items in the list.

### Accessing Loop Variables

Inside a loop, the loop variable is available for use:

```
{% for item in ITEMS %}
Item: {{item}}
Status: {% if item %}Present{% else %}Missing{% endif %}
{% endfor %}
```

## Customization Examples

### Example 1: Add Custom Header to Commit Messages

Edit `commit_message_xml.txt`:

```
{# Template: commit_message_xml.txt #}
{# Purpose: Generate structured commit messages #}
{# Variables: DIFF, FILES_CHANGED, BRANCH_NAME #}

{# Custom company header #}
Company: ACME Corp
Policy: Follow conventional commit format

{{> shared/_critical_header}}

# COMMIT MESSAGE GENERATION

Generate a commit message following these guidelines:
...

{% if BRANCH_NAME %}
Branch: {{BRANCH_NAME}}
{% endif %}

## Changed Files

{{FILES_CHANGED}}

## Diff

{{DIFF}}
```

### Example 2: Customize Developer Iteration Prompt

Edit `developer_iteration.txt` to add custom instructions:

```
{# Template: developer_iteration.txt #}
{# Purpose: Guide developer agent through implementation #}

{{> shared/_critical_header}}

{# Custom instructions #}
IMPORTANT: Always write tests before implementing features.
Follow the project's CODE_STYLE.md guidelines.

# IMPLEMENTATION MODE

{% if ITERATION %}
Iteration: {{ITERATION}} of {{TOTAL_ITERATIONS}}
{% endif %}

...

{% if PLAN_MD %}
## Implementation Plan
{{PLAN_MD}}

{# Custom reminder #}
Remember: Update tests after implementation.
{% endif %}
```

### Example 3: Conditional Output Format

Edit a review template to use different formats based on context:

```
{% if DETAILED_MODE %}
## Detailed Review
Provide line-by-line feedback with specific suggestions.
{% else %}
## Quick Review
Focus on high-level issues and security concerns only.
{% endif %}
```

### Example 4: Loop Through Files

Add a file-by-file review section:

```
## File-by-File Analysis

{% for file in FILES %}
### {{file}}

{% if file %}
Reviewing {{file}} for:
- Security issues
- Performance concerns
- Code style violations
{% endif %}

{% endfor %}
```

## Best Practices

1. **Always include template headers** - Document purpose, variables, and when the template is used
2. **Use partials for shared content** - Don't repeat common sections
3. **Provide defaults when appropriate** - Use `{{VAR|default="value"}}` for optional content
4. **Test your changes** - Run `cargo test` to ensure templates still render correctly
5. **Keep templates focused** - Each template should have a single, clear purpose
6. **Use comments liberally** - Explain why something is included, not just what it is

## Template Metadata Header

Every template should include a standardized header:

```
{# ============================================================================ #}
{# Template: template_name.txt                                                 #}
{# Version: 1.0                                                                #}
{# ============================================================================ #}
{#                                                                             #}
{# PURPOSE:                                                                    #}
{#   Brief description of what this template does                               #}
{#                                                                             #}
{# WHEN USED:                                                                  #}
{#   Describe the workflow phase or command that triggers this template         #}
{#                                                                             #}
{# TRIGGER:                                                                    #}
{#   Command or condition that causes this template to be used                  #}
{#                                                                             #}
{# VARIABLES:                                                                  #}
{#   VAR1   - Description of VAR1                                              #}
{#   VAR2   - Description of VAR2                                              #}
{#                                                                             #}
{# OUTPUT:                                                                     #}
{#   Expected format of the AI agent's response                                #}
{#                                                                             #}
{# NOTES:                                                                      #}
{#   Any important notes or constraints                                        #}
{# ============================================================================ #}
```

## Troubleshooting

### Template Rendering Fails

If a template fails to render:

1. Check that all required variables are provided
2. Verify template syntax (matched braces, correct endif/endfor tags)
3. Look for circular references in partials
4. Check the error message for specific issues

### Missing Variable Error

```
Error: Missing required variable: {{ CONTEXT }}
```

This means a required variable wasn't provided. Either:
- Provide the variable when calling the template
- Add a default value: `{{CONTEXT|default="No context provided"}}`

### Circular Reference Error

```
Error: Circular reference detected in partials: {{> header}} -> {{> footer}} -> {{> header}}
```

This means partials reference each other in a cycle. Break the cycle by removing one of the references.

## Getting Help

For questions about template customization:

1. Check existing templates for examples
2. Review the template engine tests in `template_engine.rs`
3. Refer to this guide's syntax reference
4. Run `cargo test -p ralph-workflow template_engine` for working examples
