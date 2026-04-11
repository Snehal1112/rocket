# Verification rules

Never claim a change is complete without running the verification suite.

## The commands

Run these in parallel (single message, multiple Bash calls):

```bash
cargo check
cargo test --workspace --no-run    # compiles test code without running it (fast)
yarn tsc --noEmit
yarn check                          # Biome lint + format, read-only
```

For the feature you actually touched, also run:

```bash
cargo test -p <crate> <test_prefix>
yarn test --run <pattern>
```

## Shortcuts

- `/verify` — runs the full suite via the `verify` skill. Prefer this over hand-rolling.
- `cargo check` is sufficient for fast Rust validation. Full `cargo build` is slow — only run when you need the binary.
- `cargo test --workspace --no-run` catches test compile errors in ~30s without running any test.

## Reporting

- Report results as a table with `Check | Status | Notes`.
- If any check fails, paste the relevant error output verbatim. Do not summarize or paraphrase.
- Pre-existing lint warnings are not your problem unless the user asks. Flag them as pre-existing, do not fix them silently.
- Honesty > optimism. Do not claim success unless every check passed.

## Do not

- Do not claim "tests pass" based on the previous run. Rerun after every change.
- Do not skip verification to save time. The cost of a broken main is much higher than a 30-second check.
- Do not use `--no-verify` on git commits to bypass pre-commit hooks.
