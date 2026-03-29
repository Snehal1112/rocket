# Git Commit Message Generation Prompt

## System Instructions for AI Assistants

When asked to create a Git commit message, follow this comprehensive process:

### Step 1: Analyze Current Git State
Execute these commands in parallel to gather context:
```bash
git status                    # View staged changes and repository state
git diff --staged --stat     # See summary of changed files
git diff --staged            # Examine specific changes being committed
git log --oneline -5         # Check recent commit message patterns for consistency
```

**Analysis Checklist:**
- [ ] Do all staged changes relate to a single logical purpose?
- [ ] Are changes spread across unrelated components? (auth + calendar, etc.)
- [ ] Is this a mix of different change types? (bugfix + feature + docs)
- [ ] Can changes be split into multiple focused commits?

**If multiple unrelated changes detected:**
→ STOP and recommend splitting commits (see "Multiple Unrelated Changes" section)
→ Provide specific split strategy before creating commit message

### Step 2: Apply Git Commit Best Practices

Follow the **Seven Rules of Great Git Commit Messages** (based on Chris Beams' guidelines):

1. **Separate subject from body** with a blank line
2. **Limit subject line to 50 characters** (hard limit: never exceed)
3. **Capitalize the subject line** (first letter only)
4. **Do not end subject line with a period**
5. **Use imperative mood** in subject line (complete: "If applied, this commit will...")
6. **Wrap body text at 72 characters** for readability
7. **Use body to explain "what" and "why"**, not "how"

### Step 3: Commit Message Structure Template

#### Standard Format:
```
<Type>: <Brief description in imperative mood>
[blank line]
<Optional body explaining what and why>
[blank line]
<Optional footer with issue references and attribution>
```

#### Conventional Commits Format (Optional):
```
<type>(<scope>): <subject>
[blank line]
<Optional body>
[blank line]
<Optional footer>
```

#### Subject Line Types (Choose Most Appropriate):
- **Add**: New feature, file, or functionality
- **Update**: Modify existing functionality
- **Fix**: Bug fix or error correction
- **Remove**: Delete code, files, or features
- **Refactor**: Code restructuring without behavior change
- **Improve**: Enhancement to existing functionality
- **Implement**: Complete implementation of planned feature
- **Configure**: Setup, configuration, or tooling changes
- **Docs**: Documentation only changes
- **Test**: Add or modify tests
- **Chore**: Maintenance tasks (dependencies, build, etc.)
- **Perf**: Performance improvements
- **Style**: Code style changes (formatting, no logic change)
- **CI**: CI/CD configuration changes

#### Scope/Component Guidelines (Optional):
Add scope to indicate affected component:
```
fix(auth): prevent session timeout during file upload
feat(calendar): add recurring event support
docs(readme): update installation instructions
chore(deps): update composer dependencies
perf(mapi): optimize store access queries
```

Common scopes for this project:
- `server/util`, `server/mapi`, `server/auth`
- `client/calendar`, `client/mail`, `client/contacts`
- `build`, `config`, `deps`, `ci`

#### Subject Line Guidelines:
- Start with action verb in imperative mood
- Be specific but concise
- Avoid generic words like "changes" or "updates"
- Focus on the primary change/benefit
- No unnecessary punctuation (aligns with code review philosophy)
- Use inline values where helpful: `username=%s` instead of descriptions

#### Body Guidelines (When Needed):
- Explain **motivation** for the change
- **Contrast** with previous behavior
- Note any **side effects** or **consequences**
- Reference **relevant issues** or **documentation**
- Use **bullet points** for multiple items
- Keep paragraphs focused and concise
- Avoid redundant phrases - be direct and clear
- Add context as needed, but prefer brevity

#### Footer Guidelines:

**Issue References (GitLab format):**
```
Fixes: #613
Relates to: #587, #601
Blocks: #620
Closes: #613
```

**Breaking Changes:**
```
BREAKING CHANGE: Remove support for PHP 7.3

Migration required: Update to PHP 7.4+ before upgrading.
See migration guide at docs/php74-migration.md
```

### Step 4: Quality Checklist

Before finalizing, verify the commit message:
- [ ] Subject line completes: "If applied, this commit will..."
- [ ] Subject ≤ 50 characters, no ending period
- [ ] Uses imperative mood (Add, Fix, Update, not Added, Fixed, Updated)
- [ ] Body wraps at 72 characters (if present)
- [ ] Explains why the change was made
- [ ] Follows project's existing commit style
- [ ] Is clear to someone unfamiliar with the context
- [ ] Includes proper issue references (Fixes: #XXX format for GitLab)
- [ ] No unnecessary punctuation or verbose descriptions
- [ ] Uses inline values where appropriate (username=%s)

### Step 5: Implementation

Create commit using heredoc format for proper formatting:
```bash
git commit -m "$(cat <<'EOF'
Subject line here

Optional body paragraph explaining the why and what.
Can include multiple paragraphs if needed.

- Key feature 1
- Key feature 2
- Important consideration

Fixes: #613
EOF
)"
```

### Examples of Well-Formed Commit Messages

#### Simple commit (subject only):
```
Fix user authentication timeout issue
```

#### With scope (Conventional Commits):
```
fix(auth): prevent token expiration during active sessions
```

#### Project-specific examples:
```
Compress log messages per code review feedback

Improve error logging clarity in util.php

Restore shared folder validation with non-fatal error handling
```

#### Complex commit with full structure:
```
Add error handling for failed IPM_SUBTREE access

Adds validation check for rootFolder before attempting to query
hierarchy table. When folder access fails (typically due to
insufficient permissions), logs detailed error information and
returns empty array instead of proceeding with invalid folder
reference.

This prevents potential MAPI errors when users lack permissions
to open the IPM_SUBTREE of a store.

Fixes: #613
```

#### Refactoring commit:
```
Refactor authentication module for better testability

Separates authentication logic into smaller, focused functions
to improve unit test coverage and maintainability. No functional
changes to user-facing behavior.

- Extract token validation logic
- Simplify error handling flow
- Add comprehensive JSDoc documentation

Relates to: #587
```

#### Breaking change commit:
```
Remove legacy MAPI connection pooling

The connection pooling implementation had race conditions and
is replaced by the new session management in php-mapi 2.0.

BREAKING CHANGE: Remove MAPI_ENABLE_POOLING configuration option

Migration: Remove MAPI_ENABLE_POOLING from config.php as it no
longer has any effect. Connection management is now automatic.

Fixes: #650
```

### Common Anti-Patterns to Avoid

❌ **Bad Examples:**
```
Fixed stuff
Updated files
Changes
WIP
More work on feature
Bug fix
Final fix for issue
Applied code review feedback
```

✅ **Good Examples:**
```
Fix memory leak in image processing pipeline
Update user permissions validation logic
Add automated backup scheduling feature
Remove deprecated payment gateway integration
Compress log messages per code review feedback
```

### Context-Specific Guidelines

#### For Feature Additions:
- Focus on user benefit or business value
- Mention key technical approaches if relevant
- Note any configuration or migration needs
- Use `Add` or `Implement` for completely new features
- Use `feat()` scope for Conventional Commits

#### For Bug Fixes:
- Describe the problem being solved
- Avoid implementation details unless critical
- Reference error conditions or symptoms
- Use `Fix` prefix consistently
- Use `fix()` scope for Conventional Commits
- Include issue reference: `Fixes: #XXX`

#### For Refactoring:
- Emphasize that behavior is unchanged
- Explain motivation (performance, maintainability, etc.)
- Note any API changes affecting other developers
- Use `Refactor` prefix
- Use `refactor()` scope for Conventional Commits

#### For Configuration/Tooling:
- Explain the benefit or necessity
- Note any developer workflow changes
- Include setup instructions if complex
- Use `Configure`, `Chore`, or `CI` prefix
- Use `chore()` or `ci()` scope for Conventional Commits

#### For Documentation:
- Specify what documentation was changed
- Use `Docs` or `Update` prefix
- Use `docs()` scope for Conventional Commits
- Keep it brief - documentation is self-explanatory

#### For Performance:
- Include metrics or benchmarks when applicable
- Explain the optimization approach briefly
- Use `Improve` or `Perf` prefix
- Use `perf()` scope for Conventional Commits

### Code Review Philosophy Alignment

Apply the same principles from code review to commit messages:
- **No unnecessary punctuation** - waste of space
- **Short messages with inline values** - `username=%s` not "the username"
- **Context in body/comments** - not in subject line
- **Be direct and clear** - avoid verbose descriptions

Examples:
```
❌ Failed to open IPM_SUBTREE.
✅ Failed to open IPM_SUBTREE username=%s

❌ This commit comprehensively updates the error handling system.
✅ Improve error handling for failed store access

❌ The MAPI error is now being logged with all the details.
✅ Log MAPI errors with error code and context
```

### Special Considerations

#### Multiple Unrelated Changes - Commit Splitting Strategy

**When to Split:**
- Changes affect unrelated components (e.g., auth + calendar)
- Mix of bug fixes and new features
- Changes solve different problems or issues
- Code changes + documentation updates (can be separate)
- Refactoring + functional changes

**When to Keep Together:**
- Changes are part of same logical feature
- Changes are interdependent (one requires the other)
- Small related tweaks (formatting + minor logic in same function)
- Code review feedback addressing single concern

**How to Split Commits:**

1. **Analyze the changes:**
   ```bash
   git diff --staged --stat     # See all changed files
   git diff --staged            # Review actual changes
   ```

2. **Unstage everything:**
   ```bash
   git reset HEAD               # Unstage all files
   ```

3. **Stage and commit related changes separately:**
   ```bash
   # Commit 1: Auth changes
   git add server/includes/core/class.mapisession.php
   git add server/includes/util.php
   git commit -m "Fix authentication timeout handling"

   # Commit 2: Calendar changes
   git add server/includes/modules/class.calendarmodule.php
   git commit -m "Add recurring event validation"

   # Commit 3: Documentation
   git add README.md
   git add docs/calendar.md
   git commit -m "Update calendar documentation"
   ```

4. **For partial file changes (advanced):**
   ```bash
   git add -p file.php          # Interactive staging - choose hunks
   git commit -m "First logical change"

   git add file.php             # Stage remaining changes
   git commit -m "Second logical change"
   ```

**Example Split Decision:**

❌ **Bad - Single commit for unrelated changes:**
```
Fix auth timeout and add calendar validation

- Fix session timeout in auth module
- Add recurring event validation
- Update calendar documentation
```

✅ **Good - Three focused commits:**
```
Commit 1: Fix authentication session timeout
Commit 2: Add recurring event validation for calendar
Commit 3: Update calendar module documentation
```

**AI Assistant Behavior:**
When analyzing `git diff --staged`, if you detect multiple unrelated changes:
1. **Alert the user** that changes should be split
2. **Suggest logical groupings** based on components/concerns
3. **Provide specific git commands** for each commit
4. **Create separate commit messages** for each group
5. **Ask user to confirm** the split strategy before proceeding

#### Other Considerations

- **Breaking Changes**: Use `BREAKING CHANGE:` footer and explain migration path
- **Security Fixes**: Be mindful of not exposing vulnerability details in public commits
- **Performance**: Include relevant metrics or benchmarks when applicable
- **Dependencies**: Note version changes and reason for update
- **Merge Commits**: Follow project convention (usually auto-generated by GitLab)

### Integration with Development Workflow

This prompt should be used:
- Before every commit to maintain consistency
- When reviewing others' commit messages
- As part of code review process
- When teaching Git best practices to team members

### GitLab-Specific Features

**Issue References:**
- `Fixes: #XXX` - Closes the issue when merged
- `Closes: #XXX` - Same as Fixes
- `Relates to: #XXX` - Creates a reference without closing
- `Blocks: #XXX` - Indicates blocking relationship

**Multiple Issues:**
```
Fixes: #613
Relates to: #587, #601
```

**Merge Request References:**
```
Related to !432
```

Remember: Great commit messages are a gift to your future self and your teammates. They provide crucial context that code alone cannot convey. Keep them concise, clear, and consistent with project standards.
