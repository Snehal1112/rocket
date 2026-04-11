# Anti-patterns

Things not to do. Every item here has burned time or broken something in the past.

## Over-engineering

- Do not add error handling, validation, or fallbacks for scenarios that can't happen. Trust internal invariants. Validate only at system boundaries (user input, external APIs).
- Do not add feature flags or compat shims when a direct change is possible.
- Do not create helpers, utilities, or abstractions for one-time operations. Three similar lines of code beat a premature abstraction.
- Do not design for hypothetical future requirements. The right amount of complexity is what the task actually needs — no more.
- Do not add `try`/`catch` around internal calls that can't fail. Trust the type system.

## Scope creep

- Do not refactor unrelated code while fixing a bug. The fix does not need surrounding code cleaned up.
- Do not add features beyond what was asked. A simple feature does not need extra configurability.
- Do not add docstrings, comments, or type annotations to code you did not change.
- Do not rename variables, reorder imports, or reformat whitespace in files you are editing for other reasons.

## Silent failure

- Do not swallow errors with `.ok()` / `let _ = ...` without a comment explaining why the error is safe to drop.
- Do not use `.unwrap_or(default)` where the default masks a real bug. Use `.unwrap_or_else(|e| { log::warn!(...); default })` or propagate.
- Do not fall back from `dir_name` to `name` silently. Skip or error. Silent mismatch is worse than a loud failure. (Real example: the Contract Lock baseline walker — see `.claude/review-contract-lock.md`.)
- Do not `catch` and re-throw with the original error lost. At least include it as `cause`.

## Memory and identity

- Do not return a fresh empty array/object from a Zustand selector on every call. Use a module-level `EMPTY_<X>` sentinel. Fresh identity re-renders everything downstream.
- Do not clone large domain structs in hot paths. Borrow.
- Do not hold a `Mutex` lock across an `.await` or a long computation. Drop the guard first.

## Git

- Do not commit without an explicit user ask.
- Do not `--amend` after a pre-commit hook failure. Create a new commit.
- Do not `git add -A` / `git add .` — sweeps in `.env` and credentials. Stage specific files by name.
- Do not force-push to `main`. Ever.

## Task completion

- Do not claim "done" without running the verification commands.
- Do not claim "tests pass" based on a previous run. Rerun after every change.
- Do not summarize what you did at the end of every response. The diff speaks for itself.
- Do not estimate how long a task will take. Focus on what to do, not how long it will take.

## Exploration

- Do not `grep` / `glob` before checking `CLAUDE.md` files and `.claude/rules/`.
- Do not spawn a subagent for a task you can do in one tool call.
- Do not read an entire 2000-line file when you know the function you need — use `Grep` with line numbers, then `Read` with offset/limit.
