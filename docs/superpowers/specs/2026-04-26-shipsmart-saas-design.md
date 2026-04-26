# ShipSmart — SaaS Design Spec

**Date:** 2026-04-26
**Author:** Snehal Dangroshiya

---

## Overview

ShipSmart is a SaaS tool that scores engineering complexity and API design quality — giving engineers instant feedback on PRs and giving engineering managers team-level trend visibility. It embeds the specific knowledge of an engineer-turned-EM: the instinct to find the minimum-intervention fix that unblocks shipping velocity.

---

## Users

| Role | Primary Need |
|---|---|
| Senior Engineer / Tech Lead | Instant complexity feedback on every PR, before review |
| Engineering Manager | Team-level complexity trends, high-risk PR visibility, API debt tracking |

---

## What It Analyzes

### 1. Pull Request Complexity
Triggered automatically via GitHub / GitLab webhook on PR open/update.

**Static rules (scoring):**
- Cyclomatic complexity delta per file
- Files changed vs lines changed ratio (churn signal)
- Number of responsibilities per class/service (SRP violation heuristic)
- Nesting depth (max depth across changed files)
- Public surface area growth (new exported symbols)

**Score:** 0–100. 0–40 = low, 41–65 = medium, 66–100 = high complexity.

**LLM layer (explanation):**
- Plain-English summary of what makes this PR complex
- Top 3 specific, actionable fix suggestions referencing actual file/class names

### 2. API Design Health
Triggered by manual OpenAPI spec upload (JSON or YAML).

**Static rules (scoring):**
- Naming consistency (camelCase vs snake_case mixed)
- Endpoint count per resource (too many doing similar things)
- Response payload nesting depth
- Versioning presence (`/v1/`, `/v2/` etc.)
- HTTP method correctness (GET mutating state, etc.)

**Score:** 0–100 per spec. Broken into sub-scores per category.

**LLM layer (explanation):**
- Summary of top design issues
- Suggested renames, restructuring, or consolidation

---

## Architecture

### Components

```
GitHub/GitLab Webhook
        │
        ▼
  Webhook Service         ← receives PR events, extracts diffs
        │
        ▼
  Analyzer Engine
  ├── Static Rules        ← deterministic scoring (fast, cheap)
  └── LLM Explainer       ← Claude/GPT-4 for summary + fixes
        │
        ▼
  Results Store           ← Postgres: scores, explanations, metadata
        │
   ┌────┴────┐
   ▼         ▼
 Bot       Dashboard
(PR comment) (web app)
```

### Tech Stack

| Layer | Choice | Reason |
|---|---|---|
| Backend | Node.js (TypeScript) | Fast iteration, strong GitHub SDK ecosystem |
| Analyzer | Custom rules + Claude API | Hybrid: cheap scoring + smart explanations |
| Database | Postgres | Relational — scores link to PRs, repos, teams |
| Bot | GitHub App / GitLab integration | Native PR comment posting |
| Dashboard | React + shadcn/ui | Consistent, accessible component library |
| Auth | GitHub OAuth | Zero friction — engineers already have GitHub |
| Hosting | Railway or Render | Simple deploy, managed Postgres included |

---

## Data Flow

1. Engineer opens a PR → GitHub fires webhook to ShipSmart
2. Webhook service extracts the diff and repo metadata
3. Static rules engine scores the diff → produces a 0–100 score
4. If score > 40: LLM generates plain-English summary + top 3 fixes
5. Bot posts a comment on the PR with score badge, summary, and fixes
6. Score + metadata saved to Postgres
7. Dashboard reads from Postgres to show team trends and high-risk PRs

For API health:
1. Engineer or EM uploads OpenAPI spec via dashboard
2. Static rules score the spec across 5 dimensions
3. LLM generates summary + improvement suggestions
4. Results saved and shown in dashboard under API Debt tab

---

## Output

### Bot PR Comment (Engineer)

```
## ShipSmart Complexity Report 🔴 74/100

**Summary:** This PR introduces a deeply nested auth middleware with 6 layers
of abstraction. PaymentService has 4 responsibilities and will be hard to test.

**Top fixes:**
1. Split `PaymentService` — extract validation logic into `PaymentValidator`
2. Flatten `AuthMiddleware` — max 2 nesting levels recommended
3. Add interface for `OrderProcessor` to enable isolated testing
```

### Dashboard — Engineer View
- Complexity score for current PR (0–100, colour-coded)
- Plain-English summary
- Top 3 fix suggestions with file references
- API health score per uploaded spec with per-category breakdown

### Dashboard — EM View
- Average team complexity score (this week vs last week)
- Count of high-risk PRs (score > 65)
- List of high-risk PRs with scores, linked to PR
- API debt issue count
- Complexity trend chart (last 5 weeks, colour-coded by severity)

---

## Pricing Model (SaaS Leverage)

| Plan | Price | Limit |
|---|---|---|
| Free | $0 | 1 repo, 50 PRs/month |
| Team | $49/month | Up to 10 repos, unlimited PRs |
| Pro | $149/month | Unlimited repos, EM dashboard, API health |

Leverage type: **Code** — zero marginal cost per additional PR analyzed beyond LLM API cost (covered by pricing margin).

---

## MVP Scope

The MVP ships these features only:

- [ ] GitHub App webhook integration (PR open/update)
- [ ] Static complexity rules engine (5 rules)
- [ ] LLM explainer via Claude API (summary + top 3 fixes)
- [ ] Bot PR comment posting
- [ ] Postgres score storage
- [ ] Dashboard: engineer PR view
- [ ] Dashboard: EM team trends view
- [ ] GitHub OAuth login
- [ ] OpenAPI spec upload + API health scoring

Out of scope for MVP:
- GitLab integration
- Slack notifications
- Custom rule configuration
- Historical export / CSV
- SSO / team management

---

## Error Handling

- Webhook failures: retry up to 3 times with exponential backoff; log and skip if all fail
- LLM timeout (>10s): post score without explanation, note "explanation unavailable"
- Invalid OpenAPI spec: return parse error with line reference, do not score
- Auth failure: redirect to GitHub OAuth, never expose raw errors to UI

---

## Testing Strategy

- Static rules engine: unit tested with fixture diffs (known input → known score)
- LLM explainer: integration tested with recorded responses (VCR-style)
- Webhook service: tested with GitHub webhook payload fixtures
- Dashboard: component tests for score display, trend chart, PR list
- E2E: one happy-path test — PR opened → bot comment posted
