---
description: Run a pre-change checklist for Rocket before coding starts
argument-hint: "Describe planned change and likely files"
---

# Preflight Change Checklist

Run a short pre-change checklist before implementation.

Planned change summary: $ARGUMENTS

## Steps

1. Scope map
- Infer likely files and layer: frontend, domain, infra, app, tauri, or mixed.

2. Required reads
- [../../CLAUDE.md](../../CLAUDE.md)
- Relevant crate CLAUDE files
- If yml/collection/environment/request/auth/variable-resolution/related tauri scope: [../../docs/superpowers/specs/opencollection-spec-reference.md](../../docs/superpowers/specs/opencollection-spec-reference.md)

3. Boundary checks
- DDD placement, trait-first flow, frontend guardrails if needed, serde split assumptions

4. Risks
- Top 3 to 6 risks with one mitigation each

5. Verification
- Minimal targeted commands first

## Output format

- Scope summary
- Required reads
- Placement decision
- Risks and mitigations
- Verification commands
- Go or no-go recommendation

Keep output concise and actionable.
