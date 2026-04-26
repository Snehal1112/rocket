# Specific Knowledge Excavator — Prompt README

A structured prompt for identifying your unique knowledge niche and building leveraged income models around it, based on Naval Ravikant's wealth philosophy.

---

## Overview

This prompt reverse-engineers your intellectual fingerprint — the rare intersection of your obsessions, career detours, and undervalued skills that nobody else holds in the same combination. It then proposes concrete, zero-marginal-cost business models built on that niche.

---

## How to Use

1. Open the prompt in your API client (e.g. Rocket API)
2. Fill in the three `# CONTEXT` fields with your real details:
   - **Obsessions** — topics you think about even when not paid to
   - **Career detours** — unexpected paths, pivots, or roles you've held
   - **Undervalued skills** — things you do well that others don't recognize as rare
3. Send to any capable LLM (Claude, GPT-4, etc.)
4. Review the niche sentence — if it passes the "Could I be trained for this?" test, the model will re-excavate automatically

---

## Prompt Structure

| Section | Purpose |
|---|---|
| `# ROLE` | Sets the analyst persona and philosophy |
| `# TASK` | Defines the goal: excavate a leveraged knowledge profile |
| `# CONTEXT` | Your input — obsessions, detours, undervalued skills |
| `# STEPS` | The 6-step reasoning process the model follows |
| `# RULES` | Guardrails: no generic niches, no labor-based models |
| `# OUTPUT FORMAT` | Structured output: niche sentence + 3 scored business models |

---

## Output Format

```
**Your Specific Knowledge Niche:** [One precise sentence]

**Business Model 1 — [Name]**
- Leverage type: [code / media / capital]
- Description: [2-3 sentences]
- Market size: X/5 | Competition: X/5 | Leverage multiplier: X/5

**Business Model 2 — [Name]**
- Leverage type: [code / media / capital]
- Description: [2-3 sentences]
- Market size: X/5 | Competition: X/5 | Leverage multiplier: X/5

**Business Model 3 — [Name]**
- Leverage type: [code / media / capital]
- Description: [2-3 sentences]
- Market size: X/5 | Competition: X/5 | Leverage multiplier: X/5
```

---

## Scoring Guide

| Metric | Scale | Notes |
|---|---|---|
| Market size | 1–5 | 5 = large addressable market |
| Competition | 1–5 | Lower is better — 1 = nearly no competition |
| Leverage multiplier | 1–5 | 5 = scales with zero marginal cost |

---

## Rules the Model Follows

- Rejects generic niches (marketing, coaching, consulting) unless your angle is genuinely differentiated
- Every business model must specify its leverage type: **code**, **media**, or **capital**
- Never suggests labor-based models — scale must be achievable without trading time for money

---

## Example Context Block

```
# CONTEXT:
- Obsessions: API design, developer tooling, systems thinking
- Career detours: Started in finance, moved to backend engineering, briefly ran a dev agency
- Undervalued skills: Can explain complex technical architecture to non-technical stakeholders; strong intuition for product-market fit in B2B dev tools
```

---

## Philosophy

Based on Naval Ravikant's concept of **specific knowledge**:

> "Specific knowledge is knowledge that you cannot be trained for. If society can train you, they can train someone else and replace you."

The prompt enforces this by testing every proposed niche against the trainability filter and discarding anything that fails.
