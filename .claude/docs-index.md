# Docs Index

Pointers to the feature and engineering docs. Open the one that matches the area you touch.

## Features

- `docs/features/script-files.md` for shared .js script files and local require().
- `docs/features/folder-settings.md` for the folder settings tab, `folder.yml` sections and inheritance.
- `docs/features/assistant-composer.md` for the AI Assistant composer (PromptEditor keys, chips, masking, storage keys).
- `docs/features/assistant-conversations.md` for stored assistant conversations (storage, pins, masking, caps).
- `docs/features/collection-trust.md` for the collection trust gate (requested vs allowed access, `{{process.env}}` gate).
- `docs/features/console-masking.md` for the Console secret masking rule (masked copy, reveal toggle, fail-closed, vault values).
- `docs/features/assistant-followups.md` for workspace-assistant decisions and open follow-ups.
- `docs/features/acp-multi-agent-support.md` for ACP agent kinds (Claude, Gemini, Generic), the session plan and the verified Gemini facts.
- `docs/features/flow-request-refs.md` for how flows follow request moves (rewrite, event, delete dialog).
- `docs/features/collection-export.md` for the collection export to Bruno and Postman (outputs, decisions, known losses, manual checks).
- `docs/features/example-masking.md` for how saved request examples get their secrets masked in a git commit.
- `docs/features/bruno-import-strictness.md` for Bruno 3 imports (files are copied as they are), what Rocket's readers now tolerate, and the known gaps.
- `docs/features/response-examples.md` for saved response examples (save, view, limits, the Try temporary tab and its save guards, manual checks).
- `docs/features/openapi-sync-ux.md` for the OpenAPI sync tab (spec updates bulk rules, View spec, Reset and Restore, Connect, local files, known limits, manual checks).
- `docs/features/ipc-command-acl.md` for the app command ACL (app-commands.txt, main-window-only access).
- `docs/features/settings-page.md` for the global Settings page (sections, lock, shortcuts, how to add a section).
- `docs/features/elevation.md` for the four elevation levels (VS Code colours, shadows, which surface uses which, borders).
- `docs/features/update-notification.md` for the update notice (schedule, install kinds, settings, manual checks).
- `docs/features/team-network.md` for the team network (discovery, pairing, presence, Network panel, Settings section, send a collection, keep in sync, limits, manual checks).

## Engineering

- `docs/engineering/ddd-decisions.md` for the DDD rulings (mixed crates, `rocket-app` size, events direction, domain model depth, assistant naming).
- `docs/engineering/benchmarks.md` for the Rocket vs Bruno vs Postman benchmark (method, per-run numbers, findings, how to re-run).
- `docs/engineering/subagents.md` for which subagent type and model to use, the cost model and the limits seen in practice.

## Roadmaps

- `docs/roadmaps/contracts-ci-ai-roadmap.md` for the contracts, CI and AI direction (projects CA-1 to CA-4).
- `.claude/sprint-plan.md` for the master roadmap split into one-week sprints.

## Other

- `rocket-landing/.claude/deploy-railway.md` for hosting the landing page on Railway (project `rocket-app`, Dockerfile, manual deploy).
- The software factory is the standalone project `~/data/rocket/rocket-factory` (home `~/.factory`, project `rocket`, global `/factory` command). Rocket's old in-repo copy is archived in `~/data/rocket/Archive/rocket-tools-factory/`.
