# Rocket docs

All reference and planning docs live here. Do not add loose `.md` files to `.claude/`. Only
rules, commands, skills and per-crate `CLAUDE.md` files stay where they are, because tools load them.

## Layout

| Folder | What is in it | Edit by hand? |
|---|---|---|
| `superpowers/specs/` | One design or spec file per feature, named `YYYY-MM-DD-<topic>-design.md`. | New files only, via the spec workflow. |
| `superpowers/plans/` | Implementation plans. A dated file is a single plan. A folder is a feature with `00-plan-index.md` and numbered plans. | New files only, via the plan workflow. |
| `features/` | Per-feature notes: decisions, limits, known gaps (export, trust gate, masking, assistant, folder settings). | Yes. |
| `roadmaps/` | `roadmap.md` (R-xx), `flow-roadmap.md` (F-xx) and the plan audit. | Yes, update when work ships. |
| `engineering/` | Subagent guidance, benchmarks, perf notes, the events and IPC rework. | Yes. |
| `manual-tests/` | Step lists for checks that need the real app. Helper `.sh` scripts stay in `.claude/manual-tests/`. | Yes. |
| `architecture/` | System diagrams and the contract state machine (HTML, SVG, Markdown). | Yes. |
| `product/` | Pitches and product writing. | Yes. |
| `research/` | Notes on other tools (for example the Bruno Git UI blueprint). | Yes. |
| `reports/` | One-off reviews: `component-duplication/`, `git-integration-review/`, the infra synthesis. | No, they are snapshots. |
| `dsm/` | Design system page. `scripts/sync-dsm-tokens.cjs` reads it. | Keep the path. |
| `archive/` | Unrelated notes. | No. |

`superpowers/` keeps its path on purpose: the superpowers skills, the software factory and
many links in `CLAUDE.md`, `.claude/` and the plans point to it.

## Start here

- Collection, request, environment or auth work: `superpowers/specs/opencollection-spec-reference.md`
  (required read, see `CLAUDE.md`).
- Collection export (Bruno ZIP, Postman): `docs/features/collection-export.md`. Its design spec
  (`2026-10-23-collection-export-design.md`) is on the export branch and lands in
  `superpowers/specs/` with it.
- Contracts: `architecture/contract-state-machine.md` and `architecture/contract_state_machine.html`.
- Overall backend picture: `architecture/rocketapi_backend_architecture.html`.

## Find the spec and plan for a feature

1. Search the spec by topic: `ls docs/superpowers/specs | grep -i <topic>`.
2. Look for a plan folder with the same topic: `ls docs/superpowers/plans | grep -i <topic>`.
3. Open its `00-plan-index.md` first. It lists the plans, their order and their status.
4. For open work, check `docs/roadmaps/roadmap.md` and `docs/roadmaps/flow-roadmap.md`.

Plan folders by area:

- **Flow:** `flow-visual-workflow-builder`, `flow-phase2-branching`, `flow-phase3-transform-node`,
  `flow-async`, `flow-tier2`, `flow-tier3`, `flow-properties-tabs`, `flow-node-properties-panel`,
  `flow-wire-script-editor`, `flow-request-debug-mode`, `2026-10-18-flow-request-ref-rewrite`.
- **AI assistant and ACP:** `acp-transport`, `acp-mcp-tool-server`, `acp-chat-panel`,
  `acp-agent-config-credentials`, `agent-local-credential`, `workspace-ai-assistant`,
  `assistant-conversation-history`, `assistant-folder-tools`, `2026-10-14-ai-agents-dialog-redesign`.
- **Collections, environments, variables:** `folder-settings`, `variables`, `variable-aware-url`,
  `environment-client-certificates`, `environment-client-certificates-vault-source`,
  `2026-10-18-env-extends-dotenv`, `create-request`, `2026-10-19-duplicate-request-backend`,
  `imports`, `workspace`.
- **Secrets and security:** `rocketvault-external-secrets`, `secret-provider-foundation`,
  `azure-key-vault-provider`, `2026-10-15-console-mask-secrets`, `auth2.0`.
- **Scripting and protocols:** `scripting`, `js-script-files`, `rok-parity-a`, `rok-parity-b`,
  `protocol-parity`, `collection-runner`, `load-testing`, `load-testing-throughput`.
- **Contracts and git:** `contract-enhancement`, `contract-lock`, `git`.
- **Software factory:** `2026-10-13-software-factory*`, `2026-10-14-factory-*`,
  `2026-10-16-*factory*`, `2026-10-17-*factory*`.
- **Small clean-ups:** the other dated files and folders from `2026-10-15` to `2026-10-18`.

## Rules for new docs

- A feature spec goes in `superpowers/specs/`, its plans in `superpowers/plans/<feature>/`.
- Diagrams and system overviews go in `architecture/`. Pitches go in `product/`. Notes on other
  tools go in `research/`.
- Reviews and audits that describe one moment in time go in `reports/`.
- Link by relative path from the new file. Keep file names in lowercase with hyphens.
