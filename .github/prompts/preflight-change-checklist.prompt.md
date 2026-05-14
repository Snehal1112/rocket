---
name: "preflight-change-checklist"
description: "Run before coding in Rocket. Builds a quick pre-change checklist covering scope, required docs, architecture placement, risk points, and verification commands."
argument-hint: "Describe planned change and expected touched files"
agent: "agent"
---

Run a pre-change checklist for this repository before implementation.

Input to use:

- Planned change summary: ${input}

Checklist steps:

1. Scope map
- Infer likely files and layer: frontend, domain, infra, app, tauri, or mixed.

2. Required reads
- [CLAUDE.md](../../CLAUDE.md)
- Affected crate CLAUDE files
- If scope touches yml/collection/environment/request/auth/variable-resolution/related tauri commands: [OpenCollection spec](../../docs/superpowers/specs/opencollection-spec-reference.md)

3. Boundary checks
- DDD placement and trait-first flow
- Frontend guardrails if frontend touched
- Serde DTO vs persistence assumptions

4. Risks
- Top 3 to 6 risks, each with one mitigation

5. Verification plan
- Minimal targeted commands first

Output format:

- Scope summary
- Required reads
- Placement decision
- Risks and mitigations
- Verification commands
- Go or no-go recommendation for implementation start

Keep it concise and actionable.
