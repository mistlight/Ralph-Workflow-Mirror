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
├── shared/                               # Shared partials (reusable sections)
│   ├── _critical_header.txt             # Warning about agent constraints
│   ├── _context_section.txt             # PROMPT and PLAN context
│   ├── _diff_section.txt                # DIFF display format
│   ├── _output_checklist.txt            # Prioritized checklist format
│   ├── _safety_no_execute.txt           # Safety constraints for agents
│   └── _unattended_mode.txt             # Unattended mode instructions
├── commit_message_xml.txt               # Normal commit message (XML format)
├── commit_simplified.txt                # Simplified commit strategy
├── commit_xsd_retry.txt                 # XSD validation retry (in-session)
├── conflict_resolution.txt              # Merge conflict resolution
├── conflict_resolution_fallback.txt     # Fallback conflict resolution
├── developer_iteration_xml.txt          # Implementation mode prompt
├── developer_iteration_continuation_xml.txt  # Continuation prompt
├── developer_iteration_xsd_retry.txt    # Dev XSD validation retry
├── fix_mode_xml.txt                     # Fix mode prompt
├── fix_mode_xsd_retry.txt               # Fix mode XSD validation retry
├── planning_xml.txt                     # Planning phase prompt
├── planning_xsd_retry.txt               # Planning XSD validation retry
├── review_xml.txt                       # Code review prompt
├── review_xsd_retry.txt                 # Review XSD validation retry
└── TEMPLATE_GUIDE.md                    # Internal reference for template developers
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

#### `developer_iteration_xml.txt`
**When used**: During implementation phase

| Variable | Description |
|----------|-------------|
| `PROMPT` | Original user request from PROMPT.md |
| `PLAN` | Implementation plan from `.agent/PLAN.md` |

#### `planning_xml.txt`
**When used**: During planning phase before implementation

| Variable | Description |
|----------|-------------|
| `PROMPT` | User requirements from PROMPT.md |

### Commit Templates

Commit templates are used during the commit message generation phase, with different strategies tried as fallbacks.

#### `commit_message_xml.txt` (Normal Strategy)
**When used**: First attempt at commit generation

| Variable | Description |
|----------|-------------|
| `DIFF` | Git diff to analyze |

#### `commit_simplified.txt` (Simplified Strategy)
**When used**: Second attempt with more direct instructions

| Variable | Description |
|----------|-------------|
| `DIFF` | Git diff to analyze |

#### `commit_xsd_retry.txt` (XSD Validation Retry)
**When used**: In-session retry when XSD validation fails

| Variable | Description |
|----------|-------------|
| `XSD_ERROR` | XSD validation error message |

**Retry Strategy**: The commit generation uses a two-strategy approach (Normal, Simplified) with in-session XSD validation retries. Each strategy allows up to 5 in-session retries when XSD validation fails, with detailed error feedback provided to the agent.

### Fix Template

#### `fix_mode_xml.txt`
**When used**: During fix mode when addressing review issues

| Variable | Description |
|----------|-------------|
| `PROMPT` | Original user request from PROMPT.md |
| `PLAN` | Implementation plan from `.agent/PLAN.md` |

### Reviewer Template

#### `review_xml.txt`
**When used**: During code review phase

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

#### `_safety_no_execute.txt`
**Purpose**: Safety constraints preventing dangerous operations

**Variables**: None (static content)

#### `_unattended_mode.txt`
**Purpose**: Instructions for unattended/autonomous mode

**Variables**: None (static content)

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

- [Agent Compatibility](agent-compatibility.md) - Understanding how different agents respond
- [Git Workflow](git-workflow.md) - How Ralph handles git operations

## Support

If you encounter issues or have questions about template customization:

1. Check the existing template files for examples
2. Review the internal `ralph-workflow/src/prompts/templates/TEMPLATE_GUIDE.md`
3. Review the source code in `ralph-workflow/src/prompts/`
4. Open an issue on the Ralph repository
