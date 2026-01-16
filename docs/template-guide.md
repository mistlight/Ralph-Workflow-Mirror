# Template Customization Guide

This guide explains how to customize the AI agent prompts used by Ralph. All agent communication is done through templates stored as `.txt` files in `ralph-workflow/src/prompts/templates/`.

## Overview

The template system allows you to customize how Ralph communicates with AI agents without modifying code. Templates use a simple syntax for variables, partials, and comments.

### Template Features

- **Variables**: Insert dynamic content using `{{VARIABLE_NAME}}`
- **Partials**: Include shared template sections using `{{> partial_name}}`
- **Comments**: Add notes using `{# comment #}` syntax

## Template Location

All agent prompt templates are located at:

```
ralph-workflow/src/prompts/templates/
```

### Directory Structure

```
templates/
├── shared/                              # Shared partials (reusable sections)
│   ├── _critical_header.txt            # Warning about agent constraints
│   ├── _context_section.txt            # PROMPT and PLAN context
│   ├── _diff_section.txt               # DIFF display format
│   └── _output_checklist.txt           # Prioritized checklist format
├── commit_message_xml.txt              # Main commit message generation
├── commit_strict_json.txt              # Strict retry v1
├── commit_strict_json_v2.txt           # Strict retry v2 with examples
├── commit_ultra_minimal.txt            # Minimal prompt v1
├── commit_ultra_minimal_v2.txt         # Minimal prompt v2
├── commit_file_list_only.txt           # File paths only
├── commit_file_list_summary.txt        # File summary only
├── commit_emergency.txt                # Emergency fallback with diff
├── commit_emergency_no_diff.txt        # Absolute last resort
├── developer_iteration.txt             # Implementation mode prompt
├── planning.txt                        # Planning phase prompt
├── fix_mode.txt                        # Fix mode prompt
├── conflict_resolution.txt             # Merge conflict resolution
└── reviewer/
    └── templates/                      # Reviewer-specific templates
        ├── detailed_review_minimal.txt      # Detailed review, minimal context
        ├── detailed_review_normal.txt       # Detailed review, normal context
        ├── incremental_review_minimal.txt   # Incremental review, minimal context
        ├── incremental_review_normal.txt    # Incremental review, normal context
        ├── universal_review_minimal.txt     # Universal review, minimal context
        ├── universal_review_normal.txt      # Universal review, normal context
        ├── standard_review_minimal.txt      # Standard review, minimal context
        ├── standard_review_normal.txt       # Standard review, normal context
        ├── comprehensive_review_minimal.txt # Comprehensive review, minimal context
        ├── comprehensive_review_normal.txt  # Comprehensive review, normal context
        ├── security_review_minimal.txt      # Security review, minimal context
        └── security_review_normal.txt       # Security review, normal context
```

## Template Syntax

### Variables

Variables are inserted using double curly braces:

```
{{VARIABLE_NAME}}
```

The template engine will replace `{{VARIABLE_NAME}}` with the actual value provided at runtime.

### Partials

Partials allow you to include shared template sections:

```
{{> shared/_partial_name}}
```

Partials are resolved relative to the `templates/` directory and help maintain consistency across templates.

### Comments

Comments are ignored during rendering:

```
{# This is a comment and will not appear in the final prompt #}
```

## Available Variables

Each template has access to different variables depending on its context. Below is a comprehensive list of all variables used across templates.

### Common Variables

| Variable | Type | Description |
|----------|------|-------------|
| `PROMPT` | string | Content of PROMPT.md - the original user request |
| `PLAN` | string | Content of `.agent/PLAN.md` - the implementation plan |
| `DIFF` | string | Git diff of changes to be reviewed or committed |

### Developer Templates

#### `developer_iteration.txt`
**When used**: During implementation phase

| Variable | Description |
|----------|-------------|
| `PROMPT` | Original user request from PROMPT.md |
| `PLAN` | Implementation plan from `.agent/PLAN.md` |

#### `planning.txt`
**When used**: During planning phase before implementation

| Variable | Description |
|----------|-------------|
| `PROMPT` | User requirements from PROMPT.md |

### Commit Templates

All commit templates are used during the commit message generation phase, with different variants used as retries.

#### `commit_message_xml.txt` (main)
**When used**: Initial commit message generation attempt

| Variable | Description |
|----------|-------------|
| `DIFF` | Git diff to generate commit message for |

#### `commit_strict_json.txt`, `commit_strict_json_v2.txt`
**When used**: When initial attempt fails to produce valid XML output

| Variable | Description |
|----------|-------------|
| `DIFF` | Git diff to generate commit message for |

#### `commit_ultra_minimal.txt`, `commit_ultra_minimal_v2.txt`
**When used**: When stricter prompts also fail

| Variable | Description |
|----------|-------------|
| `DIFF` | Git diff to generate commit message for |

#### `commit_file_list_only.txt`
**When used**: When diff content is too large, fall back to file paths only

| Variable | Description |
|----------|-------------|
| `FILE_LIST` | List of changed file paths extracted from diff |

#### `commit_file_list_summary.txt`
**When used**: When even file list is too large, provide summary only

| Variable | Description |
|----------|-------------|
| `FILE_SUMMARY` | Summary statistics of changed files |

#### `commit_emergency.txt`, `commit_emergency_no_diff.txt`
**When used**: Final fallback attempts before giving up

