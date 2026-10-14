# Tree walk syscall counts and timing (issue #95)

Measures one `get_summaries` load of the deep fixture: 500 folders, 5 HTTP requests each, an `_order.yml` in every folder (25 parents with 19 children each, built by `populate_deep` in `crates/rocket-infra/benches/support/mod.rs`).

## How to measure

```bash
export ROCKET_TREE_WALK_FIXTURE="$PWD/.superpowers/tree-walk-fixture"
rm -rf "$ROCKET_TREE_WALK_FIXTURE" && mkdir -p "$ROCKET_TREE_WALK_FIXTURE"
cargo test -j4 -p rocket-infra --test tree_walk_syscalls build_deep_fixture -- --ignored --exact
BIN=$(cargo test -j4 -p rocket-infra --test tree_walk_syscalls --no-run 2>&1 | sed -n 's/.*(\(.*tree_walk_syscalls-[0-9a-f]*\)).*/\1/p' | head -1)
strace -f -c -e trace=file -o strace.txt "$BIN" --ignored --exact load_deep_fixture_once
cargo bench -j4 -p rocket-infra --bench collection -- deep
```

Run `strace` three times and keep the smallest `total` row, because start-up of the test binary adds a small constant. Timing comes from criterion (`time: [low mid high]`). For a before and after comparison use `-- --save-baseline before deep` and then `-- --baseline before deep`.

## Informational baseline (before any production change, Linux)

| measure | value |
|---|---|
| `strace` total calls, lowest of 3 | 14087 |
| criterion `get_summaries/deep` time | [50.913 ms 51.185 ms 51.453 ms] |

This baseline is informational. The gate is the paired before and after run of the second plan, done in one worktree in one sitting.
