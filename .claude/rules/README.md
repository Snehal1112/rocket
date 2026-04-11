# Rocket rules

Focused rule files. Read the relevant ones before exploring — every rule here exists because ignoring it costs tokens, breaks tests, or violates layering.

Files are numbered in the order you should consult them. Start at `00` and skip to whichever rules are relevant to your task — do not read all of them up front.

| File | When to read |
|---|---|
| [00-shortcuts.md](00-shortcuts.md) | First — pointer map to avoid re-discovering conventions |
| [01-architecture.md](01-architecture.md) | Before touching any Rust crate or adding a new service |
| [02-ipc.md](02-ipc.md) | Before adding or modifying a Tauri command or frontend wire type |
| [03-rust-style.md](03-rust-style.md) | Before writing or editing Rust code |
| [04-frontend-style.md](04-frontend-style.md) | Before writing or editing TypeScript / React code |
| [05-frontend-design-system.md](05-frontend-design-system.md) | Before building or editing any UI — theme tokens, primitives, variants |
| [06-testing.md](06-testing.md) | Before writing or changing tests |
| [07-verification.md](07-verification.md) | Before claiming any change is complete |
| [08-commits.md](08-commits.md) | Before running any `git` command that changes history |
| [09-anti-patterns.md](09-anti-patterns.md) | When tempted to add validation, helpers, or refactors |

## How to use

1. Start with [00-shortcuts.md](00-shortcuts.md) — it tells you where everything lives so you skip the exploration phase.
2. Read the rules relevant to your task. Do not read all of them up front.
3. If a rule is wrong or out of date, fix the file directly. These are living rules.
