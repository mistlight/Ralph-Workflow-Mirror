# Agent Compatibility Guide

This guide documents compatibility between Ralph and various AI coding agents for the review process. Ralph's review phase is designed to be agent-agnostic in its prompts, but different agents may have varying levels of success due to differences in JSON output format, tool execution behavior, and other agent-specific quirks.

> **⚠️ Important Compatibility Note**: GLM, ZhipuAI, Qwen, and DeepSeek agents have known compatibility issues with review tasks. While Ralph automatically applies workarounds (universal prompt), success rates may vary. **For best results, consider using Claude Code or Codex as the reviewer.** You can override the reviewer agent with `--reviewer-agent claude` or `--reviewer-agent codex`.

> **Note**: Ralph now includes a **Universal Review Prompt** that automatically activates for agents with known compatibility issues (GLM, ZhipuAI, Qwen, DeepSeek). This simplified prompt improves success rates with these agents.

## Table of Contents

- [Known Working Agents](#known-working-agents)
- [Agents with Known Issues](#agents-with-known-issues)
- [Configuration Recommendations](#configuration-recommendations)
- [Universal Review Prompt](#universal-review-prompt)
- [Troubleshooting Guide](#troubleshooting-guide)

## Compatibility Matrix

| Agent | Developer Role | Reviewer Role | Notes |
|-------|---------------|---------------|-------|
| **Claude Code** | ✅ Excellent | ✅ Excellent | Best overall compatibility |
| **Codex (OpenAI)** | ✅ Excellent | ✅ Excellent | Great for security-focused reviews |
| **OpenCode** | ✅ Good | ✅ Good | Requires `opencode` parser |
| **CCS/GLM** | ✅ Good | ⚠️ Partial | Universal prompt auto-applied |
| **ZhipuAI/ZAI** | ✅ Good | ⚠️ Partial | Universal prompt auto-applied |
| **Qwen** | ✅ Good | ⚠️ Partial | Universal prompt auto-applied |
| **DeepSeek** | ✅ Good | ⚠️ Partial | Universal prompt auto-applied |
| **Aider** | ✅ Good | ⚠️ Limited | Use `generic` parser |
| **Gemini CLI** | ✅ Good | ⚠️ Experimental | Parser support less mature |

### Legend
- ✅ **Excellent** - Works perfectly, recommended
- ✅ **Good** - Works well with minor caveats
- ⚠️ **Partial** - Works with automatic workarounds, may have reduced capability
- ⚠️ **Limited** - Works but output may be less structured
- ⚠️ **Experimental** - Not thoroughly tested

## Known Working Agents

These agents have been tested and work well with Ralph's review process:

### Claude Code (Recommended)

**Status**: ✅ Fully Compatible

**Configuration**:
```toml
# In ~/.config/ralph/agents.toml or .agent/agents.toml
[agents.claude]
name = "claude"
command = "claude"
args = ["--json", "--full-auto", "--prompt", "<PROMPT>"]
json_parser = "claude"
```

**Recommended Settings**:
- `review_depth`: `standard` or `comprehensive`
- `reviewer_context`: `minimal` or `normal`
- `reviewer_reviews`: `2`

**Notes**:
- Best overall compatibility with Ralph
- Produces structured, well-formatted ISSUES.md
- Handles file writes and tool execution reliably
- Good at following complex review guidelines

### Codex (OpenAI)

**Status**: ✅ Fully Compatible

**Configuration**:
```toml
[agents.codex]
name = "codex"
command = "codex"
args = ["exec", "--json", "--full-auto", "<PROMPT>"]
json_parser = "codex"
```

**Recommended Settings**:
- `review_depth`: `standard` or `security`
- `reviewer_context`: `normal`
- `reviewer_reviews`: `2`

**Notes**:
- Excellent for security-focused reviews
- Good at identifying code quality issues
- May be more verbose than Claude Code

### OpenCode

**Status**: ✅ Compatible with Proper Configuration

**Configuration**:
```toml
[agents.opencode]
name = "opencode"
command = "opencode"
args = ["--json", "<PROMPT>"]
json_parser = "opencode"
```

**Recommended Settings**:
- `review_depth`: `standard`
- `reviewer_context`: `normal`
- `reviewer_reviews`: `1` or `2`

**Notes**:
- Requires the `opencode` parser (not interchangeable)
- May produce less structured output than Claude/Codex
- Works best with explicit review guidelines

## Agents with Known Issues

These agents have known compatibility issues with Ralph's review process:

### CCS/GLM

**Status**: ⚠️ Partial Compatibility - Automatic Workarounds Applied

**Known Issues**:
1. **Permission Errors**: GLM has known issues with file write permissions that can cause exit code 1 errors
2. **JSON Format Differences**: GLM may output JSON in a slightly different format than expected
3. **Tool Execution Failures**: Some tool calls may fail silently
4. **Prompt Complexity**: GLM may struggle with complex structured prompts

**Automatic Workarounds (Ralph v0.2.5+)**:
- **Universal Prompt**: Ralph automatically uses a simplified review prompt for GLM
- **Fast Fallback**: GLM exit code 1 errors now trigger immediate fallback (no indefinite retries)
- **Pre-flight Warning**: Ralph warns you before running review with GLM

**Manual Workaround Configuration**:
```toml
[agents.ccs_glm]
name = "ccs/glm"
command = "ccs"
args = ["glm", "--output-format=stream-json", "--dangerously-skip-permissions", "<PROMPT>"]
json_parser = "generic"  # Use generic parser as fallback
```

**Alternative - Use Different Reviewer**:
```bash
# Override the reviewer agent on the command line
ralph --reviewer-agent codex

# Or skip review entirely
RALPH_REVIEWER_REVIEWS=0 ralph
```

**Notes**:
- Universal prompt improves success rate but GLM may still fail
- Consider using GLM as developer only, not reviewer
- The `--dangerously-skip-permissions` flag is often required
- Exit code 1 errors with GLM are now classified as `AgentSpecificQuirk` (triggers fallback)

### ZhipuAI / ZAI

**Status**: ⚠️ Partial Compatibility - Automatic Workarounds Applied

**Known Issues**:
1. Similar to GLM (related model family)
2. May struggle with complex review guidelines
3. Exit code 1 errors common

**Automatic Workarounds**:
- Universal review prompt automatically applied
- Fast fallback on failures

**Workaround Configuration**:
```bash
# Use a different reviewer
ralph --reviewer-agent codex

# Or try generic parser
ralph --reviewer-json-parser generic
```

### Qwen / DeepSeek

**Status**: ⚠️ Experimental - Automatic Workarounds Applied

**Known Issues**:
1. These models may have weaker instruction-following capabilities
2. May not follow complex multi-section prompts reliably

**Automatic Workarounds**:
- Universal review prompt automatically applied
- Simplified output format with examples

**Recommendation**: Use for development, consider Claude/Codex for review

### Aider

**Status**: ⚠️ Limited Compatibility

**Known Issues**:
1. **Different Output Format**: Aider uses a generic text-based output format
2. **No Native JSON Streaming**: Requires generic parser
3. **Different Tool Semantics**: Tool handling differs from Claude/Codex

**Configuration**:
```toml
[agents.aider]
name = "aider"
command = "aider"
args = ["--yes", "<PROMPT>"]
json_parser = "generic"
```

**Notes**:
- Review output may be less structured
- May not populate all ISSUES.md fields correctly
- Consider using for development only

### Gemini CLI

**Status**: ⚠️ Experimental

**Configuration**:
```toml
[agents.gemini]
name = "gemini"
command = "gemini"
args = ["--json", "<PROMPT>"]
json_parser = "gemini"
```

**Notes**:
- Parser support is available but less mature
- May have issues with complex review guidelines
- Consider using `json_parser = "generic"` if issues arise

## Configuration Recommendations

### Per-Agent Configuration

For agents that have known compatibility issues, Ralph provides several configuration options to improve success rates:

#### Option 1: Force Universal Prompt

Set the `RALPH_REVIEWER_UNIVERSAL_PROMPT` environment variable to force the simplified review prompt for any agent:

```bash
# Force universal prompt for all agents
RALPH_REVIEWER_UNIVERSAL_PROMPT=1 ralph

# Force universal prompt for a specific reviewer
RALPH_REVIEWER_UNIVERSAL_PROMPT=1 ralph --reviewer-agent ccs/glm
```

Or add to `~/.config/ralph-workflow.toml`:
```toml
[general]
force_universal_prompt = true
```

#### Option 2: Override JSON Parser

Use a different parser for the reviewer agent:

```bash
# Use generic parser with any agent
ralph --reviewer-agent ccs/glm --reviewer-json-parser generic

# Or via environment variable
RALPH_REVIEWER_JSON_PARSER=generic ralph --reviewer-agent ccs/glm
```

#### Option 3: Use a Different Reviewer

The most reliable option is to use Claude Code or Codex as the reviewer while keeping GLM/CCS as the developer:

```bash
# Use GLM for development, Claude for review
ralph --developer-agent ccs/glm --reviewer-agent claude
```

#### Option 4: Skip Review Entirely

If the review process is not critical for your use case:

```bash
# Skip review phase
RALPH_REVIEWER_REVIEWS=0 ralph
```

### Context Level Recommendations

### JSON Parser Selection

The `json_parser` setting controls how Ralph interprets the agent's output:

| Parser | Best For | Notes |
|--------|----------|-------|
| `claude` | Claude Code | Native parser, most reliable |
| `codex` | OpenAI Codex | Native parser |
| `opencode` | OpenCode | Required for OpenCode |
| `gemini` | Gemini CLI | Native parser, experimental |
| `generic` | Any agent | Fallback for unsupported agents |

### Review Depth Settings

| Setting | Description | Recommended For |
|---------|-------------|-----------------|
| `standard` | Balanced review | Most agents and use cases |
| `comprehensive` | Thorough review with language-specific checks | Claude Code, Codex |
| `security` | Security-focused review | Codex, security audits |
| `incremental` | Review only changed files | Fast feedback cycles |

### Context Level Recommendations

| Setting | Description | When to Use |
|---------|-------------|-------------|
| `minimal` | Only changed files | Large codebases, slow agents |
| `normal` | Changed files + dependencies | Default setting |
| `full` | Entire codebase | Small projects, thorough reviews |

## Universal Review Prompt

**Available in**: Ralph v0.2.5+

The Universal Review Prompt is a simplified, agent-agnostic review prompt designed to work with AI models that have weaker instruction-following capabilities or known compatibility issues with complex structured prompts.

### When is it Used?

Ralph automatically uses the Universal Review Prompt when the reviewer agent is:
- `ccs/glm` or any agent containing "glm"
- ZhipuAI agents (containing "zhipuai" or "zai")
- Qwen agents
- DeepSeek agents

You'll see a log message when it's activated:
```
ℹ Using universal/simplified review prompt for agent 'ccs/glm' (better compatibility)
```

### How it Differs

| Feature | Standard Prompt | Universal Prompt |
|---------|----------------|------------------|
| Language | Technical terms (context contamination, isolation mode) | Simple direct language |
| Structure | Multi-section with numbered lists | Simple task description |
| Output Format | Implied from context | Explicit template with examples |
| Examples | None | Full example of ISSUES.md format |
| Severity Levels | Described in detail | Simple list with examples |

### Example Output Format

The Universal Prompt includes this explicit example:

```markdown
# Code Review Issues

## Critical Issues
- [ ] [src/main.rs:42] Null pointer dereference risk

## High Priority
- [ ] [src/auth.rs:15] Missing input validation

## Medium Priority
- [ ] [src/utils.rs:78] Function may return null

## Low Priority
- [ ] [src/config.rs:10] Missing documentation
```

And specifies: "If no issues found, write exactly: `No issues found.`"

### Customization

If you want to force the Universal Prompt for a different agent, you can:

1. **Environment variable**: Set `RALPH_REVIEWER_UNIVERSAL_PROMPT=1` to force universal prompt for all agents
2. **Config file**: Add `force_universal_prompt = true` to the `[general]` section in `~/.config/ralph-workflow.toml`
3. **Source code**: Modify the `should_use_universal_prompt` function in `src/phases/review.rs`

Example:
```bash
# Force universal prompt for any agent
RALPH_REVIEWER_UNIVERSAL_PROMPT=1 ralph --reviewer-agent claude
```

## Why Do Some Agents Fail?

Understanding why certain AI agents struggle with the review process can help you choose the right agent for your needs.

### Technical Causes

1. **JSON Output Format Differences**
   - Different agents structure their JSON output differently
   - Ralph expects specific event formats (e.g., `text_delta`, `tool_use`)
   - The `generic` parser can handle many variations but may miss some events

2. **Tool Execution Behavior**
   - Review agents need to reliably produce the expected outputs (issues/fixes) in the configured format
   - The orchestrator may write workflow files on the agent’s behalf, but agents still need compatible tool/IO behavior
   - Some agents have permission issues or different tool semantics (notably some GLM/CCS setups)

3. **Prompt Complexity Handling**
   - AI models vary in their ability to follow complex, multi-section prompts
   - The Universal Review Prompt simplifies instructions for models with weaker instruction-following
   - Some models may ignore parts of complex prompts or misinterpret structured guidelines

4. **Context Window and Processing**
   - Reviewing code requires understanding large codebases
   - Models with smaller context windows may miss important details
   - The `reviewer_context` setting can help manage this

### How Ralph Handles These Issues

Ralph includes several automatic mitigations:

1. **Universal Review Prompt**: Automatically activates for GLM, ZhipuAI, Qwen, and DeepSeek
2. **Fast Fallback**: Known-problematic agents trigger quick fallback instead of retries
3. **Pre-flight Warnings**: Users are warned before running review with problematic agents
4. **Post-flight Validation**: ISSUES.md is validated after review to catch issues early
5. **Error Classification**: Exit codes and stderr are analyzed to determine recovery strategy

### Recommendation Matrix

| Use Case | Recommended Agent | Why |
|----------|------------------|-----|
| **Best Overall** | Claude Code (`claude`) | Excellent compatibility, reliable output |
| **Security Review** | Codex (`codex`) | Strong security analysis capabilities |
| **Cost-Effective** | CCS/GLM (`ccs/glm`) | Good for development, use different reviewer |
| **Testing Alternatives** | Any + `--reviewer-json-parser generic` | Generic parser works with most agents |

## Troubleshooting Guide

### Review Agent Fails with Exit Code 1

**Symptoms**: Agent exits with code 1 repeatedly with "AgentSpecificQuirk" error message.

**Possible Causes**:
1. **Permission denied** (common with GLM/CCS)
2. **Tool execution failure**
3. **Agent-specific quirk**
4. **Prompt complexity** (agent can't follow complex instructions)

**Solutions**:

1. **Check if universal prompt is activated**:
   Look for this log message:
   ```
   ℹ Using universal/simplified review prompt for agent 'ccs/glm' (better compatibility)
   ```

2. **Check agent logs**:
   ```bash
   cat .agent/logs/reviewer_review_1_<agent>.log
   ```

3. **Try a different parser**:
   ```bash
   ralph --reviewer-json-parser generic
   ```

4. **Use a different reviewer agent**:
   ```bash
   ralph --reviewer-agent codex
   # or
   ralph --reviewer-agent claude
   ```

5. **Skip review entirely**:
   ```bash
   RALPH_REVIEWER_REVIEWS=0 ralph
   ```

6. **Enable debug logging**:
   ```bash
   ralph --verbosity debug
   ```

**Note**: As of Ralph v0.2.5, GLM and similar agents with exit code 1 errors now trigger immediate fallback to the next agent instead of retrying indefinitely.

### ISSUES.md Not Created After Review

**Symptoms**: Review completes but no ISSUES.md file is created.

**Possible Causes**:
1. **Agent failed silently**
2. **Parser ignored all events**
3. **Agent used different output format**

**Solutions**:

1. **Check pre-flight validation output** - Ralph now warns you before running review with problematic agents:
   ```
   ⚠ Note: Agent 'ccs/glm' may have compatibility issues with review tasks.
   ℹ If review fails, consider these workarounds:
   ```

2. **Check post-flight validation** - Look for:
   ```
   ⚠ Post-flight check: ISSUES.md not found after review.
   ```

3. **Review agent logs** for errors:
   ```bash
   cat .agent/logs/reviewer_review_1_<agent>.log
   ```

4. **Try with `--verbosity debug`** for more diagnostic information

5. **Switch to a known-compatible agent** like Claude or Codex

### Parser Ignores Many Events

**Symptoms**: Warning message "Parser ignored >50% of events".

**Possible Causes**:
1. **Wrong parser selected**
2. **Agent outputs unexpected JSON format**
3. **Agent-specific quirk**

**Solutions**:
1. **Check agent compatibility** in this guide
2. **Try generic parser**:
   ```toml
   json_parser = "generic"
   ```
3. **Check raw agent output**:
   ```bash
   cat .agent/logs/reviewer_review_1_<agent>.log
   ```

### Review Finds No Issues But Should

**Symptoms**: ISSUES.md created but empty or declares "no issues found" when issues exist.

**Possible Causes**:
1. **Context too limited** (`reviewer_context = "minimal"`)
2. **Review depth too shallow**
3. **Agent not following guidelines**

**Solutions**:
1. **Increase context**:
   ```toml
   reviewer_context = "normal"  # or "full"
   ```

2. **Use comprehensive review**:
   ```toml
   review_depth = "comprehensive"
   ```

3. **Try a different agent** known for thorough reviews

## Contributing

If you test Ralph with an agent not listed here, please contribute your findings:

1. **Test the agent** with both development and review roles
2. **Document any issues** encountered
3. **Share working configurations** (anonymized)
4. **Submit a PR** to update this guide

## Additional Resources

- **Main README**: [../README.md](../README.md)
- **Configuration Guide**: See `ralph --help` for CLI options
- **Issue Tracker**: Report compatibility issues on GitHub

---

Last updated: 2026-01-12
