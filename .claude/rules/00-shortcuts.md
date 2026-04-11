# Token-saving shortcuts

Read this first. It tells you where everything lives so you skip the exploration phase.

## Where rules live

| Question | File |
|---|---|
| What's the crate layout? | Root `CLAUDE.md` |
| How do I add a new service? | `.claude/rules/01-architecture.md` |
| How do I cross the IPC boundary? | `.claude/rules/02-ipc.md` |
| What's the Rust code style? | `.claude/rules/03-rust-style.md` |
| What's the TS/React style? | `.claude/rules/04-frontend-style.md` |
| What colors / primitives / tokens can I use? | `.claude/rules/05-frontend-design-system.md` |
| How do I write tests? | `.claude/rules/06-testing.md` |
| How do I verify a change? | `.claude/rules/07-verification.md` |
| Can I commit this? | `.claude/rules/08-commits.md` |
| Am I over-engineering? | `.claude/rules/09-anti-patterns.md` |
| Per-crate design details? | `crates/<name>/CLAUDE.md` (auto-loads) |

## Where project docs live

| Topic | File |
|---|---|
| Frontend architecture (stores, tabs, shortcuts) | `.claude/frontend.md` |
| Tauri command modules and wiring | `.claude/tauri-commands.md` |
| Design reference | `.claude/design-reference.md` |
| Sidebar known bugs (before touching sidebar) | `.claude/sidebar-known-issues.md` |
| Contract Lock follow-ups (C1/I1/I4) | `.claude/review-contract-lock.md` |
| Specs | `docs/superpowers/specs/` |
| Plans | `docs/superpowers/plans/` |

## Skills

| Skill | Use |
|---|---|
| `/verify` | Run the full verification suite |
| `/1-git-commit` | Generate a conventional commit message |
| `/0-memorize` | Update CLAUDE.md after learning something repo-wide |
| `rocket-workflow` | DDD boundaries and verification commands |
| `systematic-debugging` | For any unexpected behaviour or test failure |
| `verification-before-completion` | Before claiming any work is done |

## Commands

```bash
# Dev
yarn tauri dev            # full desktop + Vite HMR
yarn dev                  # frontend only

# Checks (fast)
cargo check
cargo test --workspace --no-run
yarn tsc --noEmit
yarn check

# Single tests
cargo test -p <crate> <test_name>
yarn test --run <pattern>

# Build (slow)
yarn tauri build
```

## Before you explore

If you're about to `grep` or `glob` for something, first check:

1. Root `CLAUDE.md` — crate table
2. Per-crate `CLAUDE.md` — design rules for that crate
3. `.claude/rules/` — conventions
4. `.claude/frontend.md` / `.claude/tauri-commands.md` — architecture

Most questions are answered without a single tool call.