| Variable | Description |
|----------|-------------|
| `DIFF` | Git diff (emergency_no_diff variant doesn't use this) |

### Fix Template

#### `fix_mode.txt`
**When used**: During fix mode when addressing review issues

| Variable | Description |
|----------|-------------|
| `PROMPT` | Original user request from PROMPT.md |
| `PLAN` | Implementation plan from `.agent/PLAN.md` |

### Reviewer Templates

All reviewer templates are used during the review phase. Each has minimal and normal variants.

#### Detailed Review Templates
**When used**: Detailed review phase with structured output

| Variable | Description |
|----------|-------------|
| `MODE` | Review mode identifier ("DETAILED REVIEW MODE") |
| `PROMPT` | Original user request from PROMPT.md |
| `PLAN` | Implementation plan from `.agent/PLAN.md` |
| `DIFF` | Git diff of changes to review |

#### Incremental Review Templates
**When used**: Incremental review focusing on recent changes

| Variable | Description |
|----------|-------------|
| `PROMPT` | Original user request from PROMPT.md |
| `PLAN` | Implementation plan from `.agent/PLAN.md` |
| `DIFF` | Git diff of changes to review |

#### Universal Review Templates
**When used**: Universal review for maximum agent compatibility

| Variable | Description |
|----------|-------------|
| `PROMPT` | Original user request from PROMPT.md |
| `PLAN` | Implementation plan from `.agent/PLAN.md` |
| `DIFF` | Git diff of changes to review |

#### Standard, Comprehensive, Security Review Templates
**When used**: Guided review with language-specific guidelines

| Variable | Description |
|----------|-------------|
| `PROMPT` | Original user request from PROMPT.md |
| `PLAN` | Implementation plan from `.agent/PLAN.md` |
| `DIFF` | Git diff of changes to review |

### Rebase Template

#### `conflict_resolution.txt`
**When used**: During rebase operations when merge conflicts occur

| Variable | Description |
|----------|-------------|
| `CONTEXT` | Formatted context section containing PROMPT.md and PLAN.md content |
| `CONFLICTS` | Formatted conflicts section showing conflicted files |

### Shared Partials

These are reusable sections included by other templates:

#### `_critical_header.txt`
**Purpose**: Warning about agent having NO access to external resources

**Variables**: None (static content)

#### `_context_section.txt`
**Purpose**: Provides PROMPT and PLAN context

**Variables**:
- `PROMPT` - Original user request
- `PLAN` - Implementation plan

#### `_diff_section.txt`
**Purpose**: Displays diff in code block format

**Variables**:
- `DIFF` - Git diff content

#### `_output_checklist.txt`
**Purpose**: Defines prioritized checklist output format

**Variables**: None (static template)

## Modifying Templates

### Step-by-Step Guide

1. **Locate the template**: Find the template file in `ralph-workflow/src/prompts/templates/`

2. **Edit the file**: Open the `.txt` file in your text editor

3. **Make changes**: Modify the prompt text, add or remove variables as needed

4. **Rebuild**: Run `cargo build` or `cargo build --release` to compile the changes

5. **Test**: Run Ralph to verify the changes work as expected

### Example: Modifying Commit Message Template

To customize how commit messages are generated:

1. Open `ralph-workflow/src/prompts/templates/commit_message_xml.txt`

2. Edit the prompt instructions. For example, to add a custom prefix:

   ```
   # Commit Message Generation

   Generate a conventional commit message for the following diff.

   {# Your custom instruction #}
   Include the project prefix "MYPROJECT:" at the start of the subject.

   Diff:
   {{DIFF}}

   Output format:
   <ralph-commit>
     <ralph-subject>type: description</ralph-subject>
   </ralph-commit>
   ```

3. Save and rebuild

### Adding Custom Variables

If you want to use a variable that isn't currently provided:

1. Check the source code in `ralph-workflow/src/prompts/` to see where the template is rendered
2. Add the variable to the `HashMap` passed to `template.render()`
3. Rebuild the project

For example, to add a `BRANCH_NAME` variable to commit templates, you would modify `commit.rs`:

```rust
let variables = HashMap::from([
    ("DIFF", diff_content.to_string()),
    ("BRANCH_NAME", get_current_branch().to_string()),  // Add this
]);
```

## Template Best Practices

1. **Keep prompts focused**: Clear, specific instructions work better than vague ones
2. **Use partials for shared content**: Avoid duplicating instructions across templates
3. **Document your changes**: Add comments explaining why you made changes
4. **Test thoroughly**: Verify your changes work across different scenarios
5. **Maintain output format compatibility**: If changing output format, update parsing code too

## Troubleshooting

### Template Doesn't Seem to Update

- Make sure you've rebuilt the project with `cargo build`
- Templates are embedded at compile time via `include_str!()`
- Verify you're editing the correct file

### Variable Not Replaced

- Check variable name matches exactly (case-sensitive)
- Verify the variable is being passed in the source code
- Ensure you're using `{{VARIABLE}}` syntax (not `{VARIABLE}`)

### Template Rendering Fails

- Check for syntax errors in variable names
- Verify partial references are correct
- Look for unbalanced braces or special characters

## Related Documentation

- [Variable Reference](template-variables.md) - Detailed variable documentation
- [Agent Compatibility](agent-compatibility.md) - Understanding how different agents respond
- [Git Workflow](git-workflow.md) - How Ralph handles git operations

## Support

If you encounter issues or have questions about template customization:

1. Check the existing template files for examples
2. Review the source code in `ralph-workflow/src/prompts/`
3. Open an issue on the Ralph GitHub repository
